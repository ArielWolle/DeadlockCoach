//! Full-frame scanner: JPEG bytes -> GameSnapshot (clock, souls, items, heroes, net worth).
use crate::{hud, ocr::Reader, parse, ColorLibrary};
use coach_core::GameSnapshot;
use std::{collections::HashMap, path::Path};
use std::sync::Arc;

/// A hero score is the mean of its best K frames (robust to dimmed/critical/dead portraits).
const TOP_K: usize = 5;

fn score_of(v: &[f32]) -> f32 { v.iter().sum::<f32>() / v.len().max(1) as f32 }

/// Hero class name of a portrait template: extra variants are stored as `hero_x@variant.png`
/// (e.g. a real top-bar capture next to the card art) and count as the same hero.
pub fn hero_key(template: &str) -> &str { template.split('@').next().unwrap_or(template) }

pub struct Scanner {
    ocr: Reader,
    items: Arc<ColorLibrary>,
    heroes: Arc<ColorLibrary>,
    heroes_crit: Arc<ColorLibrary>,
    /// Own unspent souls (pixel-located `$` row, see hudtext) with purchase context.
    souls: parse::Souls,
    /// When the shop was last seen (ms), and the previous frame's owned items.
    shop_seen: Option<u64>,
    prev_items: Vec<String>,
    /// Accepted level and the last raw level read.
    level: (Option<u32>, Option<u32>),
    unsecured: crate::hudtext::Unsecured,
    /// Own respawn countdown and the death panel of the current death.
    respawn: crate::death::Countdown,
    panel: crate::death::Panel,
    /// Top panel row source read by the detector when line reads missed it (compatibility death_cause).
    panel_top_raw: Option<String>,
    /// Summed template scores per slot (0-5 ally, 6-11 enemy) and frames counted.
    sums: Vec<HashMap<String, Vec<f32>>>,
    frames: u32,
    last_totals: (Option<u32>, Option<u32>),
    pending_totals: (Option<(u32, u32)>, Option<(u32, u32)>),
    /// When each team total was last confirmed (ms).
    totals_at: (Option<u64>, Option<u64>),
    hero_names: Vec<(String, String)>,
    match_id: Option<u64>,
    pending_match: Option<u64>,
    /// Highest verified match clock this match (a new match restarts the clock).
    clock_max: Option<u32>,
    virt_ms: u64,
    last_board: Vec<coach_core::PlayerRow>,
    board_history: Vec<Vec<coach_core::PlayerRow>>,
    map_analyzer: crate::minimap::Analyzer,
    map_tracker: crate::minimap::Tracker,
    vitals: crate::vitals::Tracker,
    regions:Vec<coach_perception::RegionObservation>,
    team_status:Vec<coach_core::HeroStatus>,
    /// Refresh intervals for slow fields (team banners, earned souls, match id).
    cadence: coach_perception::track::Cadence,
    /// Exact scoreboard name rectangles per slot, acquired by the detector once per match.
    name_rects: Vec<Option<coach_perception::Bounds>>,
    /// Match clock: detector-anchored, wall-time extrapolated, cheaply confirmed.
    clock_value: coach_perception::track::Extrapolated,
    /// Per-phase timing of the most recent scan.
    pub laps: coach_perception::track::Laps,
    /// Independent clock evidence for verification only (e.g. a glyph-located read from the
    /// automatic scanner); never used directly as the clock value. Consumed by the next scan.
    pub clock_hint: Option<u32>,
    /// Set when the fixed clock box misread against the glyph verifier in this context.
    fixed_clock_distrusted: bool,
    status_rr: coach_perception::track::RoundRobin,
    worth_rr: coach_perception::track::RoundRobin,
    /// Whether the previous frame showed the in-match HUD (skip pre-game probe lines).
    seen_hud: bool,
    was_dead: bool,
    /// The previous frame showed the scoreboard (Tab), possibly over the death screen.
    was_board: bool,
    /// Your hero as last named on a live gameplay frame (own top-bar slot or minimap marker).
    own_hero: Option<String>,
    totals_flip: bool,
    /// Per ally slot: summed |slot health - your health| and joint reads (see own_slot).
    own_slot: [(f32, u32); 6],
    /// Per slot: identity votes this match (portrait samples, scoreboard names).
    hero_votes: Vec<HashMap<String, f32>>,
    /// Per slot: identity latched for the match (see HeroLatches).
    hero_latch: HeroLatches,
    /// Your latched slot and how many checks in a row another slot has won.
    own_latched: Option<(usize, u32)>,
    /// Per ally slot: frames on which only that slot showed the own-portrait backdrop.
    own_backdrop: [u32; 6],
    /// Per item slot: latched item (see latch_item).
    item_latch: [ItemLatch; 12],
    /// A match-boundary screen (PostGame / Countdown) was seen since the last HUD frame with a clock.
    between: bool,
    /// The end-of-match banner was seen and no HUD frame continued the match since.
    match_over: bool,
    /// Wall time of the last HUD frame (gameplay, scoreboard, death screen).
    last_hud_ms: Option<u64>,
    /// Per-match resets so far.
    resets: u32,
    /// "win" / "loss" read from the end-of-match banner of this match.
    match_result: Option<String>,
    /// Lane structure states of this match (minimap diamonds).
    structures: crate::structures::Tracker,
    /// Midboss taker evidence (team-total steps around a kill).
    midboss_taker: crate::objectives::MidbossTaker,
    /// Last ability-bar read: when, and the slots.
    abilities: Option<(u64, Vec<coach_core::AbilityState>)>,
}

/// What a HUD frame says about the match boundary (see [`match_boundary`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BoundaryEvidence {
    /// A PostGame or Countdown screen was seen since the last HUD frame with a clock.
    pub between: bool,
    /// Wall time since the last HUD frame (ms).
    pub gap_ms: Option<u64>,
    /// This frame's clock reading (probe vote, not yet verified).
    pub raw_clock: Option<u32>,
    /// The current match's clock extrapolated to now (wall time).
    pub expected: Option<u32>,
    /// The verified clock after this frame's verification.
    pub verified: Option<u32>,
    /// Highest verified clock of the current match.
    pub clock_max: Option<u32>,
}

/// A new match starts on this frame: reset per-match memory before reading it (`Before`, the
/// clock re-anchors too) or keep the clock that verified the restart (`Verified`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary { Same, Before, Verified }

/// A clock below this is the start of a match.
pub const MATCH_START_SECS: u32 = 90;
/// A match whose verified clock passed this can be told apart from a restart.
pub const MATCH_RUNNING_SECS: u32 = 120;
/// Wall time without any HUD frame after which a clock that went back means another match.
pub const MATCH_GAP_MS: u64 = 300_000;

/// Match boundary rule. The HUD match id is not needed: recorded sessions read the same id
/// (127139499) in consecutive matches, and noisy id reads must not reset a running match.
/// - PostGame / Countdown seen, then a HUD frame with a start-of-match clock (< 90 s) that does
///   not continue the current clock: new match, reset before this frame is read.
/// - No HUD frame for 5+ minutes, then a clock 2+ minutes behind the extrapolated one.
/// - The verified clock restarted: from > 120 s to < 90 s (the verifier itself needs two
///   consistent reads to go back, so a misread cannot do it).
pub fn match_boundary(e: &BoundaryEvidence) -> Boundary {
    let continues = |c: u32| e.expected.is_some_and(|x| x.abs_diff(c) <= 10);
    if let Some(c) = e.raw_clock {
        // "0:31" for 30:31 (a dropped tens-of-minutes digit) is a misread, not a restart.
        let tens = e.expected.is_some_and(|x| coach_perception::track::tens_of_minutes_apart(c, x));
        if e.between && c < MATCH_START_SECS && !continues(c) && !tens { return Boundary::Before; }
        if e.gap_ms.is_some_and(|g| g >= MATCH_GAP_MS) && e.expected.is_some_and(|x| c + 120 < x) { return Boundary::Before; }
    }
    match (e.verified, e.clock_max) {
        (Some(v), Some(m)) if m > MATCH_RUNNING_SECS && v < MATCH_START_SECS && v + 60 < m => Boundary::Verified,
        _ => Boundary::Same,
    }
}

/// Scoreboard columns whose hero-name line is probed on death screens (Tab over the death screen).
const BOARD_PROBE: [usize; 4] = [1, 4, 8, 10];

/// Your top-bar slot, once one slot's bar has clearly followed your own HP.
pub fn own_slot(err: &[(f32, u32); 6]) -> Option<usize> {
    let mut m: Vec<(usize, f32)> = err.iter().enumerate().filter(|(_, e)| e.1 >= 8).map(|(i, e)| (i, e.0 / e.1 as f32)).collect();
    m.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (best, second) = (m.first()?, m.get(1)?);
    (best.1 < 0.08 && second.1 >= 2.0 * best.1 + 0.05).then_some(best.0)
}

/// The ally top-bar slot drawn with your own square backdrop: your portrait sits on a bright,
/// saturated team-coloured square, the others on round frames with the game world in the
/// corners. Measured (corner patches V >= 0.5, S >= 0.45, matching hue): rec5 slot 0, rec3
/// slot 1, rec4 slot 4, rec2 slot 5 (so your slot is not fixed). A single frame can light a
/// second corner pair (bright scenery), so the result is a per-frame vote, None unless unique.
pub fn own_backdrop_slot(img: &image::DynamicImage) -> Option<usize> {
    use image::GenericImageView;
    let (kx, ky) = (img.width() as f32 / 1600.0, img.height() as f32 / 900.0);
    let patch = |x0: f32| -> Option<(f32, f32, f32)> {
        let (mut hs, mut s, mut v, mut n) = (vec![], 0f32, 0f32, 0f32);
        for dy in 0..5 { for dx in 0..5 {
            let (x, y) = (((x0 + dx as f32) * kx) as u32, ((9.0 + dy as f32) * ky) as u32);
            if x >= img.width() || y >= img.height() { return None; }
            let p = img.get_pixel(x, y);
            let (h, ss, vv) = crate::minimap::hsv(p[0], p[1], p[2]);
            hs.push(h); s += ss; v += vv; n += 1.0;
        }}
        hs.sort_by(|a, b| a.total_cmp(b));
        Some((hs[hs.len() / 2], s / n, v / n))
    };
    let lit: Vec<usize> = hud::ALLY_CENTERS.iter().enumerate().filter(|(_, cx)| {
        let (Some(a), Some(b)) = (patch(**cx - 25.0), patch(**cx + 20.0)) else { return false };
        let dh = (a.0 - b.0).abs().min(360.0 - (a.0 - b.0).abs());
        a.2.min(b.2) >= 0.5 && a.1.min(b.1) >= 0.45 && dh <= 15.0
    }).map(|(i, _)| i).collect();
    match lit.as_slice() { [i] => Some(*i), _ => None }
}

