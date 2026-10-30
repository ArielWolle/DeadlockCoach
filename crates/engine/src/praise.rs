//! Positive reinforcement: short calls that name a good decision the frames proved, so the coach
//! encourages what works instead of only correcting what doesn't (rec12, a 12/3/11 Grey Talon win:
//! "point out and encourage the good behaviour that caused our team and myself to do so well").
//!
//! - An enemy-dead window used: you in their half with a teammate, a lane front moving up, or
//!   their structure's health banner while two or more enemies were dead.
//! - The urn delivered: carrying it, then your souls jumping by the urn's value.
//! - Shopping on time: souls spent on the death screen, or spent within a minute of reaching the
//!   tier normally bought at that game time.
//! - Deathless streaks (10 and 20 minutes) with the share of the game spent dead.
//! - Walking out of base with a teammate after a death alone.
//! - The team's soul lead growing (reliable team totals only).
//!
//! Evidence rules as everywhere: an unread value is never evidence. Praise is said sparingly: only
//! in a calm moment ([`PraiseCoach`] checks health, enemies near, the urn, the team), at most
//! [`PER_WINDOW`] per [`WINDOW_SECS`] and [`MIN_GAP_SECS`] apart (enforced by the engine), and two
//! good things waiting at the same time share one call. Needs the full scan (top bar, status
//! effects, structure banner): not registered in the fast path.
use crate::{laning::Source, positioning::TeamPicture, Cue, Priority, Rule};
use coach_core::{GameSnapshot, HeroTrack, Lane, UrnState, ZoneArea};
use serde::Deserialize;
use std::{cell::RefCell, collections::VecDeque};

/// Engine-level praise budget: at most `PER_WINDOW` praise calls per `WINDOW_SECS` of game time
/// (about one per 3-4 minutes), never two within `MIN_GAP_SECS`.
pub const PER_WINDOW: usize = 3;
pub const WINDOW_SECS: u32 = 600;
pub const MIN_GAP_SECS: u32 = 60;
/// A minor praise (a short window, prompt shopping, the lead growing) waits for this long without
/// any compliment, so the budget stays free for the big moments (urn, a base push, death-screen
/// shopping, walking out with a teammate, a deathless streak).
pub const MINOR_QUIET_SECS: u32 = 180;
/// Cues counted against the praise budget (`praise:*`); the farm-pace compliments only wait for it.
pub fn is_praise(key: &str) -> bool { key.starts_with("praise:") }
pub fn is_compliment(key: &str) -> bool { is_praise(key) || key == "pace:ahead" || key == "pace:on-track" }
/// Whether a praise-family cue may be spoken `now`, given the clocks of the praise calls spoken.
pub fn budget_allows(key: &str, spoken: &[u32], last_compliment: Option<u32>, now: u32) -> bool {
    if !is_compliment(key) { return true; }
    if last_compliment.is_some_and(|t| now >= t && now - t < MIN_GAP_SECS) { return false; }
    !is_praise(key) || spoken.iter().filter(|t| **t <= now && now - **t < WINDOW_SECS).count() < PER_WINDOW
}

#[derive(Clone, Deserialize)]
pub struct Thresholds {
    pub calm_hp: f32, pub near_radius: f32,
    pub window_dead: usize, pub window_min_secs: u32, pub window_valid_secs: u32, pub with_ally: f32, pub half_y: f32, pub lane_advance: f32,
    pub urn_gain: u32, pub urn_after_secs: u32, pub urn_valid_secs: u32,
    pub spent_valid_secs: u32, pub tier_buy_secs: u32, pub tier_valid_secs: u32, pub purchase_drop: u32,
    pub streaks: Vec<u32>, pub streak_valid_secs: u32, pub streak_gap_secs: u32,
    pub solo_distance: f32, pub group_radius: f32, pub group_after_secs: u32, pub group_valid_secs: u32,
    pub lead_back_min: u32, pub lead_back_max: u32, pub lead_growth: u32, pub lead_min: u32, pub lead_min_pct: u32, pub lead_jump_pct: u32,
    pub lead_every_secs: u32, pub lead_share_pct: u32, pub lead_min_clock: u32, pub lead_valid_secs: u32,
}
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub cards: Vec<Card> }

