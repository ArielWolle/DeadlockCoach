// Consistency test: for each template set, how often does the same slot give the same hero across frames?
use coach_vision::{hud, ColorLibrary};
use std::collections::HashMap;
fn main() {
    let sets = [("topbar", true), ("h_small", false), ("h_card", true), ("h_card", false), ("h_mm", false)];
    let frames: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    let imgs: Vec<_> = frames.iter().skip(25).take(30).filter_map(|p| image::open(p).ok()).collect();
    for (set, top) in sets {
        let lib = ColorLibrary::load(std::path::Path::new(&format!("data/icons/{set}")), top).unwrap();
        let mut agree = 0.0; let mut total = 0.0;
        for cx in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()) {
            let mut votes: HashMap<String, u32> = HashMap::new();
            for im in &imgs {
                if let Some((n, _, _)) = lib.identify(&hud::crop(im, hud::portrait(*cx))) { *votes.entry(n).or_default() += 1; }
            }
            let best = votes.values().max().copied().unwrap_or(0) as f32;
            agree += best; total += imgs.len() as f32;
        }
        println!("{set} top_square={top}: consistency {:.0}%", 100.0 * agree / total);
    }
}
