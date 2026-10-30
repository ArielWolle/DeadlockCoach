//! Replays a recorded match (one JSON object per line with a "snapshot" field, or a live
//! recording.json with "frames": [{"snapshot": ...}]) through the default engine (plus timers and
//! counters, as the web coach registers them) and prints every spoken cue with its game clock.
//! Active-coach cues are marked with '*', calls the engine spoke in fight mode with '!'. Ends with
//! a per-family count (key up to the second ':') and cadence metrics: calls per minute, the most
//! calls in any 30 s / 60 s, calls per fight-mode episode, death reviews.
//! Usage: cargo run -p coach-engine --example active_replay -- <replay.jsonl|recording.json> [--all]
use coach_core::GameSnapshot;
use coach_engine::{counter_rule_from_json, default_rules, rules_from_timings, Engine, RulesConfig};
use std::collections::BTreeMap;
use std::io::BufRead;

fn snapshots(path: &str) -> Vec<GameSnapshot> {
    let parse = |v: &serde_json::Value| v.get("snapshot").cloned().and_then(|s| serde_json::from_value::<GameSnapshot>(s).ok());
    if path.ends_with(".json") {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("read recording")).expect("recording json");
        return v["frames"].as_array().map(|f| f.iter().filter_map(parse).collect()).unwrap_or_default();
    }
    let file = std::fs::File::open(path).expect("open replay");
    std::io::BufReader::new(file).lines().map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok()).filter_map(|v| parse(&v)).collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("replay .jsonl path");
    let all = args.any(|a| a == "--all");
    // As the web coach registers them: the Urn timer readout is replaced by the Urn plan (plans.rs).
    let mut rules = rules_from_timings(&coach_engine::plans::unplanned_timings(include_str!("../../../data/timings.json"))).expect("timings");
    rules.extend(default_rules(RulesConfig::default()));
    rules.push(counter_rule_from_json(include_str!("../../../data/counters.json")).expect("counters"));
    let mut engine = Engine::new(rules);
    // COACH_NO_BUDGET=1: without the cadence budget and fight mode (to see what they hold back).
    if std::env::var("COACH_NO_BUDGET").is_ok() { engine.budget = false; }
    let (mut mine, mut first, mut last) = (0usize, None::<u32>, 0u32);
    let mut families: BTreeMap<String, usize> = BTreeMap::new();
    let mut spoken: Vec<u32> = vec![];
    // Fight-mode episodes: (start, end, calls).
    let mut fights: Vec<(u32, u32, usize)> = vec![];
    let mut reviews = 0usize;
    for snap in snapshots(&path) {
        if let Some(t) = snap.game_clock_secs { first.get_or_insert(t); last = last.max(t); }
        let cues = engine.step(&snap);
        let t = snap.game_clock_secs.unwrap_or(0);
        let fight = engine.in_fight();
        if fight {
            match fights.last_mut() { Some(f) if t <= f.1 + 10 => f.1 = t, _ => fights.push((t, t, 0)) }
        }
        for c in cues {
            let family = c.key.split(':').take(2).collect::<Vec<_>>().join(":");
            let family = if family.starts_with("timer:") || family.starts_with("counter:") { family.split(':').next().unwrap_or("").to_string() } else { family };
            *families.entry(family).or_default() += 1;
            spoken.push(t);
            if c.key.starts_with("death:review") { reviews += 1; }
            if fight { if let Some(f) = fights.last_mut() { f.2 += 1; } }
            let active = c.key.starts_with("active:") || c.key == "unspent:base" || c.text.contains("Group up and push");
            if active { mine += 1; }
            if active || all {
                let words = c.text.split_whitespace().count();
                println!("{}{}{:>2}:{:02} [{}] ({words}w) {}", if active { "*" } else { " " }, if fight { "!" } else { " " }, t / 60, t % 60, c.key, c.text);
            }
        }
    }
    let span = last.saturating_sub(first.unwrap_or(0));
    println!("-- {mine} active cues over {}:{:02} of game clock ({:.0} s per cue)", span / 60, span % 60, if mine > 0 { span as f32 / mine as f32 } else { 0.0 });
    for (f, n) in families { println!("## {n:>3} {f}"); }
    let most = |w: u32| spoken.iter().map(|&a| spoken.iter().filter(|&&b| b >= a && b < a + w).count()).max().unwrap_or(0);
    println!("== calls {} over {:.1} min: {:.2} calls/min; max {} in 30 s, {} in 60 s; death reviews {reviews}",
        spoken.len(), span as f32 / 60.0, spoken.len() as f32 * 60.0 / span.max(1) as f32, most(30), most(60));
    let fights: Vec<&(u32, u32, usize)> = fights.iter().filter(|f| f.1 > f.0).collect();
    let busiest = fights.iter().map(|f| f.2).max().unwrap_or(0);
    println!("== fight-mode episodes {}: calls {:?}, max {busiest}", fights.len(), fights.iter().map(|f| format!("{}:{:02}-{}:{:02}={}", f.0 / 60, f.0 % 60, f.1 / 60, f.1 % 60, f.2)).collect::<Vec<_>>());
}
