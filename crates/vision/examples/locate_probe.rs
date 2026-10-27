//! Map and currency localization per frame (why a frame forms no game context).
//! Usage: locate_probe FRAME...
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let loc = coach_vision::localize::MapLocator::default();
        let map = loc.locate(&img);
        let held = loc.hold(&img, coach_perception::Bounds { x: 1508, y: 629, w: 402, h: 420 }, 0.0).map(|l| l.score);
        print!("hold-score={held:?} ");
        let cur = coach_vision::localize::locate_currency_limited(&img, &reader, 4);
        println!("{} map={:?} currency={:?}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy(),
            map.map(|m| (m.bounds.x, m.bounds.y, m.bounds.w, m.bounds.h)), cur.map(|(v, r)| (v, r.bounds.x, r.bounds.y, r.bounds.w, r.bounds.h)));
    }
}
