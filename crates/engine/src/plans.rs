//! Plans ahead instead of readouts (docs/coach-review-13.md items 3, 6, 7 and 8):
//! - objective plans 25-45 s before the Urn, the Guardian / Walker resistance drops and the
//!   Midboss respawn, or when enough enemies are dead for the Midboss (`plan:`): who goes, where,
//!   and what your hero does there;
//! - lead protection (`lead:`): a falling lead, an enemy Midboss (Rejuvenator) or a fed enemy;
//! - low health after a won fight (`reset:`): heal first, then the objective;
//! - named threats (`threat:`): the enemy hero close to you or missing near you, its dangerous
//!   ability and your counter-play.
//!
//! Evidence rules (data/coaching/plans.json, sources in docs/strategy): times are wiki facts; the
//! team picture comes from the top bar (all twelve slots); an enemy without a health bar is
//! "missing", never assumed far away. Snapshot fields the vision may not fill yet (structures,
//! midboss_taken_by) are used when present and replaced by heuristics when empty.
use crate::{laning::Source, positioning::TeamPicture, review, Cue, Priority, Rule};
use coach_core::{GameSnapshot, Lane, Pt, ZoneArea};
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

#[derive(Clone, Deserialize)]
pub struct Thresholds {
    pub plan_ahead_max: u32, pub plan_ahead_min: u32,
    pub near_radius: f32, pub enemy_half_y: f32, pub missing_min_clock: u32,
    pub missing_min_secs: u32, pub missing_max_secs: u32, pub threat_repeat_secs: u32, pub threat_gap_secs: u32,
    pub lead_min: i64, pub lead_drop: i64, pub lead_window_secs: u32, pub lead_gap_secs: u32,
    pub fed_ratio: f32, pub fed_reads: u32, pub fed_secs: u32, pub fed_min_clock: u32, pub fed_min_networth: u32, pub fed_sum_tolerance: f32,
    pub midboss_swing: i64, pub midboss_window_before: u32, pub midboss_window_after: u32, pub midboss_dead_margin: usize,
    pub window_dead: usize, pub window_secs: u32, pub window_allies_alive: usize,
    pub reset_hp: f32, pub reset_min_hp: f32, pub reset_dead: usize, pub reset_rearm_secs: u32,
}
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub family: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct RoleRow { pub hero: String, pub role: String }
#[derive(Clone, Deserialize)]
pub struct Jobs { pub urn: String, pub siege: String, pub midboss: String }
#[derive(Clone, Deserialize)]
pub struct Threat { pub hero: String, pub ability: String, pub reach: String, pub danger: String, pub counter: HashMap<String, String>,
    /// Counter-item need (data/coaching/review.json "counters": spirit / debuff / cc / bullet) when
    /// this ability tops a death panel; empty = no item answer.
    #[serde(default)] pub need: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub roles: Vec<RoleRow>, pub jobs: HashMap<String, Jobs>,
    pub jobs_sources: Vec<Source>, pub threats: Vec<Threat>, pub cards: Vec<Card> }

/// Who took a Midboss kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taker { Ally, Enemy, Unknown }

#[derive(Debug, Clone)]
struct Kill { clock: u32, taker: Option<Taker> }

#[derive(Default)]
struct State {
    clock: u32,
    votes: HashMap<String, u32>,
    /// Accepted (clock, ally - enemy) team souls samples; a jump is accepted once a second read agrees.
    lead: Vec<(u32, i64)>, last_nw: Option<(u32, u32)>, pending_nw: Option<(u32, u32)>,
    /// Raw team totals and top-bar dead counts of the last ~90 s (Midboss taker heuristic).
    nw_hist: Vec<(u32, Option<u32>, Option<u32>)>, dead_hist: Vec<(u32, usize, usize)>,
    lead_said_at: Option<u32>, falling_peak: i64, fed_said: Vec<String>, midboss_said: Vec<usize>,
    /// Fed candidate: (hero, frames agreeing, since clock, souls); clock of the last fed call.
    fed: Option<(String, u32, u32, u32)>, fed_at: Option<u32>,
    mb_up_seen: bool, mb_up_at: Option<u32>, mb_first_false: Option<u32>, kills: Vec<Kill>, taken_prev: Option<String>,
    /// Enemy heroes without a health bar since (clock), their last known lane.
    missing_since: HashMap<String, u32>, last_lane: HashMap<String, Lane>,
    threat_said: HashMap<String, u32>, threat_last: Option<u32>,
    /// Death-panel damage per enemy hero: this match, and the current death (largest panel read).
    damage: HashMap<String, u32>, cur_death: HashMap<String, u32>, was_dead: bool,
    reset_said_at: Option<u32>,
    last_you: Option<(u32, Pt)>,
}

