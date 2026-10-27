//! Calibrate tight single-line boxes for batched recognition (1600x900 canonical frame).
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let boxes: &[(&str, u32, u32, u32, u32)] = &[
        ("clock a", 775, 12, 50, 16), ("clock b", 768, 8, 64, 28), ("clock c", 770, 16, 60, 20),
        ("clock d", 765, 10, 70, 22), ("clock e", 765, 14, 70, 22), ("clock f", 765, 18, 70, 22), ("clock g", 762, 6, 76, 34),
        ("clock h", 770, 12, 60, 18),
        ("kills a", 770, 85, 60, 16), ("kills b", 740, 84, 120, 18), ("kills c", 780, 86, 40, 14),
        ("souls a", 70, 742, 130, 30), ("souls b", 60, 735, 150, 40), ("souls region", 64, 720, 144, 54),
        ("match", 1440, 868, 160, 16), ("respawn n", 845, 665, 80, 40), ("cause", 1352, 276, 200, 22),
        ("back", 10, 860, 130, 32), ("max hp", 145, 675, 110, 45),
    ];
    for path in std::env::args().skip(1) {
        let frame = image::open(&path).unwrap().resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let crops: Vec<_> = boxes.iter().map(|(_, x, y, w, h)| frame.crop_imm(*x, *y, *w, *h)).collect();
        let lines = reader.read_lines(&crops).unwrap();
        println!("== {path}");
        for ((name, ..), text) in boxes.iter().zip(lines) { println!("  {name:<14} {text:?}"); }
    }
}
