use coach_vision::{hud, ocr::Reader, parse, scan::Scanner};
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let rd = Reader::load(std::path::Path::new("data/models")).unwrap();
    let s = Scanner::load(std::path::Path::new(".")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    files.sort();
    for p in &files {
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(p).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        let sc = s.classify(&bytes);
        let clock = rd.read(&hud::crop(&img, hud::CLOCK)).unwrap_or_default();
        let souls = rd.read(&hud::crop(&img, hud::SOULS)).unwrap_or_default();
        println!("{name} | {:?} | clock {:?}->{:?} | souls {:?}->{:?}", sc, clock.trim(), parse::parse_clock(&clock), souls.trim().replace(char::from(10), "/"), parse::parse_souls(&souls));
    }
}