pub struct PlanCoach { catalog: Arc<Catalog>, st: RefCell<State> }
/// The shipped policy (data/coaching/plans.json).
pub fn catalog() -> Arc<Catalog> {
    static C: std::sync::OnceLock<Arc<Catalog>> = std::sync::OnceLock::new();
    C.get_or_init(|| Arc::new(serde_json::from_str(include_str!("../../../data/coaching/plans.json")).expect("shipped plans policy"))).clone()
}
impl Default for PlanCoach {
    fn default() -> Self { Self { catalog: catalog(), st: Default::default() } }
}
impl PlanCoach {
    pub fn catalog(&self) -> &Catalog { &self.catalog }
}
/// Objective timers whose readout the plans replace ("Soul Urn in 45 seconds" becomes the Urn
/// plan). Register timers through this when the plan coach is registered, so the same objective
/// is not announced twice.
pub const PLANNED_TIMERS: &[&str] = &["Soul Urn", "Power-ups"];
pub fn unplanned_timings(json: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<Vec<serde_json::Value>>(json.trim_start_matches('\u{feff}')) else { return json.to_string() };
    v.retain(|t| t.get("name").and_then(|n| n.as_str()).is_none_or(|n| !PLANNED_TIMERS.contains(&n)));
    serde_json::to_string(&v).unwrap_or_else(|_| json.to_string())
}

fn k(v: i64) -> String { format!("{}", (v as f64 / 1000.0).round() as i64) }
fn lane_name(l: Lane) -> &'static str { crate::laning::lane_name(l) }
fn dist(a: Pt, b: Pt) -> f32 { (a.x - b.x).hypot(a.y - b.y) }
fn hp_ratio(s: &GameSnapshot) -> Option<f32> { s.hp.zip(s.max_hp).filter(|(h, m)| *m >= 100 && h <= m).map(|(h, m)| h as f32 / m as f32) }
fn capitalize(s: &str) -> String { let mut c = s.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }
fn enemy_id(h: &coach_core::HeroStatus) -> String { h.hero.clone().unwrap_or_else(|| format!("slot{}", h.slot)) }

