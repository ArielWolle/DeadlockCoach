//! Skill taxonomy (data/coaching/skills.json): every rule cue carries the skill it coaches, so the
//! browser can stay quiet about skills the player has mastered (docs/player-profile.md "Skills and
//! mastery"). A card id ("review.burst") is the most specific mapping; otherwise the cue key is
//! matched against key patterns (exact, or a prefix ending in '*'; the longest pattern wins).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasteredAfter { pub observations: u32, pub window: u32, #[serde(default = "one")] pub max_failures: u32 }
fn one() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub label: String,
    /// "basic" | "intermediate" | "advanced".
    pub level: String,
    pub mastered_after: MasteredAfter,
    pub description: String,
    #[serde(default)] pub cues: Vec<String>,
    #[serde(default)] pub cards: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog { pub schema: u32, pub skills: Vec<SkillDef> }

pub fn catalog() -> &'static Catalog {
    static C: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    C.get_or_init(|| serde_json::from_str(include_str!("../../../data/coaching/skills.json")).expect("shipped skill taxonomy"))
}

fn matches(pattern: &str, key: &str) -> bool {
    match pattern.strip_suffix('*') { Some(prefix) => key.starts_with(prefix), None => key == pattern }
}

/// The skill a cue key coaches (None = no skill, e.g. a play-pattern summary of several habits).
pub fn for_key(key: &str) -> Option<String> {
    let mut best: Option<(&str, usize)> = None;
    for s in &catalog().skills {
        for p in s.cues.iter().filter(|p| matches(p, key)) {
            // An exact pattern beats any prefix of the same length.
            let rank = p.len() * 2 + usize::from(!p.ends_with('*'));
            if best.is_none_or(|b| rank > b.1) { best = Some((&s.id, rank)); }
        }
    }
    best.map(|b| b.0.to_string())
}

/// The skill of a coaching card ("<module>.<card id>", e.g. "active.cc_zipline").
pub fn for_card(card: &str) -> Option<String> {
    catalog().skills.iter().find(|s| s.cards.iter().any(|c| c == card)).map(|s| s.id.clone())
}

/// Card first, then the key.
pub fn of(card: Option<&str>, key: &str) -> Option<String> { card.and_then(for_card).or_else(|| for_key(key)) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taxonomy_is_valid() {
        let c = catalog();
        assert!((20..=30).contains(&c.skills.len()), "{}", c.skills.len());
        let mut ids: Vec<&str> = c.skills.iter().map(|s| s.id.as_str()).collect();
        ids.sort(); ids.dedup();
        assert_eq!(ids.len(), c.skills.len(), "duplicate skill ids");
        for s in &c.skills {
            assert!(["basic", "intermediate", "advanced"].contains(&s.level.as_str()), "{}", s.id);
            assert!(s.mastered_after.observations >= 1 && s.mastered_after.window >= 1, "{}", s.id);
            assert!(!s.label.is_empty() && !s.description.is_empty(), "{}", s.id);
        }
        // A card belongs to one skill only.
        let mut cards: Vec<&String> = c.skills.iter().flat_map(|s| s.cards.iter()).collect();
        let n = cards.len(); cards.sort(); cards.dedup(); assert_eq!(cards.len(), n, "a card is mapped twice");
    }
    #[test]
    fn keys_map_to_the_most_specific_skill() {
        assert_eq!(for_key("unspent-dead").as_deref(), Some("shop_on_death"));
        assert_eq!(for_key("unspent:base").as_deref(), Some("shop_in_base"));
        assert_eq!(for_key("unspent").as_deref(), Some("dont_die_holding_souls"));
        assert_eq!(for_key("map:wave:Green").as_deref(), Some("catch_waves"));
        assert_eq!(for_key("map:urn-you").as_deref(), Some("urn_play"));
        // plan:urn_* (urn) is longer than plan:* (objectives).
        assert_eq!(for_key("plan:urn_contest:420").as_deref(), Some("urn_play"));
        assert_eq!(for_key("plan:guardian_weak").as_deref(), Some("objective_timing"));
        assert_eq!(for_key("plan:midboss_window:1").as_deref(), Some("use_enemy_dead_windows"));
        assert_eq!(for_key("lead").as_deref(), Some("throw_awareness"));
        assert_eq!(for_key("lead:fed:hero_haze").as_deref(), Some("throw_awareness"));
        assert_eq!(for_key("praise:lead:3").as_deref(), Some("throw_awareness"));
        assert_eq!(for_key("praise:spent_dead").as_deref(), Some("shop_on_death"));
        assert_eq!(for_key("active:hold_base:out").as_deref(), Some("walk_out_with_team"));
        assert_eq!(for_key("review:pattern:3"), None);
        assert_eq!(of(Some("active.cc_zipline"), "active:cc").as_deref(), Some("zipline_discipline"));
        assert_eq!(of(Some("active.cc_stunned"), "active:cc").as_deref(), Some("escape_after_cc"));
    }
}
