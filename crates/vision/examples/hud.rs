// Usage: hud <frame.jpg>  -> OCR the clock and souls regions.
use coach_vision::{ocr::Reader, Region};
fn main() {
    let path = std::env::args().nth(1).expect("frame path");
    let img = image::open(&path).unwrap();
    let rd = Reader::load(std::path::Path::new("data/models")).expect("models");
    let regions = [
        ("clock", Region { x: 0.47, y: 0.01, w: 0.06, h: 0.035 }),
        ("souls", Region { x: 0.04, y: 0.80, w: 0.09, h: 0.06 }),
        ("hp", Region { x: 0.24, y: 0.65, w: 0.08, h: 0.06 }),
    ];
    for (name, r) in regions {
        let (w, h) = (img.width() as f32, img.height() as f32);
        let c = img.crop_imm((r.x * w) as u32, (r.y * h) as u32, (r.w * w) as u32, (r.h * h) as u32);
        c.save(format!("frames/crop_{name}.png")).unwrap();
        println!("{name}: {:?}", rd.read(&c));
    }
}
