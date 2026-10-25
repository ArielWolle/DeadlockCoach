//! HUD layout (fractions of the frame, measured on 16:9 captures) and readers.
use crate::{Library, Region};
use image::DynamicImage;

const W: f32 = 1600.0;
const H: f32 = 900.0;

fn px(x: f32, y: f32, w: f32, h: f32) -> Region {
    Region { x: x / W, y: y / H, w: w / W, h: h / H }
}

pub const ALLY_CENTERS: [f32; 6] = [310.0, 383.0, 457.0, 530.0, 603.0, 677.0];
pub const ENEMY_CENTERS: [f32; 6] = [920.0, 994.0, 1067.0, 1140.0, 1213.0, 1286.0];

pub fn portrait(cx: f32) -> Region { px(cx - 26.0, 8.0, 52.0, 52.0) }
pub fn networth_label(cx: f32) -> Region { px(cx - 24.0, 80.0, 48.0, 18.0) }
pub const CLOCK: Region = Region { x: 0.47, y: 0.01, w: 0.06, h: 0.035 };
pub const SOULS: Region = Region { x: 0.04, y: 0.80, w: 0.09, h: 0.06 };
pub const ALLY_TOTAL: Region = Region { x: 728.0 / W, y: 40.0 / H, w: 44.0 / W, h: 20.0 / H };
pub const ENEMY_TOTAL: Region = Region { x: 832.0 / W, y: 40.0 / H, w: 44.0 / W, h: 20.0 / H };

/// 12 inventory slots: (col, row) -> region. Columns 0..6, rows 0..2.
pub fn item_slot(col: usize, row: usize) -> Region {
    let cx = 44.0 + col as f32 * 51.0;
    let cy = 810.0 + row as f32 * 50.0;
    px(cx - 20.0, cy - 20.0, 40.0, 40.0)
}

pub fn crop(img: &DynamicImage, r: Region) -> DynamicImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    img.crop_imm((r.x * w) as u32, (r.y * h) as u32, ((r.w * w) as u32).max(1), ((r.h * h) as u32).max(1))
}

/// Standard deviation of luma: near 0 means an empty / flat slot.
pub fn contrast(img: &DynamicImage) -> f32 {
    let g = img.to_luma8();
    let n = (g.width() * g.height()) as f32;
    let m = g.pixels().map(|p| p[0] as f32).sum::<f32>() / n;
    (g.pixels().map(|p| (p[0] as f32 - m).powi(2)).sum::<f32>() / n).sqrt()
}

/// Identify an icon in a region; None if too flat or low confidence.
pub fn identify(img: &DynamicImage, r: Region, lib: &Library, min_score: f32, min_margin: f32) -> Option<String> {
    let c = crop(img, r);
    if contrast(&c) < 18.0 { return None; }
    let (name, score, margin) = lib.identify(&c.to_luma8())?;
    (score >= min_score && margin >= min_margin).then_some(name)
}

/// "2.6k" / "21k" / "950" -> souls. Tolerates OCR noise after the number ("2.6h", "3.71").
pub fn parse_networth(s: &str) -> Option<u32> {
    let s = s.trim();
    let mut int = String::new();
    let mut frac: Option<char> = None;
    let mut seen_dot = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if seen_dot { frac = Some(c); break; } else { int.push(c); }
        } else if c == '.' && !int.is_empty() && !seen_dot { seen_dot = true; }
        else if !int.is_empty() { break; }
    }
    let n: u32 = int.parse().ok()?;
    let has_k = s.chars().any(|c| c == 'k' || c == 'K');
    if seen_dot || has_k {
        Some(n * 1000 + frac.and_then(|c| c.to_digit(10)).unwrap_or(0) * 100)
    } else { Some(n) }
}
/// Team totals are shown as "21k"; the small "k" is often lost, so bare numbers < 1000 are thousands.
pub fn parse_team_total(s: &str) -> Option<u32> {
    let v = parse_networth(s)?;
    Some(if v < 1000 { v * 1000 } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn networth() {
        assert_eq!(parse_networth("2.6k"), Some(2600));
        assert_eq!(parse_networth("21k"), Some(21000));
        assert_eq!(parse_networth("950"), Some(950));
        assert_eq!(parse_networth("x"), None);
        assert_eq!(parse_networth("2.6h"), Some(2600));
        assert_eq!(parse_networth("3.71"), Some(3700));
    }
}
