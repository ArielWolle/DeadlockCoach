//! Reusable fast-path building blocks for any HUD adapter.
//!
//! Expensive work (full-frame localization, text *detection*) runs only to acquire an
//! element. Once found, its box is kept in a [`Tracked`] slot and read cheaply each frame,
//! usually by batched line recognition. Repeated read failures drop the box and allow a
//! rate-limited re-acquisition. [`Cadence`] lets slow-changing fields refresh less often.
use crate::Bounds;
use std::collections::HashMap;

/// An element's last confirmed location, with miss counting and acquisition throttling.
#[derive(Debug, Clone, Default)]
pub struct Tracked {
    pub bounds: Option<Bounds>,
    misses: u32,
    last_acquire_ms: Option<u64>,
}

impl Tracked {
    /// Consecutive failed reads before the box is considered lost.
    pub const MAX_MISSES: u32 = 2;

    pub fn hit(&mut self, bounds: Bounds) { self.bounds = Some(bounds); self.misses = 0; }
    /// Record a failed read. Returns `true` when the element is now considered lost.
    pub fn miss(&mut self) -> bool {
        if self.bounds.is_none() { return true; }
        self.misses += 1;
        if self.misses >= Self::MAX_MISSES { self.bounds = None; self.misses = 0; true } else { false }
    }
    /// True when an (expensive) acquisition attempt is allowed now. Records the attempt.
    pub fn may_acquire(&mut self, now_ms: u64, cooldown_ms: u64) -> bool {
        if self.bounds.is_some() { return false; }
        let allowed = self.last_acquire_ms.is_none_or(|t| now_ms < t || now_ms - t >= cooldown_ms);
        if allowed { self.last_acquire_ms = Some(now_ms); }
        allowed
    }
    /// Drop the box but keep the acquisition throttle (use when a companion element is lost).
    pub fn forget(&mut self) { self.bounds = None; self.misses = 0; }
    pub fn reset(&mut self) { *self = Self::default(); }
}

/// Per-key refresh intervals. A key is due on first use and whenever its interval elapsed.
#[derive(Debug, Clone, Default)]
pub struct Cadence { last: HashMap<String, u64> }

impl Cadence {
    /// Returns true (and records the run) when `key` should be refreshed at `now_ms`.
    pub fn due(&mut self, key: &str, now_ms: u64, every_ms: u64) -> bool {
        let due = self.last.get(key).is_none_or(|t| now_ms < *t || now_ms - *t >= every_ms);
        if due { self.last.insert(key.to_owned(), now_ms); }
        due
    }
    pub fn reset(&mut self) { self.last.clear(); }
}

/// A value that advances with wall time (e.g. a match clock, in seconds). An expensive but
/// reliable reader anchors it; cheap per-frame reads only *confirm* the extrapolation. Repeated
/// disagreement (or a long time without verification) requests another expensive read, so a
/// misreading cheap reader can never move the value by itself.
#[derive(Debug, Clone, Default)]
pub struct Extrapolated {
    anchor: Option<(u32, u64)>,
    last_confirm_ms: u64,
    mismatches: u32,
    last_verify_ms: Option<u64>,
    /// Verified readings that disagree with the extrapolation, kept while they agree with each other.
    pending: Vec<(u32, u64)>,
}

/// Whether `a` and `b` differ by whole tens of minutes (600 s multiples, ±2 s): the
/// tens-of-minutes digit dropped or added ("5:31" for "15:31"). rec8's live coach read
/// "5:31" for 15:31 when a slightly mis-scaled context cut the narrow "1".
pub fn tens_of_minutes_apart(a: u32, b: u32) -> bool {
    let d = a.abs_diff(b);
    d >= 598 && { let r = d % 600; r <= 2 || r >= 598 }
}

