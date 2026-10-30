//! Who took the Midboss. The minimap only shows that it went down (see `minimap::Tracker`:
//! gone for 20 s = killed). The kill pays the taking team a lump of souls, which shows on the
//! top bar as a step in that team's net-worth total well beyond normal income (rec13 30:05: ally
//! 194k -> 199k; 39:30: enemy 247k -> 254k; rec8 38:23: ally 257k -> 265k). Nothing else in the
//! window gives one team 5k+ at once except a team fight, so the answer is given only when one
//! team's step clearly beats the other's; otherwise None.
use std::collections::VecDeque;

/// A step must be at least this large ...
pub const MIN_STEP: u32 = 5_000;
/// ... and exceed the team's expected income over the step by this much.
pub const MIN_EXCESS: f32 = 2_500.0;
/// The winner's excess must beat the other team's best qualifying excess by this much.
pub const MARGIN: f32 = 2_000.0;
/// Steps longer than this are income, not a lump.
/// (A jump beyond the plausibility band needs three consistent reads, read every other frame,
/// before the scanner confirms it, so the confirmed step can span ~20-30 s.)
pub const MAX_STEP_MS: u64 = 40_000;
/// Evidence window around the detected kill: the icon was already gone 20 s when the kill is
/// detected, and team totals are read every other frame.
pub const BEFORE_MS: u64 = 30_000;
pub const AFTER_MS: u64 = 20_000;
/// Two detections this far apart are separate kills.
const SAME_KILL_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Event { down_at: u64, taker: Option<&'static str>, decided: bool }

#[derive(Default, Clone)]
pub struct MidbossTaker {
    /// Confirmed team-total reads (ms, value): [ally, enemy].
    totals: [VecDeque<(u64, u32)>; 2],
    event: Option<Event>,
}

/// Best step excess of one team inside [from, to].
fn best_excess(reads: &VecDeque<(u64, u32)>, from: u64, to: u64, clock_secs: u32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for w in reads.iter().collect::<Vec<_>>().windows(2) {
        let ((t0, v0), (t1, v1)) = (*w[0], *w[1]);
        if t1 < from || t1 > to || t1 <= t0 || t1 - t0 > MAX_STEP_MS || v1 < v0 + MIN_STEP { continue; }
        // Expected income: 1.5x the team's average rate so far (income grows over a match).
        let rate = 1.5 * v0 as f32 / clock_secs.max(60) as f32;
        let excess = (v1 - v0) as f32 - rate * (t1 - t0) as f32 / 1000.0;
        if excess >= MIN_EXCESS && best.is_none_or(|b| excess > b) { best = Some(excess); }
    }
    best
}

impl MidbossTaker {
    /// A confirmed team-total read (`ally`: whose total).
    pub fn total(&mut self, ally: bool, value: u32, now_ms: u64) {
        let q = &mut self.totals[if ally { 0 } else { 1 }];
        if q.back().is_some_and(|(t, _)| now_ms < *t) { q.clear(); }
        q.push_back((now_ms, value));
        while q.front().is_some_and(|(t, _)| now_ms.saturating_sub(*t) > 180_000) { q.pop_front(); }
    }

    /// Per frame. `down_ms`: None = no minimap this frame (state kept), Some(None) = Midboss up
    /// (or never seen), Some(Some(ms)) = down for `ms`. Returns the taker while it is down.
    pub fn observe(&mut self, down_ms: Option<Option<u64>>, now_ms: u64, clock_secs: Option<u32>) -> Option<&'static str> {
        match down_ms {
            Some(None) => { self.event = None; }
            Some(Some(d)) => {
                let at = now_ms.saturating_sub(d);
                if self.event.is_none_or(|e| e.down_at.abs_diff(at) > SAME_KILL_MS) { self.event = Some(Event { down_at: at, taker: None, decided: false }); }
            }
            None => {}
        }
        if self.event.is_some_and(|e| now_ms < e.down_at || now_ms - e.down_at > 600_000) { self.event = None; }
        let e = self.event.as_mut()?;
        if !e.decided && now_ms >= e.down_at + AFTER_MS {
            let (from, to) = (e.down_at.saturating_sub(BEFORE_MS), e.down_at + AFTER_MS);
            let clock = clock_secs.unwrap_or(1800);
            let a = best_excess(&self.totals[0], from, to, clock);
            let b = best_excess(&self.totals[1], from, to, clock);
            e.taker = match (a, b) {
                (Some(_), None) => Some("ally"),
                (None, Some(_)) => Some("enemy"),
                (Some(x), Some(y)) if x >= y + MARGIN => Some("ally"),
                (Some(x), Some(y)) if y >= x + MARGIN => Some("enemy"),
                _ => None,
            };
            e.decided = true;
        }
        e.taker
    }
}