/// Your slot from the backdrop votes: a clear leader (>= 6 votes, 3x the runner-up), or three
/// unanimous votes (rec12 named you only at 0:50 of the new match while slot 4 had won every
/// vote since 0:11; a wrong early latch is still taken over by 10 checks of another slot).
pub fn own_from_votes(votes: &[u32; 6]) -> Option<usize> {
    let mut v: Vec<(usize, u32)> = votes.iter().copied().enumerate().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    (v[0].1 >= 6 && v[0].1 >= 3 * v[1].1 || v[0].1 >= 3 && v[1].1 == 0).then_some(v[0].0)
}

/// Per-sample decay of the hero identity votes.
pub const HERO_VOTE_DECAY: f32 = 0.98;

/// Per-slot hero identities from the match's votes: each slot's most-voted hero (at least two
/// votes; otherwise this frame's assignment), unique across slots (stronger votes first).
pub fn latched_heroes(votes: &[HashMap<String, f32>], fresh: &[Option<String>]) -> Vec<Option<String>> {
    let mut ranked: Vec<(f32, usize, &String)> = votes.iter().enumerate()
        .flat_map(|(i, v)| v.iter().filter(|(_, n)| **n >= 2.0).map(move |(h, n)| (*n, i, h))).collect();
    // Ties: this frame's assignment first, then by name for determinism.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| (fresh.get(b.1).and_then(|f| f.as_ref()) == Some(b.2)).cmp(&(fresh.get(a.1).and_then(|f| f.as_ref()) == Some(a.2)))).then_with(|| a.2.cmp(b.2)));
    let mut out: Vec<Option<String>> = vec![None; fresh.len()];
    let mut used = std::collections::HashSet::new();
    for (_, slot, hero) in ranked {
        if slot < out.len() && out[slot].is_none() && !used.contains(hero) { out[slot] = Some(hero.clone()); used.insert(hero.clone()); }
    }
    for (slot, f) in fresh.iter().enumerate() {
        if out[slot].is_none() { if let Some(h) = f { if !used.contains(h) { out[slot] = Some(h.clone()); used.insert(h.clone()); } } }
    }
    out
}

/// Margin over the runner-up that makes a portrait sample "strong". Only strong samples can
/// overturn a latched identity: rec8's Grey Talon carrying the urn (mint backdrop, other art)
/// read Silver at 0.62 vs 0.57 for five minutes and took the slot (and your hero) over.
pub const STRONG_MARGIN: f32 = 0.08;
/// Contrary strong evidence that switches a latched identity (a sample counts 1, a scoreboard
/// name 3); every sample of the latched hero halves it, so it has to be sustained.
pub const SWITCH_EVIDENCE: f32 = 6.0;
/// Votes (and lead: twice the runner-up) a slot needs before its identity latches.
pub const LATCH_VOTES: f32 = 3.0;

/// Latched identity of one top-bar slot and the contrary evidence against it.
#[derive(Debug, Clone, Default)]
pub struct HeroLatch { pub hero: Option<String>, challenge: HashMap<String, f32>, named: bool }

/// Per-slot hero identities latched for the match (0-5 ally, 6-11 enemy). A hero is in one slot
/// only: a slot does not latch (or switch to) a hero another slot holds, except by a name read.
#[derive(Debug, Clone)]
pub struct HeroLatches { pub slots: Vec<HeroLatch> }

impl Default for HeroLatches { fn default() -> Self { HeroLatches { slots: vec![HeroLatch::default(); 12] } } }

impl HeroLatches {
    pub fn heroes(&self) -> Vec<Option<String>> { self.slots.iter().map(|l| l.hero.clone()).collect() }
    fn holder(&self, slot: usize, hero: &str) -> Option<usize> {
        self.slots.iter().enumerate().position(|(j, l)| j != slot && l.hero.as_deref() == Some(hero))
    }
    /// Whether `slot` may take `hero`: nobody else holds it, or this slot's votes for it are at
    /// least twice the holder's (the holder then lets go).
    fn may_take(&self, slot: usize, hero: &str, votes: &[HashMap<String, f32>]) -> bool {
        let v = |i: usize| votes.get(i).and_then(|m| m.get(hero)).copied().unwrap_or(0.0);
        self.holder(slot, hero).is_none_or(|j| v(slot) >= LATCH_VOTES && v(slot) >= 2.0 * v(j))
    }
    fn take(&mut self, slot: usize, hero: String) {
        for (j, o) in self.slots.iter_mut().enumerate() { if j != slot && o.hero.as_ref() == Some(&hero) { o.hero = None; o.challenge.clear(); o.named = false; } }
        let l = &mut self.slots[slot];
        l.hero = Some(hero); l.challenge.clear(); l.named = false;
    }
    /// Whether the slot's identity was confirmed by a scoreboard name read.
    pub fn named(&self, slot: usize) -> bool { self.slots.get(slot).is_some_and(|l| l.named) }
    /// One portrait sample of a live slot: its confident winner (if any) and margin over the
    /// runner-up; `votes` are all slots' identity votes.
    pub fn sample(&mut self, slot: usize, winner: Option<(&str, f32)>, votes: &[HashMap<String, f32>]) {
        if slot >= self.slots.len() { return; }
        if let Some((h, margin)) = winner {
            let l = &mut self.slots[slot];
            match l.hero.as_deref() {
                Some(cur) if cur == h => { for v in l.challenge.values_mut() { *v *= 0.5; } }
                // An identity a scoreboard name confirmed is not challenged by portraits: names are
                // far more reliable (rec12 ally slot 0 read Billy on every scoreboard while its dark
                // portrait on the amber backdrop matched Haze's silhouette and took the slot over).
                // Only name reads switch it (see `board`).
                Some(_) if margin >= STRONG_MARGIN && !l.named => { *l.challenge.entry(h.to_string()).or_default() += 1.0; }
                _ => {}
            }
        }
        self.settle(slot, votes);
        // A slot that lost its hero to another slot latches again from its own votes.
        for j in 0..self.slots.len() { if j != slot && self.slots[j].hero.is_none() { self.settle(j, votes); } }
    }
    /// A scoreboard name read for a slot: strong evidence. It latches an unlatched slot at once;
    /// two reads switch a latched identity (taking the hero from another slot that held it).
    pub fn board(&mut self, slot: usize, hero: &str, votes: &[HashMap<String, f32>]) {
        if slot >= self.slots.len() { return; }
        let l = &mut self.slots[slot];
        let take = match l.hero.as_deref() {
            Some(cur) if cur == hero => { for v in l.challenge.values_mut() { *v *= 0.5; } false }
            Some(_) => { let c = l.challenge.entry(hero.to_string()).or_default(); *c += 3.0; *c >= SWITCH_EVIDENCE }
            None => true,
        };
        if take { self.take(slot, hero.to_string()); }
        if self.slots[slot].hero.as_deref() == Some(hero) { self.slots[slot].named = true; }
        for j in 0..self.slots.len() { if self.slots[j].hero.is_none() { self.settle(j, votes); } }
    }
    fn settle(&mut self, slot: usize, votes: &[HashMap<String, f32>]) {
        let switch = self.slots[slot].challenge.iter().filter(|(_, v)| **v >= SWITCH_EVIDENCE)
            .max_by(|a, b| a.1.total_cmp(b.1)).map(|(h, _)| h.clone());
        if let Some(h) = switch.filter(|h| self.may_take(slot, h, votes)) { self.take(slot, h); }
        if self.slots[slot].hero.is_none() {
            let Some(mine) = votes.get(slot) else { return };
            let mut ranked: Vec<(&String, f32)> = mine.iter().filter(|(h, _)| self.may_take(slot, h, votes)).map(|(h, v)| (h, *v)).collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            let second = ranked.get(1).map_or(0.0, |r| r.1);
            if let Some(h) = ranked.first().filter(|r| r.1 >= LATCH_VOTES && r.1 >= 2.0 * second).map(|r| r.0.clone()) {
                self.take(slot, h);
            }
        }
    }
}

/// Slot identities: latched heroes first, then the vote / fresh assignment for unlatched slots,
/// unique across slots.
pub fn latched_assignment(latched: &[Option<String>], votes: &[HashMap<String, f32>], fresh: &[Option<String>]) -> Vec<Option<String>> {
    let mut used = std::collections::HashSet::new();
    let mut out: Vec<Option<String>> = latched.iter().map(|h| h.clone().filter(|h| used.insert(h.clone()))).collect();
    out.resize(fresh.len(), None);
    let rest = latched_heroes(votes, fresh);
    for (slot, r) in rest.into_iter().enumerate() {
        if out[slot].is_none() { if let Some(h) = r { if used.insert(h.clone()) { out[slot] = Some(h); } } }
    }
    out
}

/// One item-slot scan: empty (flat), a confident shop-art match, or a lit icon with no
/// confident match (a cooldown dims the icon and sweeps a shadow over it).
#[derive(Debug, Clone, PartialEq)]
pub enum ItemRead { Empty, Item(String), Unclear }

/// Latched item of one inventory slot.
#[derive(Debug, Clone, Default)]
pub struct ItemLatch { pub item: Option<String>, pending: Option<(String, u8)>, empty: u8 }

/// Scans of a confident different item that replace a latched one (a purchase into a sold
/// slot); single misreads swapped items in a recorded match (Rapid Rounds / Mercurial Magnum).
pub const ITEM_REPLACE_READS: u8 = 3;
/// Empty scans in a row that clear a latched item (cooldown icons dimmed for 1-3 scans).
pub const ITEM_EMPTY_READS: u8 = 4;

/// An item is listed after two matching scans, stays through unclear and briefly flat scans
/// (active items on cooldown dropped out for 1-3 frames), and is replaced only by
/// `ITEM_REPLACE_READS` confident reads of another item.
pub fn latch_item(l: &mut ItemLatch, read: ItemRead) {
    match read {
        ItemRead::Empty => {
            l.empty = l.empty.saturating_add(1); l.pending = None;
            if l.empty >= ITEM_EMPTY_READS || l.item.is_none() { l.item = None; }
        }
        ItemRead::Unclear => { l.empty = 0; }
        ItemRead::Item(n) => {
            l.empty = 0;
            if l.item.as_ref() == Some(&n) { l.pending = None; return; }
            let k = match l.pending.take() { Some((p, k)) if p == n => k.saturating_add(1), _ => 1 };
            if k >= if l.item.is_some() { ITEM_REPLACE_READS } else { 2 } { l.item = Some(n); } else { l.pending = Some((n, k)); }
        }
    }
}