impl Extrapolated {
    pub const TOLERANCE: u32 = 1;
    /// Periodic safety re-verification (the reliable reader can be expensive, e.g. ~1.6 s in WASM).
    pub const REVERIFY_MS: u64 = 120_000;
    pub const VERIFY_THROTTLE_MS: u64 = 2_000;
    /// Without any confirmation for this long, the value is no longer reported.
    /// Wall-time extrapolation stays trustworthy for a minute without a confirming read (the
    /// small clock text can fail to read for long stretches under stream overlays).
    pub const STALE_MS: u64 = 60_000;
    /// Contrary verified readings pair up only while the previous one is at most this old.
    pub const PENDING_GAP_MS: u64 = 20_000;
    /// Dropping a tens-of-minutes digit needs consistent reads over at least this long.
    pub const TENS_DROP_SPAN_MS: u64 = 30_000;

    pub fn expected(&self, now_ms: u64) -> Option<u32> {
        self.anchor.filter(|(_, t)| now_ms >= *t).map(|(v, t)| v + ((now_ms - t) / 1000) as u32)
    }
    /// Milliseconds since the last confirming read, if anchored.
    pub fn confirmed_age_ms(&self, now_ms: u64) -> Option<u64> { self.anchor.map(|_| now_ms.saturating_sub(self.last_confirm_ms)) }
    /// Current value, if anchored and recently confirmed.
    pub fn value(&self, now_ms: u64) -> Option<u32> {
        self.expected(now_ms).filter(|_| now_ms >= self.last_confirm_ms && now_ms - self.last_confirm_ms <= Self::STALE_MS)
    }
    /// Feed a cheap reading. Returns true when an expensive verification should run now.
    pub fn observe(&mut self, cheap: Option<u32>, now_ms: u64) -> bool {
        let throttle_ok = self.last_verify_ms.is_none_or(|t| now_ms < t || now_ms - t >= Self::VERIFY_THROTTLE_MS);
        let Some(expected) = self.expected(now_ms) else { return throttle_ok };
        if let Some(v) = cheap.filter(|v| v.abs_diff(expected) <= Self::TOLERANCE) {
            // Agreement within tolerance: adopt the visible reading (removes sub-second drift).
            self.anchor = Some((v, now_ms)); self.mismatches = 0; self.last_confirm_ms = now_ms;
            return self.last_verify_ms.is_none_or(|t| now_ms < t || now_ms - t >= Self::REVERIFY_MS);
        }
        // Disagreement: keep reporting the extrapolation (a cheap misread never moves it).
        // A reading off by whole minutes with matching seconds is the measured minute-digit
        // misread of soft video and does not justify an expensive re-read; any other
        // disagreement (pause, rewind, new match) verifies as soon as the throttle allows.
        self.mismatches += 1;
        // A reading whole tens of minutes *ahead* has a digit the anchor lacks: the anchor may
        // have dropped the narrow tens digit ("5:31" for 15:31), so verify (rec8 kept 5:31 for
        // 140 s because these correct reads were dismissed as minute misreads).
        if cheap.is_some_and(|v| v > expected && tens_of_minutes_apart(v, expected)) { return throttle_ok; }
        let minute_misread = cheap.is_some_and(|v| { let d = v.abs_diff(expected); d >= 59 && (d % 60 <= 1 || d % 60 >= 59) });
        if minute_misread { self.last_confirm_ms = now_ms; return false; }
        throttle_ok
    }
    /// Record the result of an expensive verification.
    pub fn verify(&mut self, value: Option<u32>, now_ms: u64) {
        self.last_verify_ms = Some(now_ms);
        let Some(v) = value else { return };
        // The value follows wall time, so even the reliable reader is not trusted when it alone
        // contradicts the extrapolation (a recorded match read 7:00 for 17:00 and +60 s): a jump is
        // adopted only after mutually consistent verifications - 3 forward, 2 backward (a new
        // match or rewind). A clock still showing exactly the last confirmed value is a pause or a
        // stalled stream (the game clock stops, wall time does not) and is adopted at once. Other
        // small backward gaps are misreads too (rec2 read 8:11 as 8:01 and 10:56 as 10:50).
        // Jumps by whole tens of minutes are the tens-digit pattern: dropping it (backward) needs
        // a long consistent streak - the reading with more digits is preferred - while restoring
        // it (forward) corrects an anchor that lacked it after two consistent reads.
        let accept = match self.expected(now_ms) {
            None => true,
            Some(e) if v.abs_diff(e) <= 3 => true,
            Some(e) if v < e && self.anchor.is_some_and(|(a, _)| a == v) => true,
            Some(e) => {
                // Evidence goes stale: a contrary read from long ago (rec8: minutes earlier) must
                // not pair with a new one, however consistent.
                if self.pending.last().is_some_and(|(_, t)| now_ms < *t || now_ms - *t > Self::PENDING_GAP_MS) { self.pending.clear(); }
                // Consistent = advancing with wall time, or standing still (paused).
                let consistent = self.pending.last().is_some_and(|(pv, pt)| v.abs_diff(pv + (now_ms.saturating_sub(*pt) / 1000) as u32) <= 2 || v.abs_diff(*pv) <= 1);
                if !consistent { self.pending.clear(); }
                self.pending.push((v, now_ms));
                let span = self.pending.first().map_or(0, |(_, t)| now_ms.saturating_sub(*t));
                match (tens_of_minutes_apart(v, e), v > e) {
                    (true, true) => self.pending.len() >= 2,
                    (true, false) => self.pending.len() >= 5 && span >= Self::TENS_DROP_SPAN_MS,
                    (false, forward) => self.pending.len() >= if forward { 3 } else { 2 },
                }
            }
        };
        if accept { self.anchor = Some((v, now_ms)); self.last_confirm_ms = now_ms; self.mismatches = 0; self.pending.clear(); }
    }
    pub fn reset(&mut self) { *self = Self::default(); }
}

