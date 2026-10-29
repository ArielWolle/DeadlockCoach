//! Replay a directory of recorded screenshots (`NNN_<timestamp_ms>.jpg`) through the automatic
//! scanner and the coaching engine, writing one JSON line per frame: snapshot, spoken cues and
//! scan time. Usage: replay_dir DIR OUT.jsonl
use std::io::Write;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader);
    let mut rules = coach_engine::default_rules(coach_engine::RulesConfig::default());
    rules.extend(coach_engine::rules_from_timings(&std::fs::read_to_string(root.join("data/timings.json")).unwrap()).unwrap());
    let mut engine = coach_engine::Engine::new(rules);
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let mut out = std::fs::File::create(&a[1]).unwrap();
    let t0: u64 = files.first().and_then(|p| p.file_stem()?.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0);
    for p in &files {
        let ts: u64 = p.file_stem().and_then(|s| s.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(t0);
        let img = image::open(p).unwrap();
        let start = std::time::Instant::now();
        let snap = auto.scan(&img, ts - t0);
        let ms = start.elapsed().as_millis();
        let souls_box = auto.regions().into_iter().find(|r| r.id == "souls").map(|r| (r.bounds.x, r.bounds.y, r.bounds.w, r.bounds.h));
        let cues = engine.step(&snap);
        let advice = coach_engine::positioning::PositionCoach::default().assess(&snap).map(|a| a.id);
        let lane = coach_engine::laning::LaneCoach::default().decide(&snap).map(|d| (format!("{:?}", d.action), d.speak));
        let row = serde_json::json!({"file": p.file_name().unwrap().to_string_lossy(), "t": ts - t0, "ms": ms, "snapshot": snap,
            "cues": cues, "advice": advice, "lane": lane, "souls_box": souls_box, "match_resets": auto.scanner().match_resets()});
        writeln!(out, "{row}").unwrap();
        eprint!(".");
    }
    eprintln!();
}
