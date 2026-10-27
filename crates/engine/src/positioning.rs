//! Positional coaching from tracked minimap heroes: gank warnings, numbers advantage or
//! disadvantage around you, missing enemies, and teammates to help.
//!
//! Evidence rules: only confirmed tracks (seen on >= 2 observations) count; enemies that are
//! not visible are "missing", never assumed dead or far away; your own position must be a
//! confirmed player marker. One recommendation per evaluation, most urgent first.
use crate::{laning::Source, Cue, Priority, Rule};
use coach_core::{GameSnapshot, HeroTrack, Lane, MinimapState, Pt};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Deserialize)]
pub struct Thresholds { pub near_radius: f32, pub fight_radius: f32, pub watch_radius: f32, pub closing_speed: f32,
    pub missing_min_ms: u64, pub missing_max_ms: u64, pub help_max_distance: f32, pub healthy: f32 }
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub cards: Vec<Card> }

/// A positional recommendation with its observed basis, for display next to the spoken cue.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Advice { pub id: String, pub text: String, pub priority: Priority, pub cooldown_secs: u32, pub evidence: Vec<String>, pub sources: Vec<Source> }

#[derive(Clone)]
pub struct PositionCoach { catalog: Arc<Catalog> }
impl Default for PositionCoach {
    fn default() -> Self { Self { catalog: Arc::new(serde_json::from_str(include_str!("../../../data/coaching/positioning.json")).expect("shipped positioning policy")) } }
}

fn count(n: usize, what: &str) -> String {
    if n == 1 { return format!("1 {what}"); }
    match what.strip_suffix('y') { Some(stem) => format!("{n} {stem}ies"), None => format!("{n} {what}s") }
}
fn lane_name(l: Lane) -> &'static str { crate::laning::lane_name(l) }
/// Spoken place name: the lane a hero stands on, else the side of the map.
pub fn area(p: Pt, lane: Option<Lane>) -> String {
    match lane { Some(l) => format!("{} lane", lane_name(l)),
        None => (if p.x < 0.38 { "the Green side jungle" } else if p.x > 0.62 { "the Yellow side jungle" } else { "mid" }).into() }
}

