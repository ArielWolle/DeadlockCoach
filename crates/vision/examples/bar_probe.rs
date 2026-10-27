//! Print the pixel health-bar reading for all twelve top-bar slots. Usage: bar_probe FRAME...
fn main() {
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let img = if img.width() != 1600 { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) } else { img }.to_rgb8();
        println!("{}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy());
        let start = std::time::Instant::now();
        let r: Vec<_> = (0..12).map(|i| coach_vision::team::read_bar(&img, i)).collect();
        let el = start.elapsed();
        for (i, b) in r.iter().enumerate() {
            println!("  {}{} timer_px={} visible={} ratio={} empty={} dead={}", if i < 6 { "A" } else { "E" }, i % 6, coach_vision::team::timer_pixels(&img, coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS).nth(i).unwrap()), b.visible,
                b.ratio.map(|v| format!("{:.0}%", v * 100.0)).unwrap_or("-".into()), b.empty, b.dead);
        }
        println!("  ({el:?})");
    }
}