/// One good thing waiting for a calm moment: (unique id, card, filled text, rank: lower first,
/// valid until).
#[derive(Debug, Clone)]
struct Pending { id: String, card: String, text: String, rank: u8, until: u32, minor: bool }

/// An enemy-dead window being watched.
#[derive(Debug, Clone, Default)]
struct Window {
    start: u32, most: usize, reads: u32, below: u32, closed: Option<u32>,
    /// You in their half with a teammate (two reads; base: in their base), first seen at.
    half: Option<u32>, half_reads: u8, base: bool,
    /// Lane fronts at the start, and lanes read at least `lane_advance` further up on two reads.
    push0: [Option<f32>; 3], adv_reads: [u8; 3], advanced: Option<Lane>,
    banner: Option<String>, queued: bool,
}

#[derive(Default)]
struct State {
    last_clock: Option<u32>, start: Option<u32>,
    dead: bool, dead_secs: u32, prev_clock: Option<u32>,
    /// Since when no death was seen (None: unknown, e.g. watched from mid-match or a gap).
    streak_from: Option<u32>, streak_said: Vec<u32>,
    /// Souls: last alive read (clock, souls); souls when the current death started; dead reads.
    last_alive_souls: Option<(u32, u32)>, death_souls: Option<u32>, dead_reads: Vec<u32>, spent_queued: bool,
    /// The spend tier reached on alive reads: (tier, first clock, reads).
    tier: Option<(u32, u32, u8)>, tier_reached: Option<(u32, u32, u32)>, low_reads: u8, low_since: u32, prev_souls: Option<u32>,
    /// Urn: souls on the last carrying read and its clock; reads above the gain since.
    urn_base: Option<u32>, urn_last: Option<u32>, urn_reads: u8,
    window: Option<Window>,
    /// Team totals: last raw read, and reliable (clock, lead) samples.
    last_nw: Option<(u32, u32)>, leads: VecDeque<(u32, u32, u32)>, lead_said: Option<u32>,
    /// When the generic "your team is ahead" call was last spoken (a growth praise waits 4 min).
    lead_rule_at: Option<u32>,
    /// Nearest visible ally (clock, distance) on recent alive frames; deaths that came alone.
    near: VecDeque<(u32, f32)>, solo_deaths: u32, respawned_at: Option<u32>, group_reads: u8, group_done: bool,
    pending: Vec<Pending>, offered: Vec<String>, said: Vec<String>,
    /// Clock of the last compliment spoken (praise or farm pace).
    last_compliment: Option<u32>,
}

/// The praise coach. One instance per engine; it keeps its own history.
pub struct PraiseCoach { catalog: Catalog, state: RefCell<State> }
impl Default for PraiseCoach {
    fn default() -> Self { Self { catalog: Self::shipped(), state: RefCell::new(State::default()) } }
}

fn thousands(n: u32) -> String { crate::active::thousands(n) }
fn mmss(t: u32) -> String { format!("{}:{:02}", t / 60, t % 60) }
fn count_word(n: usize) -> String { match n { 2 => "two".into(), 3 => "three".into(), 4 => "four".into(), 5 => "five".into(), 6 => "six".into(), n => n.to_string() } }
fn live(h: &HeroTrack) -> bool { h.confirmed() && h.unseen_ms <= 1000 }
fn carrying(s: &GameSnapshot) -> bool {
    s.minimap.as_ref().is_some_and(|m| m.urn == Some(UrnState::CarriedByYou)) || s.status_effects.iter().any(|e| e == "carrying_urn")
}
/// Base souls of the urn spawned last before `clock` (250 + 70 per minute, locked at spawn:
/// docs/timings/soul-urn.md).
fn urn_bounty(clock: u32) -> u32 {
    let spawn = if clock < 600 { 600 } else { 600 + (clock - 600) / 300 * 300 };
    250 + 70 * (spawn / 60)
}
fn lane_index(l: Lane) -> usize { match l { Lane::Green => 0, Lane::Blue => 1, Lane::Yellow => 2 } }
const LANES: [Lane; 3] = [Lane::Green, Lane::Blue, Lane::Yellow];

