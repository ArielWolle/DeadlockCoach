//! Own-health reading. Never use a dead/spectated player's HUD as personal HP.
//!
//! Acquisition (text detection inside the calibrated health panel) finds the current-HP and
//! max-HP boxes once. Afterwards both are read by batched line recognition on those boxes.
//! Two failed reads, or a change in digit count (which can widen/narrow the number past the
//! tracked box), drop tracking so the next frame re-acquires.
use crate::{hud, ocr::Reader, Region};
use coach_perception::{track::Tracked, Bounds, RegionObservation};
use image::DynamicImage;

pub const CURRENT_PANEL: Region = Region { x: 350.0 / 1600.0, y: 575.0 / 900.0, w: 145.0 / 1600.0, h: 100.0 / 900.0 };
pub const MAX_PANEL: Region = Region { x: 145.0 / 1600.0, y: 675.0 / 900.0, w: 110.0 / 1600.0, h: 45.0 / 900.0 };

/// Current-HP fraction from the large slanted health bar beside the number (1600x900 frame).
/// Measured on recorded frames: the bar runs from about (385,580) at the top to (428,770) at the
/// bottom and fills bottom-up. White = current HP; red = damage just taken; green = incoming
/// heal; dark grey = missing. Returns None when the bar is not clearly visible.
/// Allowed difference between the number and the bar: the bar measured within ~0.03 of correct
/// readings, so near-empty bars catch a dropped digit ("10" for 104/964) that a fixed 15-point
/// tolerance missed.
pub fn bar_tolerance(bar: f32) -> f32 { 0.06 + 0.15 * bar }
/// `bar_ratio` measured at full health (the top rows sit under the frame's tab).
pub const BAR_FULL: f32 = 0.97;
pub fn bar_ratio(frame: &DynamicImage) -> Option<f32> {
    if frame.width() != 1600 || frame.height() != 900 { return None; }
    let rgb = frame.crop_imm(360, 570, 90, 210).to_rgb8();
    let (mut white, mut other_bar, mut rows) = (0u32, 0u32, 0u32);
    for y in 582u32..=766 {
        let cx = 385.0 + (y as f32 - 580.0) * 43.0 / 190.0 - 2.0;
        let mut votes = [0u8; 3]; // white, bar non-white, unknown
        for dx in -2i32..=2 {
            let p = rgb.get_pixel((cx as i32 + dx - 360) as u32, y - 570);
            let (h, s, v) = crate::minimap::hsv(p[0], p[1], p[2]);
            let k = if v > 0.7 && s < 0.25 { 0 }
                else if (h < 22.0 || h > 340.0) && s > 0.45 && v > 0.35 || (70.0..160.0).contains(&h) && s > 0.35 && v > 0.45 || s < 0.3 && (0.18..0.6).contains(&v) { 1 }
                else { 2 };
            votes[k] += 1;
        }
        rows += 1;
        if votes[0] >= 3 { white += 1; } else if votes[1] >= 3 { other_bar += 1; }
    }
    // Most rows must be bar-coloured; otherwise the bar is hidden (menus, effects) or absent.
    ((white + other_bar) as f32 >= rows as f32 * 0.85).then(|| white as f32 / rows as f32)
}

#[derive(Default)]
pub struct Tracker {
    previous: Option<(u64, u32, u32, u32)>,
    hp: Tracked,
    max: Tracked,
    digits: (usize, usize),
    /// Latest health-bar fraction (`bar_ratio`) for the frame being read.
    bar: Option<f32>,
    /// A >30% single-read jump seen once without health-bar support; accepted if repeated.
    pending_jump: Option<(u32, u32)>,
    /// An implausible max-HP value and how many reads in a row repeated it.
    max_pending: Option<(u32, u8)>,
    /// Last max HP that passed max_plausible (the bar fallback scales by it).
    trusted_max: Option<(u64, u32)>,
}
pub struct Reading { pub hp: u32, pub max: u32, pub observations: u32, pub delta: Option<f32>, pub regions: Vec<RegionObservation> }

fn numbers(text: &str) -> Vec<u32> {
    text.split(|c: char| !c.is_ascii_digit() && c != ',').map(|s| s.replace(',', "")).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect()
}
/// Exact detected text box. Padding was measured to pull in neighbouring glyphs and break
/// recognition of the italic HP digits; the batch upscales small crops instead.
fn pad(b: Bounds, frame: &DynamicImage) -> (u32, u32, u32, u32) {
    (b.x, b.y, b.w.min(frame.width() - b.x), b.h.min(frame.height() - b.y))
}
fn region(id: &str, b: Bounds) -> RegionObservation {
    RegionObservation { id: id.into(), label: format!("{id} · tracked"), bounds: b, origin: coach_perception::RegionOrigin::Detected, score: None, value_path: Some(format!("/{id}")) }
}

