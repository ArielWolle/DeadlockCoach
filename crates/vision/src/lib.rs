//! Vision: resolution-independent HUD regions and icon template matching.
pub mod hud;
pub mod ocr;
pub mod parse;
pub mod scan;
pub mod screen;
use image::{imageops::{crop_imm, resize, FilterType}, GrayImage, RgbaImage};
use std::path::Path;

/// A region as fractions of the frame (0..1), so it works at any resolution.
#[derive(Debug, Clone, Copy)]
pub struct Region { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl Region {
    pub fn crop(&self, img: &GrayImage) -> GrayImage {
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let (x, y) = ((self.x * iw) as u32, (self.y * ih) as u32);
        let (w, h) = (((self.w * iw) as u32).max(1), ((self.h * ih) as u32).max(1));
        crop_imm(img, x, y, w.min(img.width() - x), h.min(img.height() - y)).to_image()
    }
}

/// Normalized cross-correlation of two equal-size images, in -1..1.
pub fn ncc(a: &GrayImage, b: &GrayImage) -> f32 {
    assert_eq!(a.dimensions(), b.dimensions());
    let n = (a.width() * a.height()) as f32;
    let (ma, mb) = (a.pixels().map(|p| p[0] as f32).sum::<f32>() / n, b.pixels().map(|p| p[0] as f32).sum::<f32>() / n);
    let (mut num, mut da, mut db) = (0f32, 0f32, 0f32);
    for (p, q) in a.pixels().zip(b.pixels()) {
        let (x, y) = (p[0] as f32 - ma, q[0] as f32 - mb);
        num += x * y; da += x * x; db += y * y;
    }
    if da == 0.0 || db == 0.0 { 0.0 } else { num / (da.sqrt() * db.sqrt()) }
}

pub struct Template { pub name: String, pub img: GrayImage }

/// Icon library loaded from data/icons/<kind>/*.png (name = file stem).
pub struct Library { pub templates: Vec<Template> }

impl Library {
    pub fn load(dir: &Path) -> std::io::Result<Library> {
        let mut templates = vec![];
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            if p.extension().map(|x| x == "png").unwrap_or(false) {
                if let Ok(img) = image::open(&p) {
                    // Composite on black so transparent icon backgrounds don't dominate.
                    let rgba: RgbaImage = img.to_rgba8();
                    let mut g = GrayImage::new(rgba.width(), rgba.height());
                    for (x, y, px) in rgba.enumerate_pixels() {
                        let l = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) * px[3] as f32 / 255.0;
                        g.put_pixel(x, y, image::Luma([l as u8]));
                    }
                    templates.push(Template { name: p.file_stem().unwrap().to_string_lossy().into(), img: g });
                }
            }
        }
        Ok(Library { templates })
    }

    /// Best-matching template for a cropped slot image. Returns (name, score, margin to runner-up).
    pub fn identify(&self, slot: &GrayImage) -> Option<(String, f32, f32)> {
        const S: u32 = 32; // compare at a fixed size: scale independent
        let s = resize(slot, S, S, FilterType::Triangle);
        let mut scored: Vec<(f32, &str)> = self.templates.iter()
            .map(|t| (ncc(&s, &resize(&t.img, S, S, FilterType::Triangle)), t.name.as_str())).collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let (best, second) = (scored.first()?, scored.get(1).map(|x| x.0).unwrap_or(0.0));
        Some((best.1.to_string(), best.0, best.0 - second))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    fn pattern(seed: u32) -> GrayImage {
        GrayImage::from_fn(48, 48, |x, y| Luma([(((x * (seed + 3) + y * (seed + 7)) * 37 + x * y * seed) % 251) as u8]))
    }
    fn lib() -> Library {
        Library { templates: (1..6).map(|i| Template { name: format!("item{i}"), img: pattern(i) }).collect() }
    }

    #[test]
    fn identifies_icon_at_different_scale_in_frame() {
        let l = lib();
        // Frame 800x450 with item3 drawn at 60x60 in the region (0.5,0.5,0.1,0.1)-ish.
        let mut frame = GrayImage::new(800, 450);
        let icon = resize(&pattern(3), 40, 40, FilterType::Triangle);
        image::imageops::overlay(&mut frame, &icon, 400, 225);
        let slot = Region { x: 400.0 / 800.0, y: 225.0 / 450.0, w: 40.0 / 800.0, h: 40.0 / 450.0 }.crop(&frame);
        let (name, score, margin) = l.identify(&slot).unwrap();
        assert_eq!(name, "item3");
        assert!(score > 0.8 && margin > 0.2, "score {score} margin {margin}");
    }

    #[test]
    fn region_scales_with_resolution() {
        let r = Region { x: 0.5, y: 0.5, w: 0.1, h: 0.1 };
        assert_eq!(r.crop(&GrayImage::new(1000, 1000)).dimensions(), (100, 100));
        assert_eq!(r.crop(&GrayImage::new(2000, 1000)).dimensions(), (200, 100));
    }
}