/// Spreads a group of slow-changing fields across frames (e.g. 12 team banners, 4 per frame),
/// bounding the per-frame recognition cost while every field still refreshes regularly.
#[derive(Debug, Clone, Default)]
pub struct RoundRobin { next: usize }
impl RoundRobin {
    pub fn take(&mut self, total: usize, k: usize) -> Vec<usize> {
        if total == 0 { return vec![]; }
        let k = k.min(total);
        let slots = (0..k).map(|i| (self.next + i) % total).collect();
        self.next = (self.next + k) % total;
        slots
    }
}

/// Per-phase timing for one frame (works in WASM via `web_time`). Adapters record laps so
/// slow phases are visible in diagnostics rather than guessed.
#[derive(Debug, Clone)]
pub struct Laps { last: web_time::Instant, pub entries: Vec<(String, f64)> }
impl Default for Laps { fn default() -> Self { Self { last: web_time::Instant::now(), entries: vec![] } } }
impl Laps {
    /// Record the time since the previous lap under `name`.
    pub fn lap(&mut self, name: &str) {
        let now = web_time::Instant::now();
        self.entries.push((name.to_owned(), (now - self.last).as_secs_f64() * 1000.0));
        self.last = now;
    }
    /// Append another component's laps under a prefix (e.g. a nested scanner).
    pub fn absorb(&mut self, prefix: &str, other: &Laps) {
        for (k, v) in &other.entries { self.entries.push((format!("{prefix}{k}"), *v)); }
        self.last = web_time::Instant::now();
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Object(self.entries.iter().map(|(k, v)| (k.clone(), serde_json::json!((v * 10.0).round() / 10.0))).collect())
    }
}

/// Collects every line crop needed for one frame, then recognizes them in a single call.
/// `add` returns the index of that crop's text in the result.
#[derive(Default)]
pub struct LineBatch { crops: Vec<image::DynamicImage> }

impl LineBatch {
    pub fn add(&mut self, crop: image::DynamicImage) -> usize { self.crops.push(crop); self.crops.len() - 1 }
    pub fn add_all(&mut self, crops: impl IntoIterator<Item = image::DynamicImage>) -> std::ops::Range<usize> {
        let start = self.crops.len(); self.crops.extend(crops); start..self.crops.len()
    }
    pub fn len(&self) -> usize { self.crops.len() }
    pub fn is_empty(&self) -> bool { self.crops.is_empty() }
    /// Run the batch. A backend failure yields empty strings, so callers simply see "unread".
    pub fn run(self, read: impl FnOnce(&[image::DynamicImage]) -> Result<Vec<String>, String>) -> Vec<String> {
        let n = self.crops.len();
        if n == 0 { return vec![]; }
        match read(&self.crops) { Ok(v) if v.len() == n => v, _ => vec![String::new(); n] }
    }
}

