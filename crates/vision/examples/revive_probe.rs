//! Measure the Rejuvenator revive cue (grey world under the respawn banner) on every Dead frame
//! of a replay. Usage: revive_probe DIR REPLAY.jsonl
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let text = std::fs::read_to_string(&a[1]).unwrap();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let j: serde_json::Value = serde_json::from_str(line).unwrap();
        let s = &j["snapshot"];
        if s["screen"].as_str() != Some("Dead") { continue; }
        let file = j["file"].as_str().unwrap();
        let img = image::open(std::path::Path::new(&a[0]).join(file)).unwrap();
        let canon = img.resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let share = coach_vision::death::world_colour_share(&canon);
        let clock = s["game_clock_secs"].as_u64().map(|c| format!("{}:{:02}", c / 60, c % 60)).unwrap_or("--".into());
        println!("{file} {clock} share={share:.3} resp={} panel={}", s["respawn_secs"], s["death_damage"].as_array().map_or(0, |v| v.len()));
    }
}