impl Tracker {
    /// Crops for batched recognition when both boxes are tracked: [current, max].
    pub fn tracked_crops(&self, frame: &DynamicImage) -> Option<[DynamicImage; 2]> {
        let (h, m) = (self.hp.bounds?, self.max.bounds?);
        let valid = |b: Bounds| b.x + b.w <= frame.width() && b.y + b.h <= frame.height();
        if !valid(h) || !valid(m) { return None; }
        let (a, b, c, d) = pad(h, frame); let (e, f, g, i) = pad(m, frame);
        Some([frame.crop_imm(a, b, c, d), frame.crop_imm(e, f, g, i)])
    }
    pub fn is_tracking(&self) -> bool { self.hp.bounds.is_some() && self.max.bounds.is_some() }
    /// Set the health-bar fraction of the frame about to be read.
    pub fn set_bar(&mut self, bar: Option<f32>) { self.bar = bar; }
    fn bar_supports(&self, hp: u32, max: u32) -> bool {
        self.bar.is_some_and(|b| { let b = (b / BAR_FULL).min(1.0); (hp as f32 / max.max(1) as f32 - b).abs() <= bar_tolerance(b) })
    }

    /// Interpret batched line reads of the tracked boxes.
    pub fn accept_lines(&mut self, current: &str, maximum: &str, now: u64) -> Option<Reading> {
        let hp = numbers(current).into_iter().next();
        let maxima: Vec<u32> = numbers(maximum).into_iter().filter(|n| (100..=20000).contains(n)).collect();
        // A tracked box can truncate a number that just gained a digit (999 -> 1,021 read as
        // "102"). A single-read change beyond 30% of max HP is therefore confirmed by an
        // immediate detector re-acquisition rather than trusted (real bursts are confirmed too).
        let jump = |hp: u32, max: u32| self.previous.is_some_and(|(t, old, _, _)| now >= t && now - t <= 20000 && hp.abs_diff(old) as f32 > max as f32 * 0.3);
        match (hp, maxima.as_slice()) {
            (Some(hp), [max]) if hp <= *max && jump(hp, *max) && !self.bar_supports(hp, *max) => { self.hp.reset(); self.max.reset(); None }
            (Some(hp), [max]) if hp <= *max => {
                let (hb, mb) = (self.hp.bounds?, self.max.bounds?);
                let digits = (hp.to_string().len(), max.to_string().len());
                let mut r = self.accept(hp, *max, now); r.regions = vec![region("hp", hb), region("max_hp", mb)];
                // The padded crop covers a one-digit width change, so the value is used now;
                // the boxes are re-acquired afterwards to re-center on the new number width.
                if digits != self.digits { self.hp.reset(); self.max.reset(); } else { self.hp.hit(hb); self.max.hit(mb); }
                Some(r)
            }
            _ => { if self.hp.miss() | self.max.miss() { self.hp.forget(); self.max.forget(); } self.previous = None; None }
        }
    }

