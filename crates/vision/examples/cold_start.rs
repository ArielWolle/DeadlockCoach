//! Scan each given frame alone six times (2 s apart) from a fresh scanner and print what it
//! identifies: picks fixtures whose single still frame names your hero and the slots.
//! Usage: cold_start FILE...
fn main() {
    for p in std::env::args().skip(1) {
        let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
        let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(std::path::Path::new(".")).unwrap(), reader);
        let img = image::open(&p).unwrap();
        let mut s = Default::default();
        for i in 0..6u64 { s = auto.scan(&img, i * 2000); }
        let s: coach_core::GameSnapshot = s;
        println!("{p}: {:?} {:?} hero={:?} A={:?} E={:?}", s.screen, s.game_clock_secs, s.hero, s.ally_heroes, s.enemy_heroes);
    }
}
