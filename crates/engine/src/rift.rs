//! Unstable Rift calls (docs/rift.md, data/coaching/rift.json; wiki facts in
//! docs/strategy/midgame-macro.md section 8). Personal, short, a plan 20-40 s ahead:
//! - before the window: one heads-up to push your wave first (`plan:rift_soon:T`, T = earliest announcement);
//! - on the announcement (or the opening, when the announcement was missed): group on the bridge,
//!   all go when we are behind (we capture faster), or skip it when two or more of us are dead
//!   and more of us than of them (`plan:rift:N` / `rift:skip:N`, one of them per Rift);
//! - while it is open: they are alone on it -> step in (`plan:rift_stop:N`); a long contest ->
//!   the draw and its soul orbs (`plan:rift_orbs:N`);
//! - after it: push behind our Rift Troopers (`plan:rift_push:N`) or defend against theirs at the
//!   Guardian (`rift:defend:N`).
//! N in the keys is the game second the Rift was first seen.
//! Team counts come from the top bar only when all twelve slots were read (TeamPicture); a Rift
//! state is only what the vision saw (`GameSnapshot::rift`). The "plan:rift" calls send you to an
//! objective: the engine keeps them quiet under a hold / back-out call (lib.rs `is_go_call`).
use crate::{laning::Source, positioning::TeamPicture, review, Cue, Priority, Rule};
use coach_core::{GameSnapshot, Lane, Pt};
use serde::Deserialize;
use std::{cell::RefCell, collections::HashSet, sync::Arc};

#[derive(Clone, Deserialize)]
pub struct Thresholds {
    pub soon_ahead_max: u32, pub soon_ahead_min: u32, pub first_announce_earliest: u32, pub next_announce_min_secs: u32,
    pub go_min_secs: u32, pub behind_pct: f32, pub skip_dead: usize, pub near_site: f32,
    pub stop_min_pct: u8, pub stop_max_pct: u8, pub draw_warn_secs: u32, pub draw_secs: u32, pub push_max_secs: u32, pub min_hp: f32, pub lane_wait_secs: u32,
}
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub family: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String,
    pub sources: Vec<Source>, #[serde(default)] pub variants: Vec<String> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub cards: Vec<Card> }

/// The shipped policy (data/coaching/rift.json).
pub fn catalog() -> Arc<Catalog> {
    static C: std::sync::OnceLock<Arc<Catalog>> = std::sync::OnceLock::new();
    C.get_or_init(|| Arc::new(serde_json::from_str(include_str!("../../../data/coaching/rift.json")).expect("shipped rift policy"))).clone()
}

#[derive(Default)]
struct State { clock: u32, said: HashSet<String> }

pub struct RiftCoach { catalog: Arc<Catalog>, st: RefCell<State> }
impl Default for RiftCoach { fn default() -> Self { Self { catalog: catalog(), st: Default::default() } } }

/// Minimap position of a bridge ("left" / "right"), as the vision places it (crates/vision/src/rift.rs SITES).
pub fn site(side: &str) -> Option<Pt> { match side { "left" => Some(Pt { x: 48.0 / 335.0, y: 178.0 / 350.0 }), "right" => Some(Pt { x: 273.0 / 335.0, y: 178.0 / 350.0 }), _ => None } }
fn lane_name(l: Lane) -> &'static str { crate::laning::lane_name(l) }
fn count_word(n: usize) -> &'static str { ["none", "one", "two", "three", "four", "five", "six"].get(n).copied().unwrap_or("six") }
fn mmss(t: u32) -> String { format!("{}:{:02}", t / 60, t % 60) }

