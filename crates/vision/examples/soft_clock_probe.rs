//! Which clock reading strategy survives a softened (video-like) frame?
use image::{imageops::FilterType, DynamicImage, GrayImage, Luma};
fn sharpen(img: &DynamicImage) -> DynamicImage { DynamicImage::ImageRgba8(image::imageops::unsharpen(&img.to_rgba8(), 1.2, 4)) }
fn threshold(img: &DynamicImage, t: u8) -> DynamicImage {
    let g = img.to_luma8();
    DynamicImage::ImageLuma8(GrayImage::from_fn(g.width(), g.height(), |x, y| Luma([if g.get_pixel(x, y)[0] >= t { 255 } else { 0 }])))
}
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let path = std::env::args().nth(1).unwrap();
    let sharp = image::open(&path).unwrap().resize_exact(1600, 900, FilterType::Triangle);
    let soft = sharp.resize_exact(1280, 720, FilterType::Triangle).resize_exact(1600, 900, FilterType::Triangle);
    for (name, frame) in [("sharp", sharp), ("soft", soft)] {
        let region = coach_vision::hud::crop(&frame, coach_vision::hud::CLOCK);
        println!("== {name}");
        println!("  detector region       {:?}", reader.read(&region));
        println!("  detector region x5    {:?}", reader.read_scaled(&region, 5));
        println!("  detector sharpened    {:?}", reader.read(&sharpen(&region)));
        for t in [120u8, 150, 180] { println!("  detector thresh {t}   {:?}", reader.read(&threshold(&region, t))); }
        println!("  locate_clock          {:?}", coach_vision::localize::locate_clock(&frame, &reader, None).map(|v| v.0));
        let lines: Vec<_> = coach_vision::screen::clock_lines().iter().map(|r| coach_vision::hud::crop(&frame, *r)).collect();
        println!("  lines                 {:?}", reader.read_lines(&lines));
        println!("  lines sharpened       {:?}", reader.read_lines(&lines.iter().map(sharpen).collect::<Vec<_>>()));
        for t in [120u8, 150, 180] { println!("  lines thresh {t}      {:?}", reader.read_lines(&lines.iter().map(|c| threshold(c, t)).collect::<Vec<_>>())); }
    }
}
