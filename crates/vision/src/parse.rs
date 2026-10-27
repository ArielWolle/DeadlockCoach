//! Turn raw OCR strings into typed values, rejecting implausible ones.

/// "1:28" -> 88. Also accepts "12:05" and "1:02:03".
pub fn parse_clock(s: &str) -> Option<u32> {
    // Use the first line/token that looks like a clock (OCR sometimes adds stray letters).
    for tok in s.split(|c: char| c.is_whitespace()) {
        let t: String = tok.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
        let parts: Option<Vec<u32>> = t.split(':').map(|p| p.parse().ok()).collect();
        match parts.as_deref() {
            Some([m, s]) if *s < 60 && t.contains(':') => return Some(m * 60 + s),
            Some([h, m, s]) if *m < 60 && *s < 60 => return Some(h * 3600 + m * 60 + s),
            _ => {}
        }
    }
    None
}

/// "$ 1,613 SOULS" -> 1613; "$7.139 SOULS" -> 7139. First digit group; ',' or '.'
/// followed by exactly three digits is a thousands separator.
pub fn parse_souls(s: &str) -> Option<u32> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut num = String::new();
    while i < c.len() {
        if c[i].is_ascii_digit() {
            num.push(c[i]);
        } else if (c[i] == ',' || c[i] == '.') && !num.is_empty()
            && i + 3 < c.len() + 0 && c[i + 1..].iter().take(3).all(|d| d.is_ascii_digit()) && c[i + 1..].iter().take(3).count() == 3
            && !c.get(i + 4).map(|d| d.is_ascii_digit()).unwrap_or(false)
        {
            // thousands separator: skip
        } else if !num.is_empty() {
            break;
        }
        i += 1;
    }
    num.parse().ok()
}

/// First integer on the first line: "413\n1769" -> 413.
pub fn parse_hp(s: &str) -> Option<u32> {
    s.lines().next().and_then(|l| l.trim().parse().ok())
}

/// "MATCH: 124933265" -> 124933265 (needs at least 6 digits).
pub fn parse_match_id(s: &str) -> Option<u64> {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() >= 6 { d.parse().ok() } else { None }
}

/// "Respawn in: 11" -> 11.
pub fn parse_respawn(s: &str) -> Option<u32> {
    let after = s.split(':').next_back()?;
    let d: String = after.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.is_empty() || d.len() > 3 { None } else { d.parse().ok() }
}

/// Up to three small integers from "5 0 6" / "4 6 8".
pub fn parse_kda(s: &str) -> Option<(u32, u32, u32)> {
    let nums: Vec<u32> = s.split(|c: char| !c.is_ascii_digit()).filter(|t| !t.is_empty() && t.len() <= 3)
        .filter_map(|t| t.parse().ok()).collect();
    match nums.as_slice() {
        [k, d, a] => Some((*k, *d, *a)),
        _ => None,
    }
}

/// A reading is accepted when it agrees with elapsed wall time, or when two
/// consecutive readings agree with each other (re-sync after a pause/shop/menu).
#[derive(Default)]
pub struct Tracker {
    clock: Option<(u32, u64)>,
    clock_pending: Option<(u32, u64)>,
    souls: Option<u32>,
    souls_pending: Option<u32>,
}

const CLOCK_TOL: i64 = 6;
const SOULS_RISE_OK: u32 = 700;

impl Tracker {
    pub fn reset(&mut self) { *self = Tracker::default(); }

    /// Returns the accepted clock in seconds, if any. `now_ms` is wall time of the frame.
    pub fn accept_clock(&mut self, c: Option<u32>, now_ms: u64) -> Option<u32> {
        let c = c?;
        let near = |base: (u32, u64)| {
            let expect = base.0 as i64 + (now_ms.saturating_sub(base.1) / 1000) as i64;
            (c as i64 - expect).abs() <= CLOCK_TOL
        };
        if let Some(l) = self.clock {
            if near(l) { self.clock = Some((c, now_ms)); self.clock_pending = None; return Some(c); }
        }
        match self.clock_pending {
            Some(p) if near(p) => {
                self.clock = Some((c, now_ms));
                self.clock_pending = None;
                Some(c)
            }
            _ => { self.clock_pending = Some((c, now_ms)); None }
        }
    }

    /// Souls rise by small steps; big rises or any drop must be seen twice.
    pub fn accept_souls(&mut self, s: Option<u32>) -> Option<u32> {
        let s = s?;
        let ok_now = match self.souls { Some(l) => s >= l && s - l <= SOULS_RISE_OK, None => false };
        if ok_now { self.souls = Some(s); self.souls_pending = None; return Some(s); }
        if let Some(p) = self.souls_pending {
            if s.abs_diff(p) <= SOULS_RISE_OK {
                self.souls = Some(s);
                self.souls_pending = None;
                return Some(s);
            }
        }
        self.souls_pending = Some(s);
        None
    }

    pub fn last_souls(&self) -> Option<u32> { self.souls }
    /// Clock expected at `now_ms` from the last accepted reading (wall-time extrapolation).
    pub fn expected_clock(&self, now_ms: u64) -> Option<u32> {
        self.clock.map(|(c, t)| c + (now_ms.saturating_sub(t) / 1000) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_clock("1:28"), Some(88));
        assert_eq!(parse_clock("C\n13:53"), Some(833));
        assert_eq!(parse_clock("garbage"), None);
        assert_eq!(parse_souls("$ 1,613 SDULS"), Some(1613));
        assert_eq!(parse_souls("$7.139 SDULS"), Some(7139));
        assert_eq!(parse_souls("$900 SOuLS"), Some(900));
        assert_eq!(parse_souls("6,032/$6,032 SDULS"), Some(6032));
        assert_eq!(parse_hp("413\n1769"), Some(413));
        assert_eq!(parse_match_id("MATCH: 124933265"), Some(124933265));
        assert_eq!(parse_respawn("Respawn in: 11"), Some(11));
        assert_eq!(parse_kda("5 0 6"), Some((5, 0, 6)));
        assert_eq!(parse_kda("4"), None);
    }

    #[test]
    fn clock_follows_wall_time_and_resyncs() {
        let mut t = Tracker::default();
        assert_eq!(t.accept_clock(Some(100), 0), None, "first read needs confirmation");
        assert_eq!(t.accept_clock(Some(105), 5000), Some(105));
        assert_eq!(t.accept_clock(Some(170), 10000), None, "glitch +60");
        assert_eq!(t.accept_clock(Some(115), 15000), Some(115), "back in line");
        // wall time passing (e.g. shop open 150 s) is not a jump
        assert_eq!(t.accept_clock(Some(265), 165000), Some(265));
        // a real discontinuity needs a confirming second read
        assert_eq!(t.accept_clock(Some(400), 170000), None);
        assert_eq!(t.accept_clock(Some(405), 175000), Some(405));
    }

    #[test]
    fn souls_reject_misreads_but_accept_spending() {
        let mut t = Tracker::default();
        assert_eq!(t.accept_souls(Some(600)), None);
        assert_eq!(t.accept_souls(Some(606)), Some(606));
        assert_eq!(t.accept_souls(Some(3656)), None, "'$' misread as 3");
        assert_eq!(t.accept_souls(Some(768)), Some(768));
        assert_eq!(t.accept_souls(Some(120)), None, "spend needs confirmation");
        assert_eq!(t.accept_souls(Some(130)), Some(130));
    }
}