impl RiftCoach {
    pub fn catalog(&self) -> &Catalog { &self.catalog }
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("rift card") }
    fn cue(&self, id: &str, variant: Option<usize>, key: String, vars: &[(&str, String)]) -> Cue {
        let c = self.card(id);
        let mut text = variant.and_then(|i| c.variants.get(i)).unwrap_or(&c.speech).clone();
        for (k, v) in vars { text = text.replace(&format!("{{{k}}}"), v); }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let priority = match c.priority.as_str() { "high" => Priority::High, "low" => Priority::Low, _ => Priority::Normal };
        Cue::card(&format!("rift.{id}"), key, text, priority, c.cooldown_secs)
    }
    /// A teammate whose top-bar hero has a minimap position near the bridge: "with Bebop ".
    fn with(s: &GameSnapshot, at: Option<Pt>, r: f32) -> String {
        let Some(at) = at else { return String::new() };
        s.hero_status.iter().filter(|h| h.ally && h.dead != Some(true) && h.position_observations >= 1)
            .filter(|h| Some(h.hero.as_deref()) != Some(s.hero.as_deref()))
            .filter_map(|h| Some((h.hero.as_deref()?, h.position?)))
            .map(|(hero, p)| (hero, (p.x - at.x).hypot(p.y - at.y))).filter(|(_, d)| *d <= r)
            .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(h, _)| format!("with {} ", review::game_data().hero_name(h))).unwrap_or_default()
    }
}

