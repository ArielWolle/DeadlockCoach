//! Replays a recorded match (one JSON object per line with a "snapshot" field) through the
//! default engine (plus timers and counters, as the web coach registers them) and prints every
//! spoken cue with its game clock. Active-coach cues are marked with '*'. Ends with a per-family
//! count (key up to the second ':').
//! Usage: cargo run -p coach-engine --example active_replay -- <replay.jsonl> [--all]
use coach_core::GameSnapshot;
use coach_engine::{counter_rule_from_json, default_rules, rules_from_timings, Engine, RulesConfig};
use std::collections::BTreeMap;
use std::io::BufRead;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("replay .jsonl path");
    let all = args.any(|a| a == "--all");
    let file = std::fs::File::open(&path).expect("open replay");
    // As the web coach registers them: the Urn timer readout is replaced by the Urn plan (plans.rs).
    let mut rules = rules_from_timings(&coach_engine::plans::unplanned_timings(include_str!("../../../data/timings.json"))).expect("timings");
    rules.extend(default_rules(RulesConfig::default()));
    rules.push(counter_rule_from_json(include_str!("../../../data/counters.json")).expect("counters"));
    let mut engine = Engine::new(rules);
    let (mut mine, mut first, mut last) = (0usize, None::<u32>, 0u32);
    let mut families: BTreeMap<String, usize> = BTreeMap::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line.expect("read line");
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let Some(snap) = v.get("snapshot").cloned().and_then(|s| serde_json::from_value::<GameSnapshot>(s).ok()) else { continue };
        if let Some(t) = snap.game_clock_secs { first.get_or_insert(t); last = last.max(t); }
        for c in engine.step(&snap) {
            let family = c.key.split(':').take(2).collect::<Vec<_>>().join(":");
            let family = if family.starts_with("timer:") || family.starts_with("counter:") { family.split(':').next().unwrap_or("").to_string() } else { family };
            *families.entry(family).or_default() += 1;
            let active = c.key.starts_with("active:") || c.key == "unspent:base" || c.text.contains("Group up and push");
            if active { mine += 1; }
            if active || all {
                let t = snap.game_clock_secs.unwrap_or(0);
                println!("{}{:>2}:{:02} [{}] {}", if active { "*" } else { " " }, t / 60, t % 60, c.key, c.text);
            }
        }
    }
    let span = last.saturating_sub(first.unwrap_or(0));
    println!("-- {mine} active cues over {}:{:02} of game clock ({:.0} s per cue)", span / 60, span % 60, if mine > 0 { span as f32 / mine as f32 } else { 0.0 });
    for (f, n) in families { println!("## {n:>3} {f}"); }
}
