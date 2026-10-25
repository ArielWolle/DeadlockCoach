//! Brain CLI.
//!   coach-brain <snapshots.jsonl> [--speak addr]   replay snapshots through the engine
//!   coach-brain --live <agent_addr>                 live: frames -> OCR -> engine -> voice
//! COACH_TOKEN for network use. Run from the repo root (models in data/models).
use coach_core::GameSnapshot;
use coach_vision::{ocr::Reader, parse, Region};

fn load_engine() -> coach_engine::Engine {
    let timings = std::env::var("COACH_TIMINGS").map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_else(|_| include_str!("../../../data/timings.json").to_string());
    coach_engine::Engine::new(coach_engine::rules_from_timings(&timings).expect("timings"))
}

fn read_snapshot(rd: &Reader, tracker: &mut parse::Tracker, jpeg: &[u8]) -> GameSnapshot {
    let Ok(img) = image::load_from_memory(jpeg) else { return GameSnapshot::default() };
    let crop = |r: Region| {
        let (w, h) = (img.width() as f32, img.height() as f32);
        img.crop_imm((r.x * w) as u32, (r.y * h) as u32, (r.w * w) as u32, (r.h * h) as u32)
    };
    let clock = rd.read(&crop(Region { x: 0.47, y: 0.01, w: 0.06, h: 0.035 })).ok().and_then(|s| parse::parse_clock(&s));
    let souls = rd.read(&crop(Region { x: 0.04, y: 0.80, w: 0.09, h: 0.06 })).ok().and_then(|s| parse::parse_souls(&s));
    tracker.accept(clock, souls)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let token = std::env::var("COACH_TOKEN").unwrap_or_default();
    let mut engine = load_engine();
    let piper = coach_audio::Piper::from_env();

    if let Some(i) = args.iter().position(|a| a == "--live") {
        let addr = args.get(i + 1).expect("--live <addr>").clone();
        let rd = Reader::load(std::path::Path::new("data/models")).expect("models (run from repo root)");
        let piper = piper.expect("piper");
        let mut tracker = parse::Tracker::default();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        loop {
            let res = coach_transport::connect(&addr, &token, &mut rx, |_meta, jpeg| {
                let snap = read_snapshot(&rd, &mut tracker, &jpeg);
                println!("snapshot: clock={:?} souls={:?}", snap.game_clock_secs, snap.souls);
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
        let snap: GameSnapshot = serde_json::from_str(line).expect("bad snapshot line");
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