/// Shop costs by class name (data/items.json). Items absent here (retired shop art) are rejected.
fn item_costs() -> &'static HashMap<String, u32> {
    static COSTS: std::sync::OnceLock<HashMap<String, u32>> = std::sync::OnceLock::new();
    COSTS.get_or_init(|| serde_json::from_str::<Vec<serde_json::Value>>(include_str!("../../../data/items.json").trim_start_matches('\u{feff}')).unwrap_or_default()
        .iter().filter_map(|i| Some((i["class_name"].as_str()?.to_string(), i["cost"].as_u64()? as u32))).collect())
}

/// Most souls a player can have spent by clock seconds: generous (a fed carry is near 1,000
/// souls a minute early), so it only removes impossible matches such as a 6,400 item at 2:59.
pub fn affordable_by(clock: u32) -> u32 { 1500 + clock * 1200 / 60 }

/// Whether a matched item may be listed as owned: a real shop item (tier-5 placeholders cost 9999
/// and are not sold in normal matches) that could be bought by now.
pub fn plausible_item(name: &str, clock: Option<u32>) -> bool {
    match item_costs().get(name) {
        None => false,
        Some(&c) if c >= 9999 => false,
        Some(&c) => clock.is_none_or(|t| c <= affordable_by(t)),
    }
}

/// Team totals only grow (mostly), and not by leaps. A value outside that band is accepted
/// only if the next reading agrees with it (so one misread cannot poison later frames).
/// Team net worth only grows (minor losses on death). A first value needs two consistent reads,
/// a rise outside the band three; a drop of more than 10% needs ten consistent reads: a recorded match read
/// ~100k as 1000 for 12 frames, which produced a false "you are behind" call.
/// `since_secs` is the time since `prev` was confirmed: the allowed rise grows with it, so a read
/// after missed frames is not mistaken for a leap.
pub fn plausible(prev: &mut Option<u32>, pending: &mut Option<(u32, u32)>, v: Option<u32>, since_secs: u64) -> Option<u32> {
    let v = v?;
    // A team earns ~70-120 souls a second (rec5); 250/s plus 2,500 is generous. rec5 accepted
    // 121k -> 173k in one read (+43%) and called a lead while 20k behind.
    let in_band = match *prev { Some(p) => v + p / 10 >= p && v as u64 <= p as u64 + 2500 + 250 * since_secs, None => false };
    let count = pending.filter(|(q, _)| v.abs_diff(*q) <= q / 10 + 1000).map(|(_, n)| n + 1).unwrap_or(1);
    let drop = prev.is_some_and(|p| v + p / 10 < p);
    let confirmed = count >= if drop { 10 } else if prev.is_none() { 2 } else { 3 };
    if in_band || confirmed { *prev = Some(v); *pending = None; Some(v) } else { *pending = Some((v, count)); None }
}

/// Range a team total must fall in given the confirmed per-hero labels (each rounded to 1k and
/// up to 45 s old) of at least four of the team's six slots; missing slots are bounded by the
/// smallest and largest known label. None when too few labels are known.
pub fn team_sum_range(status: &[coach_core::HeroStatus], ally: bool) -> Option<(u32, u32)> {
    let known: Vec<u32> = status.iter().filter(|s| s.ally == ally && s.networth_observations >= 2).filter_map(|s| s.networth).collect();
    if known.len() < 4 { return None; }
    let (sum, min, max) = (known.iter().sum::<u32>(), *known.iter().min()?, *known.iter().max()?);
    let missing = 6u32.saturating_sub(known.len() as u32);
    let (lo, hi) = (sum + missing * min * 7 / 10, sum + missing * max * 13 / 10);
    Some(((lo as f32 * 0.85) as u32 - 3000.min((lo as f32 * 0.85) as u32), (hi as f32 * 1.15) as u32 + 8000))
}

fn load_hero_names(root: &Path) -> Vec<(String, String)> {
    let Ok(s) = std::fs::read_to_string(root.join("data/heroes.json")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(s.trim_start_matches('\u{feff}')) else { return vec![] };
    v.as_array().map(|a| a.iter().filter_map(|h| Some((h["class_name"].as_str()?.to_string(), h["name"].as_str()?.to_string()))).collect()).unwrap_or_default()
}

/// Number of differing digit positions between two ids (OCR noise flips 1-2 digits).
fn digit_diff(a: u64, b: u64) -> usize {
    let (a, b) = (a.to_string(), b.to_string());
    if a.len() != b.len() { return 9; }
    a.chars().zip(b.chars()).filter(|(x, y)| x != y).count()
}

/// Whether the verified match clock went back from the highest value seen this match (a new
/// match restarts it). Without a current value it counts only when the match never had one.
/// Raw reads are not used: the frames that misread the match id also misread the clock's
/// minutes (rec8: 17:34 as 7:34), which the verified extrapolation ignores.
pub fn clock_restarted(now: Option<u32>, max: Option<u32>) -> bool {
    match now {
        Some(c) => max.is_none_or(|m| c + 90 < m),
        None => max.is_none(),
    }
}

/// "T2 Walker Killing Blow x30" -> "T2 Walker".
fn clean_cause(s: &str) -> Option<String> {
    let first = s.lines().next()?;
    // Stop at the "Killing Blow" tag (OCR: "kiling Bow", "gilling Blow", "Meleegilling") or a hit
    // count ("x32", "7"): recorded panels read "Melee kiling Bow 3", "Chimera MKI x232".
    let mut words = vec![];
    for w in first.split_whitespace() {
        let w: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
        let l = w.to_lowercase();
        if l.is_empty() { continue; }
        // "x232", "×32", "23": hit counts end the name.
        let count = l.trim_start_matches(['x', '×']);
        if count.len() < l.len() + 1 && !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()) { break; }
        if let Some(i) = ["lling", "ling", "iing", "blow"].iter().filter_map(|k| l.find(k)).min().or_else(|| (l == "bow").then_some(0)) {
            // "Meleegilling" / "Meleeiing": keep the name glued in front of the tag.
            let stem = w[..i].trim_end_matches(['k', 'g', 'i', 'l', 'K', 'G', 'I', 'L']).to_string();
            if words.is_empty() && stem.len() >= 3 { words.push(stem); }
            break;
        }
        // A mark icon read as a glued capital X ("Stalkers MarkX").
        let w = if w.len() > 3 && w.ends_with('X') && w[..w.len() - 1].ends_with(|c: char| c.is_ascii_lowercase()) { w[..w.len() - 1].to_string() } else { w };
        words.push(w);
    }
    let t = words.join(" ").trim().to_string();
    if t.len() >= 3 { Some(t) } else { None }
}

impl Scanner {
    /// `root` is the repo root containing data/.
    pub fn load(root: &Path) -> Result<Scanner, String> {
        let e = |x: std::io::Error| x.to_string();
        Ok(Self::from_resources(
            Reader::load(&root.join("data/models"))?,
            ColorLibrary::load_centered(&root.join("data/icons/shop")).map_err(e)?,
            ColorLibrary::load(&root.join("data/icons/h_card"), true).map_err(e)?,
            ColorLibrary::load(&root.join("data/icons/h_crit"), true).map_err(e)?,
            load_hero_names(root),
        ))
    }

    pub fn from_resources(ocr: Reader, items: ColorLibrary, heroes: ColorLibrary,
        heroes_crit: ColorLibrary, hero_names: Vec<(String, String)>) -> Self {
        Self::from_shared(ocr, Arc::new(items), Arc::new(heroes), Arc::new(heroes_crit), hero_names)
    }

    pub fn from_shared(ocr: Reader, items: Arc<ColorLibrary>, heroes: Arc<ColorLibrary>,
        heroes_crit: Arc<ColorLibrary>, hero_names: Vec<(String, String)>) -> Self {
        Scanner {
            ocr, items, heroes, heroes_crit,
            souls: Default::default(),
            shop_seen: None,
            prev_items: vec![],
            level: (None, None),
            unsecured: Default::default(),
            respawn: Default::default(),
            panel: Default::default(),
            panel_top_raw: None,
            sums: vec![HashMap::new(); 12],
            frames: 0,
            last_totals: (None, None),
            pending_totals: (None, None),
            totals_at: (None, None),
            hero_names,
            match_id: None,
            pending_match: None,
            clock_max: None,
            virt_ms: 0,
            last_board: vec![],
            board_history: vec![],
            map_analyzer: crate::minimap::Analyzer::new(),
            map_tracker: crate::minimap::Tracker::default(),
            vitals: crate::vitals::Tracker::default(),
            regions:vec![],
            team_status:vec![],
            cadence: Default::default(),
            name_rects: vec![None; 12],
            clock_value: Default::default(),
            laps: Default::default(),
            clock_hint: None,
            fixed_clock_distrusted: false,
            status_rr: Default::default(),
            worth_rr: Default::default(),
            seen_hud: false,
            was_dead: false,
            was_board: false,
            own_hero: None,
            totals_flip: false,
            own_slot: Default::default(),
            hero_votes: vec![HashMap::new(); 12],
            hero_latch: HeroLatches::default(),
            own_latched: None,
            own_backdrop: [0; 6],
            item_latch: Default::default(),
            between: false,
            match_over: false,
            last_hud_ms: None,
            resets: 0,
            match_result: None,
            structures: Default::default(),
            midboss_taker: Default::default(),
            abilities: None,
        }
    }

    /// A new match began (see [`match_boundary`]): per-match memory is reset and the match id
    /// is read again. `keep_clock`: the verified clock already belongs to the new match.
    fn new_match(&mut self, keep_clock: bool) {
        let clock = self.clock_value.clone();
        self.reset();
        if keep_clock { self.clock_value = clock; }
        self.match_id = None;
        self.pending_match = None;
        self.between = false;
        self.match_over = false;
        self.resets += 1;
    }

    /// Reset per-match memory (call when the clock goes back to the start).
    pub fn reset(&mut self) {
        self.souls.reset();
        self.shop_seen = None;
        self.prev_items.clear();
        self.level = (None, None);
        self.unsecured = Default::default();
        self.respawn.reset();
        self.panel = Default::default();
        self.panel_top_raw = None;
        self.fixed_clock_distrusted = false;
        self.clock_hint = None;
        self.sums = vec![HashMap::new(); 12];
        self.frames = 0;
        self.last_totals = (None, None);
        self.pending_totals = (None, None);
        self.totals_at = (None, None);
        self.last_board.clear();
        self.board_history.clear();
        self.map_tracker = crate::minimap::Tracker::default();
        self.vitals = crate::vitals::Tracker::default();
        self.team_status.clear();
        self.own_slot = Default::default();
        self.own_hero = None;
        self.hero_votes = vec![HashMap::new(); 12];
        self.hero_latch = HeroLatches::default();
        self.own_latched = None;
        self.own_backdrop = [0; 6];
        self.item_latch = Default::default();
        self.cadence.reset();
        self.name_rects = vec![None; 12];
        self.clock_value.reset();
        self.clock_max = None;
        self.between = false;
        self.match_over = false;
        self.last_hud_ms = None;
        self.match_result = None;
        self.structures = Default::default();
        self.midboss_taker = Default::default();
        self.abilities = None;
    }

