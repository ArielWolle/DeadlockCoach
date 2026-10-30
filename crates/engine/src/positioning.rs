//! Positional coaching from tracked minimap heroes plus the top-bar team picture: gank
//! warnings, numbers and health advantage or disadvantage around you, kill windows, missing
//! and unseen enemies, teammates to help, and team-wide windows while heroes are dead.
//!
//! Evidence rules: only confirmed tracks (seen on >= 2 observations) count; enemies that are
//! not visible are "missing", never assumed dead or far away; your own position must be a
//! confirmed player marker. Top-bar health is not tied to particular minimap rings: the rings
//! near you are some subset of the enemies with visible bars, so a group's health is used as a
//! guaranteed range (worst case for danger, best case only when every candidate is low).
use crate::{laning::Source, Cue, Priority, Rule};
use coach_core::{GameSnapshot, HeroTrack, Lane, MinimapState, Pt};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Deserialize)]
pub struct Thresholds { pub near_radius: f32, pub fight_radius: f32, pub watch_radius: f32, pub closing_speed: f32,
    pub missing_min_ms: u64, pub missing_max_ms: u64, pub help_max_distance: f32, pub healthy: f32,
    pub low_enemy: f32, pub low_ally: f32, pub strength_margin: f32, pub team_down: usize, pub unseen_forward: usize, pub forward_progress: f32 }
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub cards: Vec<Card> }

/// A positional recommendation with its observed basis, for display next to the spoken cue.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Advice { pub id: String, pub text: String, pub priority: Priority, pub cooldown_secs: u32, pub evidence: Vec<String>, pub sources: Vec<Source>,
    /// Minimap track ids the advice is about (missing enemies), for repeat suppression.
    #[serde(skip)] pub tracks: Vec<u32> }

/// What the top bar says about both teams (all twelve slots are required).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TeamPicture {
    pub enemies_alive: usize,
    /// Respawn seconds (when read) of dead heroes.
    pub enemies_dead: Vec<Option<u32>>,
    pub allies_dead: Vec<Option<u32>>,
    /// Alive enemies without a health bar: not seen by your team recently.
    pub enemies_unseen: usize,
    /// Health of alive enemies with a visible bar (None = bar seen, value unread).
    pub enemies_seen_hp: Vec<Option<f32>>,
    /// Health of alive allied slots (includes your own slot).
    pub allies_hp: Vec<Option<f32>>,
}
impl TeamPicture {
    /// None unless all twelve slots have a dead/alive read: a slot not read is no evidence for a
    /// count, so every count-based call stays silent rather than counting it alive.
    pub fn from(s: &GameSnapshot) -> Option<Self> {
        if !top_bar_read(s) { return None; }
        let mut t = TeamPicture::default();
        for h in &s.hero_status {
            let dead = h.dead == Some(true);
            match (h.ally, dead) {
                (true, true) => t.allies_dead.push(h.respawn_secs),
                (false, true) => t.enemies_dead.push(h.respawn_secs),
                (true, false) => t.allies_hp.push(h.health_ratio),
                (false, false) => {
                    t.enemies_alive += 1;
                    match h.bar_visible { Some(false) => t.enemies_unseen += 1, Some(true) => t.enemies_seen_hp.push(h.health_ratio), None => {} }
                }
            }
        }
        Some(t)
    }
}
/// Guaranteed (lowest, highest) total health of any `k` heroes chosen from `pool`. An unread value
/// counts as 0 for the low bound and 1 for the high bound; heroes beyond the pool count as unknown.
pub fn group_range(pool: &[Option<f32>], k: usize) -> (f32, f32) {
    let mut lo: Vec<f32> = pool.iter().map(|v| v.unwrap_or(0.0)).collect(); lo.sort_by(f32::total_cmp);
    let mut hi: Vec<f32> = pool.iter().map(|v| v.unwrap_or(1.0)).collect(); hi.sort_by(|a, b| b.total_cmp(a));
    let missing = k.saturating_sub(pool.len()) as f32;
    (lo.iter().take(k).sum(), hi.iter().take(k).sum::<f32>() + missing)
}