impl PlanCoach {
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("plans card") }
    fn cue(&self, id: &str, key: String, vars: &[(&str, String)]) -> Cue {
        let c = self.card(id);
        let mut text = c.speech.clone();
        for (k, v) in vars { text = text.replace(&format!("{{{k}}}"), v); }
        let priority = match c.priority.as_str() { "high" => Priority::High, "low" => Priority::Low, _ => Priority::Normal };
        Cue { key, text, priority, cooldown_secs: c.cooldown_secs }
    }
    /// Your hero's role ("long_range" / "default"), by majority over the match.
    fn role(&self, st: &State) -> &str {
        let me = st.votes.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(h, _)| h.as_str());
        me.and_then(|h| self.catalog.roles.iter().find(|r| r.hero == h)).map(|r| r.role.as_str()).unwrap_or("default")
    }
    fn jobs(&self, st: &State) -> &Jobs { let r = self.role(st); self.catalog.jobs.get(r).or_else(|| self.catalog.jobs.get("default")).expect("default jobs") }

    // ---------------------------------------------------------------- observation
    fn observe(&self, s: &GameSnapshot, st: &mut State, clock: u32) {
        if let Some(h) = &s.hero { *st.votes.entry(h.clone()).or_default() += 1; }
        if s.dead { st.last_you = None; } else if let Some(me) = s.minimap.as_ref().and_then(|m| m.you) { st.last_you = Some((clock, me)); }
        // Team souls: one team at under half the other's is a digit drop; a >8% jump is accepted
        // only when the next read agrees (a misread holds for one frame, a real swing stays).
        if let (Some(a), Some(e)) = (s.ally_networth, s.enemy_networth) {
            if a > 0 && e > 0 && a.min(e) * 2 >= a.max(e) {
                let jump = |p: (u32, u32)| a.abs_diff(p.0) * 100 > p.0 * 8 || e.abs_diff(p.1) * 100 > p.1 * 8;
                let near = |p: (u32, u32)| a.abs_diff(p.0) * 100 <= p.0 * 3 && e.abs_diff(p.1) * 100 <= p.1 * 3;
                let accept = match (st.last_nw, st.pending_nw) {
                    (None, _) => true,
                    (Some(p), _) if !jump(p) => true,
                    (_, Some(q)) if near(q) => true,
                    _ => { st.pending_nw = Some((a, e)); false }
                };
                if accept {
                    st.pending_nw = None; st.last_nw = Some((a, e));
                    if st.lead.last().is_none_or(|l| l.0 != clock) { st.lead.push((clock, a as i64 - e as i64)); }
                }
            }
        }
        st.lead.retain(|l| l.0 + 360 >= clock);
        st.nw_hist.push((clock, s.ally_networth, s.enemy_networth)); st.nw_hist.retain(|x| x.0 + 90 >= clock);
        let team = TeamPicture::from(s);
        if let Some(p) = &team { st.dead_hist.push((clock, p.allies_dead.len(), p.enemies_dead.len())); }
        st.dead_hist.retain(|x| x.0 + 90 >= clock);
        self.observe_midboss(s, st, clock);
        self.observe_fed(s, st, clock);
        // Enemy heroes: missing since, last lane seen.
        for h in s.hero_status.iter().filter(|h| !h.ally) {
            let id = enemy_id(h);
            if h.dead == Some(true) { st.missing_since.remove(&id); continue; }
            match h.bar_visible { Some(false) => { st.missing_since.entry(id.clone()).or_insert(clock); } Some(true) => { st.missing_since.remove(&id); } None => {} }
            if let Some(l) = h.lane { st.last_lane.insert(id, l); }
        }
        // Death panels: who hurt you this match (largest read per death).
        if s.dead {
            let enemies: Vec<String> = if s.enemy_heroes.len() >= 3 { s.enemy_heroes.clone() } else { s.hero_status.iter().filter(|h| !h.ally).filter_map(|h| h.hero.clone()).collect() };
            let mut cur: HashMap<String, u32> = HashMap::new();
            for r in &s.death_damage { if let Some(h) = review::game_data().ability_hero(&r.source, &enemies) { *cur.entry(h).or_default() += r.damage; } }
            for (h, d) in cur { let e = st.cur_death.entry(h).or_default(); *e = (*e).max(d); }
            st.was_dead = true;
        } else if st.was_dead && s.hp.is_some() {
            for (h, d) in std::mem::take(&mut st.cur_death) { *st.damage.entry(h).or_default() += d; }
            st.was_dead = false;
        }
    }

    fn observe_midboss(&self, s: &GameSnapshot, st: &mut State, clock: u32) {
        let t = &self.catalog.thresholds;
        if let Some(m) = s.minimap.as_ref().filter(|m| m.observations >= 3) {
            if m.midboss_up || m.midboss_back_ms.is_some_and(|b| m.midboss_down_ms.is_none_or(|d| b < d)) {
                st.mb_up_seen = true; st.mb_up_at = Some(clock); st.mb_first_false = None;
            } else if st.mb_up_seen {
                // The icon gone for 10 s after being seen: a kill (at the tracker's time when given).
                let at = m.midboss_down_ms.map(|ms| clock.saturating_sub((ms / 1000) as u32)).unwrap_or(clock);
                let first = *st.mb_first_false.get_or_insert(at);
                // The boss respawns 5+ minutes after a kill: an icon flicker inside that time is
                // not a second kill (rec8 46:32 / 47:57).
                if clock >= first + 10 {
                    if st.kills.last().is_none_or(|k| first >= k.clock + 240) { st.kills.push(Kill { clock: first, taker: None }); }
                    st.mb_up_seen = false; st.mb_first_false = None;
                }
            }
        }
        // An explicit taker from the vision wins over the heuristic.
        if let Some(by) = s.midboss_taken_by.as_deref().filter(|b| st.taken_prev.as_deref() != Some(*b)) {
            st.taken_prev = Some(by.to_string());
            let taker = if by == "enemy" { Taker::Enemy } else if by == "ally" { Taker::Ally } else { Taker::Unknown };
            match st.kills.last_mut().filter(|k| k.clock + 120 >= clock && k.taker.is_none_or(|t| t == Taker::Unknown)) {
                Some(k) => k.taker = Some(taker),
                None => { st.kills.push(Kill { clock, taker: Some(taker) }); st.mb_up_seen = false; }
            }
        }
        // Heuristic taker once the window after the kill has been seen: the team with more heroes
        // dead could not take it; else the team whose souls jumped by the bounty did.
        let (nw, dead) = (st.nw_hist.clone(), st.dead_hist.clone());
        for kill in st.kills.iter_mut().filter(|k| k.taker.is_none() && clock >= k.clock + t.midboss_window_after) {
            let lo = kill.clock.saturating_sub(t.midboss_window_before);
            let hi = kill.clock + t.midboss_window_after;
            let window = |pick: fn(&(u32, Option<u32>, Option<u32>)) -> Option<u32>| {
                let vals: Vec<(u32, u32)> = nw.iter().filter(|x| x.0 >= lo && x.0 <= hi).filter_map(|x| pick(x).map(|v| (x.0, v))).collect();
                Some(vals.last()?.1 as i64 - vals.first()?.1 as i64)
            };
            let swing = window(|x| x.2).zip(window(|x| x.1)).map(|(de, da)| de - da);
            let near_dead = dead.iter().filter(|x| x.0 + 10 >= kill.clock && x.0 <= kill.clock + 3)
                .fold(None, |acc: Option<(usize, usize)>, x| Some(acc.map_or((x.1, x.2), |a| (a.0.max(x.1), a.1.max(x.2)))));
            kill.taker = Some(match (near_dead, swing) {
                (Some((ad, ed)), _) if ed >= ad + t.midboss_dead_margin => Taker::Ally,
                (Some((ad, ed)), _) if ad >= ed + t.midboss_dead_margin => Taker::Enemy,
                (_, Some(w)) if w >= t.midboss_swing => Taker::Enemy,
                (_, Some(w)) if w <= -t.midboss_swing => Taker::Ally,
                _ => Taker::Unknown,
            });
        }
    }

    /// One enemy at `fed_ratio` x the average of the other five (all six read, their sum within
    /// tolerance of the team total), for `fed_reads` frames over `fed_secs`.
    fn observe_fed(&self, s: &GameSnapshot, st: &mut State, clock: u32) {
        let t = &self.catalog.thresholds;
        let en: Vec<(&str, u32)> = s.hero_status.iter().filter(|h| !h.ally).filter_map(|h| Some((h.hero.as_deref()?, h.networth?))).collect();
        if en.len() != 6 { return; }
        let sum: u32 = en.iter().map(|x| x.1).sum();
        if s.enemy_networth.is_some_and(|tot| (sum as f32 - tot as f32).abs() > tot as f32 * t.fed_sum_tolerance) { return; }
        let top = *en.iter().max_by_key(|x| x.1).expect("six");
        let avg = (sum - top.1) as f32 / 5.0;
        let fed = clock >= t.fed_min_clock && top.1 >= t.fed_min_networth && top.1 as f32 >= avg * t.fed_ratio;
        st.fed = match (fed, st.fed.take()) {
            (false, _) => None,
            (true, Some((h, n, since, _))) if h == top.0 => Some((h, n + 1, since, top.1)),
            (true, _) => Some((top.0.to_string(), 1, clock, top.1)),
        };
    }

    // ---------------------------------------------------------------- calls
    fn lead_calls(&self, st: &State, clock: u32) -> Vec<Cue> {
        let t = &self.catalog.thresholds;
        // Enemy Midboss: Rejuvenator for three minutes. A new state: said even right after another
        // lead call (rec13 39:03 lead falling, 39:38 their Midboss).
        if let Some((i, _)) = st.kills.iter().enumerate().rev().find(|(i, k)| k.taker == Some(Taker::Enemy) && clock < k.clock + 150 && !st.midboss_said.contains(i)) {
            return vec![self.cue("lead_midboss", format!("lead:midboss:{i}"), &[])];
        }
        if st.lead_said_at.is_some_and(|at| clock < at + t.lead_gap_secs) { return vec![]; }
        let now = st.lead.last().filter(|l| l.0 + 30 >= clock).map(|l| l.1);
        let data = review::game_data();
        // Fed enemy.
        // Fed enemy: one call per 10 minutes (a top-bar slot misread can crown another hero: rec8
        // named Pocket, then Lash and Nurse Harrow within 5 minutes).
        if let Some((hero, n, since, nw)) = &st.fed {
            if *n >= t.fed_reads && clock >= since + t.fed_secs && !st.fed_said.contains(hero) && st.fed_at.is_none_or(|at| clock >= at + 600) {
                let name = data.hero_name(hero);
                return vec![match now.filter(|l| *l >= t.lead_min) {
                    Some(l) => self.cue("lead_fed", format!("lead:fed:{hero}"), &[("lead", k(l)), ("hero", name)]),
                    None => self.cue("fed", format!("lead:fed:{hero}"), &[("hero", name), ("nw", k(*nw as i64))]),
                }];
            }
        }
        // A lead of lead_min+ two to three minutes ago, lead_drop lower now, still ahead.
        if let Some(now) = now.filter(|n| *n > 0) {
            let w = t.lead_window_secs;
            let past = st.lead.iter().filter(|l| l.0 + w + 30 >= clock && l.0 + w <= clock).map(|l| l.1).max();
            if let Some(past) = past.filter(|p| *p >= t.lead_min && p - now >= t.lead_drop) {
                if st.falling_peak == 0 || past >= st.falling_peak + 10_000 {
                    return vec![self.cue("lead_falling", format!("lead:falling:{}", k(past)), &[("peak", k(past)), ("now", k(now))])];
                }
            }
        }
        vec![]
    }

    fn midboss_up(s: &GameSnapshot, st: &State, clock: u32) -> bool {
        st.mb_up_seen && st.mb_up_at.is_some_and(|at| at + 20 >= clock) || s.minimap.as_ref().is_some_and(|m| m.midboss_up)
    }

    fn reset_call(&self, s: &GameSnapshot, st: &State, clock: u32, team: Option<&TeamPicture>) -> Option<Cue> {
        let t = &self.catalog.thresholds;
        let p = team?;
        let hp = hp_ratio(s).filter(|_| s.health_observations >= 1)?;
        if !(hp >= t.reset_min_hp && hp < t.reset_hp) { return None; }
        if p.enemies_dead.len() < t.reset_dead || p.enemies_dead.len() <= p.allies_dead.len() { return None; }
        // Your marker is often lost mid-fight (rec13 29:17): the last one seen in 15 s counts.
        let map = s.minimap.as_ref();
        let you = map.and_then(|m| m.you).or(st.last_you.filter(|y| y.0 + 15 >= clock).map(|y| y.1));
        let theirs = you.is_some_and(|me| me.y < t.enemy_half_y)
            || map.is_some_and(|m| m.zone_area == Some(ZoneArea::EnemyBase));
        if !theirs || st.reset_said_at.is_some_and(|at| clock < at + t.reset_rearm_secs) { return None; }
        let objective = if Self::midboss_up(s, st, clock) && clock >= 900 { "Midboss" } else { "the objective" };
        Some(self.cue("reset", "reset:fight".into(), &[("pct", format!("{:.0}", hp * 100.0)), ("objective", objective.into())]))
    }

    fn threat_call(&self, s: &GameSnapshot, st: &State, clock: u32) -> Option<Cue> {
        let t = &self.catalog.thresholds;
        if st.threat_last.is_some_and(|at| clock < at + t.threat_gap_secs) { return None; }
        let map = s.minimap.as_ref()?;
        let me = map.you?;
        // Minimap rings carry no identity: an enemy with a visible top-bar health bar but no read
        // position counts as close only when there are at least as many enemy rings near you as
        // enemies seen at all (then every seen enemy is one of the rings near you).
        let rings_near = map.heroes.iter().filter(|h| !h.ally && h.confirmed() && h.visible() && h.distance(me) <= t.near_radius).count();
        let seen = s.hero_status.iter().filter(|h| !h.ally && h.dead != Some(true) && h.bar_visible == Some(true)).count();
        let track_near = rings_near >= 1 && rings_near >= seen;
        // "Missing near you" only after laning (in lane, the missing-enemy call of positioning.rs
        // covers it) and with you in their half, where a missing diver can reach you unseen.
        let forward = clock >= t.missing_min_clock && (me.y < t.enemy_half_y || map.zone_area == Some(ZoneArea::EnemyBase));
        let role = self.role(st);
        // (close?, score) per candidate: close ones first, by damage done to you then souls;
        // missing ones by how recently they vanished (fresh = on the way).
        let mut best: Option<((u8, i64), &Threat, &str)> = None;
        for h in s.hero_status.iter().filter(|h| !h.ally && h.dead != Some(true)) {
            let Some(hero) = h.hero.as_deref() else { continue };
            let Some(threat) = self.catalog.threats.iter().find(|x| x.hero == hero) else { continue };
            if st.threat_said.get(hero).is_some_and(|at| clock < at + t.threat_repeat_secs) { continue; }
            let close = h.bar_visible == Some(true) && match h.position.filter(|_| h.position_observations >= 1) {
                Some(p) => dist(p, me) <= t.near_radius,
                None => track_near,
            };
            let missing = threat.reach == "any" && forward && st.missing_since.get(hero).is_some_and(|since| {
                let secs = clock.saturating_sub(*since);
                secs >= t.missing_min_secs && secs <= t.missing_max_secs
            }) && st.last_lane.get(hero).is_none_or(|l| map.your_lane.is_none_or(|y| y == *l));
            let score = if close {
                (0u8, -(st.damage.get(hero).copied().unwrap_or(0) as i64 * 100 + h.networth.unwrap_or(0) as i64 / 100))
            } else if missing {
                (1u8, clock.saturating_sub(st.missing_since[hero]) as i64)
            } else { continue };
            if best.as_ref().is_none_or(|b| score < b.0) { best = Some((score, threat, if close { "is close" } else { "is missing" })); }
        }
        let (_, threat, state) = best?;
        let counter = threat.counter.get(role).or_else(|| threat.counter.get("default")).cloned().unwrap_or_default();
        Some(self.cue("threat", format!("threat:{}", threat.hero), &[("hero", review::game_data().hero_name(&threat.hero)), ("state", state.into()),
            ("danger", threat.danger.clone()), ("counter", capitalize(&counter))]))
    }

    /// The lane to push for a structure plan: the most pushed lane whose enemy structure of that
    /// kind still stands (when structures are read), else your lane.
    fn siege_lane(s: &GameSnapshot, kind: &str) -> Option<Lane> {
        let map = s.minimap.as_ref();
        let standing = |l: Lane| s.structures.iter().filter(|x| !x.own && x.lane == Some(l) && x.kind == kind).all(|x| x.alive);
        let mut lanes: Vec<(Lane, f32)> = [Lane::Green, Lane::Blue, Lane::Yellow].into_iter().filter(|l| standing(*l))
            .filter_map(|l| map.and_then(|m| m.lane(l).push).map(|p| (l, p))).collect();
        lanes.sort_by(|a, b| b.1.total_cmp(&a.1));
        lanes.first().map(|l| l.0).or_else(|| crate::laning::focus(s)).filter(|l| standing(*l))
    }

    fn plan_calls(&self, s: &GameSnapshot, st: &State, clock: u32, team: Option<&TeamPicture>) -> Vec<Cue> {
        let t = &self.catalog.thresholds;
        let jobs = self.jobs(st);
        let soon = |at: u32| at > clock && (t.plan_ahead_min..=t.plan_ahead_max).contains(&(at - clock));
        let mut out = vec![];
        // Midboss respawn: 7, 6, then 5 minutes after each kill. A team call, said even while you
        // heal (rec13 36:07-36:30 at 22-39% health), down to 15 s before it.
        if let Some(last) = st.kills.last() {
            let at = last.clock + [420, 360, 300][(st.kills.len() - 1).min(2)];
            if at > clock && (15..=t.plan_ahead_max).contains(&(at - clock)) && !Self::midboss_up(s, st, clock) {
                out.push(self.cue("midboss_back", format!("plan:midboss_back:{}", st.kills.len()), &[("secs", (at - clock).to_string()), ("job", jobs.midboss.clone())]));
            }
        }
        if hp_ratio(s).is_some_and(|h| h < 0.5) { return out; }
        // Power-ups at 5:00 (from 10:00 they spawn with the Urn, whose plan is said instead).
        if soon(300) { out.push(self.cue("powerups", "plan:powerups:300".into(), &[("secs", (300 - clock).to_string())])); }
        // Soul Urn: 10:00, then every 5 minutes; Yellow first, then alternating.
        let next = if clock < 600 { 600 } else { 600 + (clock - 600).div_ceil(300) * 300 };
        if soon(next) {
            let n = (next - 600) / 300;
            let lane = if n % 2 == 0 { Lane::Yellow } else { Lane::Green };
            let secs = (next - clock).to_string();
            let map = s.minimap.as_ref();
            let yellow_right = map.and_then(|m| m.lanes_mirrored) != Some(true);
            let side_x = if (lane == Lane::Yellow) == yellow_right { 0.85 } else { 0.15 };
            let walk = if map.and_then(|m| m.you).is_some_and(|me| (me.x - side_x).abs() > 0.5) { " Start walking now." } else { "" };
            let key = format!("plan:urn:{next}");
            let (ad, ed) = team.map(|p| (p.allies_dead.len(), p.enemies_dead.len())).unwrap_or((0, 0));
            out.push(if ed >= ad + 2 { self.cue("urn_free", key, &[("secs", secs), ("lane", lane_name(lane).into()), ("count", ed.to_string()), ("walk", walk.into())]) }
                else if ad >= ed + 2 { self.cue("urn_skip", key, &[("secs", secs), ("count", ad.to_string())]) }
                else { self.cue("urn_contest", key, &[("secs", secs), ("lane", lane_name(lane).into()), ("job", jobs.urn.clone()), ("walk", walk.into())]) });
        }
        // Guardian (12:00) and Walker (18:00) resistance drops.
        for (at, id, kind) in [(720, "guardian_weak", "guardian"), (1080, "walker_weak", "walker")] {
            if !soon(at) { continue; }
            let lane = Self::siege_lane(s, kind).map(|l| format!("{} lane", lane_name(l))).unwrap_or_else(|| "your lane".into());
            out.push(self.cue(id, format!("plan:{kind}"), &[("lane", lane), ("job", jobs.siege.clone())]));
        }
        // Midboss window: enough of them dead long enough, and enough of us alive.
        if let Some(p) = team {
            let window = p.enemies_dead.iter().flatten().min().copied();
            if clock >= 900 && Self::midboss_up(s, st, clock) && p.enemies_dead.len() >= t.window_dead && 6 - p.allies_dead.len().min(6) >= t.window_allies_alive {
                if let Some(secs) = window.filter(|w| *w >= t.window_secs) {
                    out.push(self.cue("midboss_window", format!("plan:midboss_window:{}", st.kills.len()), &[("count", capitalize(&count_word(p.enemies_dead.len()))), ("secs", secs.to_string()), ("job", jobs.midboss.clone())]));
                }
            }
        }
        out
    }
}
fn count_word(n: usize) -> String { ["zero", "one", "two", "three", "four", "five", "six"].get(n).map(|s| s.to_string()).unwrap_or_else(|| n.to_string()) }

