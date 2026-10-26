use coach_vision::{minimap::*, scan::Scanner};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = &a[0];
    let clocks: Vec<Option<u32>> = std::fs::read_to_string(&a[1]).unwrap().lines().map(|l| serde_json::from_str::<serde_json::Value>(l).ok().and_then(|v| v["game_clock_secs"].as_u64()).map(|x| x as u32)).collect();
    let s = Scanner::load(std::path::Path::new(".")).unwrap();
    let an = Analyzer::new();
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    files.sort();
    let f = |o: Option<f32>| o.map(|v| format!("{:.2}", v)).unwrap_or("  - ".into());
    for (i, p) in files.iter().enumerate() {
        let bytes = std::fs::read(p).unwrap();
        if !s.classify(&bytes).has_hud() { continue; }
        let img = image::load_from_memory(&bytes).unwrap();
        let m = an.analyze(&crop_from_frame(&img));
        let lane = |l: &coach_core::LaneState| format!("push {} a{}@{} e{}@{}", f(l.push), l.ally_troopers, f(l.ally_progress), l.enemy_troopers, f(l.enemy_progress));
        println!("{i:03} c={:<5} G[{}] B[{}] Y[{}] en={} mb={} urn={:?}{}", clocks.get(i).copied().flatten().map(|c| c.to_string()).unwrap_or("-".into()), lane(&m.green), lane(&m.blue), lane(&m.yellow), m.enemy_count, m.midboss_up as u8, m.urn, m.urn_pos.map(|p| format!("@({:.2},{:.2})", p.x, p.y)).unwrap_or_default());
    }
}