    fn text(&self, img: &image::DynamicImage, r: crate::Region) -> Option<String> {
        self.ocr.read(&hud::crop(img, r)).ok()
    }
    pub fn regions(&self)->Vec<coach_perception::RegionObservation>{self.regions.clone()}

    /// Per-slot identity votes of this match (diagnostics).
    pub fn hero_votes(&self) -> &[HashMap<String, f32>] { &self.hero_votes }
    /// Own-slot evidence (diagnostics): health-gap sums, backdrop votes, latched slot.
    pub fn own_evidence(&self) -> ([(f32, u32); 6], [u32; 6], Option<usize>) { (self.own_slot, self.own_backdrop, self.own_latched.map(|(s, _)| s)) }
    /// Per-match resets so far (diagnostics).
    pub fn match_resets(&self) -> u32 { self.resets }
    /// Structure states with their site index in `structures::SITES` (diagnostics).
    pub fn structure_sites(&self, now_ms: u64) -> Vec<(usize, coach_core::StructureState)> { self.structures.site_states(now_ms, self.map_tracker.layout()) }

    /// Greedy global assignment of heroes to the 12 slots by mean score.
    pub fn hero_scores(&self) -> Vec<Option<(String, f32)>> {
        let a = self.assign_heroes();
        a.iter().enumerate().map(|(i, n)| n.as_ref().map(|n| (n.clone(), score_of(&self.sums[i][n])))).collect()
    }

    fn assign_heroes(&self) -> Vec<Option<String>> {
        let mut out: Vec<Option<String>> = vec![None; 12];
        if self.frames < 3 { return out; }
        let mut pairs: Vec<(f32, usize, &String)> = vec![];
        for (i, m) in self.sums.iter().enumerate() {
            for (n, v) in m { pairs.push((score_of(v), i, n)); }
        }
        pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let mut used = std::collections::HashSet::new();
        for (score, slot, name) in pairs {
            if score < 0.45 { break; }
            if out[slot].is_none() && used.insert(name.clone()) { out[slot] = Some(name.clone()); }
        }
        out
    }

    /// Classify the screen only (used by tools).
    pub fn classify(&self, jpeg: &[u8]) -> crate::screen::Screen {
        match image::load_from_memory(jpeg) {
            Ok(img) => crate::screen::classify(&self.ocr, &img, &self.hero_names).0,
            Err(_) => crate::screen::Screen::Unknown,
        }
    }

    /// Items per column that appear in at least 2 of the last 5 scoreboard reads
    /// (or in every read while fewer than 2 exist), so single-frame misreads vanish.
    fn voted_board(&self) -> Vec<coach_core::PlayerRow> {
        let n = self.board_history.len();
        let need = if n < 2 { 1 } else { 2 };
        let latest = self.board_history.last().cloned().unwrap_or_default();
        latest.into_iter().enumerate().map(|(i, mut row)| {
            let mut counts: HashMap<&String, usize> = HashMap::new();
            for b in &self.board_history { if let Some(r) = b.get(i) { for it in &r.items { *counts.entry(it).or_default() += 1; } } }
            let mut items: Vec<String> = counts.into_iter().filter(|(_, c)| *c >= need).map(|(k, _)| k.clone()).collect();
            items.sort();
            row.items = items;
            row
        }).collect()
    }

    /// Scan with a virtual clock (5 s per call). Use `scan_at` with real frame times in live use.
    pub fn scan(&mut self, jpeg: &[u8]) -> GameSnapshot {
        let t = self.virt_ms;
        self.virt_ms += 5000;
        self.scan_at(jpeg, t)
    }

    pub fn scan_at(&mut self, jpeg: &[u8], now_ms: u64) -> GameSnapshot {
        self.scan_with_minimap(jpeg, &[], now_ms)
    }

    /// Native-resolution, lossless minimap when supplied by an updated Agent;
    /// otherwise fall back to the compressed full-frame crop (legacy recordings).
    pub fn scan_with_minimap(&mut self, jpeg: &[u8], minimap_png: &[u8], now_ms: u64) -> GameSnapshot {
        let Ok(img) = image::load_from_memory(jpeg) else { return GameSnapshot::default() };
        let map = image::load_from_memory(minimap_png).ok()
            .filter(|i| i.width() >= 100 && i.height() >= 100
                && (i.width() as f32 / i.height() as f32 - 335.0 / 350.0).abs() < 0.05)
            .map(|i| i.to_rgb8());
        self.scan_image(&img, map.as_ref(), now_ms)
    }