impl Rule for PlanCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        let Some(clock) = s.game_clock_secs else { return vec![] };
        let mut st = self.st.borrow_mut();
        if clock + 120 < st.clock { *st = State::default(); }
        st.clock = clock;
        self.observe(s, &mut st, clock);
        let mut out = self.lead_calls(&st, clock);
        let playing = !s.dead && s.screen.as_deref().is_none_or(|v| v == "Gameplay");
        if playing {
            let team = TeamPicture::from(s);
            out.extend(self.reset_call(s, &st, clock, team.as_ref()));
            out.extend(self.threat_call(s, &st, clock));
            out.extend(self.plan_calls(s, &st, clock, team.as_ref()));
        }
        out
    }
    fn spoken(&self, cue: &Cue) {
        let mut st = self.st.borrow_mut();
        let now = st.clock;
        let mut parts = cue.key.splitn(3, ':');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("threat"), Some(hero), _) => { st.threat_said.insert(hero.to_string(), now); st.threat_last = Some(now); }
            (Some("reset"), _, _) => st.reset_said_at = Some(now),
            (Some("lead"), Some(kind), rest) => {
                st.lead_said_at = Some(now);
                match (kind, rest) {
                    ("midboss", Some(i)) => { if let Ok(i) = i.parse() { st.midboss_said.push(i); } }
                    ("fed", Some(h)) => { st.fed_said.push(h.to_string()); st.fed_at = Some(now); }
                    ("falling", Some(p)) => { if let Ok(p) = p.parse::<i64>() { st.falling_peak = p * 1000; } }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