    /// Text-detection acquisition. Expensive (~2 detector passes); call only when not tracking.
    pub fn read(&mut self, reader: &Reader, frame: &DynamicImage, now: u64) -> Option<Reading> {
        let raw = (|| {
            let current = hud::crop(frame, CURRENT_PANEL);
            let mut words = reader.read_word_regions(&current).ok()?;
            // Current HP is the dominant large number. Smaller max-HP/damage labels must not win.
            words.retain(|r| r.label.chars().all(|c| c.is_ascii_digit() || c == ',') && r.label.len() <= 6);
            words.sort_by_key(|r| std::cmp::Reverse(r.bounds.w * r.bounds.h));
            let mut hp_box = words.first()?.bounds;
            let hp = words.first()?.label.replace(',', "").parse::<u32>().ok()?;
            let maxima = reader.read_spans(&hud::crop(frame, MAX_PANEL), 3).ok()?;
            let mut values = vec![];
            for r in maxima { for n in numbers(&r.label).into_iter().filter(|n| (100..=20000).contains(n)) { values.push((n, r.bounds)); } }
            if values.len() != 1 { return None; }
            let (max, mut max_box) = values.pop()?;
            hp_box.x += (CURRENT_PANEL.x * frame.width() as f32) as u32; hp_box.y += (CURRENT_PANEL.y * frame.height() as f32) as u32;
            max_box.x += (MAX_PANEL.x * frame.width() as f32) as u32; max_box.y += (MAX_PANEL.y * frame.height() as f32) as u32;
            (hp <= max).then_some((hp, max, hp_box, max_box))
        })();
        match raw {
            Some((hp, max, hb, mb)) => {
                self.hp.hit(hb); self.max.hit(mb); self.digits = (hp.to_string().len(), max.to_string().len());
                // A fresh detector read is not trusted over a >30% jump either (a fight frame read
                // 104 for 386 while effects hid the bar): it needs the bar or a repeat next frame.
                let jump = self.previous.is_some_and(|(t, old, _, _)| now >= t && now - t <= 20000 && hp.abs_diff(old) as f32 > max as f32 * 0.3);
                if jump && !self.bar_supports(hp, max) && !self.pending_jump.is_some_and(|(p, m)| p.abs_diff(hp) as f32 <= m as f32 * 0.05) {
                    self.pending_jump = Some((hp, max)); return None;
                }
                self.pending_jump = None;
                let mut r = self.accept(hp, max, now); r.regions = vec![region("hp", hb), region("max_hp", mb)]; Some(r)
            }
            None => { self.previous = None; None }
        }
    }
    /// Cross-check a numeric reading against the health bar (`bar_ratio`). A tracked box can lock
    /// onto a truncated number (794 read as "79" for 20 s in a recorded match); on a disagreement
    /// beyond 15 percentage points the reading is rejected and both boxes are re-acquired.
    pub fn agrees_with_bar(&mut self, r: &Reading, bar: Option<f32>) -> bool {
        let Some(bar) = bar.map(|b| (b / BAR_FULL).min(1.0)) else { return true };
        let ok = (r.hp as f32 / r.max.max(1) as f32 - bar).abs() <= bar_tolerance(bar);
        if !ok { self.hp.reset(); self.max.reset(); self.previous = None; }
        ok
    }
    /// HP from the bar alone when the digits are unreadable. At very low health the red damage
    /// vignette hides the number (rec2: 6 of 12 frames under 10% HP unread) while the bar still
    /// measures within ~0.03; that is exactly when a retreat call matters. Needs a max HP read in
    /// the last 60 s (max changes only with items and levels).
    pub fn from_bar(&self, now: u64) -> Option<(u32, u32)> {
        let b = (self.bar? / BAR_FULL).min(1.0);
        let (_, max) = self.trusted_max.filter(|(t, _)| now >= *t && now - *t <= 60_000)?;
        Some(((b * max as f32).round() as u32, max))
    }
    /// Max HP changes only with items and levels, never by +-40% between reads: rec3 read 13009/13572
    /// for 1301/1357 (an extra leading digit). Such a value needs three identical reads.
    pub fn max_plausible(&mut self, max: u32, now: u64) -> bool {
        let ok = match self.trusted_max.filter(|(t, _)| now >= *t && now - *t <= 120_000) {
            None => true,
            Some((_, last)) if (max as f32) <= last as f32 * 1.4 && (max as f32) >= last as f32 * 0.6 => true,
            Some(_) => { let n = self.max_pending.filter(|(v, _)| *v == max).map(|(_, n)| n + 1).unwrap_or(1); self.max_pending = Some((max, n)); n >= 3 }
        };
        if ok { self.max_pending = None; self.trusted_max = Some((now, max)); }
        ok
    }
    /// Re-acquisition is throttled so a hidden HUD cannot cause a detector pass every frame.
    pub fn may_acquire(&mut self, now: u64) -> bool { self.hp.may_acquire(now, 2000) }

    pub fn accept(&mut self, hp: u32, max: u32, now: u64) -> Reading {
        let previous = self.previous.filter(|(t, _, m, _)| now >= *t && now - *t <= 20000 && max.abs_diff(*m) <= max / 10);
        let observations = previous.map(|p| p.3.saturating_add(1)).unwrap_or(1);
        let delta = previous.map(|(_, old, old_max, _)| hp as f32 / max as f32 - old as f32 / old_max as f32);
        self.previous = Some((now, hp, max, observations));
        Reading { hp, max, observations, delta, regions: vec![] }
    }
}
