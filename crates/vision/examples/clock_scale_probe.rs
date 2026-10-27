//! How does upscale factor affect clock recognition on sharp vs. video-blurred frames?
use image::{imageops::FilterType, DynamicImage};
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let boxes = coach_vision::screen::clock_lines();
    for path in std::env::args().skip(1) {
        let sharp = image::open(&path).unwrap().resize_exact(1600, 900, FilterType::Triangle);
        // Approximate a video-encoded share: slight blur plus downscale/upscale round trip.
        let blurred = DynamicImage::ImageRgba8(image::imageops::blur(&sharp.to_rgba8(), 0.7));
        let soft = sharp.resize_exact(1280, 720, FilterType::Triangle).resize_exact(1600, 900, FilterType::Triangle);
        for (kind, frame) in [("sharp", &sharp), ("blur0.7", &blurred), ("720p", &soft)] {
            for k in [1u32, 2, 3, 4] {
                let crops: Vec<_> = boxes.iter().map(|r| { let c = coach_vision::hud::crop(frame, *r); c.resize_exact(c.width() * k, c.height() * k, FilterType::CatmullRom) }).collect();
                let lines = reader.read_lines(&crops).unwrap();
                println!("{path} {kind:<8} x{k}: {:?} -> {:?}", lines, coach_vision::screen::vote_clock(&lines, None));
            }
        }
    }
}
