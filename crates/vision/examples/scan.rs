use coach_vision::{hud, parse, scan::Scanner, ocr::Reader};
fn main() {
    let rd = Reader::load(std::path::Path::new("data/models")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("0")).collect();
    files.sort();
    let mut s = Scanner::load(std::path::Path::new(".")).unwrap();
    for (i, p) in files.iter().enumerate() {
        let img = image::open(p).unwrap();
        let raw_clock = rd.read(&hud::crop(&img, hud::CLOCK)).unwrap_or_default();
        let raw_souls = rd.read(&hud::crop(&img, hud::SOULS)).unwrap_or_default();
        let raw_a = rd.read(&hud::crop(&img, hud::ALLY_TOTAL)).unwrap_or_default();
        let raw_e = rd.read(&hud::crop(&img, hud::ENEMY_TOTAL)).unwrap_or_default();
        let snap = s.scan(&std::fs::read(p).unwrap());
        println!("{i:02} clock {:?}->{:?} souls {:?}->{:?} A {:?}->{:?} E {:?}->{:?} items={} heroes={}/{}", raw_clock.trim(), parse::parse_clock(&raw_clock), raw_souls.trim(), snap.souls, raw_a.trim(), snap.ally_networth, raw_e.trim(), snap.enemy_networth, snap.items.len(), snap.ally_heroes.len(), snap.enemy_heroes.len());
    }
}