impl PositionCoach {
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("positioning card") }
    fn advice(&self, id: &str, vars: &[(&str, String)], evidence: Vec<String>) -> Advice {
        let c = self.card(id);
        let mut text = c.speech.clone();
        for (k, v) in vars { text = text.replace(&format!("{{{k}}}"), v); }
        let priority = match c.priority.as_str() { "high" => Priority::High, "low" => Priority::Low, _ => Priority::Normal };
        Advice { id: id.into(), text, priority, cooldown_secs: c.cooldown_secs, evidence, sources: c.sources.clone() }
    }

    pub fn assess(&self, s: &GameSnapshot) -> Option<Advice> {
        if s.dead || s.screen.as_deref().is_some_and(|v| v != "Gameplay") || s.game_clock_secs.is_none() { return None; }
        let map: &MinimapState = s.minimap.as_ref()?;
        let me = map.you?;
        let t = &self.catalog.thresholds;
        let live = |h: &&HeroTrack| h.confirmed() && h.visible();
        let enemies: Vec<&HeroTrack> = map.heroes.iter().filter(|h| !h.ally).filter(live).collect();
        let allies: Vec<&HeroTrack> = map.heroes.iter().filter(|h| h.ally).filter(live).collect();
        let near_e: Vec<&&HeroTrack> = enemies.iter().filter(|h| h.distance(me) <= t.near_radius).collect();
        let near_a = allies.iter().filter(|h| h.distance(me) <= t.near_radius).count() + 1; // + you
        let health = s.hp.zip(s.max_hp).filter(|(hp, max)| *max >= 100 && hp <= max).map(|(hp, max)| hp as f32 / max as f32);
        let healthy = health.is_some_and(|h| h >= t.healthy) && s.health_observations >= 2;
        let hp_note = health.map(|h| format!("Your health: {:.0}%", h * 100.0)).unwrap_or("Your health is unread".into());

        // 1. Gank: enemies inside the watch zone but outside your fight, moving toward you.
        let closing: Vec<&&HeroTrack> = enemies.iter().filter(|h| h.distance(me) > t.near_radius * 0.6
            && h.distance(me) <= t.watch_radius && h.closing_speed(me) >= t.closing_speed).collect();
        if !closing.is_empty() && closing.len() + near_e.len() >= near_a {
            let from_lane = closing.iter().filter_map(|h| h.lane).find(|l| Some(*l) != map.your_lane);
            let direction = match (from_lane, closing.iter().all(|h| h.lane.is_some() && h.lane == map.your_lane)) {
                (_, true) => "down your lane".to_string(),
                (Some(l), _) => format!("from {} lane", lane_name(l)),
                _ => "through the jungle".to_string(),
            };
            let ev = closing.iter().map(|h| format!("Enemy marker {:.2} map away, closing at {:.3}/s", h.distance(me), h.closing_speed(me))).chain([hp_note]).collect();
            return Some(self.advice("gank", &[("count", count(closing.len(), "enemy")), ("direction", direction)], ev));
        }
        // 2. Outnumbered around you.
        if near_e.len() >= 2 && near_e.len() > near_a {
            return Some(self.advice("outnumbered", &[("enemies", count(near_e.len(), "enemy")), ("allies", if near_a == 1 { "just you".into() } else { format!("{near_a} of you") })],
                vec![format!("{} enemy and {} allied markers (including you) within {:.2}", near_e.len(), near_a, t.near_radius), hp_note]));
        }
        // 3. Missing: confirmed enemies last seen near you that left the minimap recently.
        let missing: Vec<&HeroTrack> = map.heroes.iter().filter(|h| !h.ally && h.confirmed() && h.distance(me) <= t.watch_radius
            && (t.missing_min_ms..=t.missing_max_ms).contains(&h.unseen_ms)).collect();
        let forward = map.your_progress.is_some_and(|p| p >= 0.45);
        if !missing.is_empty() && (forward || missing.len() >= 2) && near_e.is_empty() {
            let secs = missing.iter().map(|h| h.unseen_ms).min().unwrap_or(0) / 1000;
            return Some(self.advice("missing", &[("count", count(missing.len(), "enemy")), ("seconds", secs.to_string())],
                missing.iter().map(|h| format!("Enemy last seen {:.2} map away, {} s ago", h.distance(me), h.unseen_ms / 1000)).collect()));
        }
        // 4. Numbers advantage where you are.
        let recently_missing = map.heroes.iter().filter(|h| !h.ally && !h.visible() && h.unseen_ms <= t.missing_max_ms && h.distance(me) <= t.watch_radius).count();
        // 2v1 or a two-hero margin; 3v2 is not a clear advantage.
        let advantage = near_a >= 2 * near_e.len() || near_a >= near_e.len() + 2;
        if !near_e.is_empty() && advantage && healthy && recently_missing == 0 {
            return Some(self.advice("go_in", &[("allies", format!("{near_a} of you")), ("enemies", count(near_e.len(), "enemy"))],
                vec![format!("{near_a} allied markers (including you) vs {} enemy within {:.2}", near_e.len(), t.near_radius), hp_note]));
        }
        // 5. A teammate fighting nearby: help if they are losing, join if they are winning.
        if near_e.is_empty() && healthy {
            let fight = allies.iter().filter(|a| (t.near_radius..=t.help_max_distance).contains(&a.distance(me))).filter_map(|a| {
                let foes = enemies.iter().filter(|e| e.distance(a.pos) <= t.fight_radius).count();
                let friends = allies.iter().filter(|o| o.distance(a.pos) <= t.fight_radius).count();
                (foes > 0).then_some((*a, friends, foes))
            }).max_by(|x, y| (x.2 as i32 - x.1 as i32).cmp(&(y.2 as i32 - y.1 as i32)).then(y.0.distance(me).total_cmp(&x.0.distance(me))));
            if let Some((a, friends, foes)) = fight {
                let place = area(a.pos, a.lane);
                let ev = vec![format!("{friends} allied vs {foes} enemy markers within {:.2} of a teammate {:.2} map away", t.fight_radius, a.distance(me)), hp_note];
                let vars = [("area", place), ("allies", count(friends, "ally")), ("enemies", count(foes, "enemy"))];
                return Some(if foes >= friends { self.advice("help", &vars, ev) } else if friends >= foes + 1 { self.advice("join", &vars, ev) } else { return None });
            }
        }
        None
    }
}

impl Rule for PositionCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        self.assess(s).map(|a| vec![Cue { key: format!("pos:{}", a.id), text: a.text, priority: a.priority, cooldown_secs: a.cooldown_secs }]).unwrap_or_default()
    }
}
