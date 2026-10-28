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
    tracker: parse::Tracker,
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
    /// Damage panel read once per death.
    death_cause: Option<Option<String>>,
    /// Respawn seconds last read during the current death (a jump up = a new death).
    last_respawn: Option<u32>,
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
    totals_flip: bool,
    /// Per ally slot: summed |slot health - your health| and joint reads (see own_slot).
    own_slot: [(f32, u32); 6],
    /// Per slot: identity votes this match (portrait samples, scoreboard names).
    hero_votes: Vec<HashMap<String, f32>>,
    /// Your latched slot and how many checks in a row another slot has won.
    own_latched: Option<(usize, u32)>,
    /// Per ally slot: frames on which only that slot showed the own-portrait backdrop.
    own_backdrop: [u32; 6],
    /// Per item slot: latched item (see latch_item).
    item_latch: [ItemLatch; 12],
}

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

/// Your slot from the backdrop votes: a clear leader (>= 6 votes, 3x the runner-up).
pub fn own_from_votes(votes: &[u32; 6]) -> Option<usize> {
    let mut v: Vec<(usize, u32)> = votes.iter().copied().enumerate().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    (v[0].1 >= 6 && v[0].1 >= 3 * v[1].1).then_some(v[0].0)
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
fn plausible(prev: &mut Option<u32>, pending: &mut Option<(u32, u32)>, v: Option<u32>) -> Option<u32> {
    let v = v?;
    // A team earns a few hundred souls a second: +8% (+2,500) between reads is already generous.
    // rec5 accepted 121k -> 173k in one read (+43%) and called a lead while 20k behind.
    let in_band = match *prev { Some(p) => v + p / 10 >= p && v <= p + p / 12 + 2500, None => false };
    let count = pending.filter(|(q, _)| v.abs_diff(*q) <= q / 10 + 1000).map(|(_, n)| n + 1).unwrap_or(1);
    let drop = prev.is_some_and(|p| v + p / 10 < p);
    let confirmed = count >= if drop { 10 } else if prev.is_none() { 2 } else { 3 };
    if in_band || confirmed { *prev = Some(v); *pending = None; Some(v) } else { *pending = Some((v, count)); None }
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
            tracker: parse::Tracker::default(),
            sums: vec![HashMap::new(); 12],
            frames: 0,
            last_totals: (None, None),
            pending_totals: (None, None),
            totals_at: (None, None),
            hero_names,
            match_id: None,
            pending_match: None,
            virt_ms: 0,
            last_board: vec![],
            board_history: vec![],
            map_analyzer: crate::minimap::Analyzer::new(),
            map_tracker: crate::minimap::Tracker::default(),
            vitals: crate::vitals::Tracker::default(),
            regions:vec![],
            team_status:vec![],
            cadence: Default::default(),
            death_cause: None,
            last_respawn: None,
            name_rects: vec![None; 12],
            clock_value: Default::default(),
            laps: Default::default(),
            clock_hint: None,
            fixed_clock_distrusted: false,
            status_rr: Default::default(),
            worth_rr: Default::default(),
            seen_hud: false,
            was_dead: false,
            totals_flip: false,
            own_slot: Default::default(),
            hero_votes: vec![HashMap::new(); 12],
            own_latched: None,
            own_backdrop: [0; 6],
            item_latch: Default::default(),
        }
    }

    /// Reset per-match memory (call when the clock goes back to the start).
    pub fn reset(&mut self) {
        self.tracker = parse::Tracker::default();
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
        self.hero_votes = vec![HashMap::new(); 12];
        self.own_latched = None;
        self.own_backdrop = [0; 6];
        self.item_latch = Default::default();
        self.cadence.reset();
        self.death_cause = None;
        self.name_rects = vec![None; 12];
        self.clock_value.reset();
    }

    fn text(&self, img: &image::DynamicImage, r: crate::Region) -> Option<String> {
        self.ocr.read(&hud::crop(img, r)).ok()
    }
    pub fn regions(&self)->Vec<coach_perception::RegionObservation>{self.regions.clone()}

    /// Per-slot identity votes of this match (diagnostics).
    pub fn hero_votes(&self) -> &[HashMap<String, f32>] { &self.hero_votes }

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
        self.scan_inner(img,minimap,now_ms,(None,None))
    }
    pub fn scan_located(&mut self,img:&image::DynamicImage,minimap:Option<&image::RgbImage>,now_ms:u64,clock:Option<u32>,currency:Option<u32>)->GameSnapshot{
        self.scan_inner(img,minimap,now_ms,(clock,currency))
    }
    fn scan_inner(&mut self,img:&image::DynamicImage,minimap:Option<&image::RgbImage>,now_ms:u64,located:(Option<u32>,Option<u32>))->GameSnapshot{
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
        let rects: Option<Vec<coach_perception::Bounds>> = screen::PROBE_SLOTS.iter().map(|i| self.name_rects.get(*i).copied().flatten()).collect();
        // Line budget: every recognized line costs ~width-proportional model time, so only
        // lines that can change a decision this frame are included.
        let expected = self.clock_value.expected(now_ms);
        let hint = self.clock_hint.take();
        let probe = screen::add_probe(&mut batch, img, rects.as_deref(), !self.seen_hud, self.clock_value.value(now_ms).is_some());
        // Respawn digits only matter while dead; Souls only when no tracked value is supplied.
        let respawn_n = self.was_dead.then(|| batch.add(hud::crop(img, screen::respawn_number())));
        let souls_line = located.1.is_none().then(|| batch.add(img.crop_imm(70, 742, 130, 30)));
        // Team totals alternate ally / enemy per frame.
        self.totals_flip = !self.totals_flip;
        let total_line = batch.add(hud::crop(img, if self.totals_flip { hud::ALLY_TOTAL } else { hud::ENEMY_TOTAL }));
        let vitals = self.vitals.tracked_crops(img).map(|c| batch.add_all(c));
        // Zone label under the minimap ("YORK : FACTORY"), read as 1-2 lines every 1.5 s from a
        // rotated strip at a fixed place relative to the map (the canonical context's map box).
        let zone_lines = self.cadence.due("zone_label", now_ms, 1500).then(|| {
            let (fw, fh) = (img.width() as f32, img.height() as f32);
            let strip = crate::zone::label_strip(img, (crate::minimap::FRAC_X * fw, crate::minimap::FRAC_Y * fh, crate::minimap::FRAC_W * fw, crate::minimap::FRAC_H * fh));
            batch.add_all(crate::zone::label_parts(&strip))
        });
        let urn_line = (map.urn_pos.is_some() || !map.urn_marks.is_empty()).then(|| batch.add(hud::crop(img, crate::Region { x: 710.0 / 1600.0, y: 585.0 / 900.0, w: 190.0 / 1600.0, h: 32.0 / 900.0 })));
        // Team banners and earned-souls labels rotate: 4 + 3 of 12 per frame.
        let status_slots = self.status_rr.take(12, 3);
        let status = batch.add_all(crate::team::status_crops_for(img, &status_slots));
        let worth_slots = self.worth_rr.take(12, 2);
        let worth = batch.add_all(crate::team::networth_crops_for(img, &worth_slots));
        // Health bars / dead portraits are pixel reads for all twelve slots; only dead slots add
        // a respawn-timer line. Committed below for gameplay frames only (the death screen tints them).
        let mut bar_status = self.team_status.clone();
        let dead_slots = crate::team::apply_bars(img, &mut bar_status);
        let respawn_lines = batch.add_all(crate::team::respawn_crops(img, &dead_slots));
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
        // Only HUD screens yield data; menus, shop and loading yield an empty snapshot.
        laps.lap("screen + scoreboard check");
        self.seen_hud = scr.has_hud();
        self.was_dead = scr == Screen::Dead;
        if !scr.has_hud() { self.laps = laps; return GameSnapshot { screen: Some(format!("{scr:?}")), ..Default::default() }; }

        // New match? Reset all per-match memory. The id needs detector OCR, so it is sampled.
        let mid = if self.cadence.due("match_id", now_ms, 60_000) { self.text(&img, screen::match_id_region()).and_then(|s| parse::parse_match_id(&s)) } else { None };
        // OCR of the id is noisy, so a different id must be seen twice in a row to count.
        match (self.match_id, mid) {
            (None, Some(n)) => self.match_id = Some(n),
            (Some(old), Some(n)) if digit_diff(old, n) >= 3 => {
                if self.pending_match == Some(n) {
                    self.reset();
                    self.match_id = Some(n);
                    self.pending_match = None;
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
            let agrees = near(expected) || near(located.0) || near(hint);
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
        let raw_clock = snap.game_clock_secs.or(raw_clock);
        laps.lap("clock verify");

        // Personal carry has a literal HUD instruction. Unlike proximity to a hero ring,
        // this can establish that YOU carry the urn, except while spectating.
        if let Some(i) = urn_line {
            let instruction = text(i).to_ascii_uppercase();
            if scr != Screen::Dead && raw_clock.unwrap_or(0) >= 600 && instruction.contains("DELIVER") && instruction.contains("URN") {
                map.urn = Some(coach_core::UrnState::CarriedByYou);
            }
        }
        // A cyan portrait must never create an urn before its first spawn.
        if raw_clock.unwrap_or(0) < 600 { map.urn = None; map.urn_pos = None; map.urn_marks.clear(); }
        if let Some(l) = zone_lines.clone().and_then(|r| lines.get(r)) { map.zone = crate::zone::zone_from_lines(l); }
        snap.minimap = Some(self.map_tracker.observe(map, now_ms));

        if scr == Screen::Dead {
            // While dead the HUD shows the spectated player, so personal values are not ours.
            snap.dead = true;
            snap.souls = self.tracker.last_souls();
            snap.respawn_secs = match respawn_n {
                Some(i) => parse::parse_respawn(text(i)),
                None => self.ocr.read_lines(&[hud::crop(img, screen::respawn_number())]).ok().and_then(|l| l.first().and_then(|s| parse::parse_respawn(s))),
            };
            // The damage panel keeps filling in for several seconds of the death (rec5: top row
            // "Still Life" at first, "Aura of Suffering" once complete), so it is re-read every 5 s
            // (detector OCR) and the latest reading wins. A respawn timer that jumps up is a new
            // death in a flickering run of death screens: its panel starts over.
            if snap.respawn_secs.zip(self.last_respawn).is_some_and(|(now, before)| now > before + 5) { self.death_cause = None; }
            if snap.respawn_secs.is_some() { self.last_respawn = snap.respawn_secs; }
            if self.death_cause.is_none() || self.cadence.due("death_panel", now_ms, 5000) {
                let read = self.ocr.read(&hud::crop(&img, crate::Region {
                    x: 1352.0 / 1600.0, y: 276.0 / 900.0, w: 200.0 / 1600.0, h: 22.0 / 900.0 })).ok().and_then(|s| clean_cause(&s));
                if read.is_some() || self.death_cause.is_none() { self.death_cause = Some(read); }
            }
            snap.death_cause = self.death_cause.clone().flatten();
        } else if scr == Screen::Gameplay {
            // Only a live HUD ends the death; menus and the scoreboard flicker in between.
            self.death_cause = None; self.last_respawn = None;
        }
        if scr != Screen::Dead {
            if scr == Screen::Gameplay {
                let bar = crate::vitals::bar_ratio(img);
                self.vitals.set_bar(bar);
                let mut reading = vitals.clone().and_then(|r| self.vitals.accept_lines(text(r.start), text(r.start + 1), now_ms))
                    .filter(|r| self.vitals.agrees_with_bar(r, bar)).filter(|r| self.vitals.max_plausible(r.max, now_ms));
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
            let souls = located.1.or_else(|| souls_line.and_then(|i| parse::parse_souls(text(i))));
            snap.souls = self.tracker.accept_souls(souls).or(self.tracker.last_souls());
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

        laps.lap("vitals + souls + items");
        if scr == Screen::Dead { self.vitals=crate::vitals::Tracker::default(); }

        // Heroes: accumulate card scores across frames, assign uniquely to the 12 slots.
        // Identities change rarely, so portraits are sampled every 2 s (scores keep top-K).
        let sampled = raw_clock.is_some() && self.cadence.due("portraits", now_ms, 2000);
        if sampled {
            self.frames += 1;
            for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
                let c = hud::crop(&img, hud::portrait(*cx));
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
                    if *s1 >= 0.5 && s1 - s2 >= 0.03 { *self.hero_votes[i].entry((*n).clone()).or_default() += 1.0; }
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
        let mut assign = latched_heroes(&self.hero_votes, &fresh);
        laps.lap("portraits");

        // Scoreboard: names are far more reliable than portraits, so they win.
        if scr == Screen::Scoreboard {
            let (rows, found) = crate::scoreboard::read_with_rects(&self.ocr, &self.items, &self.hero_names, &img);
            for (slot, rect) in self.name_rects.iter_mut().zip(found) { if rect.is_some() { *slot = rect; } }
            for (i, r) in rows.iter().enumerate() { if let Some(h) = &r.hero {
                assign[i] = Some(h.clone());
                // A name read outweighs several portrait samples in the identity vote.
                if let Some(v) = self.hero_votes.get_mut(i) { *v.entry(h.clone()).or_default() += 3.0; }
            } }
            self.board_history.push(rows.clone());
            if self.board_history.len() > 5 { self.board_history.remove(0); }
            self.last_board = self.voted_board();
            snap.players = self.last_board.clone();
        }
        for (i, r) in self.last_board.iter().enumerate() {
            if let Some(h) = &r.hero { if assign[i].is_none() { assign[i] = Some(h.clone()); } }
        }
        snap.ally_heroes = assign[..6].iter().flatten().cloned().collect();
        snap.enemy_heroes = assign[6..].iter().flatten().cloned().collect();
        if scr==Screen::Gameplay {
            self.team_status=crate::team::apply_status_slots(img,&lines[status.clone()],&status_slots,&bar_status);
            crate::team::apply_respawn(&lines[respawn_lines.clone()],&dead_slots,&mut self.team_status);
            for(i,status)in self.team_status.iter_mut().enumerate(){status.hero=assign[i].clone();}
            crate::team::apply_networth_slots(&lines[worth.clone()],&worth_slots,snap.game_clock_secs.or(raw_clock).unwrap_or(0),&mut self.team_status);
            snap.hero_status=self.team_status.clone();
            crate::team::positions(&mut snap,map_crop,&self.heroes);
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
            if let Some(i) = own_backdrop_slot(img) { self.own_backdrop[i] += 1; }
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
        }
        // Your hero does not change while dead or on the scoreboard either.
        if let Some((slot, _)) = self.own_latched { snap.hero = assign[slot].clone(); }
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
        if self.totals_flip {
            let a = hud::pick_total(&hud::team_total_candidates(text(total_line)), self.last_totals.0);
            if plausible(&mut self.last_totals.0, &mut self.pending_totals.0, a).is_some() { self.totals_at.0 = Some(now_ms); }
        } else {
            let e = hud::pick_total(&hud::team_total_candidates(text(total_line)), self.last_totals.1);
            if plausible(&mut self.last_totals.1, &mut self.pending_totals.1, e).is_some() { self.totals_at.1 = Some(now_ms); }
        }
        snap.ally_networth = self.last_totals.0;
        snap.enemy_networth = self.last_totals.1;
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