    /// Pixel entry point shared with WebAssembly. Canvas supplies RGBA directly, so no
    /// lossy JPEG roundtrip is required. Main HUD stays at calibrated 16:9 coordinates.
    pub fn scan_image(&mut self, img: &image::DynamicImage, minimap: Option<&image::RgbImage>, now_ms: u64) -> GameSnapshot {
        self.scan_inner(img,minimap,now_ms,(None,None),None,false)
    }
    pub fn scan_located(&mut self,img:&image::DynamicImage,minimap:Option<&image::RgbImage>,now_ms:u64,clock:Option<u32>,currency:Option<u32>)->GameSnapshot{
        self.scan_inner(img,minimap,now_ms,(clock,currency),None,false)
    }
    /// Scan a canonical 1600x900 game context. `source` is the full-resolution frame and the
    /// rectangle the context was cut from (small text such as the death panel is read from it).
    /// `no_map`: the minimap was not located on this frame (shop, minimap hidden by effects):
    /// no minimap state is reported and gameplay additionally needs the own health bar.
    pub fn scan_context(&mut self,img:&image::DynamicImage,minimap:Option<&image::RgbImage>,now_ms:u64,currency:Option<u32>,
        source:Option<(&image::DynamicImage,coach_perception::Bounds)>,no_map:bool)->GameSnapshot{
        self.scan_inner(img,minimap,now_ms,(None,currency),source,no_map)
    }
    /// Shop screen: souls (the shop draws them higher), and while dead the respawn banner at
    /// the top left. Nothing else of the HUD is visible.
    fn shop_snapshot(&mut self, img: &image::DynamicImage, souls_read: Option<u32>, souls_px: bool, level_read: Option<u32>, now_ms: u64) -> GameSnapshot {
        use crate::death;
        self.shop_seen = Some(now_ms);
        let mut snap = GameSnapshot { match_id: self.match_id, screen: Some("Shop".into()), ..Default::default() };
        // Your hero stays known in the shop (rec8 lost it on all 46 shop frames).
        // After the end-of-match banner the post-game menus (classified Shop: they have the same
        // "ESC BACK" anchor) belong to no match: no hero, no clock (rec11/rec12 reported the
        // finished match's Violet and its clock through minutes of menus).
        if !self.match_over {
            snap.hero = self.own_latched.and_then(|(s, _)| self.hero_latch.slots[s].hero.clone()).or_else(|| self.own_hero.clone());
        }
        // The clock is not drawn in the shop: report the extrapolated clock only while recently confirmed.
        if !self.match_over && self.clock_value.confirmed_age_ms(now_ms).is_some_and(|a| a <= 60_000) { snap.game_clock_secs = self.clock_value.value(now_ms); }
        let banner = { let b = death::SHOP_RESPAWN_LABEL; let rgb = img.crop_imm(b.x, b.y, b.w, b.h).to_rgb8();
            rgb.pixels().filter(|p| { let (h, s, v) = crate::minimap::hsv(p[0], p[1], p[2]); (h < 12.0 || h > 350.0) && s > 0.45 && v > 0.75 }).count() as f32 / (b.w * b.h) as f32 > 0.45 };
        let digits = banner.then(|| death::digit_glyphs(img, death::SHOP_RESPAWN_BOX)).flatten();
        if let Some((b, n)) = digits {
            snap.dead = true;
            let read = self.ocr.read_lines(&[death::digits_crop(img, b)]).ok().and_then(|l| l.first().and_then(|s| death::read_count(s, n)));
            let (secs, fresh) = self.respawn.observe(read, now_ms);
            if fresh { self.panel = Default::default(); self.panel_top_raw = None; }
            snap.respawn_secs = secs; snap.respawn_age_ms = secs.and(self.respawn.age_ms(now_ms));
            self.fill_death(&mut snap);
        }
        self.was_dead = snap.dead;
        if let Some((v, age)) = self.souls.observe_with(souls_read, now_ms, true, !souls_px) { snap.souls = Some(v); snap.souls_age_ms = Some(age); }
        snap.level = self.accept_level(level_read);
        snap
    }
    /// Death-panel fields of the current death (held across shop/death frames of one death).
    fn fill_death(&self, snap: &mut GameSnapshot) {
        snap.death_damage = self.panel.rows.iter().map(|r| coach_core::DamageRow { source: r.source.clone(), hits: r.hits, damage: r.damage }).collect();
        snap.death_killing_blow = self.panel.rows.iter().find(|r| r.killing_blow).map(|r| r.source.clone());
        snap.death_total_damage = self.panel.total;
        let top = self.panel.rows.first().filter(|r| r.index == 0).map(|r| r.source.clone());
        snap.death_cause = top.or_else(|| self.panel_top_raw.clone()).or_else(|| self.panel.rows.first().map(|r| r.source.clone()));
    }
    fn accept_level(&mut self, read: Option<u32>) -> Option<u32> { crate::hudtext::accept_level(&mut self.level, read) }
    fn scan_inner(&mut self,img:&image::DynamicImage,minimap:Option<&image::RgbImage>,now_ms:u64,located:(Option<u32>,Option<u32>),
        source:Option<(&image::DynamicImage,coach_perception::Bounds)>,no_map:bool)->GameSnapshot{
        let view = crate::hudtext::View { canon: img, source };
        self.regions.clear();
        use crate::screen::{self, Screen};
        use coach_perception::track::LineBatch;
        // One batched recognition call reads every tracked single-line field for this frame.
        // Text *detection* runs only for acquisition, throttled, or for rare fields below.
        let fallback;
        let map_crop = match minimap { Some(m) => m, None => {
            fallback = crate::minimap::crop_from_frame(img); &fallback
        }};
        let mut laps = coach_perception::track::Laps::default();
        // The lane layout is latched per match by the tracker (per-frame colours flipped it).
        let mut map = self.map_analyzer.run_layout(map_crop, &mut vec![], self.map_tracker.layout());
        laps.lap("minimap");
        let mut batch = LineBatch::default();
        // Top-bar readers use a copy with the bar moved back to its calibrated rows when it is
        // drawn lower (rec5 298-326).
        let top_dy = crate::team::top_bar_offset(img);
        let top = crate::team::top_bar_aligned(img, top_dy);
        let top: &image::DynamicImage = &top;
        let rects: Option<Vec<coach_perception::Bounds>> = screen::PROBE_SLOTS.iter().map(|i| self.name_rects.get(*i).copied().flatten()).collect();
        // Line budget: every recognized line costs ~width-proportional model time, so only
        // lines that can change a decision this frame are included.
        let expected = self.clock_value.expected(now_ms);
        let hint = self.clock_hint.take();
        let probe = screen::add_probe(&mut batch, img, rects.as_deref(), !self.seen_hud, self.clock_value.value(now_ms).is_some());
        // Respawn digits only matter while dead: the white digit glyphs are located and read as a
        // tight crop whose digit count must match (the fixed box dropped digits: 30 -> 0, 74 -> 4).
        let respawn_n = if self.was_dead { crate::death::digit_glyphs(img, crate::death::RESPAWN_BOX).map(|(b, n)| (batch.add(crate::death::digits_crop(img, b)), n)) } else { None };
        // Souls are located from the `$` glyph (the row moves with the HUD state) and read right
        // of it; a souls label located by the automatic scanner is the fallback.
        let souls_img = crate::hudtext::souls_area(&view);
        let souls_px = (img.width() == 1600 && img.height() == 900).then(|| crate::hudtext::locate_souls(&souls_img)).flatten();
        let souls_lines = souls_px.map(|s| (batch.add_all(crate::hudtext::souls_crops(&souls_img, &s)), s.count));
        let level_line = souls_px.and_then(|s| crate::hudtext::level_glyphs(&souls_img, &s)).map(|(b, n)| (batch.add(crate::death::digits_crop(&souls_img, b)), n));
        let unsecured_line = souls_px.and_then(|s| crate::hudtext::unsecured_line(&souls_img, &s)).map(|b| batch.add(souls_img.crop_imm(b.x, b.y, b.w, b.h)));
        // Status text / debuff badges on live-HUD frames: a cheap pixel gate (text-like glyph rows
        // in fixed areas) runs every frame; only gated lines (typically none) are recognised.
        let status_crops = if self.seen_hud && !self.was_dead { crate::status::crops(&view) } else { vec![] };
        let status_kinds: Vec<crate::status::Kind> = status_crops.iter().map(|(k, _)| *k).collect();
        let status_lines = (!status_crops.is_empty()).then(|| batch.add_all(status_crops.into_iter().map(|(_, c)| c)));
        // Team totals alternate ally / enemy per frame.
        // Frames without a located minimap do not read totals (see below), so they keep the turn.
        if !no_map { self.totals_flip = !self.totals_flip; }
        let total_line = batch.add(hud::crop(top, if self.totals_flip { hud::ALLY_TOTAL } else { hud::ENEMY_TOTAL }));
        let vitals = self.vitals.tracked_crops(img).map(|c| batch.add_all(c));
        // Zone label under the minimap ("YORK : FACTORY"), read as 1-2 lines every 1.5 s from a
        // rotated strip at a fixed place relative to the map (the canonical context's map box).
        let zone_lines = self.cadence.due("zone_label", now_ms, 1500).then(|| {
            let (fw, fh) = (img.width() as f32, img.height() as f32);
            let strip = crate::zone::label_strip(img, (crate::minimap::FRAC_X * fw, crate::minimap::FRAC_Y * fh, crate::minimap::FRAC_W * fw, crate::minimap::FRAC_H * fh));
            batch.add_all(crate::zone::label_parts(&strip))
        });
        // "DELIVER THE URN" is read when its pale-green glyph pixels are present (a cheap gate on
        // every frame: rec8 27:27-27:41 lost the carry whenever the minimap vase was not found)
        // or when the minimap shows an urn.
        let urn_line = match crate::status::urn_prompt(&view) {
            Some(c) => Some((batch.add(c), true)),
            None => (map.urn_pos.is_some() || !map.urn_marks.is_empty()).then(|| (batch.add(crate::status::urn_prompt_crop(&view)), false)),
        };
        // Objective health banner (a Patron under attack), gated by its bar and name pixels.
        let banner_line = crate::status::objective_banner(&view).map(|(c, health)| (batch.add(c), health));
        // Team banners and earned-souls labels rotate: 3 + 6 of 12 per frame (the 18x14 label
        // crops are cheap; rec5 read 0.5% of slot-frames with 2 per frame and a wide crop).
        let status_slots = self.status_rr.take(12, 3);
        let status = batch.add_all(crate::team::status_crops_for(top, &status_slots));
        let worth_slots = self.worth_rr.take(12, 6);
        let worth = batch.add_all(crate::team::networth_crops_for(top, &worth_slots));
        // While dead, Tab overlays the scoreboard on the death screen (rec5 503-519), which the
        // screen classifier reports as Dead: four hero-name lines tell.
        let board_probe = (self.was_dead || self.was_board).then(|| batch.add_all(crate::scoreboard::hero_line_crops_for(img, &BOARD_PROBE)));
        // Health bars / dead portraits are pixel reads for all twelve slots; only dead slots add
        // a respawn-timer line. Committed below for gameplay frames only (the death screen tints them).
        let mut bar_status = self.team_status.clone();
        let dead_slots = crate::team::apply_bars(top, &mut bar_status);
        let respawn_lines = batch.add_all(crate::team::respawn_crops(top, &dead_slots));
        // Ability bar: a pixel test of the four icons at most every 2 s on live-HUD frames; only
        // slots showing cooldown digits add a line.
        let ability_slots = (self.seen_hud && !self.was_dead && self.cadence.due("abilities", now_ms, 2000)).then(|| crate::abilities::slots(img)).flatten();
        let ability_lines: [Option<usize>; 4] = std::array::from_fn(|i| ability_slots
            .and_then(|s| s[i].1).map(|d| batch.add(crate::abilities::digits_crop(img, &d))));
        let batched = batch.len();
        let lines = batch.run(|crops| self.ocr.read_lines(crops));
        laps.lap(&format!("line batch ({batched} lines)"));
        let text = |i: usize| lines.get(i).map(String::as_str).unwrap_or("");

        // Detector name check runs only when the pixel panel cue fires, plus a slow 15 s safety
        // net for unfamiliar themes. Cached exact name rectangles make later checks line-only.
        let (ocr, heroes, cadence) = (&self.ocr, &self.hero_names, &mut self.cadence);
        let (mut scr, raw_clock) = screen::interpret_probe(&lines, &probe, heroes, located.0, expected,
            || (crate::scoreboard::panel_cue(img) || cadence.due("scoreboard_probe", now_ms, 15_000))
                && crate::scoreboard::looks_like_scoreboard(ocr, img, heroes));
        // The small clock text failed to read on 22.5% of a recorded match's frames (stream
        // overlays, effects), which dropped whole gameplay frames, and the death screen was never
        // recognized because its banner check needed a clock read. With a recently verified clock,
        // the respawn banner means dead and the own health bar (only drawn on the live HUD) means
        // gameplay. The extrapolated clock is not fed back as a reading (`raw_clock` stays None).
        if scr == Screen::Unknown && expected.is_some() && self.clock_value.confirmed_age_ms(now_ms).is_some_and(|a| a <= 120_000) {
            if screen::probe_says_respawn(&lines, &probe) { scr = Screen::Dead; }
            else if crate::vitals::bar_ratio(img).is_some() { scr = Screen::Gameplay; }
        }
        // Without a located minimap this frame, gameplay also needs the own health bar (only drawn
        // on the live HUD): rec5 lost the midboss pit, ziplines and base fights to Unknown.
        let bar_now = crate::vitals::bar_ratio(img);
        if no_map && scr == Screen::Gameplay && bar_now.is_none() { scr = Screen::Unknown; }
        // The end-of-match banner keeps the clock and top bar (rec5 570-572: classified Gameplay;
        // its red tint even passes the health-bar check). A pixel gate (a large cream text block)
        // runs the one-line read only when such text is present.
        // The banner is also drawn over the death screen and the Tab scoreboard (rec13 590: Dead
        // with "Respawn in 1"; 591: Scoreboard): a match that ends while you are dead or holding
        // Tab had no PostGame frame at all.
        if matches!(scr, Screen::Gameplay | Screen::Unknown | Screen::Dead | Screen::Scoreboard) && screen::post_game_gate(img) {
            if let Some(r) = self.ocr.read_lines(&[screen::post_game_crop(img)]).ok().and_then(|l| l.into_iter().next()).and_then(|s| screen::post_game_result(&s)) {
                scr = Screen::PostGame;
                self.match_result = Some(r.into());
            }
        }
        // The post-game summary has the shop's "ESC BACK" anchor; its "PLAY AGAIN" button tells
        // (rec13 597-599 were Shop frames with the last clock and hero).
        if scr == Screen::Shop && screen::play_again_gate(img)
            && self.ocr.read_lines(&[screen::play_again_crop(img)]).ok().and_then(|l| l.into_iter().next()).is_some_and(|s| screen::says_play_again(&s)) {
            scr = Screen::PostGame;
        }
        if scr == Screen::Unknown && screen::looks_black(img) { scr = Screen::Loading; }
        laps.lap("screen + scoreboard check");
        // The pixel-located row is authoritative when found; the label located by the automatic
        // scanner and the fixed box are fallbacks only when no `$` row was found.
        // The old fixed box is not a fallback any more: without a `$` glyph it read a bare "1" on
        // empty death-screen HUDs and "947" for a clipped "1,947" (rec4).
        let souls_read = match souls_lines.clone() {
            Some((r, n)) => crate::hudtext::souls_value(&[text(r.start), text(r.start + 1)], n),
            None => located.1,
        };
        let level_read = level_line.and_then(|(i, n)| crate::death::read_count(text(i), n));
        if scr == Screen::Shop {
            self.seen_hud = false;
            self.laps = laps;
            return self.shop_snapshot(img, souls_read, souls_lines.is_some(), level_read, now_ms);
        }
        // Only HUD screens yield data; menus, post-game and loading yield an empty snapshot.
        self.seen_hud = scr.has_hud();
        self.was_dead = scr == Screen::Dead;
        let board_visible = scr == Screen::Scoreboard || scr == Screen::Dead && board_probe.clone().is_some_and(|r| lines[r].iter().filter(|l| crate::scoreboard::match_hero(l, &self.hero_names).is_some()).count() >= 2);
        self.was_board = board_visible;
        if matches!(scr, Screen::PostGame | Screen::Countdown) { self.between = true; }
        if scr == Screen::PostGame { self.match_over = true; }
        // Post-game screens report the banner's result of this match (the PLAY AGAIN summary after
        // it has no Victory / Defeat word of its own).
        if !scr.has_hud() { self.laps = laps; return GameSnapshot { screen: Some(format!("{scr:?}")),
            match_result: if scr == Screen::PostGame { self.match_result.clone() } else { None }, ..Default::default() }; }
        // New match? Decided before anything of this frame is read into the match's memory: a
        // first scoreboard of the new match must not vote for the old match's slots.
        let evidence = BoundaryEvidence { between: self.between, gap_ms: self.last_hud_ms.map(|t| now_ms.saturating_sub(t)),
            raw_clock, expected, verified: None, clock_max: self.clock_max };
        self.last_hud_ms = Some(now_ms);
        // The boundary screen is forgotten once the clock continues the match (a false end banner).
        if raw_clock.is_some_and(|c| expected.is_none_or(|x| x.abs_diff(c) <= 10)) { self.between = false; }
        if raw_clock.is_some() { self.match_over = false; self.match_result = None; }
        if match_boundary(&evidence) == Boundary::Before {
            self.new_match(false);
            self.clock_hint = hint;
            return self.scan_inner(img, minimap, now_ms, located, source, no_map);
        }
        // Max HP changes with levels: a level-up makes the remembered max stale, so the bar-only
        // HP estimate (bar x remembered max) must wait for a fresh max read (rec5 frame 481
        // reported 2786/3144 at a level-up to 3199/3199).
        let level_before = self.level.0;
        let level_now = self.accept_level(level_read);
        if level_now.zip(level_before).is_some_and(|(n, b)| n != b) { self.vitals.forget_max(); }

        // New match? Reset all per-match memory. The id needs detector OCR, so it is sampled.
        let mid = if self.cadence.due("match_id", now_ms, 60_000) { self.text(&img, screen::match_id_region()).and_then(|s| parse::parse_match_id(&s)) } else { None };
        // OCR of the id is noisy, so a different id must be seen twice in a row to count. A new
        // match also restarts the clock: while the verified clock continues the current match's,
        // a different id is a misread (rec8's 126566052 read 6566052 and 176566052 twice in a row
        // and reset hero identities and your slot four times mid-match).
        let clock_now = self.clock_value.value(now_ms);
        let restarted = clock_restarted(clock_now, self.clock_max);
        match (self.match_id, mid) {
            (None, Some(n)) => self.match_id = Some(n),
            (Some(old), Some(n)) if digit_diff(old, n) >= 3 => {
                if self.pending_match == Some(n) && restarted {
                    self.new_match(true);
                    self.match_id = Some(n);
                } else { self.pending_match = Some(n); }
            }
            (Some(_), Some(_)) => self.pending_match = None, // same id, or single-digit OCR noise
            _ => {}
        }

        laps.lap("match id");
        let mut snap = GameSnapshot { match_id: self.match_id, screen: Some(format!("{scr:?}")), ..Default::default() };
        // The clock advances with wall time: cheap batched reads only confirm the extrapolation;
        // the reliable detector read anchors it and re-verifies on repeated disagreement.
        if self.clock_value.observe(raw_clock, now_ms) {
            // Fast path: one detector pass on the fixed clock box, accepted when it agrees with
            // *independent* evidence: the verified extrapolation, a supplied located clock, or a
            // glyph-located hint. Cheap fixed-box reads are not independent: they share any pixel
            // offset (a 1 px shift read 5:13 as 6:13 in every fixed box). Otherwise locate the
            // exact glyphs and read glyph-scaled context with a vote.
            let fixed = self.ocr.read(&hud::crop(img, hud::CLOCK)).ok().and_then(|s| parse::parse_clock(&s)).filter(|v| *v <= 14400);
            let near = |a: Option<u32>| matches!((fixed, a), (Some(f), Some(e)) if f.abs_diff(e) <= 1);
            // Independent evidence that itself lacks the tens-of-minutes digit of the verified
            // extrapolation does not vouch for a fixed read (rec8 live: a mis-scaled context and a
            // narrow window-scale box both read "5:31" for 15:31 and "agreed").
            let tens_drop = matches!((fixed, expected), (Some(f), Some(e)) if f < e && coach_perception::track::tens_of_minutes_apart(f, e));
            let agrees = near(expected) || (!tens_drop && (near(located.0) || near(hint)));
            // The glyph search (~3 s in WASM) runs only when the fixed read is suspect: first anchor,
            // no read, or a whole-minute-only difference (the measured misread pattern).
            let minute_only = [expected, raw_clock, located.0, hint].into_iter().flatten()
                .any(|e| fixed.is_some_and(|f| f.abs_diff(e) >= 59 && matches!((f as i64 - e as i64).rem_euclid(60), 0 | 1 | 59)));
            // Once the fixed box has disagreed with the glyph verifier in this context, it stays
            // distrusted until reset (the context's pixel offset does not change).
            let suspect = !agrees && (self.fixed_clock_distrusted || expected.is_none() || fixed.is_none() || minute_only);
            let verified = if !suspect { fixed } else {
                let (fw, fh) = (img.width() as f32, img.height() as f32);
                let area = coach_perception::Bounds { x: (0.5 * fw) as u32, y: ((hud::CLOCK.y + hud::CLOCK.h / 3.0) * fh) as u32, w: (hud::CLOCK.w * fw) as u32, h: (hud::CLOCK.h * fh) as u32 };
                let glyph = crate::localize::locate_clock_limited(img, &self.ocr, Some(area), 1).map(|(v, _)| v).filter(|v| *v <= 14400);
                if glyph.is_some() && fixed.is_some() && glyph != fixed { self.fixed_clock_distrusted = true; }
                glyph.or(fixed)
            };
            self.clock_value.verify(verified.or(located.0), now_ms);
        }
        snap.game_clock_secs = self.clock_value.value(now_ms);
        // The verified clock restarted (> 120 s to < 90 s): a new match even with the same id
        // read (rec11 -> rec12). This frame is read again into the new match's memory.
        let verified_now = snap.game_clock_secs;
        if match_boundary(&BoundaryEvidence { verified: verified_now, clock_max: self.clock_max, ..Default::default() }) == Boundary::Verified {
            self.new_match(true);
            self.clock_max = verified_now;
            self.last_hud_ms = Some(now_ms);
            self.clock_hint = hint;
            return self.scan_inner(img, minimap, now_ms, located, source, no_map);
        }
        if let Some(c) = verified_now { self.clock_max = Some(self.clock_max.map_or(c, |m| m.max(c))); }
        let raw_clock = snap.game_clock_secs.or(raw_clock);
        laps.lap("clock verify");

        // Personal carry has a literal HUD instruction. Unlike proximity to a hero ring,
        // this can establish that YOU carry the urn, except while spectating.
        let mut carrying_urn = false;
        if let Some((i, gated)) = urn_line {
            let instruction = text(i).to_ascii_uppercase();
            let says = if gated { crate::status::says_deliver_urn(&instruction) } else { instruction.contains("DELIVER") && instruction.contains("URN") };
            if scr != Screen::Dead && raw_clock.unwrap_or(0) >= 600 && says {
                map.urn = Some(coach_core::UrnState::CarriedByYou);
                carrying_urn = true;
            }
        }
        if let Some((i, health)) = banner_line.filter(|_| matches!(scr, Screen::Gameplay | Screen::Dead)) {
            if let Some(name) = crate::status::match_banner(text(i)) {
                snap.objective_banner = Some(coach_core::ObjectiveBanner { name: name.into(), own: crate::status::banner_is_own(name, self.map_tracker.layout()), health_ratio: Some((health * 100.0).round() / 100.0) });
            }
        }
        // A cyan portrait must never create an urn before its first spawn.
        if raw_clock.unwrap_or(0) < 600 { map.urn = None; map.urn_pos = None; map.urn_marks.clear(); }
        if let Some(l) = zone_lines.clone().and_then(|r| lines.get(r)) { map.zone = crate::zone::zone_from_lines(l); }
        if !no_map { snap.minimap = Some(self.map_tracker.observe(map, now_ms)); }
        // Lane structures: read on frames with the minimap, states held across frames without it.
        if !no_map { let reads = crate::structures::read(map_crop); self.structures.observe(reads.as_ref(), now_ms); }
        snap.structures = self.structures.states(now_ms, self.map_tracker.layout().or(snap.minimap.as_ref().and_then(|m| m.lanes_mirrored)));
        // Ability bar (live HUD only; the death screen shows the spectated hero's).
        if scr == Screen::Gameplay {
            if let Some(s) = ability_slots {
                let mut digits: [Vec<String>; 4] = std::array::from_fn(|i| ability_lines[i].map(|l| vec![text(l).to_string()]).unwrap_or_default());
                // Slots whose read does not match the glyph count get one retry crop (one call).
                let retry: Vec<(usize, crate::abilities::Digits)> = (0..4).filter_map(|i| s[i].1.filter(|d| digits[i].iter().all(|t| crate::abilities::cooldown_secs(t, d).is_none())).map(|d| (i, d))).collect();
                if !retry.is_empty() {
                    let crops: Vec<_> = retry.iter().map(|(_, d)| crate::abilities::digits_retry_crop(img, d)).collect();
                    if let Ok(t) = self.ocr.read_lines(&crops) { for ((i, _), t) in retry.iter().zip(t) { digits[*i].push(t); } }
                }
                self.abilities = Some((now_ms, crate::abilities::states(&s, &digits)));
            }
            // Between reads (cadence) the last read is held briefly, cooldowns counting down.
            if let Some((at, a)) = self.abilities.as_ref().filter(|(at, _)| now_ms >= *at && now_ms - at <= 2500) {
                let gone = ((now_ms - at) / 1000) as u32;
                snap.abilities = a.iter().map(|s| coach_core::AbilityState { cooldown_secs: s.cooldown_secs.map(|c| c.saturating_sub(gone)), ..s.clone() }).collect();
            }
        }

        if scr == Screen::Dead {
            snap.dead = true;
            // Respawn countdown: a read continues the running countdown (±2 s) or starts a new one
            // after two agreeing reads (rec5: 30 read as 0, 7 as 77); between reads it is
            // extrapolated for at most 10 s, with its age.
            let read = match respawn_n {
                Some((i, n)) => crate::death::read_count(text(i), n),
                None => crate::death::digit_glyphs(img, crate::death::RESPAWN_BOX).and_then(|(b, n)|
                    self.ocr.read_lines(&[crate::death::digits_crop(img, b)]).ok().and_then(|l| l.first().and_then(|s| crate::death::read_count(s, n)))),
            };
            // No countdown read for 15 s (a recording gap, a long menu): this may be another death.
            if self.respawn.age_ms(now_ms).is_some_and(|a| a > 15_000) { self.respawn.reset(); self.panel = Default::default(); self.panel_top_raw = None; }
            let (secs, fresh) = self.respawn.observe(read, now_ms);
            // A new countdown (the timer jumped up) is a new death: its panel starts over.
            if fresh { self.panel = Default::default(); self.panel_top_raw = None; }
            snap.respawn_secs = secs;
            snap.respawn_age_ms = secs.and(self.respawn.age_ms(now_ms));
            // The damage panel keeps filling in after the death (rec5 frame 291: top row Still Life
            // 613, frame 300: Aura of Suffering 789), so it is re-read every 2 s until its Total
            // Damage repeats. Rows are line reads on the full-resolution panel.
            if !self.panel.stable && self.cadence.due("death_panel", now_ms, 2000) {
                let size = crate::death::panel_size(&view);
                let panel = view.region(crate::death::PANEL_CANONICAL, size, size);
                let (rows, crops) = crate::death::panel_crops(&panel);
                if !rows.is_empty() {
                    let lines = self.ocr.read_lines(&crops).unwrap_or_default();
                    let (parsed, total) = crate::death::parse_panel(&panel, &rows, &lines);
                    self.panel.update(parsed, total, now_ms);
                }
            }
            // When the line reads did not identify the top row (small 1600x900 text, world sources
            // without an icon), the detector reads it, at most every 5 s: death_cause stays the top
            // source as before, normalised against the dictionary when it matches.
            if self.panel.rows.first().is_none_or(|r| r.index != 0) && self.cadence.due("death_top", now_ms, 5000) {
                let raw = self.ocr.read(&hud::crop(img, crate::Region { x: 1352.0 / 1600.0, y: 276.0 / 900.0, w: 200.0 / 1600.0, h: 22.0 / 900.0 })).ok();
                if let Some(c) = raw.as_deref().and_then(|s| crate::death::normalise_source(s).or_else(|| clean_cause(s))) { self.panel_top_raw = Some(c); }
            }
            self.fill_death(&mut snap);
        } else if scr == Screen::Gameplay {
            // Only a live HUD ends the death; menus and the scoreboard flicker in between.
            self.respawn.reset(); self.panel = Default::default(); self.panel_top_raw = None;
        }
        if scr != Screen::Dead {
            if scr == Screen::Gameplay {
                let bar = crate::vitals::bar_ratio(img);
                self.vitals.set_bar(bar);
                let mut reading = vitals.clone().and_then(|r| self.vitals.accept_lines(text(r.start), text(r.start + 1), now_ms))
                    .filter(|r| self.vitals.agrees_with_bar(r, bar)).filter(|r| self.vitals.max_plausible(r.max, now_ms));
                // Low HP: a tracked read is re-read by the detector (whole number); the tracked
                // value is the fallback when the detector cannot read it.
                if reading.is_none() {
                    if let Some((hp, max)) = self.vitals.take_redetect() {
                        reading = self.vitals.read(&self.ocr, img, now_ms).filter(|r| self.vitals.agrees_with_bar(r, bar)).filter(|r| self.vitals.max_plausible(r.max, now_ms));
                        if reading.is_none() && hp <= max {
                            let held = self.vitals.accept_held(hp, max, now_ms);
                            reading = Some(held).filter(|r| self.vitals.agrees_with_bar(r, bar)).filter(|r| self.vitals.max_plausible(r.max, now_ms));
                        }
                    }
                }
                // Lost or suspicious tracking re-acquires (detector) right away, throttled.
                if reading.is_none() && !self.vitals.is_tracking() && self.vitals.may_acquire(now_ms) {
                    reading = self.vitals.read(&self.ocr, img, now_ms).filter(|r| self.vitals.agrees_with_bar(r, bar)).filter(|r| self.vitals.max_plausible(r.max, now_ms));
                }
                if let Some(v) = reading {
                    snap.hp=Some(v.hp); snap.max_hp=Some(v.max); snap.health_observations=v.observations; snap.health_delta_ratio=v.delta;
                    self.regions=v.regions;
                } else if let Some((hp, max)) = self.vitals.from_bar(now_ms) {
                    // Digits unreadable (damage vignette, effects): the bar is still a measurement.
                    snap.hp=Some(hp); snap.max_hp=Some(max); snap.health_observations=1;
                }
            }
            // Items: non-flat slots matched against shop art with a confidence gate. A slot's item
            // is listed once the same match repeats on two scans, and only if it is a real shop
            // item affordable by now (a recorded match listed 6,400 and placeholder tier-5 items,
            // each for 1-5 frames, from blurred icons).
            let clock = snap.game_clock_secs.or(raw_clock);
            for row in 0..2 {
                for col in 0..6 {
                    let c = hud::crop(&img, hud::item_slot(col, row));
                    let seen = if hud::contrast(&c) < 18.0 { ItemRead::Empty } else {
                        match self.items.identify(&c)
                            .filter(|(name, score, margin)| *score >= 0.5 && *margin >= 0.1 && !name.contains("non_player") && plausible_item(name, clock)) {
                            Some((name, _, _)) => ItemRead::Item(name),
                            None => ItemRead::Unclear,
                        }
                    };
                    let latch = &mut self.item_latch[row * 6 + col];
                    latch_item(latch, seen);
                    if let Some(n) = &latch.item { if !snap.items.contains(n) { snap.items.push(n.clone()); } }
                }
            }
        }

        // Souls (also on the death screen when its HUD shows them: rec5's death HUD showed the
        // same values as the shop opened during that death and the first frame after respawning;
        // while spectating another hero the row is absent). A drop is a purchase right away when
        // the shop was seen in the last 30 s or a new item appeared; otherwise it needs a second read.
        // A new item explains a drop only when the drop matches its shop cost (item matches
        // flicker: rec5 frame 335 "gained" an item it did not buy).
        let new_cost: u32 = if self.prev_items.is_empty() { 0 } else { snap.items.iter().filter(|i| !self.prev_items.contains(i)).filter_map(|i| item_costs().get(i)).sum() };
        if !snap.items.is_empty() { self.prev_items = snap.items.clone(); }
        let drop = self.souls.held(now_ms).zip(souls_read).filter(|((h, _), r)| r < h).map(|((h, _), r)| h - r);
        let bought = new_cost > 0 && drop.is_some_and(|d| d + 300 >= new_cost / 2 && d <= new_cost + 500);
        let spending = bought || self.shop_seen.is_some_and(|t| now_ms >= t && now_ms - t <= 30_000);
        if let Some((v, age)) = self.souls.observe_with(souls_read, now_ms, spending, scr == Screen::Dead || souls_lines.is_none()) { snap.souls = Some(v); snap.souls_age_ms = Some(age); }
        snap.level = level_now;
        snap.unsecured_souls = self.unsecured.observe(unsecured_line.and_then(|i| crate::hudtext::parse_unsecured(text(i))), now_ms);
        if scr == Screen::Gameplay {
            if let Some(r) = status_lines.clone() { snap.status_effects = crate::status::interpret(&status_kinds, &lines[r]); }
            if carrying_urn { snap.status_effects.push("carrying_urn".into()); }
        }
        laps.lap("vitals + souls + items");
        if scr == Screen::Dead { self.vitals=crate::vitals::Tracker::default(); }

        // Heroes: accumulate card scores across frames, assign uniquely to the 12 slots.
        // Identities change rarely, so portraits are sampled every 2 s (scores keep top-K).
        let sampled = raw_clock.is_some() && self.cadence.due("portraits", now_ms, 2000);
        if sampled {
            self.frames += 1;
            for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
                let c = hud::crop(top, hud::portrait(*cx));
                // Best variant per hero: card art, critical tint, and real top-bar captures
                // (`hero_x@...`, e.g. Haze's near-black silhouette, which the card art misses).
                let mut best: HashMap<String, f32> = HashMap::new();
                for (n, s) in self.heroes.scores(&c).into_iter().chain(self.heroes_crit.scores(&c)) {
                    let e = best.entry(hero_key(&n).to_string()).or_insert(f32::MIN);
                    *e = e.max(s);
                }
                // Identity vote: this sample's confident winner. The top-K means below keep each
                // hero's best-ever scores, so one spurious high read in another slot could take a
                // hero away for good (rec2 ally slot 2 read Bookworm for 2 minutes while every
                // sample's winner was Bebop). Dead / greyed samples rarely clear the margin.
                // A dead (greyed) portrait neither votes nor ages the votes: over a long late-game
                // death its winner is noise (rec2 slot 2 flipped to Bookworm during a respawn).
                let dead = dead_slots.contains(&i) || bar_status.get(i).is_some_and(|s| s.dead == Some(true));
                let mut ranked: Vec<(&String, f32)> = best.iter().map(|(n, s)| (n, *s)).collect();
                ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
                if !dead { for v in self.hero_votes[i].values_mut() { *v *= HERO_VOTE_DECAY; } }
                if let (false, [(n, s1), (_, s2), ..]) = (dead, ranked.as_slice()) {
                    let winner = (*s1 >= 0.5 && s1 - s2 >= 0.03).then(|| (n.as_str(), s1 - s2));
                    if let Some((n, _)) = winner { *self.hero_votes[i].entry(n.to_string()).or_default() += 1.0; }
                    // The latch keeps a confident identity against a stream of weak contrary
                    // winners; only sustained strong ones (or name reads) switch it.
                    self.hero_latch.sample(i, winner, &self.hero_votes);
                }
                for (n, s) in best {
                    let v = self.sums[i].entry(n).or_default();
                    v.push(s);
                    v.sort_by(|a, b| b.partial_cmp(a).unwrap());
                    v.truncate(TOP_K);
                }
            }
        }
        // Identities do not change within a match: the per-slot majority of the sample votes is
        // reported (rec5 slot 3 flipped Vyper / Infernus), unique across slots; the top-K
        // assignment only fills slots without enough votes. Votes decay (x0.95 per 2 s sample,
        // half-life ~30 s) so a few early misreads stop holding a slot.
        let fresh = self.assign_heroes();
        let mut assign = latched_assignment(&self.hero_latch.heroes(), &self.hero_votes, &fresh);
        laps.lap("portraits");

