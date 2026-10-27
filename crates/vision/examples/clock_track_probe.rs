//! Compare tracked clock/currency crop variants built from located boxes (also in a resized window).
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let path = std::env::args().nth(1).unwrap();
    let source = image::open(&path).unwrap();
    let scaled = source.resize_exact(2000, 1125, image::imageops::FilterType::Triangle);
    let mut window = image::RgbaImage::from_pixel(2500, 1500, image::Rgba([25, 25, 25, 255]));
    image::imageops::overlay(&mut window, &scaled.to_rgba8(), 220, 120);
    for (name, frame) in [("native", source), ("window", image::DynamicImage::ImageRgba8(window))] {
        println!("== {name}");
        if let Some((v, r)) = coach_vision::localize::locate_clock(&frame, &reader, None) {
            let b = r.bounds; println!("  clock {v} {b:?}");
            let (cx, cy, h) = (b.x as f32 + b.w as f32 / 2., b.y as f32 + b.h as f32 / 2., b.h as f32);
            for (dx, dy, w, hh) in [(-2.45f32, -0.8f32, 5.0f32, 1.6f32), (-2.95, -0.4, 6.0, 2.0), (-3.45, -0.6, 7.0, 2.2), (-2.95, -0.8, 6.0, 1.8)] {
                let c = frame.crop_imm((cx + dx * h) as u32, (cy + dy * h) as u32, (w * h) as u32, (hh * h) as u32);
                println!("    {}x{} -> {:?}", c.width(), c.height(), reader.read_lines(&[c]));
            }
        }
        if let Some((v, r)) = coach_vision::localize::locate_currency(&frame, &reader) {
            let b = r.bounds; println!("  currency {v} {b:?} label {:?}", r.label);
            for (l, rt, vv) in [(0.0f32, 0.0f32, 0.0f32), (0.05, 0.45, 0.1), (0.05, 0.2, 0.1), (0.1, 0.3, 0.3)] {
                let (pl, pr, pv) = ((b.w as f32 * l) as u32, (b.w as f32 * rt) as u32, (b.h as f32 * vv) as u32);
                let c = frame.crop_imm(b.x - pl, b.y - pv, b.w + pl + pr, b.h + 2 * pv);
                println!("    pad {l}/{rt}/{vv} {}x{} -> {:?}", c.width(), c.height(), reader.read_lines(&[c]));
            }
        }
    }
}
