//! Contact sheet of minimap hero detections: white box = ally, red = enemy, green = you.
//! Usage: mm_heroes OUT.png FRAME...
use coach_vision::minimap::*;
/// The located minimap resampled to canonical 335x350 (or the fixed crop when not found).
pub fn canonical_map(img: &image::DynamicImage) -> image::RgbImage {
    if img.width() == 335 { return img.to_rgb8(); }
    match coach_vision::localize::MapLocator::default().locate(img) {
        Some(m) => img.crop_imm(m.bounds.x, m.bounds.y, m.bounds.w, m.bounds.h).resize_exact(335, 350, image::imageops::FilterType::Triangle).to_rgb8(),
        None => crop_from_frame(img),
    }
}
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let files = &a[1..];
    let k = 3u32;
    let cols = files.len().min(3) as u32;
    let rows = (files.len() as u32).div_ceil(cols);
    let mut sheet = image::RgbImage::new(335 * k * cols, 350 * k * rows);
    let an = Analyzer::new();
    for (i, f) in files.iter().enumerate() {
        let img = image::open(f).unwrap();
        let c = canonical_map(&img);
        let mut canvas = image::imageops::resize(&c, 335 * k, 350 * k, image::imageops::FilterType::Lanczos3);
        let st = an.analyze(&c);
        let mut mark = |p: &coach_core::Pt, col: [u8; 3], r: i32| {
            let (cx, cy) = ((p.x * 335.0 * k as f32) as i32, (p.y * 350.0 * k as f32) as i32);
            for d in -r..=r { for (dx, dy) in [(d, -r), (d, r), (-r, d), (r, d)] {
                let (xx, yy) = (cx + dx, cy + dy);
                if xx >= 0 && yy >= 0 && (xx as u32) < canvas.width() && (yy as u32) < canvas.height() { canvas.put_pixel(xx as u32, yy as u32, image::Rgb(col)); }
            }}
        };
        for p in &st.allies { mark(p, [0, 140, 255], 15); mark(p, [0, 140, 255], 16); }
        for p in &st.enemies { mark(p, [255, 0, 255], 15); mark(p, [255, 0, 255], 16); }
        if let Some(p) = &st.you { mark(p, [0, 255, 0], 16); }
        let px = |v: &Vec<coach_core::Pt>| v.iter().map(|p| format!("({:.0},{:.0})", p.x * 335.0, p.y * 350.0)).collect::<Vec<_>>().join(" ");
        println!("{i} {}: A{} [{}] E{} [{}] you={:?}", std::path::Path::new(f).file_name().unwrap().to_string_lossy(), st.allies.len(), px(&st.allies), st.enemies.len(), px(&st.enemies), st.you.map(|p| ((p.x * 335.0) as i32, (p.y * 350.0) as i32)));
        image::imageops::overlay(&mut sheet, &canvas, ((i as u32 % cols) * 335 * k) as i64, ((i as u32 / cols) * 350 * k) as i64);
    }
    sheet.save(&a[0]).unwrap();
}
