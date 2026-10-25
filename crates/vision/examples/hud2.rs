// Usage: hud2 <frame.jpg> -> heroes, net worths, items with scores.
use coach_vision::{hud, ocr::Reader, Library};
use std::path::Path;
fn main() {
    let img = image::open(std::env::args().nth(1).unwrap()).unwrap();
    let heroes = coach_vision::ColorLibrary::load(Path::new("data/icons/topbar"), true).unwrap();
    let items = Library::load(Path::new("data/icons/items")).unwrap();
    let rd = Reader::load(Path::new("data/models")).unwrap();
    for (side, cs) in [("ally", hud::ALLY_CENTERS), ("enemy", hud::ENEMY_CENTERS)] {
        for cx in cs {
            let c = hud::crop(&img, hud::portrait(cx));
            let h = heroes.identify(&c);
            let nw = rd.read(&hud::crop(&img, hud::networth_label(cx))).ok();
            println!("{side} {cx}: hero={h:?} nw={nw:?}");
        }
    }
    for row in 0..2 { for col in 0..6 {
        let c = hud::crop(&img, hud::item_slot(col, row));
        println!("slot {col},{row}: contrast={:.0} id={:?}", hud::contrast(&c), items.identify(&c.to_luma8()));
    }}
}
