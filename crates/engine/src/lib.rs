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

/// "If the enemy has `when_enemy_has`, suggest `suggest`." Data-driven (data/counters.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterDef {
    pub when_enemy_has: String,
    pub suggest: String,
    pub reason: String,
    /// Item class_name of `suggest`, used to skip if already owned.
    #[serde(default)]
    pub suggest_class: String,
    /// Spoken name of the enemy (hero display name); falls back to `when_enemy_has`.
    #[serde(default)]
    pub label: String,
}

pub struct CounterRule(pub Vec<CounterDef>);

impl Rule for CounterRule {
    /// One cue per enemy hero/item, listing up to 3 suggestions not already owned.
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let mut seen: Vec<&str> = vec![];
        let mut cues = vec![];
        for trigger in snap.enemy_items.iter().chain(snap.enemy_heroes.iter()) {
            if seen.contains(&trigger.as_str()) { continue; }
            seen.push(trigger);
            let mine: Vec<&CounterDef> = self.0.iter()
                .filter(|c| &c.when_enemy_has == trigger
                    && !snap.items.iter().any(|i| i == &c.suggest || i == &c.suggest_class))
                .take(3).collect();
            let Some(first) = mine.first() else { continue };
            let who = if first.label.is_empty() { trigger.as_str() } else { first.label.as_str() };
            let names: Vec<&str> = mine.iter().map(|c| c.suggest.as_str()).collect();
            cues.push(Cue {
                key: format!("counter:{trigger}"),
                text: format!("Enemy has {who}. Good items against them: {}.", names.join(", ")),
                priority: Priority::Low,
                cooldown_secs: 600,
            });
        }
        cues
    }
}
pub fn counter_rule_from_json(json: &str) -> Result<Box<dyn Rule>, String> {
    Ok(Box::new(CounterRule(serde_json::from_str(json).map_err(|e| e.to_string())?)))
}

/// Parse data/timings.json into timer rules.
pub fn rules_from_timings(json: &str) -> Result<Vec<Box<dyn Rule>>, String> {
    let defs: Vec<TimerDef> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(defs.into_iter().map(|d| Box::new(TimerRule(d)) as Box<dyn Rule>).collect())
}

/// Tunable thresholds (data/rules.json). Values are starting guesses, not game facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    pub unspent_souls: u32,
    pub lead_pct: f32,
    pub lead_min_clock_secs: u32,
}
impl Default for RulesConfig {
    fn default() -> Self { RulesConfig { unspent_souls: 1500, lead_pct: 15.0, lead_min_clock_secs: 300 } }
}

pub struct UnspentSoulsRule(pub RulesConfig);
impl Rule for UnspentSoulsRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        match snap.souls {
            Some(s) if s >= self.0.unspent_souls => vec![Cue {
                key: "unspent".into(),
                text: format!("You are holding {} souls. Spend them at the shop.", s),
                priority: Priority::Normal,
                cooldown_secs: 120,
            }],
            _ => vec![],
        }
    }
}

pub struct NetworthLeadRule(pub RulesConfig);
impl Rule for NetworthLeadRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let (Some(a), Some(e), Some(t)) = (snap.ally_networth, snap.enemy_networth, snap.game_clock_secs) else { return vec![] };
        if t < self.0.lead_min_clock_secs || a == 0 || e == 0 { return vec![]; }
        let diff = (a as f32 - e as f32) / e as f32 * 100.0;
        let (key, text) = if diff >= self.0.lead_pct {
            ("lead", "Your team is ahead in souls. Take objectives and avoid risky fights.")
        } else if diff <= -self.0.lead_pct {
            ("behind", "Your team is behind in souls. Farm safely and fight near your structures.")
        } else { return vec![] };
        vec![Cue { key: key.into(), text: text.into(), priority: Priority::Normal, cooldown_secs: 240 }]
    }
}

pub fn default_rules(cfg: RulesConfig) -> Vec<Box<dyn Rule>> {
    vec![Box::new(UnspentSoulsRule(cfg.clone())), Box::new(NetworthLeadRule(cfg))]
}

pub struct Engine {
    rules: Vec<Box<dyn Rule>>,
    last_said: HashMap<String, u32>,
    /// Max cues returned per step (rest are held back and retried next step).
    pub max_per_step: usize,
    /// Minimum game-seconds between any two spoken cues.
    pub min_gap_secs: u32,
    last_cue_at: Option<u32>,
}

impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self { Engine { rules, last_said: HashMap::new(), max_per_step: 1, min_gap_secs: 8, last_cue_at: None } }

    /// Evaluate all rules; returns cues not suppressed by cooldown, highest priority first.
    pub fn step(&mut self, snap: &GameSnapshot) -> Vec<Cue> {
        let Some(now) = snap.game_clock_secs else { return vec![] };
        if let Some(l) = self.last_cue_at {
            if now >= l && now - l < self.min_gap_secs { return vec![]; }
            if now < l { self.last_said.clear(); self.last_cue_at = None; } // new match
        }
        let mut out: Vec<Cue> = self.rules.iter().flat_map(|r| r.evaluate(snap)).collect();
        out.retain(|c| match self.last_said.get(&c.key) {
            Some(&t) if now >= t && now - t < c.cooldown_secs => false,
            _ => true,
        });
        out.sort_by(|a, b| b.priority.cmp(&a.priority));
        out.truncate(self.max_per_step);
        for c in &out { self.last_said.insert(c.key.clone(), now); }
        if !out.is_empty() { self.last_cue_at = Some(now); }
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
    fn shipped_timings_parse() {
        let r = rules_from_timings(include_str!("../../../data/timings.json")).unwrap();
        let mut e = Engine::new(r);
        assert_eq!(e.step(&at(580))[0].text, "Soul Urn in 20 seconds");
    }

    #[test]
    fn counter_rule_fires_once_and_skips_owned() {
        let r = counter_rule_from_json(r#"[{"when_enemy_has":"A","suggest":"B","reason":"test."}]"#).unwrap();
        let mut e = Engine::new(vec![r]);
        let mut s = at(100); s.enemy_items = vec!["A".into()];
        assert_eq!(e.step(&s)[0].text, "Enemy has A. Good items against them: B.");
        assert!(e.step(&s).is_empty());
        let mut s2 = at(1000); s2.enemy_items = vec!["A".into()]; s2.items = vec!["B".into()];
        assert!(e.step(&s2).is_empty());
    }

    #[test]
    fn unspent_and_lead_rules() {
        let mut e = Engine::new(default_rules(RulesConfig::default()));
        let mut s = at(400);
        s.souls = Some(2000); s.ally_networth = Some(20000); s.enemy_networth = Some(15000);
        e.max_per_step = 5;
        let texts: Vec<String> = e.step(&s).into_iter().map(|c| c.text).collect();
        assert!(texts.iter().any(|t| t.contains("2000 souls")));
        assert!(texts.iter().any(|t| t.contains("ahead")));
        assert!(e.step(&s).is_empty());
    }

    #[test]
    fn counter_matches_enemy_hero() {
        let r = counter_rule_from_json(r#"[{"when_enemy_has":"hero_x","suggest":"Y","reason":"r."}]"#).unwrap();
        let mut s = at(10); s.enemy_heroes = vec!["hero_x".into()];
        assert_eq!(Engine::new(vec![r]).step(&s).len(), 1);
    }

    #[test]
    fn no_clock_no_cue() {
        assert!(eng().step(&GameSnapshot::default()).is_empty());
    }
}
