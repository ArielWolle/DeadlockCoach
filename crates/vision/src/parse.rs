//! Turn raw OCR strings into typed values, rejecting implausible ones.
use coach_core::GameSnapshot;

/// "1:28" -> 88. Also accepts "12:05" and "1:02:03".
pub fn parse_clock(s: &str) -> Option<u32> {
    let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
    let parts: Vec<u32> = t.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    match parts.as_slice() {
        [m, s] if *s < 60 => Some(m * 60 + s),
        [h, m, s] if *m < 60 && *s < 60 => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

/// "$ 1,613 SOULS" -> 1613. Takes the first digit group (commas allowed).
pub fn parse_souls(s: &str) -> Option<u32> {
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() { num.push(c); }
        else if c == ',' && !num.is_empty() { continue; }
        else if !num.is_empty() { break; }
    }
    num.parse().ok()
}

/// First integer on the first line: "413\n1769" -> 413.
pub fn parse_hp(s: &str) -> Option<u32> {
    s.lines().next().and_then(|l| l.trim().parse().ok())
}

/// Keeps snapshots sane across frames.
#[derive(Default)]
pub struct Tracker { last_clock: Option<u32>, last_souls: Option<u32> }

impl Tracker {
    /// Accept a reading only if the clock moves forward by a plausible amount.
    pub fn accept(&mut self, clock: Option<u32>, souls: Option<u32>) -> GameSnapshot {
        let mut snap = GameSnapshot::default();
        if let Some(c) = clock {
            let ok = match self.last_clock { Some(l) => c >= l && c - l <= 60, None => true };
            if ok { self.last_clock = Some(c); snap.game_clock_secs = Some(c); }
        }
        if let Some(s) = souls {
            // Souls can drop when spending, but a huge jump either way is likely misread.
            let ok = match self.last_souls { Some(l) => s <= l + 5000, None => s < 100_000 };
            if ok { self.last_souls = Some(s); snap.souls = Some(s); }
        }
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        assert_eq!(parse_clock("1:28"), Some(88));
        assert_eq!(parse_clock("garbage"), None);
        assert_eq!(parse_souls("$ 1,613 SDULS"), Some(1613));
        assert_eq!(parse_souls("$900 SOuLS"), Some(900));
        assert_eq!(parse_hp("413\n1769"), Some(413));
    }
    #[test]
    fn rejects_backwards_clock() {
        let mut t = Tracker::default();
        assert_eq!(t.accept(Some(100), Some(900)).game_clock_secs, Some(100));
        assert_eq!(t.accept(Some(40), Some(900)).game_clock_secs, None);
        assert_eq!(t.accept(Some(105), Some(90000)).souls, None);
    }
}