/// Majority vote over candidate readings of the same field (e.g. several overlapping crops).
/// Ties prefer the candidate closest to `expected`, then the first listed.
pub fn vote<T: Copy + PartialEq>(candidates: &[Option<T>], distance: impl Fn(T) -> u64) -> Option<T> {
    let mut best: Option<(T, usize, u64)> = None;
    for value in candidates.iter().flatten() {
        let count = candidates.iter().flatten().filter(|v| *v == value).count();
        let d = distance(*value);
        if best.is_none_or(|(_, c, bd)| count > c || (count == c && d < bd)) { best = Some((*value, count, d)); }
    }
    best.map(|(v, ..)| v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tracked_drops_after_repeated_misses_and_throttles_acquisition() {
        let mut t = Tracked::default();
        assert!(t.may_acquire(0, 2000)); assert!(!t.may_acquire(1000, 2000)); assert!(t.may_acquire(2000, 2000));
        t.hit(Bounds { x: 1, y: 2, w: 3, h: 4 });
        assert!(!t.may_acquire(9000, 2000), "a tracked element is never re-acquired");
        assert!(!t.miss()); assert!(t.bounds.is_some());
        assert!(t.miss()); assert!(t.bounds.is_none());
    }
    #[test]
    fn cadence_runs_first_then_at_interval() {
        let mut c = Cadence::default();
        assert!(c.due("a", 100, 1000)); assert!(!c.due("a", 900, 1000)); assert!(c.due("a", 1100, 1000));
        assert!(c.due("b", 900, 1000)); assert!(c.due("a", 50, 1000), "a clock reset makes it due again");
    }
    #[test]
    fn extrapolated_value_ignores_cheap_misreads_and_reverifies_on_disagreement() {
        let mut c = Extrapolated::default();
        assert!(c.observe(Some(373), 0), "unanchored: verify immediately");
        c.verify(Some(313), 0);
        assert!(!c.observe(Some(314), 1000)); assert_eq!(c.value(1000), Some(314));
        assert!(!c.observe(Some(375), 2000), "a whole-minute misread with matching seconds is not re-verified");
        assert_eq!(c.value(2000), Some(315), "the misread never moves the value");
        assert!(c.observe(Some(400), 3000), "a seconds disagreement (pause/new match) verifies once the throttle allows");
        assert_eq!(c.value(3000), Some(316));
        c.verify(Some(316), 3000);
        assert!(!c.observe(Some(500), 4000), "verification is throttled to every 2 s");
        assert_eq!(c.value(4000), Some(317));
        assert!(!c.observe(Some(320), 7000));
        assert!(c.observe(Some(437), 123_000), "a confirmed value is still re-verified every 120 s");
        // A single verified misread no longer moves an established value; consistent repeats do.
        let mut j = Extrapolated::default(); j.verify(Some(1000), 0);
        j.verify(Some(420), 2000); assert_eq!(j.expected(2000), Some(1002), "one 7:00-for-17:00 misread is rejected");
        j.verify(Some(1063), 4000); j.verify(Some(1065), 6000); assert_eq!(j.expected(6000), Some(1006), "two +60 reads are not enough");
        j.verify(Some(1067), 8000); assert_eq!(j.expected(8000), Some(1067), "three consistent reads move it");
        j.verify(Some(5), 10_000); j.verify(Some(7), 12_000); assert_eq!(j.expected(12_000), Some(7), "new match: two backward reads");
        let mut s = Extrapolated::default(); s.verify(Some(10), 0); assert_eq!(s.value(59_000), Some(69)); assert_eq!(s.value(61_000), None, "unconfirmed values go stale after a minute");
        // A pause: the clock still shows the last confirmed value, adopted at once.
        let mut p = Extrapolated::default(); p.verify(Some(313), 0); p.verify(Some(313), 5000); assert_eq!(p.expected(5000), Some(313));
        // A small backward misread (8:11 read as 8:01) is not; a held pause (same value twice) is.
        let mut m = Extrapolated::default(); m.verify(Some(490), 0); m.verify(Some(481), 1000); assert_eq!(m.expected(1000), Some(491));
        let mut h = Extrapolated::default(); h.verify(Some(490), 0); h.verify(Some(490), 6000); h.verify(Some(490), 9000); assert_eq!(h.expected(9000), Some(490));
    }
    #[test]
    fn tens_of_minutes_pattern() {
        assert!(tens_of_minutes_apart(331, 931)); assert!(tens_of_minutes_apart(932, 331)); assert!(tens_of_minutes_apart(5, 1205));
        assert!(!tens_of_minutes_apart(371, 931)); assert!(!tens_of_minutes_apart(931, 931)); assert!(!tens_of_minutes_apart(320, 931));
    }
    #[test]
    fn extrapolated_never_drops_the_tens_digit_on_a_stale_pair() {
        // rec8 live: anchored at 15:22, a stale contrary 5:xx read from minutes earlier paired with
        // a fresh "5:31" (a mis-scaled context cut the narrow "1") and moved the clock to 5:31.
        let mut c = Extrapolated::default();
        c.verify(Some(911), 0);
        c.verify(Some(311), 1000);                 // an old dropped-digit read stays pending...
        assert_eq!(c.expected(1000), Some(912));
        for t in [3000u64, 9000, 15_000, 20_000] { assert!(!c.observe(Some(911 + (t / 1000) as u32), t)); }
        c.verify(Some(331), 21_500);               // ...and pairs with nothing: it is stale
        assert_eq!(c.expected(21_500), Some(932));
        c.verify(Some(333), 23_500); c.verify(Some(335), 25_500); c.verify(Some(337), 27_500);
        assert_eq!(c.expected(27_500), Some(938), "four consistent dropped-digit reads in 6 s do not move it");
        assert!(!c.observe(Some(338), 28_500), "a cheap dropped-digit read is a misread, not re-verified");
        assert_eq!(c.value(28_500), Some(939));
        // A long consistent streak (e.g. the clock really restarted with a matching offset) does.
        let mut s = Extrapolated::default(); s.verify(Some(931), 0);
        for k in 0..=15u64 { s.verify(Some(331 + 2 * k as u32), 2000 * k); }
        assert_eq!(s.expected(30_000), Some(361));
    }
    #[test]
    fn extrapolated_restores_a_missing_tens_digit_quickly() {
        // Anchored on "5:31" for 15:31: correct cheap reads 10 minutes ahead are not dismissed as
        // minute misreads but verified, and two consistent verified reads correct the anchor.
        let mut c = Extrapolated::default();
        c.verify(Some(331), 0);
        assert!(!c.observe(Some(332), 1000), "agreeing cheap read");
        assert!(c.observe(Some(934), 3000), "a reading 10 minutes ahead is verified");
        c.verify(Some(934), 3000); assert_eq!(c.expected(3000), Some(334));
        assert!(c.observe(Some(936), 5000));
        c.verify(Some(936), 5000); assert_eq!(c.expected(5000), Some(936), "two consistent reads restore the digit");
        assert!(!c.observe(Some(337), 6000), "now the dropped-digit cheap read is the misread");
        assert_eq!(c.value(6000), Some(937));
    }
    #[test]
    fn vote_prefers_majority_then_expected_value() {
        assert_eq!(vote(&[Some(313u32), Some(303), Some(313), Some(373)], |v| v.abs_diff(0) as u64), Some(313));
        assert_eq!(vote(&[None, Some(862), None], |_| 0), Some(862));
        assert_eq!(vote(&[Some(10), Some(20)], |v: u32| v.abs_diff(19) as u64), Some(20));
        assert_eq!(vote::<u32>(&[None, None], |_| 0), None);
    }
}