impl PraiseCoach {
    pub fn shipped() -> Catalog { serde_json::from_str(include_str!("../../../data/coaching/praise.json")).expect("shipped praise policy") }
    pub fn catalog(&self) -> &Catalog { &self.catalog }
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("praise card") }
    fn fill(&self, id: &str, vars: &[(&str, String)]) -> String {
        let mut t = self.card(id).speech.clone();
        for (k, v) in vars { t = t.replace(&format!("{{{k}}}"), v); }
        t
    }
    /// `minor`: said only after [`MINOR_QUIET_SECS`] without any compliment, so the budget is left for the big moments.
    fn queue(&self, st: &mut State, id: String, card: &str, text: String, rank: u8, until: u32, minor: bool) {
        if st.said.contains(&id) || st.pending.iter().any(|p| p.id == id) { return; }
        st.pending.push(Pending { id, card: card.into(), text, rank, until, minor });
    }

    /// Safe to say something positive: alive in gameplay, health read twice at `calm_hp`+ and not
    /// falling, no enemy marker close, not carrying the urn, no crowd control, the team not wiped.
    fn calm(&self, s: &GameSnapshot) -> bool {
        let t = &self.catalog.thresholds;
        if s.dead || s.screen.as_deref() != Some("Gameplay") || carrying(s) { return false; }
        if !crate::laning::plausible_health(s).is_some_and(|h| h >= t.calm_hp) || s.health_observations < 2 { return false; }
        if s.health_delta_ratio.is_some_and(|d| d < -0.03) { return false; }
        if s.status_effects.iter().any(|e| matches!(e.as_str(), "stunned" | "silenced" | "disarmed" | "knocked_off_zipline")) { return false; }
        let Some(m) = s.minimap.as_ref() else { return false };
        let Some(me) = m.you else { return false };
        if m.heroes.iter().any(|h| !h.ally && live(h) && h.distance(me) <= t.near_radius) { return false; }
        !TeamPicture::from(s).is_some_and(|p| p.allies_dead.len() >= 3 && p.allies_dead.len() > p.enemies_dead.len())
    }

    fn watch_window(&self, st: &mut State, s: &GameSnapshot, clock: u32, alive: bool) {
        let t = &self.catalog.thresholds;
        let Some(p) = TeamPicture::from(s) else { return };
        let dead = p.enemies_dead.len();
        let m = s.minimap.as_ref();
        if dead >= t.window_dead {
            // A new window (the previous one, if queued, keeps its pending entry).
            if !st.window.as_ref().is_some_and(|w| w.closed.is_none()) {
                st.window = Some(Window { start: clock, push0: m.map(|m| LANES.map(|l| m.lane(l).push)).unwrap_or_default(), ..Default::default() });
            }
            let w = st.window.as_mut().expect("open window");
            w.reads += 1; w.below = 0; w.most = w.most.max(dead);
            // Not the walk to the urn drop-off (rec12 16:39: just delivered, in their half).
            let urn_walk = st.urn_last.is_some_and(|u| clock <= u + 30) || carrying(s);
            if alive && !urn_walk {
                if let Some(m) = m {
                    // Clearly past the middle (a mid-lane front sits at y 0.45-0.5), a teammate beside you.
                    if let Some(me) = m.you.filter(|p| p.y < t.half_y) {
                        let ally = m.heroes.iter().any(|h| h.ally && live(h) && h.distance(me) <= t.with_ally);
                        if ally {
                            w.half_reads += 1;
                            if w.half_reads >= 2 { w.half.get_or_insert(clock); }
                            w.base |= m.zone_area == Some(ZoneArea::EnemyBase) || me.y < 0.25;
                        }
                    }
                }
            }
            // A lane front moving up: three reads at least `lane_advance` past where it stood when
            // the window opened, with three or more enemies dead (lane fronts are noisy).
            if let Some(m) = m {
                for l in LANES {
                    let i = lane_index(l);
                    let (Some(p0), Some(p)) = (w.push0[i], m.lane(l).push) else { if w.push0[i].is_none() { w.push0[i] = m.lane(l).push; } continue };
                    if p >= p0 + t.lane_advance && alive && m.your_lane == Some(l) { w.adv_reads[i] += 1; if w.adv_reads[i] >= 3 && w.most >= 3 && w.advanced.is_none() { w.advanced = Some(l); } } else { w.adv_reads[i] = 0; }
                }
            }
            if let Some(b) = s.objective_banner.as_ref().filter(|b| b.own == Some(false) && !b.name.trim().is_empty()) {
                let n = b.name.trim().to_ascii_uppercase();
                w.banner = Some(if n.contains("HIDDEN") || n.contains("ARCHMOTHER") || n.contains("PATRON") { "Patron".into() } else { b.name.trim().to_string() });
            }
        } else if let Some(w) = st.window.as_mut().filter(|w| w.closed.is_none()) {
            // Closed after two reads under the count (a slot flickering alive is not the end).
            w.below += 1;
            if w.below >= 2 {
                w.closed = Some(clock);
                let id = format!("window:{}", w.start);
                for p in st.pending.iter_mut().filter(|p| p.id == id) { p.until = clock + t.window_valid_secs; }
            }
        }
        // Queue the praise once the window held on two reads for `window_min_secs` with evidence;
        // while it is open, the text follows the evidence (rec12 27:17: four dead, then five and
        // the Patron's banner).
        let Some(w) = st.window.clone() else { return };
        if w.closed.is_some() && w.queued || w.reads < 2 || clock < w.start + t.window_min_secs { return; }
        let what = if let Some(b) = &w.banner { Some(format!("your team was hitting their {b}")) }
            else if w.half.is_some() { Some(if w.base { "you pushed into their base with your team".to_string() } else { "you pushed into their half with your team".to_string() }) }
            else { w.advanced.map(|l| format!("you and your team pushed {} lane up", crate::laning::lane_name(l))) };
        let Some(what) = what else { return };
        // Strong: their base or structure, or three or more dead; else a minor praise.
        let minor = w.banner.is_none() && !w.base && (w.most < 3 || w.half.is_none());
        let until = w.closed.map(|c| c + t.window_valid_secs).unwrap_or(u32::MAX);
        let text = self.fill("window", &[("time", mmss(w.start)), ("count", count_word(w.most)), ("what", what)]);
        let id = format!("window:{}", w.start);
        if let Some(p) = st.pending.iter_mut().find(|p| p.id == id) { p.text = text; p.minor = minor; p.until = until; }
        else if !w.queued { self.queue(st, id, "window", text, if minor { 3 } else { 1 }, until, minor); }
        if let Some(x) = st.window.as_mut() { x.queued = true; }
    }

    fn watch_lead(&self, st: &mut State, s: &GameSnapshot, clock: u32) {
        let t = &self.catalog.thresholds;
        let (Some(a), Some(e)) = (s.ally_networth, s.enemy_networth) else { return };
        let prev = st.last_nw.replace((a, e));
        // Reliable: no digit drop (one total under half the other) and no jump since the last read.
        let jump = |x: u32, px: u32| x.abs_diff(px) * 100 > px.max(1) * t.lead_jump_pct;
        let Some((pa, pe)) = prev else { return };
        if a == 0 || e == 0 || a.min(e) * 2 < a.max(e) || jump(a, pa) || jump(e, pe) { return; }
        // Team totals never fall: earlier samples more than 3% above this reading were misreads
        // (rec12 read 27,000 for 6:00-6:42, then 25,000 at 6:45).
        st.leads.retain(|x| x.1 * 100 <= a * 103 && x.2 * 100 <= e * 103);
        st.leads.push_back((clock, a, e));
        while st.leads.front().is_some_and(|x| x.0 + t.lead_back_max + 60 < clock) { st.leads.pop_front(); }
        // Early totals are a few thousand souls: a lead story needs the midgame.
        if clock < t.lead_min_clock { return; }
        let lead = a as i64 - e as i64;
        if st.lead_said.is_some_and(|c| clock < c + t.lead_every_secs) || st.lead_rule_at.is_some_and(|c| clock < c + t.lead_back_min)
            || st.pending.iter().any(|p| p.card == "lead") { return; }
        if lead < t.lead_min.max(e * t.lead_min_pct / 100) as i64 { return; }
        // The reading closest to 5 minutes back within [lead_back_min, lead_back_max].
        let Some(&(then, ta, te)) = st.leads.iter().filter(|x| x.0 + t.lead_back_min <= clock && x.0 + t.lead_back_max >= clock)
            .min_by_key(|x| (clock - x.0).abs_diff(300)) else { return };
        let before = ta as i64 - te as i64;
        let growth = lead - before;
        if growth < (t.lead_growth as i64).max(before * 3 / 10) { return; }
        // Held on the last three readings (not one high read).
        let recent: Vec<i64> = st.leads.iter().rev().take(3).map(|x| x.1 as i64 - x.2 as i64).collect();
        if recent.len() < 3 || recent.iter().any(|l| *l < before + growth / 2) { return; }
        let from = if before.abs() < 2000 { "even".to_string() } else if before < 0 { format!("{} behind", thousands(before.unsigned_abs() as u32)) } else { thousands(before as u32) };
        let me = s.hero.as_ref().and_then(|h| s.hero_status.iter().find(|p| p.ally && p.hero.as_ref() == Some(h) && p.networth_observations >= 2)).and_then(|p| p.networth);
        let share = me.filter(|m| *m * 3 <= a).map(|m| m * 100 / a).filter(|p| *p >= t.lead_share_pct)
            .map(|p| format!(" You hold {p}% of your team's souls, more than a sixth.")).unwrap_or_default();
        let text = self.fill("lead", &[("from", from), ("time", mmss(then)), ("to", thousands(lead as u32)), ("at", mmss(clock)), ("share", share)]);
        self.queue(st, format!("lead:{clock}"), "lead", text, 6, clock + t.lead_valid_secs, true);
    }
}

