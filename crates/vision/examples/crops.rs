use coach_vision::hud;
fn main() {
    let img = image::open(std::env::args().nth(1).unwrap()).unwrap();
    let mut sheet = image::RgbImage::new(400, 200);
    for (i, r) in [hud::ALLY_TOTAL, hud::ENEMY_TOTAL].iter().enumerate() {
        let c = hud::crop(&img, *r).resize_exact(400, 100, image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut sheet, &c, 0, (i as i64) * 100);
    }
    sheet.save("frames/totals.png").unwrap();
}
