//! Jungle coaching: camp spawn warnings and counter-jungling (invading) decisions.
//!
//! Evidence rules (data/coaching/jungle.json, sources in docs/jungle): spawn times are wiki facts.
//! Invading needs the wiki's two conditions: your lane is pushed into the opponent's side, and the
//! opposing heroes are accounted for (at most one enemy without a top-bar health bar, i.e. not
//! seen recently). A missing enemy is a reason not to invade. Camp clears and respawns are not
//! observed from screenshots, so no respawn timer is claimed.
use crate::{laning::Source, positioning::{Advice, TeamPicture}, Cue, Priority, Rule};
use coach_core::GameSnapshot;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Deserialize)]
pub struct Thresholds { pub warn_secs: u32, pub crashed_front: f32, pub forward_progress: f32, pub healthy: f32, pub max_unaccounted: usize, pub near_radius: f32 }
#[derive(Clone, Deserialize)]
pub struct Spawn { pub id: String, pub secs: u32 }
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub spawns: Vec<Spawn>, pub cards: Vec<Card> }

#[derive(Clone)]
pub struct JungleCoach { catalog: Arc<Catalog> }
impl Default for JungleCoach {
    fn default() -> Self { Self { catalog: Arc::new(serde_json::from_str(include_str!("../../../data/coaching/jungle.json")).expect("shipped jungle policy")) } }
}

impl JungleCoach {
    fn advice(&self, id: &str, vars: &[(&str, String)], evidence: Vec<String>) -> Advice {
        let c = self.catalog.cards.iter().find(|c| c.id == id).expect("jungle card");
        let mut text = c.speech.clone();
        for (k, v) in vars { text = text.replace(&format!("{{{k}}}"), v); }
        let priority = match c.priority.as_str() { "high" => Priority::High, "low" => Priority::Low, _ => Priority::Normal };
        Advice { id: id.into(), text, priority, cooldown_secs: c.cooldown_secs, evidence, sources: c.sources.clone() }
    }

    /// Camp spawn warning when a tier is about to appear for the first time.
    pub fn spawn_warning(&self, s: &GameSnapshot) -> Option<Advice> {
        let clock = s.game_clock_secs?;
        if s.dead { return None; }
        let t = &self.catalog.thresholds;
        let spawn = self.catalog.spawns.iter().find(|sp| clock < sp.secs && sp.secs - clock <= t.warn_secs)?;
        let secs = spawn.secs - clock;
        Some(self.advice(&format!("spawn_{}", spawn.id), &[("seconds", secs.to_string())],
            vec![format!("Game clock {}:{:02}; first {} camp spawn at {}:{:02}", clock / 60, clock % 60, spawn.id, spawn.secs / 60, spawn.secs % 60)]))
    }

    /// Invade or stay-home decision once your lane's wave is crashed into the enemy guardian.
    pub fn invade(&self, s: &GameSnapshot) -> Option<Advice> {
        let t = &self.catalog.thresholds;
        let clock = s.game_clock_secs?;
        if s.dead || s.screen.as_deref().is_some_and(|v| v != "Gameplay") || clock < 120 { return None; }
        let map = s.minimap.as_ref().filter(|m| m.observations >= 3)?;
        let lane = crate::laning::focus(s)?;
        let me = map.you?;
        let state = map.lane(lane);
        let crashed = state.push.is_some_and(|p| p >= t.crashed_front) && state.ally_troopers >= 2 && state.ally_progress.is_some_and(|p| p >= 0.65);
        let forward = map.your_lane == Some(lane) && map.your_progress.is_some_and(|p| p >= t.forward_progress);
        let healthy = s.hp.zip(s.max_hp).is_some_and(|(hp, max)| max >= 100 && hp as f32 / max as f32 >= t.healthy) && s.health_observations >= 2;
        let enemy_near = map.heroes.iter().any(|h| !h.ally && h.confirmed() && h.visible() && h.distance(me) <= t.near_radius);
        let objective_soon = clock >= 570 && (clock < 600 && 600 - clock <= 30 || clock >= 600 && 300 - clock % 300 <= 30);
        if !(crashed && forward && healthy) || enemy_near || objective_soon { return None; }
        // Without the top bar the coach does not know where the opposing heroes are: no call.
        let team = TeamPicture::from(s)?;
        let lane_name = crate::laning::lane_name(lane);
        let mut evidence = vec![format!("{lane_name} lane front at {:.0}% with {} allied trooper dots near their side", state.push.unwrap_or(0.0) * 100.0, state.ally_troopers),
            format!("Top bar: {} enemies alive, {} not seen recently, {} dead", team.enemies_alive, team.enemies_unseen, team.enemies_dead.len())];
        let missing_tracks = map.heroes.iter().filter(|h| !h.ally && h.confirmed() && !h.visible() && h.unseen_ms <= 30_000 && h.distance(me) <= 0.28).count();
        if missing_tracks > 0 { evidence.push(format!("{missing_tracks} enemy marker(s) near you left the minimap in the last 30 s")); }
        let unaccounted = team.enemies_unseen.max(missing_tracks.min(team.enemies_alive));
        if unaccounted <= t.max_unaccounted {
            let accounted = if !team.enemies_dead.is_empty() && unaccounted == 0 { format!("{} dead and the rest are visible", if team.enemies_dead.len() == 1 { "one enemy is".to_string() } else { format!("{} enemies are", team.enemies_dead.len()) }) }
                else if unaccounted == 0 { "every enemy has been seen recently".into() } else { "only one enemy is unaccounted for".into() };
            Some(self.advice("invade", &[("accounted", accounted)], evidence))
        } else {
            Some(self.advice("invade_unsafe", &[("count", format!("{unaccounted} enemies are"))], evidence))
        }
    }

    pub fn assess(&self, s: &GameSnapshot) -> Option<Advice> { self.invade(s).or_else(|| self.spawn_warning(s)) }
}

impl Rule for JungleCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        self.assess(s).map(|a| vec![Cue { key: format!("jungle:{}", a.id), text: a.text, priority: a.priority, cooldown_secs: a.cooldown_secs }]).unwrap_or_default()
    }
}
