//! Per-stage timing of the full-scene scan on saved frames. Usage:
//! cargo run --release -p coach-vision --example profile_scan -- samples/frames/00059_1792966931596.jpg
use coach_vision::{hud, minimap, ocr::Reader, screen, ColorLibrary};
use std::time::Instant;

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = f();
    println!("{label:<34} {:>8.1} ms", start.elapsed().as_secs_f64() * 1000.0);
    value
}

fn main() {
    let root = std::path::Path::new(".");
    let reader = Reader::load(&root.join("data/models")).unwrap();
    let items = ColorLibrary::load_centered(&root.join("data/icons/shop")).unwrap();
    let heroes = ColorLibrary::load(&root.join("data/icons/h_card"), true).unwrap();
    let critical = ColorLibrary::load(&root.join("data/icons/h_crit"), true).unwrap();
    for path in std::env::args().skip(1) {
        let frame = image::open(&path).unwrap().resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        println!("== {path}");
        let total = Instant::now();
        time("classify (detector OCR)", || screen::classify(&reader, &frame, &[]));
        let _ = time("clock read (detector)", || reader.read(&hud::crop(&frame, hud::CLOCK)));
        let _ = time("clock read (line only)", || reader.read_line(&frame.crop_imm(775, 12, 50, 16)));
        let _ = time("match id (detector)", || reader.read(&hud::crop(&frame, screen::match_id_region())));
        let _ = time("souls (detector)", || reader.read(&hud::crop(&frame, hud::SOULS)));
        time("minimap analyze", || minimap::Analyzer::new().analyze(&minimap::crop_from_frame(&frame)));
        time("vitals (2x detector)", || coach_vision::vitals::Tracker::default().read(&reader, &frame, 0));
        time("items 12 slots", || {
            for row in 0..2 { for col in 0..6 {
                let c = hud::crop(&frame, hud::item_slot(col, row));
                if hud::contrast(&c) >= 18.0 { items.identify(&c); }
            }}
        });
        time("hero portraits 12x2 libs", || {
            for cx in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()) {
                let c = hud::crop(&frame, hud::portrait(*cx));
                heroes.scores(&c); critical.scores(&c);
            }
        });
        time("team totals (2 line)", || {
            reader.read_line(&hud::crop(&frame, hud::ALLY_TOTAL)).ok();
            reader.read_line(&hud::crop(&frame, hud::ENEMY_TOTAL)).ok()
        });
        time("team health 12 line", || coach_vision::team::health(&reader, &frame, &[]));
        time("team networth stacked", || {
            let mut slots = vec![coach_core::HeroStatus::default(); 12];
            coach_vision::team::networth(&reader, &frame, 300, &mut slots)
        });
        let raw = image::open(&path).unwrap();
        let map = coach_vision::localize::MapLocator::default();
        time("auto: map locate (full frame)", || map.locate(&raw));
        time("auto: currency locate", || coach_vision::localize::locate_currency(&raw, &reader));
        time("auto: clock locate", || coach_vision::localize::locate_clock(&raw, &reader, None));
        println!("{:<34} {:>8.1} ms", "TOTAL", total.elapsed().as_secs_f64() * 1000.0);
        // Steady-state candidate: every tracked text field in one recognition batch.
        let mut crops = vec![frame.crop_imm(775, 12, 50, 16), frame.crop_imm(401, 607, 55, 33),
            hud::crop(&frame, hud::ALLY_TOTAL), hud::crop(&frame, hud::ENEMY_TOTAL),
            hud::crop(&frame, screen::back_anchor()), hud::crop(&frame, screen::respawn_text_line()),
            hud::crop(&frame, screen::match_id_region())];
        for cx in hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS) {
            crops.push(frame.crop_imm((cx - 32.) as u32, 63, 64, 18));
            crops.push(frame.crop_imm((cx - 18.) as u32, 82, 36, 14));
        }
        let single = time(&format!("{} lines one-by-one", crops.len()), || crops.iter().map(|c| reader.read_line(c).unwrap_or_default()).collect::<Vec<_>>());
        let batch = time(&format!("{} lines batched", crops.len()), || reader.read_lines(&crops).unwrap());
        let same = single.iter().zip(&batch).filter(|(a, b)| a.trim() == b.trim()).count();
        println!("batch agrees on {same}/{} lines; first: {:?}", crops.len(), &batch[..4]);
        // End-to-end through the real scanners: frame 1 acquires, later frames track.
        let mut scanner = coach_vision::scan::Scanner::load(root).unwrap();
        for (i, t) in [0u64, 1000, 2000, 3000, 4000, 5000].iter().enumerate() {
            let start = Instant::now(); let s = scanner.scan_image(&frame, None, *t);
            println!("calibrated scan frame {i}: {:>7.1} ms  clock={:?} souls={:?} hp={:?}/{:?}", start.elapsed().as_secs_f64() * 1000.0, s.game_clock_secs, s.souls, s.hp, s.max_hp);
        }
        let scaled = raw.resize_exact(2000, 1125, image::imageops::FilterType::Triangle);
        let mut window = image::RgbaImage::from_pixel(2500, 1500, image::Rgba([25, 25, 25, 255]));
        image::imageops::overlay(&mut window, &scaled.to_rgba8(), 220, 120);
        let window = image::DynamicImage::ImageRgba8(window);
        time("window to_luma8 (2500x1500)", || window.to_luma8());
        let inner = window.crop_imm(220, 120, 2000, 1125);
        time("resize 2000x1125->1600x900 triangle", || inner.resize_exact(1600, 900, image::imageops::FilterType::Triangle));
        time("resize 2000x1125->1600x900 thumbnail", || inner.thumbnail_exact(1600, 900));
        let located = map.locate(&window).unwrap();
        time("map track (verify box)", || map.track(&window, located.bounds));
        let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader.clone());
        for (i, t) in [0u64, 1000, 2000, 3000, 4000, 5000].iter().enumerate() {
            let start = Instant::now(); let s = auto.scan(&window, *t);
            println!("auto 2500x1500 window frame {i}: {:>7.1} ms  clock={:?} souls={:?} hp={:?}", start.elapsed().as_secs_f64() * 1000.0, s.game_clock_secs, s.souls, s.hp);
        }
        println!("last auto phases: {}", auto.laps.entries.iter().map(|(k, v)| format!("{k}={v:.0}")).collect::<Vec<_>>().join("  "));
    }
}
