use coach_vision::{hud, scoreboard as sb, ColorLibrary};
fn main() {
    let img = image::open(std::env::args().nth(1).unwrap()).unwrap();
    let cols: Vec<usize> = std::env::args().skip(2).filter_map(|s| s.parse().ok()).collect();
    let lib = ColorLibrary::load_centered(std::path::Path::new("data/icons/shop")).unwrap();
    let cs: Vec<f32> = hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).cloned().collect();
    let cell = 64u32;
    let mut sheet = image::RgbImage::new(cell*4*cols.len() as u32, cell*sb::ITEM_ROWS as u32);
    for (ci, col) in cols.iter().enumerate() { for r in 0..sb::ITEM_ROWS { for c in 0..2 {
        let crop = hud::crop(&img, sb::item_cell(cs[*col], c, r));
        let x0 = (ci as u32*4 + c as u32*2) * cell;
        let a = crop.resize_exact(cell, cell, image::imageops::FilterType::Nearest).to_rgb8();
        image::imageops::overlay(&mut sheet, &a, x0 as i64, (r as u32*cell) as i64);
        let m = lib.top(&crop, 2);
        println!("col{col} r{r} c{c} contrast {:.0}: {} {:.2} / {} {:.2}", hud::contrast(&crop), m[0].1, m[0].0, m[1].1, m[1].0);
        if let Ok(t) = image::open(format!("data/icons/shop/{}.png", m[0].1)) {
            let b = t.resize_exact(cell, cell, image::imageops::FilterType::Triangle).to_rgb8();
            image::imageops::overlay(&mut sheet, &b, (x0+cell) as i64, (r as u32*cell) as i64);
        }
    }}}
    sheet.save("frames/sbitems.png").unwrap();
}
