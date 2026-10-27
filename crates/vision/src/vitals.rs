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

#[derive(Default)]
pub struct Tracker {
    previous: Option<(u64, u32, u32, u32)>,
    hp: Tracked,
    max: Tracked,
    digits: (usize, usize),
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

    /// Interpret batched line reads of the tracked boxes.
    pub fn accept_lines(&mut self, current: &str, maximum: &str, now: u64) -> Option<Reading> {
        let hp = numbers(current).into_iter().next();
        let maxima: Vec<u32> = numbers(maximum).into_iter().filter(|n| (100..=20000).contains(n)).collect();
        // A tracked box can truncate a number that just gained a digit (999 -> 1,021 read as
        // "102"). A single-read change beyond 30% of max HP is therefore confirmed by an
        // immediate detector re-acquisition rather than trusted (real bursts are confirmed too).
        let jump = |hp: u32, max: u32| self.previous.is_some_and(|(t, old, _, _)| now >= t && now - t <= 20000 && hp.abs_diff(old) as f32 > max as f32 * 0.3);
        match (hp, maxima.as_slice()) {
            (Some(hp), [max]) if hp <= *max && jump(hp, *max) => { self.hp.reset(); self.max.reset(); None }
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
                let mut r = self.accept(hp, max, now); r.regions = vec![region("hp", hb), region("max_hp", mb)]; Some(r)
            }
            None => { self.previous = None; None }
        }
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