/// Rejuvenator charges on the top bar: the team holding the Rejuvenator shows 1-3 small gold
/// winged icons under its souls total, beside the Midboss icon below the clock (rec8 38:45 two
/// under our total, gone after the 39:17 revive; rec13 31:07 three). Canonical 1600x900
/// coordinates of the (aligned) top bar: ally side left of centre, enemy side mirrored.
pub const CHARGES_ALLY: coach_perception::Bounds = coach_perception::Bounds { x: 704, y: 62, w: 82, h: 46 };
pub const CHARGES_ENEMY: coach_perception::Bounds = coach_perception::Bounds { x: 814, y: 62, w: 82, h: 46 };

/// Gold of the charge icons.
pub fn charge_gold(r: u8, g: u8, b: u8) -> bool { let (h, s, v) = crate::minimap::hsv(r, g, b); (38.0..=60.0).contains(&h) && s > 0.55 && v > 0.65 }

/// Charge icons visible on one side: compact gold blobs of icon size.
pub fn charge_icons(top: &image::DynamicImage, area: coach_perception::Bounds) -> usize {
    crate::hudtext::blobs(top, area, charge_gold).into_iter().filter(|b| {
        let (w, h) = (b.w(), b.h());
        (8..=26).contains(&w) && (8..=26).contains(&h) && b.area >= 30 && b.area as f32 >= 0.3 * (w * h) as f32
    }).count()
}

/// Rejuvenator charges per side this frame: [ally, enemy]. Only plausible counts (1-3) count.
pub fn rejuvenator_charges(top: &image::DynamicImage) -> [usize; 2] {
    if top.width() != 1600 || top.height() != 900 { return [0, 0]; }
    [CHARGES_ALLY, CHARGES_ENEMY].map(|a| { let n = charge_icons(top, a); if n <= 3 { n } else { 0 } })
}

/// Consecutive frames that must show charges on exactly one side before it decides the holder.
pub const CHARGE_FRAMES: u32 = 2;

