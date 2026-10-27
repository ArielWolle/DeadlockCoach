//! Diagnose souls acquisition on a frame: proposals, OCR spans, and the located value.
//! Usage: souls_probe FRAME
fn main() {
    let f = std::env::args().nth(1).unwrap();
    let img = image::open(&f).unwrap();
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let located = coach_vision::localize::locate_currency_limited(&img, &reader, 6);
    println!("located: {:?}", located.as_ref().map(|(v, r)| (v, r.bounds, &r.label)));
    if let Some((v, r)) = &located { println!("glyphs {:?} corrected {}", coach_vision::localize::souls_digit_glyphs(&img, r.bounds), coach_vision::localize::correct_souls(&img, r.bounds, *v, &r.label)); }
    let n: Vec<u32> = std::env::args().skip(2).filter_map(|s| s.parse().ok()).collect();
    if n.len() == 4 { let b = coach_perception::Bounds { x: n[0], y: n[1], w: n[2], h: n[3] };
        println!("glyphs in {b:?}: {:?}", coach_vision::localize::souls_digit_glyphs(&img, b)); }
    let (w, h) = (img.width() as f32, img.height() as f32);
    let crop = img.crop_imm((0.03 * w) as u32, (0.79 * h) as u32, (0.14 * w) as u32, (0.06 * h) as u32);
    for s in reader.read_spans(&crop, 3).unwrap_or_default() { println!("span {:?} {:?}", s.label, s.bounds); }
}
