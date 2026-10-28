//! Top hero template scores per top-bar slot. Usage: portrait_probe SLOT FRAME... (SLOT 0-11, or -1 for all)
use coach_vision::{hud, ColorLibrary};
use std::collections::HashMap;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::Path::new(".");
    let card = ColorLibrary::load(&root.join("data/icons/h_card"), true).unwrap();
    let crit = ColorLibrary::load(&root.join("data/icons/h_crit"), true).unwrap();
    let only: i32 = a[0].parse().unwrap();
    for f in &a[1..] {
        let img = image::open(f).unwrap();
        for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
            if only >= 0 && only as usize != i { continue; }
            let c = hud::crop(&img, hud::portrait(*cx));
            let mut best: HashMap<String, f32> = HashMap::new();
            for (n, s) in card.scores(&c).into_iter().chain(crit.scores(&c)) {
                let n = coach_vision::scan::hero_key(&n).to_string();
                let e = best.entry(n).or_insert(f32::MIN); *e = e.max(s);
            }
            let mut v: Vec<_> = best.into_iter().collect();
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            let top: Vec<String> = v.iter().take(4).map(|(n, s)| format!("{}={:.3}", n.trim_start_matches("hero_"), s)).collect();
            println!("{} slot {i}: {}", std::path::Path::new(f).file_name().unwrap().to_string_lossy(), top.join(" "));
        }
    }
}