#[derive(Clone)]
pub struct PositionCoach { catalog: Arc<Catalog>, spoken: Spoken }
impl Default for PositionCoach {
    fn default() -> Self { Self { catalog: Arc::new(serde_json::from_str(include_str!("../../../data/coaching/positioning.json")).expect("shipped positioning policy")), spoken: Spoken::default() } }
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
fn respawn_note(v: &[Option<u32>]) -> String {
    v.iter().flatten().min().map(|s| format!(", the first back in about {s} seconds")).unwrap_or_default()
}

impl PositionCoach {
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("positioning card") }
    fn advice(&self, id: &str, vars: &[(&str, String)], evidence: Vec<String>) -> Advice {
        let c = self.card(id);
        let mut text = c.speech.clone();
        for (k, v) in vars { text = text.replace(&format!("{{{k}}}"), v); }
        let priority = match c.priority.as_str() { "high" => Priority::High, "low" => Priority::Low, _ => Priority::Normal };
        Advice { id: id.into(), text, priority, cooldown_secs: c.cooldown_secs, evidence, sources: c.sources.clone(), tracks: vec![] }
    }

    pub fn assess(&self, s: &GameSnapshot) -> Option<Advice> {
        if s.dead || s.screen.as_deref().is_some_and(|v| v != "Gameplay") || s.game_clock_secs.is_none() { return None; }
        let t = &self.catalog.thresholds;
        let team = TeamPicture::from(s);
        let health = s.hp.zip(s.max_hp).filter(|(hp, max)| *max >= 100 && hp <= max).map(|(hp, max)| hp as f32 / max as f32);
        let healthy = health.is_some_and(|h| h >= t.healthy) && s.health_observations >= 2;
        let hp_note = health.map(|h| format!("Your health: {:.0}%", h * 100.0)).unwrap_or("Your health is unread".into());
        let team_note = team.as_ref().map(|p| format!("Top bar: {} enemies alive ({} unseen), {} enemies dead, {} allies dead",
            p.enemies_alive, p.enemies_unseen, p.enemies_dead.len(), p.allies_dead.len()));

        let positional = s.minimap.as_ref().and_then(|map| map.you.map(|me| (map, me)));
        if let Some((map, me)) = positional {
            if let Some(a) = self.local(s, map, me, team.as_ref(), health, healthy, &hp_note, team_note.clone()) { return Some(a); }
        }
        // Team-wide windows from dead heroes (no position needed).
        let p = team.as_ref()?;
        let near_enemy = positional.is_some_and(|(map, me)| map.heroes.iter().any(|h| !h.ally && h.confirmed() && h.visible() && h.distance(me) <= t.near_radius));
        let ev = vec![team_note.clone().unwrap_or_default(), hp_note.clone()];
        // A window is only worth calling with time left: rec8 said "the first back in about 1
        // seconds" twice. Enemies count with a read timer of WINDOW_MIN_SECS or more (an unread
        // timer is not a window); teammates count unless their timer is nearly done.
        // The count is the heroes dead for the whole window it names (rec8 32:52 said "2 enemies"
        // with three dead), and the call needs a target you reach in time (rec8 29:41, 31:11).
        let window: Vec<u32> = p.enemies_dead.iter().flatten().copied().filter(|r| *r >= WINDOW_MIN_SECS).collect();
        if window.len() >= t.team_down && p.enemies_dead.len() > p.allies_dead.len() && !near_enemy && health.is_some_and(|h| h >= 0.5) {
            let secs = window.iter().copied().min().unwrap_or(0);
            if let Some(target) = window_target(s, secs, TRAVEL_SECS_PER_MAP, WINDOW_MARGIN_SECS) {
                return Some(self.advice("enemies_down", &[("count", count_word(window.len())), ("secs", secs.to_string()), ("target", target)], ev));
            }
        }
        let down: Vec<Option<u32>> = p.allies_dead.iter().copied().filter(|r| r.is_none_or(|r| r >= 8)).collect();
        if down.len() >= t.team_down && p.allies_dead.len() > p.enemies_dead.len() {
            return Some(self.advice("allies_down", &[("count", count(down.len(), "teammate")), ("respawn", respawn_note(&down))], ev));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn local(&self, s: &GameSnapshot, map: &MinimapState, me: Pt, team: Option<&TeamPicture>, health: Option<f32>, healthy: bool, hp_note: &str, team_note: Option<String>) -> Option<Advice> {
        let t = &self.catalog.thresholds;
        // Present = seen now, seen in the last 3 s (single-frame dropouts undercounted 121 of 298
        // rec3 frames), or last seen within 0.08 of you in the last 10 s: your own portrait covers
        // heroes standing on you (9 of 9 wrong rec3 fight calls were such overlaps).
        let live = |h: &&HeroTrack| h.confirmed() && (h.visible() || h.unseen_ms <= RECENT_MS || h.unseen_ms <= HIDDEN_MS && h.distance(me) <= HIDDEN_RADIUS);
        let enemies: Vec<&HeroTrack> = map.heroes.iter().filter(|h| !h.ally).filter(live).collect();
        let allies: Vec<&HeroTrack> = map.heroes.iter().filter(|h| h.ally).filter(live).collect();
        let near_e: Vec<&&HeroTrack> = enemies.iter().filter(|h| h.distance(me) <= t.near_radius).collect();
        let near_allies = allies.iter().filter(|h| h.distance(me) <= t.near_radius).count();
        let near_a = near_allies + 1; // + you
        // For "you are outnumbered / being ganked" only, a teammate hidden under your portrait
        // counts for 15 s (rec3 frame 72: unseen 11 s, still standing on you). Commit calls keep the
        // stricter count, so an old sighting can never talk you into a fight.
        let guard_a = near_a + map.heroes.iter().filter(|h| h.ally && h.confirmed() && !live(h)
            && h.unseen_ms <= HIDDEN_ALLY_MS && h.distance(me) <= HIDDEN_RADIUS).count();
        let mut ev_base = vec![hp_note.to_string()]; ev_base.extend(team_note);
        // Health ranges for groups of k enemies / k allies besides you (None = no top-bar data).
        let enemy_range = |k: usize| team.map(|p| group_range(&p.enemies_seen_hp, k)).unwrap_or((0.0, k as f32));
        let ally_range = |k: usize| team.map(|p| {
            // Drop the allied slot that looks like you (closest to your HUD health), if any.
            let mut pool = p.allies_hp.clone();
            if let Some(h) = health { if let Some(i) = pool.iter().enumerate().filter_map(|(i, v)| v.map(|v| (i, (v - h).abs()))).filter(|(_, d)| *d <= 0.08).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i) { pool.remove(i); } }
            group_range(&pool, k)
        }).unwrap_or((0.7 * k as f32, k as f32));
        let mine = health.unwrap_or(0.6);

        // 1. Gank: enemies inside the watch zone but outside your fight, moving toward you, unless
        //    every candidate is known to be low.
        let closing: Vec<&&HeroTrack> = enemies.iter().filter(|h| h.distance(me) > t.near_radius * 0.6
            && h.distance(me) <= t.watch_radius && h.closing_speed(me) >= t.closing_speed).collect();
        let threat = closing.len() + near_e.len();
        if !closing.is_empty() && threat >= guard_a {
            let (_, hi) = enemy_range(threat);
            let weak = team.is_some() && hi / threat as f32 <= t.low_enemy && healthy;
            if !weak {
                let from_lane = closing.iter().filter_map(|h| h.lane).find(|l| Some(*l) != map.your_lane);
                let direction = match (from_lane, closing.iter().all(|h| h.lane.is_some() && h.lane == map.your_lane)) {
                    (_, true) => "down your lane".to_string(),
                    (Some(l), _) => format!("from {} lane", lane_name(l)),
                    _ => "through the jungle".to_string(),
                };
                let mut ev: Vec<String> = closing.iter().map(|h| format!("Enemy marker {:.2} map away, closing at {:.3}/s", h.distance(me), h.closing_speed(me))).collect();
                ev.extend(ev_base.clone());
                return Some(self.advice("gank", &[("count", count(closing.len(), "enemy")), ("direction", direction)], ev));
            }
        }
        let k = near_e.len();
        let (e_lo, e_hi) = enemy_range(k);
        let (a_lo, _) = ally_range(near_allies);
        let our_floor = mine + a_lo;
        // 2. Outnumbered around you, unless the enemies there are certainly weaker than your group.
        if k >= 2 && k > guard_a && e_hi > our_floor {
            let mut ev = vec![format!("{k} enemy and {near_a} allied markers (including you) within {:.2}", t.near_radius),
                format!("Nearby enemy health total between {e_lo:.1} and {e_hi:.1} heroes' worth; your side at least {our_floor:.1}")];
            ev.extend(ev_base.clone());
            return Some(self.advice("outnumbered", &[("enemies", count(k, "enemy")), ("allies", if near_a == 1 { "just you".into() } else { format!("{near_a} of you") })], ev));
        }
        let recently_missing = map.heroes.iter().filter(|h| !h.ally && !h.visible() && h.unseen_ms <= t.missing_max_ms && h.distance(me) <= t.watch_radius).count();
        // 3. Kill window: every enemy that could be near you is low, and you are healthy.
        if k >= 1 && team.is_some() && healthy && e_hi / k as f32 <= t.low_enemy && (near_a >= k || k == 1) && recently_missing == 0 {
            let top = team.map(|p| p.enemies_seen_hp.iter().flatten().copied().fold(0.0f32, f32::max)).unwrap_or(1.0);
            let pct = ((top * 100.0 / 5.0).ceil() * 5.0).max(5.0) as u32;
            let mut ev = vec![format!("{k} enemy markers within {:.2}; every enemy with a visible health bar is at or below {:.0}%", t.near_radius, top * 100.0)];
            ev.extend(ev_base.clone());
            return Some(self.advice("kill_window", &[("count", count(k, "enemy")), ("is_are", if k == 1 { "is".into() } else { "are".into() }), ("pct", pct.to_string())], ev));
        }
        // 4. Missing: confirmed enemies last seen near you that left the minimap recently. Dead
        //    enemies are not missing: the count is capped by alive enemies not on the map.
        let mut missing: Vec<&HeroTrack> = map.heroes.iter().filter(|h| !h.ally && h.confirmed() && h.distance(me) <= t.watch_radius
            && (t.missing_min_ms..=t.missing_max_ms).contains(&h.unseen_ms) && !live(h)).collect();
        if let Some(p) = team { missing.truncate(p.enemies_alive.saturating_sub(enemies.len())); }
        let forward = map.your_progress.is_some_and(|p| p >= 0.45);
        if !missing.is_empty() && (forward || missing.len() >= 2) && near_e.is_empty() {
            let secs = missing.iter().map(|h| h.unseen_ms).min().unwrap_or(0) / 1000;
            let mut ev: Vec<String> = missing.iter().map(|h| format!("Enemy last seen {:.2} map away, {} s ago", h.distance(me), h.unseen_ms / 1000)).collect();
            ev.extend(ev_base.clone());
            let mut a = self.advice("missing", &[("count", count(missing.len(), "enemy")), ("seconds", secs.to_string())], ev);
            a.tracks = missing.iter().map(|h| h.id).collect();
            return Some(a);
        }
        // 5. Unseen: several alive enemies have no health bar while you are pushed up.
        if let Some(p) = team { if p.enemies_unseen >= t.unseen_forward && near_e.is_empty() && map.your_progress.is_some_and(|v| v >= t.forward_progress) {
            return Some(self.advice("unseen", &[("count", count(p.enemies_unseen, "enemy"))], ev_base.clone()));
        }}
        // 6. Numbers (and, when known, health) advantage where you are: 2v1 or a two-hero margin.
        let advantage = near_a >= 2 * k || near_a >= k + 2;
        let stronger = team.is_none() || our_floor >= t.strength_margin * e_hi;
        // Two or more enemies with unknown positions could join the fight.
        let accounted = team.is_none_or(|p| p.enemies_unseen <= 1);
        if k >= 1 && advantage && stronger && accounted && healthy && recently_missing == 0 {
            let mut ev = vec![format!("{near_a} allied markers (including you) vs {k} enemy within {:.2}", t.near_radius)];
            if team.is_some() { ev.push(format!("Your side at least {our_floor:.1} heroes' health vs at most {e_hi:.1}")); }
            ev.extend(ev_base.clone());
            return Some(self.advice("go_in", &[("allies", format!("{near_a} of you")), ("enemies", count(k, "enemy"))], ev));
        }
        // 7. A teammate fighting nearby: help if they are losing, join if they are winning.
        if near_e.is_empty() && healthy {
            // Enemies are counted in a wider circle than allies: a group arriving is part of the
            // fight (rec3 said "join, 2 against 1" next to five enemies), allies must be in it.
            let fight = allies.iter().filter(|a| (t.near_radius..=t.help_max_distance).contains(&a.distance(me))).filter_map(|a| {
                let foes = enemies.iter().filter(|e| e.distance(a.pos) <= t.fight_radius * 1.6).count();
                let friends = allies.iter().filter(|o| o.distance(a.pos) <= t.fight_radius).count();
                (foes > 0).then_some((*a, friends, foes))
            }).max_by(|x, y| (x.2 as i32 - x.1 as i32).cmp(&(y.2 as i32 - y.1 as i32)).then(y.0.distance(me).total_cmp(&x.0.distance(me))));
            // An even fight is not "trouble" (rec3: six "1 against 1" help calls during laning).
            if let Some((a, friends, foes)) = fight.filter(|(_, f, e)| e > f || f > e) {
                let place = area(a.pos, a.lane);
                let mut ev = vec![format!("{friends} allied vs {foes} enemy markers within {:.2} of a teammate {:.2} map away", t.fight_radius, a.distance(me))];
                ev.extend(ev_base.clone());
                let vars = [("area", place), ("allies", count(friends, "ally")), ("enemies", count(foes, "enemy"))];
                return Some(if foes > friends { self.advice("help", &vars, ev) } else { self.advice("join", &vars, ev) });
            }
        }
        let _ = s;
        None
    }
}

/// Minimap x of a lane: Blue in the centre, Yellow right unless the layout is mirrored.
pub fn lane_x(l: Lane, mirrored: Option<bool>) -> f32 {
    let yellow_right = mirrored != Some(true);
    match l { Lane::Blue => 0.5, Lane::Yellow => if yellow_right { 0.85 } else { 0.15 }, Lane::Green => if yellow_right { 0.15 } else { 0.85 } }
}
/// Rough minimap y of an enemy lane structure (their base at the top): outer Guardian, Walker,
/// base Guardian. A coach estimate for walking times only.
fn structure_y(i: usize) -> f32 { [0.3, 0.22, 0.14].get(i).copied().unwrap_or(0.14) }
/// The next standing enemy structure of a lane as read on the minimap, in lane order (outer
/// Guardian, Walker, base Guardian): (index in that order, spoken name). None when the lane's
/// structures were not read or all are down.
pub fn next_structure(s: &GameSnapshot, l: Lane) -> Option<(usize, &'static str)> {
    let lane: Vec<&coach_core::StructureState> = s.structures.iter().filter(|x| !x.own && x.lane == Some(l)).collect();
    if lane.is_empty() { return None; }
    let mut guardians = 0;
    for x in lane {
        let (order, name) = match x.kind.as_str() { "guardian" => { guardians += 1; if guardians == 1 { (0, "Guardian") } else { (2, "base Guardian") } } "walker" => (1, "Walker"), _ => continue };
        if x.alive { return Some((order, name)); }
    }
    None
}
/// Every enemy lane structure of some lane is down and their Shrines are read standing.
pub fn shrines_open(s: &GameSnapshot) -> bool {
    let lanes = [Lane::Green, Lane::Blue, Lane::Yellow];
    let lane_down = |l: Lane| { let v: Vec<_> = s.structures.iter().filter(|x| !x.own && x.lane == Some(l) && x.kind != "shrine").collect(); v.len() >= 3 && v.iter().all(|x| !x.alive) };
    lanes.into_iter().any(lane_down) && s.structures.iter().any(|x| !x.own && x.kind == "shrine" && x.alive)
}
/// What to do with an enemy-dead window of `secs` seconds, reachable from where you stand
/// (rec8 29:41 / 31:11 said "push Yellow" from Blue with 17-19 s left): your lane when you are on
/// one, else the nearest lane you reach with `margin` seconds to spare at `secs_per_map` seconds
/// per minimap width; its next standing enemy structure when that is reachable too. None when no
/// lane is reachable in time or your position is unknown.
pub fn window_target(s: &GameSnapshot, secs: u32, secs_per_map: f32, margin: u32) -> Option<String> {
    let m = s.minimap.as_ref()?;
    let me = m.you?;
    let budget = secs.saturating_sub(margin) as f32;
    let walk = |x: f32, y: f32| (me.x - x).hypot(me.y - y) * secs_per_map;
    // Joining a lane: its x at your height, or forward to its front when the front is ahead of you.
    let join = |l: Lane| { let front = m.lane(l).push.map(|p| 0.92 - 0.84 * p).unwrap_or(me.y); walk(lane_x(l, m.lanes_mirrored), front.min(me.y)) };
    let lane = match m.your_lane.filter(|l| join(*l) <= budget) {
        Some(l) => l,
        None => [Lane::Green, Lane::Blue, Lane::Yellow].into_iter().map(|l| (l, join(l))).filter(|x| x.1 <= budget).min_by(|a, b| a.1.total_cmp(&b.1))?.0,
    };
    let name = lane_name(lane);
    if let Some((i, structure)) = next_structure(s, lane) {
        if walk(lane_x(lane, m.lanes_mirrored), structure_y(i)) <= budget { return Some(format!("take their {name} {structure}")); }
    } else if shrines_open(s) && s.structures.iter().filter(|x| !x.own && x.lane == Some(lane) && x.kind != "shrine").count() >= 3 && walk(0.5, 0.1) <= budget {
        return Some("hit their Shrines".into());
    }
    Some(format!("push {name} lane with the team"))
}
/// The objective for a long window (our Rejuvenator): their Shrines when a lane is open to
/// them, else the most pushed lane into its next standing structure, else the most pushed lane.
pub fn push_target(s: &GameSnapshot) -> Option<String> {
    if shrines_open(s) { return Some("their Shrines".into()); }
    let m = s.minimap.as_ref();
    let mut lanes: Vec<(Lane, f32)> = [Lane::Green, Lane::Blue, Lane::Yellow].into_iter().map(|l| (l, m.and_then(|m| m.lane(l).push).unwrap_or(0.0))).collect();
    lanes.sort_by(|a, b| b.1.total_cmp(&a.1));
    if let Some((l, (_, st))) = lanes.iter().find_map(|(l, _)| next_structure(s, *l).map(|x| (*l, x))) { return Some(format!("{} lane into their {st}", lane_name(l))); }
    lanes.first().filter(|x| x.1 > 0.0).map(|x| format!("{} lane", lane_name(x.0)))
}

/// Seconds an enemy-dead window must still last to be worth a call.
pub const WINDOW_MIN_SECS: u32 = 15;
/// Seconds to walk one minimap width (a coach estimate) and the spare seconds a window target
/// must leave on arrival.
pub const TRAVEL_SECS_PER_MAP: f32 = 45.0;
pub const WINDOW_MARGIN_SECS: u32 = 3;
/// "Two", "Three"... for spoken counts at the start of a sentence.
pub fn count_word(n: usize) -> String { match n { 1 => "One".into(), 2 => "Two".into(), 3 => "Three".into(), 4 => "Four".into(), 5 => "Five".into(), 6 => "Six".into(), n => n.to_string() } }
/// One team-wide window (two or more heroes of a team dead) as named so far. A window is said
/// once: again only when none of the heroes named is still dead (a new window), and it ends when
/// fewer than two are dead for WINDOW_GAP_SECS (top-bar flicker does not end it).
#[derive(Clone, Default, Debug)]
pub struct WindowMemo { said: Vec<usize>, current: Vec<usize>, open_at: u32 }
pub const WINDOW_GAP_SECS: u32 = 8;
impl WindowMemo {
    /// This frame's dead slots (`None` = top bar not read); true when a call would be new.
    pub fn observe(&mut self, dead: Option<Vec<usize>>, clock: u32, min: usize) -> bool {
        if let Some(d) = dead {
            if d.len() >= min { self.open_at = clock; } else if clock > self.open_at + WINDOW_GAP_SECS || clock < self.open_at { self.said.clear(); }
            self.current = d;
        }
        self.said.is_empty() || !self.current.iter().any(|d| self.said.contains(d))
    }
    /// The window was spoken.
    pub fn said(&mut self) { for d in &self.current { if !self.said.contains(d) { self.said.push(*d); } } }
}
/// All twelve top-bar slots read with a dead/alive state.
pub fn top_bar_read(s: &GameSnapshot) -> bool { s.hero_status.len() == 12 && s.hero_status.iter().all(|h| h.dead.is_some()) }
/// Dead slots of one team when the top bar was read.
pub fn dead_set(s: &GameSnapshot, ally: bool) -> Option<Vec<usize>> { top_bar_read(s).then(|| dead_slots(s, ally)) }
/// Top-bar slots (0-5) of one team shown dead.
pub fn dead_slots(s: &GameSnapshot, ally: bool) -> Vec<usize> {
    if s.hero_status.len() != 12 { return vec![]; }
    s.hero_status.iter().filter(|h| h.ally == ally && h.dead == Some(true)).map(|h| h.slot).collect()
}

/// A track unseen this long still counts as present (one-frame dropouts).
pub const RECENT_MS: u64 = 3_000;
/// A track last seen this close to you, this recently, is assumed hidden under your portrait.
pub const HIDDEN_MS: u64 = 10_000;
pub const HIDDEN_RADIUS: f32 = 0.08;
/// How long a teammate hidden under you still cancels an outnumbered / gank warning.
pub const HIDDEN_ALLY_MS: u64 = 15_000;
/// How long a teammate hidden under you still cancels an outnumbered / gank warning.

/// Positional calls on the minimap are only as good as one frame's ring detection, so a call is
/// spoken when the same advice holds on two consecutive assessments (gank warnings, which are
/// about motion, excepted), and a "commit" call never follows a "back off" call within 30 s or
/// vice versa (rec3 said "outnumbered" then "go in" 3 s apart as a teammate's ring flickered).
///
/// Team-wide calls are said once per situation (rec8 repeated enemies-down 11 times, allies-down 8
/// and missing 11): enemies / allies down once per window ([`WindowMemo`]; the active coach's
/// lane-specific window call shares the key), missing once per set of missing tracks for 90 s.
/// Each is committed when the engine speaks it ([`Rule::spoken`]).
#[derive(Clone, Default)]
pub struct Spoken { last: std::cell::RefCell<(Option<(String, u32)>, Option<(String, u32)>)>, memo: std::cell::RefCell<Memo> }
#[derive(Clone, Default)]
struct Memo { missing_said: Vec<(u32, u32)>, missing_now: Vec<u32>, last_clock: u32, enemies: WindowMemo, allies: WindowMemo, new_enemies: bool, new_allies: bool }
fn aggressive(id: &str) -> bool { matches!(id, "go_in" | "join" | "kill_window" | "enemies_down") }
fn defensive(id: &str) -> bool { matches!(id, "outnumbered" | "gank" | "allies_down") }

impl Rule for PositionCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        let Some(clock) = s.game_clock_secs else { return vec![] };
        let advice = self.assess(s);
        let mut memo = self.spoken.memo.borrow_mut();
        if clock + 120 < memo.last_clock { *memo = Memo::default(); }
        memo.last_clock = clock;
        memo.missing_said.retain(|(_, t)| clock >= *t && clock - t < 90);
        let down = self.catalog.thresholds.team_down;
        memo.new_enemies = memo.enemies.observe(dead_set(s, false), clock, down);
        memo.new_allies = memo.allies.observe(dead_set(s, true), clock, down);
        memo.missing_now.clear();
        let mut st = self.spoken.last.borrow_mut();
        let (prev, said) = (st.0.clone(), st.1.clone());
        st.0 = advice.as_ref().map(|a| (a.id.clone(), clock));
        let Some(a) = advice else { return vec![] };
        let stable = a.id == "gank" || prev.is_some_and(|(id, t)| id == a.id && clock >= t && clock - t <= 6);
        let contradicts = said.is_some_and(|(id, t)| clock >= t && clock - t < 30 && (aggressive(&a.id) && defensive(&id) || defensive(&a.id) && aggressive(&id) && a.id != "gank"));
        if !stable || contradicts { return vec![]; }
        let repeat = match a.id.as_str() {
            "missing" => !a.tracks.is_empty() && a.tracks.iter().all(|id| memo.missing_said.iter().any(|m| m.0 == *id)),
            "enemies_down" => !memo.new_enemies,
            "allies_down" => !memo.new_allies,
            _ => false,
        };
        if repeat { return vec![]; }
        if a.id == "missing" { memo.missing_now = a.tracks.clone(); }
        st.1 = Some((a.id.clone(), clock));
        vec![Cue::card(&format!("positioning.{}", a.id), format!("pos:{}", a.id), a.text, a.priority, a.cooldown_secs)]
    }
    fn spoken(&self, cue: &Cue) {
        let mut memo = self.spoken.memo.borrow_mut();
        match cue.key.as_str() {
            "pos:enemies_down" => memo.enemies.said(),
            "pos:allies_down" => memo.allies.said(),
            "pos:missing" => { let at = memo.last_clock; let ids = std::mem::take(&mut memo.missing_now); memo.missing_said.extend(ids.into_iter().map(|id| (id, at))); }
            _ => {}
        }
    }
}