        // Scoreboard: names are far more reliable than portraits, so they win.
        // Read on Tab screens only, including Tab over the death screen. Hero names come from the
        // cheap per-column hero-name line; the detector runs only until every slot's name
        // rectangle (used by the screen probe) is known.
        if board_visible {
            let acquire = self.name_rects.iter().any(Option::is_none);
            let (mut rows, found) = crate::scoreboard::read_board(&self.ocr, &self.items, &self.hero_names, &img, acquire);
            for (slot, rect) in self.name_rects.iter_mut().zip(found) { if rect.is_some() { *slot = rect; } }
            // K/D/A never decrease within a match: a lower read than the previous board is a misread.
            if let Some(prev) = self.board_history.last() {
                for (r, p) in rows.iter_mut().zip(prev) {
                    if r.kills.zip(p.kills).is_some_and(|(a, b)| a < b) { r.kills = None; }
                    if r.deaths.zip(p.deaths).is_some_and(|(a, b)| a < b) { r.deaths = None; }
                    if r.assists.zip(p.assists).is_some_and(|(a, b)| a < b) { r.assists = None; }
                }
            }
            for (i, r) in rows.iter().enumerate() { if let Some(h) = &r.hero {
                assign[i] = Some(h.clone());
                // A name read outweighs several portrait samples in the identity vote.
                if let Some(v) = self.hero_votes.get_mut(i) { *v.entry(h.clone()).or_default() += 3.0; }
                self.hero_latch.board(i, h, &self.hero_votes);
            } }
            self.board_history.push(rows.clone());
            if self.board_history.len() > 5 { self.board_history.remove(0); }
            self.last_board = self.voted_board();
            snap.players = self.last_board.clone();
            // Your row: the slot whose bar follows your HP, else the ally row of your hero.
            let own_row = own_slot(&self.own_slot).or_else(|| self.own_hero.as_ref().and_then(|h| (0..6).find(|i| assign[*i].as_ref() == Some(h))));
            if let Some(own) = own_row.and_then(|s| rows.get(s)) {
                snap.kills = own.kills; snap.deaths = own.deaths; snap.assists = own.assists;
            }
        }
        for (i, r) in self.last_board.iter().enumerate() {
            if let Some(h) = &r.hero { if assign[i].is_none() { assign[i] = Some(h.clone()); } }
        }
        snap.ally_heroes = assign[..6].iter().flatten().cloned().collect();
        snap.enemy_heroes = assign[6..].iter().flatten().cloned().collect();
        // The top bar is drawn on gameplay, death and Tab screens alike (rec5: all 86 death
        // frames showed it, with ally respawn timers), so it is read on every HUD screen.
        // Frames without a located minimap (midboss pit, ziplines, base fights) often have the top
        // bar covered by banners: they keep the previous team state.
        if scr.has_hud() && !no_map {
            self.team_status=crate::team::apply_status_slots(top,&lines[status.clone()],&status_slots,&bar_status);
            let respawn=crate::team::checked_respawn_lines(top,&lines[respawn_lines.clone()],&dead_slots);
            crate::team::apply_respawn(&respawn,&dead_slots,now_ms,&mut self.team_status);
            for(i,status)in self.team_status.iter_mut().enumerate(){status.hero=assign[i].clone();}
            crate::team::apply_networth_slots(&lines[worth.clone()],&worth_slots,snap.game_clock_secs.or(raw_clock).unwrap_or(0),&mut self.team_status);
            snap.hero_status=self.team_status.clone();
            crate::team::positions(&mut snap,map_crop,&self.heroes);
            // While dead the minimap "you" marker follows the spectated hero: not your identity.
            if scr==Screen::Dead{snap.hero=None;snap.own_networth=None;}
            self.team_status=snap.hero_status.clone();
            // You are one of the six ally top-bar slots: the one whose health bar follows your own
            // HP. Accumulate the gap per slot (rec2: 0.035 mean for the right slot, >= 0.25 for
            // every other) and name your hero once one slot clearly wins.
            if let (Some(hp), Some(max)) = (snap.hp, snap.max_hp.filter(|m| *m > 0)) {
                let own = hp as f32 / max as f32;
                for s in snap.hero_status.iter().filter(|s| s.ally && s.slot < 6) {
                    if let Some(r) = s.health_ratio { let e = &mut self.own_slot[s.slot]; e.0 += (r - own).abs(); e.1 += 1; }
                }
            }
            // Once a slot clearly won it stays yours for the match (rec5 lost the hero for 35
            // frames after a respawn while the accumulated gaps were briefly ambiguous); another
            // slot takes over only after winning 10 checks in a row.
            // Not while dead: the death screen spectates a teammate and lit slot 0 on rec11's
            // death frames (your slot was 2), which would latch the wrong hero at match start.
            if scr != Screen::Dead { if let Some(i) = own_backdrop_slot(img) { self.own_backdrop[i] += 1; } }
            // The health-bar correlation needs damage to separate full-HP slots (rec5: no answer for
            // the first 48 frames); the backdrop vote answers within seconds. Bars win when both exist.
            if let Some(slot) = own_slot(&self.own_slot).or_else(|| own_from_votes(&self.own_backdrop)) {
                self.own_latched = match self.own_latched {
                    Some((s, _)) if s == slot => Some((s, 0)),
                    Some((s, n)) if n + 1 < 10 => Some((s, n + 1)),
                    _ => Some((slot, 0)),
                };
            }
            if let Some((slot, _)) = self.own_latched {
                if snap.own_networth.is_none() { snap.own_networth = snap.hero_status.iter().find(|s| s.ally && s.slot == slot).and_then(|s| s.networth); }
            }
            if scr == Screen::Gameplay { if let Some(h) = &snap.hero { self.own_hero = Some(h.clone()); } }
            // Your earned souls stay readable from your own slot label while dead or on Tab.
            if snap.own_networth.is_none() {
                if let Some(h) = snap.hero.clone().or_else(|| self.own_hero.clone()) {
                    snap.own_networth = snap.hero_status.iter().find(|s| s.ally && s.hero.as_ref() == Some(&h)).and_then(|s| s.networth);
                }
            }
        }
        // Your hero does not change while dead or on the scoreboard either: the identity of your
        // latched slot, else the last one named (the minimap marker follows the spectated hero).
        if let Some((slot, _)) = self.own_latched { if assign[slot].is_some() { snap.hero = assign[slot].clone(); } }
        if snap.hero.is_none() && scr != Screen::Gameplay { snap.hero = self.own_hero.clone(); }
        if snap.hero.is_some() && (scr == Screen::Gameplay || self.own_latched.is_some()) { self.own_hero = snap.hero.clone(); }
        // Enemy items seen on the most recent scoreboard.
        for r in self.last_board.iter().filter(|r| !r.ally) {
            for it in &r.items { if !snap.enemy_items.contains(it) { snap.enemy_items.push(it.clone()); } }
        }

