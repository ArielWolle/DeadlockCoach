//! Own-HP bar fraction for frames (resized to 1600x900). Usage: hpbar_probe FRAME...
fn main() {
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let img = if img.width() != 1600 { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) } else { img };
        let r = coach_vision::vitals::bar_ratio(&img);
        println!("{} {}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy(), r.map(|v| format!("{:.2}", v)).unwrap_or("-".into()));
    }
}
