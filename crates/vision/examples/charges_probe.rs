//! Rejuvenator charge icons on the top bar for every frame of a replay (prints frames with any).
//! Usage: charges_probe DIR REPLAY.jsonl
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let text = std::fs::read_to_string(&a[1]).unwrap();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let j: serde_json::Value = serde_json::from_str(line).unwrap();
        let s = &j["snapshot"];
        let file = j["file"].as_str().unwrap();
        let img = image::open(std::path::Path::new(&a[0]).join(file)).unwrap();
        let canon = img.resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let top = coach_vision::team::top_bar_aligned(&canon, coach_vision::team::top_bar_offset(&canon));
        let c = coach_vision::objectives::rejuvenator_charges(&top);
        let raw = [coach_vision::objectives::CHARGES_ALLY, coach_vision::objectives::CHARGES_ENEMY].map(|b| coach_vision::objectives::charge_icons(&top, b));
        if raw != [0, 0] {
            let clock = s["game_clock_secs"].as_u64().map(|c| format!("{}:{:02}", c / 60, c % 60)).unwrap_or("--".into());
            println!("{file} {clock} {} charges={c:?} raw={raw:?} mb={}", s["screen"], s["midboss_taken_by"]);
        }
    }
}