        // These fields are tightly calibrated single lines. Direct recognition avoids
        // two full detection-model passes; multiline/uncertain fields keep full detection.
        // A total not confirmed for a minute is stale: it is neither reported nor used as the
        // plausibility baseline (rec5 compared a fresh ally total with a 7-minute-old enemy one
        // after a recording gap and called "ahead" while 16k behind).
        let fresh = |at: Option<u64>| at.is_some_and(|t| now_ms >= t && now_ms - t <= 60_000);
        if !fresh(self.totals_at.0) { self.last_totals.0 = None; }
        if !fresh(self.totals_at.1) { self.last_totals.1 = None; }
        // Frames classified without a located minimap (midboss pit, ziplines, base fights) only
        // report held totals: their top bar is often covered by banners (rec5 357: "1,000").
        // Team-total candidates are limited to what the clock allows (rec5 after its recording gap
        // read "184" as 18k and "197" as ambiguous, so the ally total stuck at 18k-62k and the
        // enemy total stayed unknown for 300 frames) and to the per-hero labels' sum.
        let clock_now = snap.game_clock_secs.or(raw_clock);
        let since = |at: Option<u64>| at.map(|t| now_ms.saturating_sub(t) / 1000).unwrap_or(0);
        let ally = self.totals_flip;
        let mut cands = hud::team_total_candidates_at(text(total_line), clock_now);
        if let Some((lo, hi)) = team_sum_range(&self.team_status, ally) {
            cands.retain(|v| *v >= lo && *v <= hi);
            // A baseline the labels contradict is dropped, not defended: rec5's white enemy
            // digits read "193" as "103" on consecutive frames right after the recording gap.
            let (last, pending) = if ally { (&mut self.last_totals.0, &mut self.pending_totals.0) } else { (&mut self.last_totals.1, &mut self.pending_totals.1) };
            if last.is_some_and(|v| v < lo || v > hi) { *last = None; *pending = None; }
        }
        if no_map {
        } else if ally {
            let a = hud::pick_total(&cands, self.last_totals.0);
            if let Some(v) = plausible(&mut self.last_totals.0, &mut self.pending_totals.0, a, since(self.totals_at.0)) { self.totals_at.0 = Some(now_ms); self.midboss_taker.total(true, v, now_ms); }
        } else {
            let e = hud::pick_total(&cands, self.last_totals.1);
            if let Some(v) = plausible(&mut self.last_totals.1, &mut self.pending_totals.1, e, since(self.totals_at.1)) { self.totals_at.1 = Some(now_ms); self.midboss_taker.total(false, v, now_ms); }
        }
        snap.ally_networth = self.last_totals.0;
        snap.enemy_networth = self.last_totals.1;
        // Midboss taker: decided from the team-total steps around the kill the minimap tracker
        // reported. Frames without the minimap keep the event; a Midboss seen again ends it.
        let down = snap.minimap.as_ref().and_then(|m| if m.midboss_down_ms.is_some() { Some(m.midboss_down_ms) } else if m.midboss_up { Some(None) } else { None });
        snap.midboss_taken_by = self.midboss_taker.observe(down, now_ms, clock_now).map(String::from);
        // One hero cannot hold more than ~45% of its team's total (a recorded match had 6.8%
        // impossible per-hero values, e.g. 12.4k of a 15k team).
        for s in snap.hero_status.iter_mut().chain(self.team_status.iter_mut()) {
            let total = if s.ally { self.last_totals.0 } else { self.last_totals.1 };
            if s.networth.zip(total).is_some_and(|(n, t)| n as f32 > t as f32 * 0.45) { s.networth = None; s.networth_observations = 0; }
        }
        laps.lap("scoreboard + team");
        self.laps = laps;
        snap
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn death_cause_drops_the_killing_blow_tag_and_counts() {
        for (raw, want) in [("Melee kiling Bow 3", "Melee"), ("Meleegilling Blow  23", "Melee"), ("Chimera MKI x232", "Chimera MKI"), ("Hyper Beam 7", "Hyper Beam"), ("T2 Walker Killing Blow x30", "T2 Walker"), ("Meleeiing Bow", "Melee"), ("Chimera MKI ×232", "Chimera MKI"), ("Temple Guardian Killing Blow", "Temple Guardian"), ("Stalkers MarkX", "Stalkers Mark")] {
            assert_eq!(super::clean_cause(raw).as_deref(), Some(want), "{raw}");
        }
    }
}
