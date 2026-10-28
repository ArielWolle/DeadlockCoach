//! Scoreboard item cells: best shop-icon matches per cell and a contact sheet (crop | best match).
//! Usage: sb_items_probe FRAME OUT.png [DY]  (DY shifts the item grid vertically, 1600x900 px)
use coach_vision::{hud, scoreboard as sb, ColorLibrary};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let img = image::open(&a[0]).unwrap();
    let img = image::DynamicImage::ImageRgba8(if img.width() >= 1600 { img.thumbnail_exact(1600, 900).to_rgba8() } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle).to_rgba8() });
    let dy: f32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let lib = ColorLibrary::load_centered(std::path::Path::new("data/icons/shop")).unwrap();
    let cs: Vec<f32> = hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).cloned().collect();
    let cell = 40u32;
    let mut sheet = image::RgbImage::new(cell * 4 * 12, cell * sb::ITEM_ROWS as u32);
    let (mut good, mut n) = (0, 0);
    for (col, cx) in cs.iter().enumerate() { for r in 0..sb::ITEM_ROWS { for c in 0..2 {
        let mut reg = sb::item_cell(*cx, c, r); reg.y += dy / 900.0;
        let crop = hud::crop(&img, reg);
        let x0 = (col as u32 * 4 + c as u32 * 2) * cell;
        image::imageops::overlay(&mut sheet, &crop.resize_exact(cell, cell, image::imageops::FilterType::Nearest).to_rgb8(), x0 as i64, (r as u32 * cell) as i64);
        if hud::contrast(&crop) < 14.0 { continue; }
        n += 1;
        let m = lib.top(&crop, 2);
        if m[0].0 >= 0.72 && m[0].0 - m[1].0 >= 0.05 { good += 1; }
        println!("col{col} r{r} c{c}: {} {:.2} / {} {:.2}", m[0].1, m[0].0, m[1].1, m[1].0);
        if let Ok(t) = image::open(format!("data/icons/shop/{}.png", m[0].1)) {
            image::imageops::overlay(&mut sheet, &t.resize_exact(cell, cell, image::imageops::FilterType::Triangle).to_rgb8(), (x0 + cell) as i64, (r as u32 * cell) as i64);
        }
    }}}
    println!("cells {n} confident {good}");
    sheet.save(&a[1]).unwrap();
}
