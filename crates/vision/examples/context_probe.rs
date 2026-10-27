//! Print the registered game context and clock reads for a letterboxed native frame.
use image::{DynamicImage, Rgba, RgbaImage};
fn main() {
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    let source = image::open(std::env::args().nth(1).unwrap()).unwrap();
    // A 1600x1000 input is treated as already letterboxed; optional "crop" arg removes the bars.
    let framed = if source.height() == 1000 { source } else {
        let mut framed = RgbaImage::from_pixel(1600, 1000, Rgba([0, 0, 0, 255]));
        image::imageops::overlay(&mut framed, &source.to_rgba8(), 0, 50);
        DynamicImage::ImageRgba8(framed)
    };
    let framed = if std::env::args().nth(2).as_deref() == Some("crop") { framed.crop_imm(0, 50, 1600, 900) } else { framed };
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader.clone());
    for t in [0u64, 5000] {
        let s = auto.scan(&framed, t);
        println!("t={t} clock={:?} souls={:?}", s.game_clock_secs, s.souls);
        for r in auto.regions() { if ["game_context", "minimap", "souls", "clock"].contains(&r.id.as_str()) { println!("   {} {:?} {:?}", r.id, r.bounds, r.label); } }
    }
    let exact = framed.crop_imm(0, 50, 1600, 900);
    println!("detector on exact canonical CLOCK: {:?}", reader.read(&coach_vision::hud::crop(&exact, coach_vision::hud::CLOCK)));
}
