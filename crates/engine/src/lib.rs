//! Coaching engine: GameSnapshot in, spoken Cues out. Pure logic, no I/O,
//! so it can be tested with fake snapshots and replayed from recorded sessions.
use coach_core::GameSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority { Low, Normal, High }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Dedup key: the same key is not repeated within its cooldown.
    pub key: String,
    pub text: String,
    pub priority: Priority,
    pub cooldown_secs: u32,
}

/// A recurring objective, loaded from data/timings.json (values are config,
/// not hardcoded; verify them against the current patch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDef {
    pub name: String,
    pub first_spawn_secs: u32,
    pub interval_secs: u32,
    pub warn_ahead_secs: u32,
}

pub trait Rule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue>;
}

pub struct TimerRule(pub TimerDef);

impl Rule for TimerRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let (Some(t), d) = (snap.game_clock_secs, &self.0) else { return vec![] };
        if t + d.warn_ahead_secs < d.first_spawn_secs || d.interval_secs == 0 { return vec![]; }
        // Next spawn at or after now.
        let n = if t <= d.first_spawn_secs { 0 } else { (t - d.first_spawn_secs).div_ceil(d.interval_secs) };
        let next = d.first_spawn_secs + n * d.interval_secs;
        let until = next - t;
        if until <= d.warn_ahead_secs && until > 0 {
            vec![Cue {
                key: format!("timer:{}:{}", d.name, next),
                text: format!("{} in {} seconds", d.name, until),
                priority: Priority::Normal,
                cooldown_secs: d.interval_secs.max(30),
            }]
        } else { vec![] }
    }
}

pub struct Engine {
    rules: Vec<Box<dyn Rule>>,
    last_said: HashMap<String, u32>,
}

impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self { Engine { rules, last_said: HashMap::new() } }

    /// Evaluate all rules; returns cues not suppressed by cooldown, highest priority first.
    pub fn step(&mut self, snap: &GameSnapshot) -> Vec<Cue> {
        let now = snap.game_clock_secs.unwrap_or(0);
        let mut out: Vec<Cue> = self.rules.iter().flat_map(|r| r.evaluate(snap)).collect();
        out.retain(|c| match self.last_said.get(&c.key) {
            Some(&t) if now >= t && now - t < c.cooldown_secs => false,
            _ => true,
        });
        for c in &out { self.last_said.insert(c.key.clone(), now); }
        out.sort_by(|a, b| b.priority.cmp(&a.priority));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: u32) -> GameSnapshot { GameSnapshot { game_clock_secs: Some(t), ..Default::default() } }
    fn eng() -> Engine {
        Engine::new(vec![Box::new(TimerRule(TimerDef {
            name: "Test boss".into(), first_spawn_secs: 600, interval_secs: 300, warn_ahead_secs: 30 }))])
    }

    #[test]
    fn warns_before_first_spawn_once() {
        let mut e = eng();
        assert!(e.step(&at(500)).is_empty());
        assert_eq!(e.step(&at(575))[0].text, "Test boss in 25 seconds");
        assert!(e.step(&at(580)).is_empty(), "deduped");
    }

    #[test]
    fn warns_again_for_next_spawn() {
        let mut e = eng();
        e.step(&at(575));
        assert_eq!(e.step(&at(880))[0].text, "Test boss in 20 seconds");
    }

    #[test]
    fn no_clock_no_cue() {
        assert!(eng().step(&GameSnapshot::default()).is_empty());
    }
}
