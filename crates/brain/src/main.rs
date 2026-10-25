//! Brain CLI: replay GameSnapshot JSON lines through the coach engine.
//! Usage: coach-brain <snapshots.jsonl> [--speak <agent_addr>]   (COACH_TOKEN for --speak)
use coach_core::GameSnapshot;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().expect("usage: coach-brain <snapshots.jsonl> [--speak addr]");
    let speak = args.iter().position(|a| a == "--speak").and_then(|i| args.get(i + 1)).cloned();

    let timings = std::env::var("COACH_TIMINGS").map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_else(|_| include_str!("../../../data/timings.json").to_string());
    let mut engine = coach_engine::Engine::new(coach_engine::rules_from_timings(&timings).expect("timings"));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let piper = if speak.is_some() { Some(coach_audio::Piper::from_env().expect("piper")) } else { None };

    for line in std::fs::read_to_string(path).expect("read file").lines().filter(|l| !l.trim().is_empty()) {
        let snap: GameSnapshot = serde_json::from_str(line).expect("bad snapshot line");
        for cue in engine.step(&snap) {
            println!("[{:>5}s] {:?} {}", snap.game_clock_secs.unwrap_or(0), cue.priority, cue.text);
            if let Some(p) = &piper {
                if let Ok(wav) = p.synth(&cue.text) { let _ = tx.send(wav); }
            }
        }
    }
    if let Some(addr) = speak {
        // Send queued cues, then give the Agent time to receive them.
        let token = std::env::var("COACH_TOKEN").unwrap_or_default();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(8),
            coach_transport::connect(&addr, &token, &mut rx, |_, _| {})).await;
    }
}
