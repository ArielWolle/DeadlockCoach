//! Raw screen-probe text (back, respawn, countdown, 4 clock boxes, 4 name boxes) per frame,
//! after scaling to 1600x900. Usage: probe_lines FRAME...
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap().resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let lines = reader.read_lines(&coach_vision::screen::probe_crops(&img, None)).unwrap();
        println!("{} bar={:?} {:?}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy(), coach_vision::vitals::bar_ratio(&img), lines);
    }
}
