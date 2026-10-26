use coach_vision::{hud, ColorLibrary};
fn main() {
    let img = image::open(std::env::args().nth(1).unwrap()).unwrap();
    let lib = ColorLibrary::load_centered(std::path::Path::new("data/icons/shop")).unwrap();
    for row in 0..2 { for col in 0..4 {
        let c = hud::crop(&img, hud::item_slot(col, row));
        let t = lib.top(&c, 3);
        println!("slot {col},{row} contrast {:.0}: {}", hud::contrast(&c), t.iter().map(|(s, n)| format!("{n}={s:.2}")).collect::<Vec<_>>().join("  "));
    }}
}
