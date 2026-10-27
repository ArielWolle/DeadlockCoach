//! Inspect tracked HP crops and scoreboard name crop variants under batched recognition.
use coach_vision::{hud, ocr::Reader, scoreboard};
fn main() {
    let reader = Reader::load(std::path::Path::new("data/models")).unwrap();
    for path in std::env::args().skip(1) {
        let frame = image::open(&path).unwrap().resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        println!("== {path}");
        let mut vitals = coach_vision::vitals::Tracker::default();
        if let Some(r) = vitals.read(&reader, &frame, 0) {
            println!("  acquired hp={} max={} boxes={:?}", r.hp, r.max, r.regions.iter().map(|g| g.bounds).collect::<Vec<_>>());
            let crops = vitals.tracked_crops(&frame).unwrap();
            println!("  tracked crops {:?} -> {:?}", crops.iter().map(|c| (c.width(), c.height())).collect::<Vec<_>>(), reader.read_lines(&crops));
            let tight: Vec<_> = r.regions.iter().map(|g| frame.crop_imm(g.bounds.x, g.bounds.y, g.bounds.w, g.bounds.h)).collect();
            println!("  tight crops -> {:?}", reader.read_lines(&tight));
            let up: Vec<_> = tight.iter().map(|c| c.resize_exact(c.width() * 3, c.height() * 3, image::imageops::FilterType::CatmullRom)).collect();
            println!("  tight x3 -> {:?}", reader.read_lines(&up));
            for frac in [0.2f32, 0.35, 0.5] {
                let wide: Vec<_> = r.regions.iter().map(|g| { let b = g.bounds; let p = (b.h as f32 * frac) as u32;
                    frame.crop_imm(b.x - p, b.y, b.w + 2 * p, b.h) }).collect();
                println!("  hpad {frac} -> {:?}", reader.read_lines(&wide));
            }
        }
        let cx = hud::ALLY_CENTERS[1];
        for (name, x, y, w, h) in [("name", cx - 36., 120., 72., 14.), ("tight9", cx - 34., 120., 68., 9.), ("tight8", cx - 34., 121., 68., 8.), ("tight10", cx - 34., 119., 68., 10.)] {
            let c = frame.crop_imm(x as u32, y as u32, w as u32, h as u32);
            let up = |k: u32| c.resize_exact(c.width() * k, c.height() * k, image::imageops::FilterType::CatmullRom);
            println!("  {name:<7} line={:?} x2={:?} x3={:?} detect={:?}", reader.read_lines(&[c.clone()]), reader.read_lines(&[up(2)]), reader.read_lines(&[up(3)]), reader.read(&c));
        }
        // Detect once to get the exact text rectangle, then recognize only that rectangle.
        let c = hud::crop(&frame, scoreboard::name_region(cx));
        if let Ok(spans) = reader.read_spans(&c, 3) {
            for s in spans { let b = s.bounds; let sub = c.crop_imm(b.x, b.y, b.w, b.h);
                println!("  span {:?} {:?} -> tight line {:?} x3 {:?}", s.label, b, reader.read_lines(&[sub.clone()]),
                    reader.read_lines(&[sub.resize_exact(sub.width() * 3, sub.height() * 3, image::imageops::FilterType::CatmullRom)])); }
        }
    }
}
