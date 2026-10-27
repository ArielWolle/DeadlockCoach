//! Calibration dump of top-bar health bars: RGB/HSV down the bar core for each slot.
//! Usage: bar_ascii FRAME [SLOT]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let img = image::open(&a[0]).unwrap();
    let img = if img.width() != 1600 { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) } else { img }.to_rgb8();
    let centers: Vec<f32> = coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS).collect();
    let only: Option<usize> = a.get(1).map(|s| s.parse().unwrap());
    for (i, cx) in centers.iter().enumerate() {
        if only.is_some_and(|o| o != i) { continue; }
        println!("slot {i} cx {cx}");
        for y in 20..66u32 {
            let t = (y as f32 - 24.0) / 38.0;
            // Bar core: ally right edge fixed at cx-20; enemy left edge cx+25 -> cx+22.
            let x = if i < 6 { cx - 21.0 } else { cx + 25.0 - 3.0 * t.clamp(0.0, 1.0) + 1.0 };
            let p = img.get_pixel(x as u32, y);
            let (h, s, v) = coach_vision::minimap::hsv(p[0], p[1], p[2]);
            println!("  {y:2} x{:4} {:3} {:3} {:3}  h{h:4.0} s{s:4.2} v{v:4.2}", x as u32, p[0], p[1], p[2]);
        }
    }
}
