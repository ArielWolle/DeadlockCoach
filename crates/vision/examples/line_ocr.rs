//! Compare direct line recognition against the detector-backed OCR on calibrated regions.
use coach_vision::{hud, ocr::Reader, scoreboard};
fn main() {
    let rd = Reader::load(std::path::Path::new("data/models")).unwrap();
    for p in std::env::args().skip(1) {
        let img = image::open(&p).unwrap(); println!("{p}");
        for (name, r) in [("clock", hud::CLOCK), ("souls", hud::SOULS),
            ("ally", hud::ALLY_TOTAL), ("enemy", hud::ENEMY_TOTAL),
            ("scoreboard-name", scoreboard::name_region(hud::ALLY_CENTERS[1]))] {
            let crop = hud::crop(&img, r);
            println!("{name}: line={:?}, detector={:?}", rd.read_line(&crop), rd.read(&crop));
        }
    }
}