impl Rule for RiftCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        let Some(clock) = s.game_clock_secs else { return vec![] };
        let mut st = self.st.borrow_mut();
        if clock + 120 < st.clock { *st = State::default(); }
        st.clock = clock;
        let playing = !s.dead && s.screen.as_deref().is_none_or(|v| v == "Gameplay");
        if !playing { return vec![]; }
        let t = &self.catalog.thresholds;
        let r = s.rift.as_ref();
        // One episode per Rift, keyed by when it was first seen (an announcement that never opened
        // is not in the history, so a count would repeat).
        let n = r.and_then(|r| r.seen_at.or(r.opened_at)).unwrap_or(0);
        let phase = r.map(|r| r.phase.as_str()).unwrap_or("none");
        let said = |k: &str| st.said.contains(k);
        let lane = r.and_then(|r| r.lane);
        let at = r.and_then(|r| r.side.as_deref()).and_then(site);
        let (where_, where_short) = match lane { Some(l) => (format!("in {}", lane_name(l)), lane_name(l).to_string()), None => ("on a side lane".into(), "to the Rift".into()) };
        let mut out = vec![];
        // Low on health: no call that sends you into the point (the skip and after-calls stay).
        let hurt = s.hp.zip(s.max_hp).is_some_and(|(h, m)| m >= 100 && h <= m && (h as f32) < m as f32 * t.min_hp);
        match phase {
            "none" => {
                // The next announcement window: the first from 11:00, a later one 6 minutes after
                // the previous Rift's effect (opening - 80 s) plus the 60 s to its announcement.
                let earliest = match r.and_then(|r| r.history.last()) {
                    None => Some(t.first_announce_earliest),
                    Some(e) => e.opened_at.map(|o| o.saturating_sub(20) + t.next_announce_min_secs),
                };
                if let Some(at) = earliest.filter(|a| *a > clock && (t.soon_ahead_min..=t.soon_ahead_max).contains(&(a - clock))) {
                    let key = format!("plan:rift_soon:{at}");
                    if !said(&key) { out.push(self.cue("soon", None, key, &[("time", mmss(at))])); }
                }
            }
            "announced" | "open" | "contested" => {
                let r = r.expect("phase from a rift");
                let (go_key, skip_key) = (format!("plan:rift:{n}"), format!("rift:skip:{n}"));
                let team = TeamPicture::from(s);
                let alive = team.as_ref().map(|p| 6 - p.allies_dead.len().min(6));
                // Short-handed: two or more of us dead and more of ours than theirs.
                let many_dead = team.as_ref().is_some_and(|p| p.allies_dead.len() >= t.skip_dead && p.allies_dead.len() > p.enemies_dead.len());
                if !said(&go_key) && !said(&skip_key) {
                    let secs = r.opened_at.filter(|o| phase == "announced" && *o >= clock + t.go_min_secs).map(|o| o - clock);
                    let behind = match (s.ally_networth, s.enemy_networth) { (Some(a), Some(e)) if a > 0 && e > 0 => (e as f32) >= a as f32 * (1.0 + t.behind_pct), _ => false };
                    if many_dead {
                        let dead = team.as_ref().map(|p| p.allies_dead.len()).unwrap_or(0);
                        let reason = format!("{} of us are dead", count_word(dead));
                        out.push(self.cue("skip", None, skip_key.clone(), &[("reason", reason)]));
                    } else if hurt || lane.is_none() && secs.is_some_and(|x| x > t.lane_wait_secs) {
                        // Hurt: no call into the point. Lane unknown right after the text: the
                        // minimap ping on the bridge follows within seconds (rec17 11:01 / 11:03).
                    } else if behind && phase != "contested" {
                        out.push(self.cue("behind", None, go_key.clone(), &[("where_short", where_short.clone())]));
                    } else if phase != "contested" && (secs.is_some() || phase == "open") {
                        // Without a full top-bar read the "N of us are up" part is left out.
                        let up = alive.map(|a| format!("{} of us are up,", count_word(a))).unwrap_or_default();
                        let with = Self::with(s, at, t.near_site);
                        let vars = [("where", where_.clone()), ("secs", secs.unwrap_or(0).to_string()), ("up", up), ("with", with)];
                        out.push(self.cue("go", secs.is_none().then_some(0), go_key.clone(), &vars));
                    }
                }
                // They are alone in the point: our progress flat, theirs climbing.
                let stop_key = format!("plan:rift_stop:{n}");
                // (Ours held at 95%+ is not "alone": a lone team there would have claimed it.)
                if phase == "open" && !hurt && r.enemy_rising && !r.ally_rising && r.ally_progress.is_none_or(|p| p < 95) && !said(&stop_key) && !said(&skip_key) && !many_dead
                    && r.enemy_progress.is_some_and(|p| (t.stop_min_pct..=t.stop_max_pct).contains(&p)) {
                    out.push(self.cue("stop", None, stop_key, &[]));
                }
                let orbs = format!("plan:rift_orbs:{n}");
                if phase == "contested" && r.contested_secs.is_some_and(|c| c >= t.draw_warn_secs && c < t.draw_secs) && !said(&orbs) {
                    let secs = t.draw_secs.saturating_sub(r.contested_secs.unwrap_or(0)).max(3);
                    out.push(self.cue("draw", None, orbs, &[("secs", secs.to_string())]));
                }
            }
            "draw" => {
                let orbs = format!("plan:rift_orbs:{n}");
                if !said(&orbs) { out.push(self.cue("draw", Some(0), orbs, &[])); }
            }
            "captured" | "lost" => {
                let r = r.expect("phase from a rift");
                let fresh = r.ended_at.is_some_and(|e| clock <= e + t.push_max_secs);
                if fresh && phase == "captured" {
                    let key = format!("plan:rift_push:{n}");
                    let where_ = lane.map(|l| format!("in {}", lane_name(l))).unwrap_or_else(|| "are ours".into());
                    if !said(&key) { out.push(self.cue("ours", None, key, &[("where", where_)])); }
                } else if fresh {
                    let key = format!("rift:defend:{n}");
                    let w = lane.map(|l| lane_name(l).to_string()).unwrap_or_else(|| "the Rift's lane".into());
                    if !said(&key) { out.push(self.cue("theirs", None, key, &[("where_short", w)])); }
                }
            }
            _ => {}
        }
        out
    }
    fn spoken(&self, cue: &Cue) {
        if cue.key.starts_with("plan:rift") || cue.key.starts_with("rift:") { self.st.borrow_mut().said.insert(cue.key.clone()); }
    }
}