/// Color template library: compares resized RGB crops (center 70%), robust for portraits.
/// Portrait templates ("top_square") get several crop-window variants; the best variant counts.
pub struct ColorLibrary { pub templates: Vec<(String, Vec<Vec<f32>>)> }

const CS: u32 = 24;

fn color_vec(img: &image::DynamicImage) -> Vec<f32> {
    let (w, h) = (img.width(), img.height());
    let (mx, my) = (w * 15 / 100, h * 15 / 100);
    let c = img.crop_imm(mx, my, (w - 2 * mx).max(1), (h - 2 * my).max(1));
    let r = resize(&c.to_rgb8(), CS, CS, FilterType::Triangle);
    let mut v: Vec<f32> = r.pixels().flat_map(|p| p.0.map(|x| x as f32)).collect();
    let m = v.iter().sum::<f32>() / v.len() as f32;
    let n = v.iter().map(|x| (x - m).powi(2)).sum::<f32>().sqrt().max(1e-6);
    for x in &mut v { *x = (*x - m) / n; }
    v
}

fn variants(img: &image::DynamicImage, top_square: bool) -> Vec<Vec<f32>> {
    if !top_square { return vec![color_vec(img)]; }
    let (w, h) = (img.width(), img.height());
    let mut out = vec![];
    for size_pct in [100u32, 85, 70] {
        let s = (w * size_pct / 100).min(h);
        let x0 = (w - s) / 2;
        for y_pct in [0u32, 8, 16, 24] {
            let y0 = (h * y_pct / 100).min(h - s);
            out.push(color_vec(&img.crop_imm(x0, y0, s, s)));
        }
    }
    out
}

impl ColorLibrary {
    pub fn load(dir: &Path, top_square: bool) -> std::io::Result<ColorLibrary> {
        let mut templates = vec![];
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            if p.extension().map(|x| x == "png").unwrap_or(false) {
                if let Ok(img) = image::open(&p) {
                    templates.push((p.file_stem().unwrap().to_string_lossy().into(), variants(&img, top_square)));
                }
            }
        }
        Ok(ColorLibrary { templates })
    }

    /// Scores of every template (best variant), in template order.
    pub fn scores(&self, img: &image::DynamicImage) -> Vec<(String, f32)> {
        let v = color_vec(img);
        self.templates.iter().map(|(n, vs)| {
            let best = vs.iter().map(|t| t.iter().zip(&v).map(|(a, b)| a * b).sum::<f32>()).fold(f32::MIN, f32::max);
            (n.clone(), best)
        }).collect()
    }

    /// Top-n (score, name).
    pub fn top(&self, img: &image::DynamicImage, n: usize) -> Vec<(f32, String)> {
        let mut s: Vec<(f32, String)> = self.scores(img).into_iter().map(|(a, b)| (b, a)).collect();
        s.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        s.truncate(n);
        s
    }

    /// (name, score, margin to runner-up)
    pub fn identify(&self, img: &image::DynamicImage) -> Option<(String, f32, f32)> {
        let s = self.top(img, 2);
        let (b, second) = (s.first()?, s.get(1).map(|x| x.0).unwrap_or(0.0));
        Some((b.1.clone(), b.0, b.0 - second))
    }
}