/// Rejuvenator holder from the charge icons: a side is reported once its icons were seen on
/// `CHARGE_FRAMES` consecutive frames with none on the other side, and held while the Midboss
/// event lasts (the icons go when the charges are spent).
#[derive(Default, Clone, Debug)]
pub struct ChargeHolder { streak: (Option<bool>, u32), pub holder: Option<(&'static str, u64)> }
impl ChargeHolder {
    /// Feed one gameplay frame's charge counts. Returns the side newly confirmed, if any.
    pub fn observe(&mut self, charges: [usize; 2], now_ms: u64) -> Option<&'static str> {
        let side = match charges { [a, 0] if a > 0 => Some(true), [0, e] if e > 0 => Some(false), _ => None };
        let Some(ally) = side else { self.streak = (None, 0); return None };
        self.streak = if self.streak.0 == Some(ally) { (Some(ally), self.streak.1 + 1) } else { (Some(ally), 1) };
        if self.streak.1 >= CHARGE_FRAMES {
            let s = if ally { "ally" } else { "enemy" };
            let new = self.holder.is_none_or(|(h, _)| h != s);
            self.holder = Some((s, now_ms));
            return new.then_some(s);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn charge_holder_needs_consecutive_one_sided_frames() {
        let mut c = ChargeHolder::default();
        assert_eq!(c.observe([2, 0], 0), None);
        assert_eq!(c.observe([2, 0], 2000), Some("ally"));
        assert_eq!(c.observe([2, 0], 4000), None, "already known");
        let mut d = ChargeHolder::default();
        assert_eq!(d.observe([1, 0], 0), None); assert_eq!(d.observe([1, 1], 2000), None); assert_eq!(d.observe([1, 0], 4000), None);
        assert_eq!(d.observe([0, 3], 6000), None); assert_eq!(d.observe([0, 3], 8000), Some("enemy"));
    }
    /// Feed alternate ally / enemy totals every 2 s from `reads` (time s, ally k, enemy k).
    fn run(reads: &[(u64, u32, u32)], down_at_s: u64) -> Vec<Option<&'static str>> {
        let mut m = MidbossTaker::default();
        let mut out = vec![];
        for (i, (t, a, e)) in reads.iter().enumerate() {
            let now = t * 1000;
            if i % 2 == 0 { m.total(true, a * 1000, now) } else { m.total(false, e * 1000, now) }
            let down = (now >= down_at_s * 1000).then(|| now - down_at_s * 1000);
            out.push(m.observe(Some(down), now, Some((*t as u32).max(60))));
        }
        out
    }
    #[test]
    fn rec13_ally_then_enemy_lumps() {
        // rec13 ~29:30-30:30: ally 194k -> 199k across the kill; enemy income only.
        let reads: Vec<(u64, u32, u32)> = (0..30).map(|k| { let t = 1770 + 2 * k; (t, if t < 1805 { 194 } else { 199 }, 172 + (k as u32 / 10)) }).collect();
        let got = run(&reads, 1792);
        assert_eq!(got.last().copied().flatten(), Some("ally"));
        assert!(got[..5].iter().all(|g| g.is_none()), "undecided before the window closes");
        // rec13 39:30: enemy 247k -> 254k; ally +1k.
        let reads: Vec<(u64, u32, u32)> = (0..30).map(|k| { let t = 2350 + 2 * k; (t, 293 + (t >= 2380) as u32, if t < 2370 { 247 } else { 254 }) }).collect();
        assert_eq!(run(&reads, 2378).last().copied().flatten(), Some("enemy"));
    }
    #[test]
    fn no_lump_or_both_lumps_is_unknown() {
        // rec8 46:01: enemy income only (3k steps), ally flat.
        let reads: Vec<(u64, u32, u32)> = (0..45).map(|k| { let t = 2730 + 2 * k; (t, 313, 319 + k as u32 / 8) }).collect();
        let got = run(&reads, 2761);
        assert!(got.iter().all(|g| g.is_none()));
        // Both teams jump 8k (a team fight elsewhere): no answer.
        let reads: Vec<(u64, u32, u32)> = (0..45).map(|k| { let t = 2730 + 2 * k; (t, if t < 2750 { 300 } else { 308 }, if t < 2751 { 300 } else { 308 }) }).collect();
        assert!(run(&reads, 2761).iter().all(|g| g.is_none()));
    }
    #[test]
    fn respawn_clears_the_taker() {
        let mut m = MidbossTaker::default();
        m.total(true, 100_000, 0); m.total(true, 108_000, 4000);
        assert_eq!(m.observe(Some(Some(5_000)), 5_000, Some(1200)), None, "window still open");
        assert_eq!(m.observe(Some(Some(25_000)), 25_000, Some(1200)), Some("ally"));
        assert_eq!(m.observe(None, 26_000, Some(1200)), Some("ally"), "held on frames without a minimap");
        assert_eq!(m.observe(Some(None), 400_000, Some(1500)), None);
    }
}
