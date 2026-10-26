//! Brain CLI.
//!   coach-brain <snapshots.jsonl> [--speak addr]   replay snapshots through the engine
//!   coach-brain --live <agent_addr>                 live: frames -> OCR -> engine -> voice
//! COACH_TOKEN for network use. Run from the repo root (models in data/models).
use coach_core::GameSnapshot;
use coach_vision::scan::Scanner;

fn load_engine() -> coach_engine::Engine {
    let timings = std::env::var("COACH_TIMINGS").map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_else(|_| include_str!("../../../data/timings.json").to_string());
    let mut rules = coach_engine::rules_from_timings(&timings).expect("timings");
    let cfg = std::fs::read_to_string("data/rules.json").ok().and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()).unwrap_or_default();
    rules.extend(coach_engine::default_rules(cfg));
    let counters = std::fs::read_to_string("data/counters.json").unwrap_or_else(|_| "[]".into()).trim_start_matches('\u{feff}').to_string();
    rules.push(coach_engine::counter_rule_from_json(&counters).expect("counters.json"));
    coach_engine::Engine::new(rules)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let token = std::env::var("COACH_TOKEN").unwrap_or_default();
    let mut engine = load_engine();
    let focus=std::env::var("COACH_LANE").ok().and_then(|s|match s.to_lowercase().as_str() {
        "green"=>Some(coach_core::Lane::Green),"blue"=>Some(coach_core::Lane::Blue),"yellow"=>Some(coach_core::Lane::Yellow),_=>None
    });
    let piper = coach_audio::Piper::from_env();

    if let Some(i) = args.iter().position(|a| a == "--live") {
        let addr = args.get(i + 1).expect("--live <addr>").clone();
        let mut scanner = Scanner::load(std::path::Path::new(".")).expect("assets (run from repo root)");
        std::fs::create_dir_all("sessions").ok();
        let sid = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let log_path = format!("sessions/{sid}.jsonl");
        // Record EVERY frame for later calibration. Write a label into tag.txt (e.g. "scoreboard")
        // and it is put in the filenames of the following frames. Disable with COACH_NO_RECORD=1.
        let frame_dir = format!("sessions/{sid}_frames");
        let record = std::env::var("COACH_NO_RECORD").is_err();
        if record { std::fs::create_dir_all(&frame_dir).ok(); }
        let mut n = 0u32;
        let piper = piper.expect("piper");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        loop {
            let res = coach_transport::connect_with_minimap(&addr, &token, &mut rx, |meta, jpeg, map_png| {
                if record {
                    let tag = std::fs::read_to_string("tag.txt").unwrap_or_default();
                    let tag: String = tag.trim().chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
                    let _ = std::fs::write(format!("{frame_dir}/{n:05}_{tag}.jpg"), &jpeg);
                    if !map_png.is_empty() {
                        let _ = std::fs::write(format!("{frame_dir}/{n:05}_{tag}.minimap.png"), &map_png);
                    }
                    n += 1;
                }
                let mut snap = scanner.scan_with_minimap(&jpeg, &map_png, meta.timestamp_ms);
                snap.coaching_context.focus_lane=focus;
                println!("snapshot: {:?} clock={:?} souls={:?} items={} allies={} enemies={} nw={:?}/{:?} dead={}", snap.screen, snap.game_clock_secs, snap.souls, snap.items.len(), snap.ally_heroes.len(), snap.enemy_heroes.len(), snap.ally_networth, snap.enemy_networth, snap.dead);
                if let Ok(line) = serde_json::to_string(&snap) {
                    use std::io::Write;
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) { let _ = writeln!(f, "{line}"); }
                }
                for cue in engine.step(&snap) {
                    println!("CUE: {}", cue.text);
                    if let Ok(wav) = piper.synth(&cue.text) { let _ = tx.send(wav); }
                }
            }).await;
            eprintln!("link ended: {res:?}; retrying in 3s");
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }

    let path = args.first().expect("usage: coach-brain <snapshots.jsonl> [--speak addr] | --live addr");
    let speak = args.iter().position(|a| a == "--speak").and_then(|i| args.get(i + 1)).cloned();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    for line in std::fs::read_to_string(path).expect("read file").lines().filter(|l| !l.trim().is_empty()) {
        let mut snap: GameSnapshot = serde_json::from_str(line).expect("bad snapshot line");
        snap.coaching_context.focus_lane=focus.or(snap.coaching_context.focus_lane);
        for cue in engine.step(&snap) {
            println!("[{:>5}s] {:?} {}", snap.game_clock_secs.unwrap_or(0), cue.priority, cue.text);
            if speak.is_some() {
                if let Ok(wav) = piper.as_ref().expect("piper").synth(&cue.text) { let _ = tx.send(wav); }
            }
        }
    }
    if let Some(addr) = speak {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(8),
            coach_transport::connect(&addr, &token, &mut rx, |_, _| {})).await;
    }
}
