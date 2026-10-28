//! Read the canonical clock boxes of a frame through a slightly mis-scaled game context (a
//! minimap box located a few pixels too small shifts and scales the whole 1600x900 context).
//! Usage: clock_shift_probe IMAGE [x y w h ...] (default: the rec8 live mislocation 30 15 1886 1062)
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let frame = image::open(&a[0]).unwrap();
    let rects: Vec<u32> = a[1..].iter().filter_map(|s| s.parse().ok()).collect();
    let rects = if rects.len() >= 4 { rects } else { vec![5, 2, 1915, 1077, 30, 15, 1886, 1062] };
    for r in rects.chunks(4) {
        let canon = frame.crop_imm(r[0], r[1], r[2], r[3]).resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let lines: Vec<_> = coach_vision::screen::clock_lines().into_iter().map(|l| coach_vision::hud::crop(&canon, l)).collect();
        let read = reader.read_lines(&lines).unwrap_or_default();
        let fixed = reader.read(&coach_vision::hud::crop(&canon, coach_vision::hud::CLOCK)).ok();
        let area = coach_perception::Bounds { x: 800, y: ((coach_vision::hud::CLOCK.y + coach_vision::hud::CLOCK.h / 3.0) * 900.0) as u32, w: (coach_vision::hud::CLOCK.w * 1600.0) as u32, h: (coach_vision::hud::CLOCK.h * 900.0) as u32 };
        let glyph = coach_vision::localize::locate_clock_limited(&canon, &reader, Some(area), 1).map(|(v, s)| (v, s.bounds));
        println!("{r:?} lines={read:?} vote={:?} fixed={fixed:?} glyph={glyph:?}", coach_vision::screen::vote_clock(&read, None));
    }
}