impl Rule for PraiseCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        let Some(clock) = s.game_clock_secs else { return vec![] };
        let t = self.catalog.thresholds.clone();
        let mut st = self.state.borrow_mut();
        if st.last_clock.is_some_and(|c| clock + 120 < c) { *st = State::default(); }
        st.last_clock = Some(clock);
        let first = *st.start.get_or_insert(clock);
        if st.streak_from.is_none() && st.prev_clock.is_none() && first <= 120 { st.streak_from = Some(first); }
        // A recording gap may hide a death: the streak is unknown until the next respawn.
        let gap = st.prev_clock.is_some_and(|p| clock > p + t.streak_gap_secs);
        if gap { st.streak_from = None; }
        let dt = st.prev_clock.map(|p| clock.saturating_sub(p).min(10)).unwrap_or(0);
        st.prev_clock = Some(clock);
        let screen = s.screen.as_deref();
        let dead_frame = s.dead || screen == Some("Dead");
        // Menus between death screens (shop, scoreboard) and gameplay frames without a health
        // reading (rec12 9:55, between two death screens) are still the same death.
        let back = screen == Some("Gameplay") && s.hp.is_some() && !dead_frame;
        let dead = dead_frame || st.dead && !back;
        let alive = !dead && screen == Some("Gameplay");
        let st_ref = &mut *st;

        // --- Deaths: time dead, the streak, souls at death, alone or not.
        if dead {
            if st_ref.dead { st_ref.dead_secs += dt; }
            if !st_ref.dead {
                st_ref.dead = true; st_ref.streak_from = None; st_ref.streak_said.clear();
                // The last alive reading within a minute (shop and death screens between are not analysed).
                st_ref.death_souls = st_ref.last_alive_souls.filter(|(c, _)| *c + 60 >= clock).map(|x| x.1).or(s.souls);
                st_ref.dead_reads.clear(); st_ref.spent_queued = false;
                // Alone: teammates were visible and all a quarter of the map away on the reads of
                // the last 15 s (unseen is not "far").
                let recent: Vec<f32> = st_ref.near.iter().filter(|x| x.0 + 15 >= clock).map(|x| x.1).collect();
                if recent.len() >= 2 && recent.iter().all(|d| *d > t.solo_distance) { st_ref.solo_deaths += 1; }
                st_ref.near.clear(); st_ref.tier = None; st_ref.tier_reached = None; st_ref.urn_base = None; st_ref.urn_last = None;
                st_ref.respawned_at = None;
            }
            if let Some(v) = s.souls { st_ref.dead_reads.push(v); }
            // Two reads in a row at least a death-screen tier under the souls at death, and under
            // that tier: bought while dead.
            let need = crate::economy::death_spend(clock);
            let low = match st_ref.dead_reads.as_slice() { [.., a, b] => Some((*a).max(*b)), _ => None };
            if let (Some(at), Some(low), false) = (st_ref.death_souls, low, st_ref.spent_queued) {
                if at >= need && low < need && at - low >= need {
                    st_ref.spent_queued = true;
                    let text = self.fill("spent_dead", &[("spent", thousands(at - low))]);
                    self.queue(st_ref, format!("spent_dead:{clock}"), "spent_dead", text, 2, u32::MAX, false);
                }
            }
        } else if back && st_ref.dead {
            st_ref.dead = false; st_ref.respawned_at = Some(clock); st_ref.streak_from = Some(clock);
            st_ref.group_reads = 0; st_ref.group_done = false;
            for p in st_ref.pending.iter_mut().filter(|p| p.card == "spent_dead" && p.until == u32::MAX) { p.until = clock + t.spent_valid_secs; }
        }
        self.watch_window(st_ref, s, clock, alive);
        if alive {
            let map = s.minimap.as_ref();
            let me = map.and_then(|m| m.you);
            if let (Some(me), Some(m)) = (me, map) {
                if let Some(d) = m.heroes.iter().filter(|h| h.ally && live(h)).map(|h| h.distance(me)).min_by(f32::total_cmp) { st_ref.near.push_back((clock, d)); }
            }
            while st_ref.near.front().is_some_and(|x| x.0 + 30 < clock) { st_ref.near.pop_front(); }

            // --- The urn: lowest souls while carrying, then a jump by its value soon after.
            if carrying(s) {
                // The last reading while carrying: souls farmed on the way are not the urn's.
                if let Some(v) = s.souls { st_ref.urn_base = Some(v); }
                st_ref.urn_last = Some(clock); st_ref.urn_reads = 0;
            } else if let (Some(base), Some(last), Some(v)) = (st_ref.urn_base, st_ref.urn_last, s.souls) {
                let need = t.urn_gain.min(urn_bounty(last));
                if clock <= last + t.urn_after_secs && v >= base + need {
                    st_ref.urn_reads += 1;
                    if st_ref.urn_reads >= 2 {
                        let text = self.fill("urn", &[("gain", thousands((v - base) / 100 * 100))]);
                        self.queue(st_ref, format!("urn:{last}"), "urn", text, 0, clock + t.urn_valid_secs, false);
                        st_ref.urn_base = None; st_ref.urn_last = None;
                    }
                } else if clock > last + t.urn_after_secs { st_ref.urn_base = None; st_ref.urn_last = None; }
            }
        }
        // --- Shopping on time (alive, base or field; the shop and scoreboard screens count).
        if !dead && matches!(screen, Some("Gameplay" | "Shop" | "Scoreboard")) {
            if let Some(v) = s.souls {
                let tier = crate::economy::spend_target(clock);
                if v >= tier {
                    let (tt, since, n) = st_ref.tier.filter(|x| x.0 == tier).unwrap_or((tier, clock, 0));
                    st_ref.tier = Some((tt, since, n + 1));
                    // Reached: two reads at the tier, or one right after a read just under it (a
                    // steady climb; the shop screen between is not analysed: rec12 6:19 -> 6:26).
                    let climb = st_ref.prev_souls.is_some_and(|p| p < tier && p + 400 >= tier);
                    if n + 1 >= 2 || climb { st_ref.tier_reached = Some((tier, since, v.max(st_ref.tier_reached.map_or(0, |r| r.2)))); }
                    st_ref.low_reads = 0;
                } else {
                    st_ref.tier = None;
                    if let Some((tier, since, peak)) = st_ref.tier_reached {
                        if clock > since + t.tier_buy_secs && st_ref.low_reads == 0 { st_ref.tier_reached = None; }
                        else if peak >= v + t.purchase_drop {
                            st_ref.low_reads += 1;
                            if st_ref.low_reads == 1 { st_ref.low_since = clock; }
                            if st_ref.low_reads >= 2 {
                                let secs = st_ref.low_since.saturating_sub(since);
                                let text = self.fill("spent_tier", &[("spent", thousands((peak - v) / 100 * 100)), ("secs", secs.to_string()), ("tier", thousands(tier))]);
                                self.queue(st_ref, format!("spent_tier:{since}"), "spent_tier", text, 3, clock + t.tier_valid_secs, true);
                                st_ref.tier_reached = None; st_ref.low_reads = 0;
                            }
                        }
                    }
                }
                if alive { st_ref.last_alive_souls = Some((clock, v)); }
                st_ref.prev_souls = Some(v);
            }
        }
        if alive {
            // --- Deathless streak milestones.
            if let Some(from) = st_ref.streak_from {
                let start = st_ref.start.unwrap_or(from);
                for m in t.streaks.clone() {
                    if clock >= from + m && !st_ref.streak_said.contains(&m) {
                        st_ref.streak_said.push(m);
                        let pct = st_ref.dead_secs * 100 / clock.saturating_sub(start).max(1);
                        let since = if from > start + 60 { " since your last respawn".to_string() } else { String::new() };
                        let text = self.fill("streak", &[("min", (m / 60).to_string()), ("since", since), ("pct", pct.to_string())]);
                        self.queue(st_ref, format!("streak:{from}:{m}"), "streak", text, 4, clock + t.streak_valid_secs, false);
                    }
                }
            }
            // --- Walking out with a teammate after a death alone.
            if let (Some(r), false) = (st_ref.respawned_at, st_ref.group_done) {
                let me = s.minimap.as_ref().and_then(|m| m.you);
                if st_ref.solo_deaths == 0 || clock > r + t.group_after_secs { st_ref.group_done = true; }
                else if let (Some(me), Some(m)) = (me, s.minimap.as_ref()) {
                    // Out of base (the base label reaches y 0.78) and still on your half.
                    if me.y < 0.78 && me.y > 0.5 {
                        let with = m.heroes.iter().any(|h| h.ally && live(h) && h.distance(me) <= t.group_radius);
                        if with { st_ref.group_reads += 1; } else { st_ref.group_done = true; }
                        if st_ref.group_reads >= 2 {
                            st_ref.group_done = true;
                            let n = st_ref.solo_deaths;
                            let text = self.fill("group", &[("deaths", if n == 1 { "a death".into() } else { format!("{n} deaths") })]);
                            self.queue(st_ref, format!("group:{r}"), "group", text, 2, clock + t.group_valid_secs, false);
                            st_ref.solo_deaths = 0;
                        }
                    }
                }
            }
        }
        self.watch_lead(st_ref, s, clock);

        // --- Offer: drop what expired, then the best two waiting, in a calm moment.
        st_ref.pending.retain(|p| p.until >= clock);
        st_ref.offered.clear();
        if st_ref.pending.is_empty() || !self.calm(s) { return vec![]; }
        let quiet = st_ref.last_compliment.is_none_or(|c| clock >= c + MINOR_QUIET_SECS);
        let mut picks: Vec<Pending> = st_ref.pending.iter().filter(|p| quiet || !p.minor).cloned().collect();
        if picks.is_empty() { return vec![]; }
        picks.sort_by_key(|p| p.rank);
        picks.truncate(2);
        let mut text = picks[0].text.clone();
        if let Some(second) = picks.get(1) {
            // Two at once: the observation of each, without the closing explanation.
            let first = |t: &str| t.find(". ").map(|i| t[..=i].to_string()).unwrap_or_else(|| t.to_string());
            let other = first(&second.text);
            let mut c = other.chars();
            let rest = c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default();
            text = format!("{} Also, {rest}", first(&text));
        }
        st_ref.offered = picks.iter().map(|p| p.id.clone()).collect();
        let card = self.card(&picks[0].card);
        let priority = if card.priority == "low" { Priority::Low } else { Priority::Normal };
        vec![Cue::card(&format!("praise.{}", picks[0].card), format!("praise:{}", picks[0].id), text, priority, card.cooldown_secs)]
    }
    fn spoken(&self, cue: &Cue) {
        let mut st = self.state.borrow_mut();
        if cue.key == "lead" { st.lead_rule_at = st.last_clock; return; }
        if is_compliment(&cue.key) { st.last_compliment = st.last_clock; }
        if !is_praise(&cue.key) { return; }
        let ids = std::mem::take(&mut st.offered);
        if ids.iter().any(|i| i.starts_with("lead:")) { st.lead_said = st.last_clock; }
        st.pending.retain(|p| !ids.contains(&p.id));
        st.said.extend(ids);
    }
}
