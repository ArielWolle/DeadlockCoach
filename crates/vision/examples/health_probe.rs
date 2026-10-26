//! Inspect the calibrated current/max-health fields on saved captures.
use coach_vision::{hud, ocr::Reader, Region};
fn main() {
    let reader = Reader::load(std::path::Path::new("data/models")).unwrap();
    for path in std::env::args().skip(1) {
        let image = image::open(&path).unwrap(); println!("{path}");
        for (name,r) in [
            ("current",Region { x:350.0/1600.0,y:575.0/900.0,w:145.0/1600.0,h:100.0/900.0 }),
            ("current-tight",Region { x:365.0/1600.0,y:600.0/900.0,w:115.0/1600.0,h:65.0/900.0 }),
            ("max",Region { x:145.0/1600.0,y:675.0/900.0,w:110.0/1600.0,h:45.0/900.0 }),
        ] { println!("{name}: {:?}; words={:?}",reader.read(&hud::crop(&image,r)),reader.read_words(&hud::crop(&image,r))); }
        let map=coach_vision::minimap::Analyzer::new().analyze(&coach_vision::minimap::crop_from_frame(&image));
        println!("own={:?} lane={:?} progress={:?}",map.you,map.your_lane,map.your_progress);
    }
}
