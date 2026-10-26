//! Full-frame scanner: JPEG bytes -> GameSnapshot (clock, souls, items, heroes, net worth).
use crate::{hud, ocr::Reader, parse, ColorLibrary};
use coach_core::GameSnapshot;
use std::{collections::HashMap, path::Path};

/// A hero score is the mean of its best K frames (robust to dimmed/critical/dead portraits).
const TOP_K: usize = 5;

fn score_of(v: &[f32]) -> f32 { v.iter().sum::<f32>() / v.len().max(1) as f32 }

pub struct Scanner {
    ocr: Reader,
    items: ColorLibrary,
    heroes: ColorLibrary,
    heroes_crit: ColorLibrary,
    tracker: parse::Tracker,
    /// Summed template scores per slot (0-5 ally, 6-11 enemy) and frames counted.
    sums: Vec<HashMap<String, Vec<f32>>>,
    frames: u32,
    last_totals: (Option<u32>, Option<u32>),
}

/// Team totals only grow (mostly), and not by leaps: reject OCR glitches.
fn plausible(prev: Option<u32>, v: Option<u32>) -> Option<u32> {
    let v = v?;
    match prev {
        Some(p) if v + p / 10 < p || v > p + p / 2 + 3000 => None,
        _ => Some(v),
    }
}

impl Scanner {
    /// `root` is the repo root containing data/.
    pub fn load(root: &Path) -> Result<Scanner, String> {
        let e = |x: std::io::Error| x.to_string();
        Ok(Scanner {
            ocr: Reader::load(&root.join("data/models"))?,
            items: ColorLibrary::load(&root.join("data/icons/shop"), false).map_err(e)?,
            heroes: ColorLibrary::load(&root.join("data/icons/h_card"), true).map_err(e)?,
            heroes_crit: ColorLibrary::load(&root.join("data/icons/h_crit"), true).map_err(e)?,
            tracker: parse::Tracker::default(),
            sums: vec![HashMap::new(); 12],
            frames: 0,
            last_totals: (None, None),
        })
    }

    /// Reset per-match memory (call when the clock goes back to the start).
    pub fn reset(&mut self) {
        self.tracker = parse::Tracker::default();
        self.sums = vec![HashMap::new(); 12];
        self.frames = 0;
        self.last_totals = (None, None);
    }

    fn text(&self, img: &image::DynamicImage, r: crate::Region) -> Option<String> {
        self.ocr.read(&hud::crop(img, r)).ok()
    }

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
            Ok(img) => crate::screen::classify(&self.ocr, &img),
            Err(_) => crate::screen::Screen::Unknown,
        }
    }

    pub fn scan(&mut self, jpeg: &[u8]) -> GameSnapshot {
        let Ok(img) = image::load_from_memory(jpeg) else { return GameSnapshot::default() };
        // Only read gameplay HUD on gameplay frames; other screens yield an empty snapshot.
        if crate::screen::classify(&self.ocr, &img) != crate::screen::Screen::Gameplay { return GameSnapshot::default(); }
        let clock = self.text(&img, hud::CLOCK).and_then(|s| parse::parse_clock(&s));
        let souls = self.text(&img, hud::SOULS).and_then(|s| parse::parse_souls(&s));
        let mut snap = self.tracker.accept(clock, souls);

        // Items: non-flat slots matched against shop art with a confidence gate.
        for row in 0..2 {
            for col in 0..6 {
                let r = hud::item_slot(col, row);
                let c = hud::crop(&img, r);
                if hud::contrast(&c) < 18.0 { continue; }
                if let Some((name, score, margin)) = self.items.identify(&c) {
                    if score >= 0.7 && margin >= 0.15 { snap.items.push(name); }
                }
            }
        }

        // Heroes: accumulate scores across frames, then assign heroes to slots uniquely
        // (a hero can appear only once). Skip frames where the top bar is hidden (no clock).
        if snap.game_clock_secs.is_some() {
            self.frames += 1;
            for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
                let c = hud::crop(&img, hud::portrait(*cx));
                let crit: HashMap<String, f32> = self.heroes_crit.scores(&c).into_iter().collect();
                for (n, s) in self.heroes.scores(&c) {
                    let s = s.max(crit.get(&n).copied().unwrap_or(f32::MIN));
                    let v = self.sums[i].entry(n).or_default();
                    v.push(s);
                    v.sort_by(|a, b| b.partial_cmp(a).unwrap());
                    v.truncate(TOP_K);
                }
            }
        }
        let assign = self.assign_heroes();
        snap.ally_heroes = assign[..6].iter().flatten().cloned().collect();
        snap.enemy_heroes = assign[6..].iter().flatten().cloned().collect();
        let a = self.text(&img, hud::ALLY_TOTAL).and_then(|s| hud::parse_team_total(&s));
        let e = self.text(&img, hud::ENEMY_TOTAL).and_then(|s| hud::parse_team_total(&s));
        snap.ally_networth = plausible(self.last_totals.0, a);
        snap.enemy_networth = plausible(self.last_totals.1, e);
        if snap.ally_networth.is_some() { self.last_totals.0 = snap.ally_networth; }
        if snap.enemy_networth.is_some() { self.last_totals.1 = snap.enemy_networth; }        snap
    }
}
