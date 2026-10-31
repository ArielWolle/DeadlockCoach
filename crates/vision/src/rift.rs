//! Unstable Rift (deadlock.wiki/Unstable_Rift, docs/rift.md). What the screen shows, measured on
//! the recordings (1920x1080 captures, coordinates below at that scale):
//! - the announcement "AN UNSTABLE RIFT IS OPENING..." / "Go to the Rift to claim its power",
//!   large slanted cream text in the centre for ~2-3 s (rec17 11:01), with a bright teal ping
//!   ring on the minimap at the Rift's bridge for ~5 s (rec17 11:03-11:05, rec16 12:48);
//! - from the opening (20 s later) a dark capture panel top centre under the clock: our progress
//!   "33%" left of a teal Rift icon (a cream bar growing left), theirs "0%" right of it (a red bar
//!   growing right), the bounty "$ 1482" under the icon (1482 + 222 per minute from minute 13:
//!   2592 at 18:xx, 5922 at 33:xx), portraits of the heroes in the point below each side;
//! - the outcome on that panel: "RIFT SECURED!" (teal, we took it) or "RIFT LOST" (red); then
//!   the panel goes. Both sides at 99% means both teams are in the point (contested).
//! A draw (60 s contested) was not seen in any recording: it is reported only when a draw word
//! is read on the panel; a panel that goes without an outcome read leaves the outcome unknown.
use crate::minimap::hsv;
use coach_core::{Lane, RiftState};
use coach_perception::Bounds;
use image::DynamicImage;

/// A box given at 1920x1080 scale, rendered at that scale from the best pixels of the view.
fn at_1920(view: &crate::hudtext::View, x: u32, y: u32, w: u32, h: u32) -> DynamicImage {
    let b = Bounds { x: x * 5 / 6, y: y * 5 / 6, w: (w * 5 / 6).max(1), h: (h * 5 / 6).max(1) };
    view.region(b, w, h)
}

/// Teal of the Rift icon and the bounty text (hue ~160, measured s 0.4-0.5 on the icon's thin lines).
pub fn teal(r: u8, g: u8, b: u8) -> bool { let (h, s, v) = hsv(r, g, b); (140.0..=190.0).contains(&h) && (0.28..=0.9).contains(&s) && v > 0.5 }

/// Icon + bounty box (1920 scale) and the minimum teal pixels: panel frames measured 580-1430,
/// gameplay 0; the Midboss banner's teal bar reaches 200-1500 there but has no dark track rows.
pub const ICON: (u32, u32, u32, u32) = (915, 135, 90, 77);
pub const ICON_MIN_TEAL: usize = 300;
/// The dark progress track left and right of the icon (1920 scale): a row near-black over both
/// outer ends (measured 1.0 on panel frames, 0-0.5 on most others).
pub const TRACK: (u32, u32, u32, u32) = (830, 158, 260, 26);
pub const TRACK_MIN_DARK: f32 = 0.7;
/// Percent labels, bounty / outcome line (1920 scale).
pub const ALLY_PCT: (u32, u32, u32, u32) = (840, 152, 104, 34);
pub const ENEMY_PCT: (u32, u32, u32, u32) = (988, 152, 96, 34);
pub const LOWER: (u32, u32, u32, u32) = (840, 188, 260, 42);
/// The bounty alone ("$ 1482" under the icon; the portraits beside it spoil a wide read).
pub const BOUNTY: (u32, u32, u32, u32) = (916, 185, 76, 26);
/// Announcement halves (the text rises to the right): "AN UNSTABLE RIFT" and "IS OPENING...".
pub const ANNOUNCE_LEFT: (u32, u32, u32, u32) = (510, 270, 440, 90);
pub const ANNOUNCE_RIGHT: (u32, u32, u32, u32) = (880, 236, 530, 84);

fn crop(view: &crate::hudtext::View, b: (u32, u32, u32, u32)) -> DynamicImage { at_1920(view, b.0, b.1, b.2, b.3) }
fn shifted(b: (u32, u32, u32, u32), dy: i32) -> (u32, u32, u32, u32) { (b.0, (b.1 as i32 + dy).max(0) as u32, b.2, b.3) }

/// The panel sits lower while a Patron banner is drawn above it (rec17 33:03: labels 6-8 px down,
/// "Nearby Defender Resistance" under it). Located by the top of the teal icon (the first row with
/// [`ICON_TOP_PX`] teal pixels; the bounty / outcome text under it varies): row [`ICON_TOP`] on
/// the usual panel (1920 scale).
pub const ICON_TOP: i32 = 146;
pub const ICON_TOP_PX: usize = 3;
pub const ICON_SEARCH: (u32, u32, u32, u32) = (930, 118, 64, 60);
/// Vertical offset of the panel (1920 px); None without an icon top in the search box. Only a
/// lower panel moves the crops: the outcome animation's sparks raise the icon top (-13..-15 px)
/// while its text stays in place.
pub fn panel_offset(view: &crate::hudtext::View) -> Option<i32> {
    let c = crop(view, ICON_SEARCH).to_rgb8();
    let (w, h) = c.dimensions();
    let top = (0..h).find(|y| (0..w).filter(|x| { let p = c.get_pixel(*x, *y); teal(p[0], p[1], p[2]) }).count() >= ICON_TOP_PX)?;
    let d = ICON_SEARCH.1 as i32 + top as i32 - ICON_TOP;
    Some(if (4..=20).contains(&d) { d } else { 0 })
}

/// Pixel gate for the capture panel (cheap, every live / death frame): the panel's offset.
pub fn panel_gate(view: &crate::hudtext::View) -> Option<i32> {
    if view.canon.width() != 1600 || view.canon.height() != 900 { return None; }
    let dy = panel_offset(view)?;
    let icon = crop(view, shifted(ICON, dy)).to_rgb8();
    if icon.pixels().filter(|p| teal(p[0], p[1], p[2])).count() < ICON_MIN_TEAL { return None; }
    let t = crop(view, shifted(TRACK, dy)).to_rgb8();
    let (w, h) = t.dimensions();
    let cols: Vec<u32> = (5..40).chain(w.saturating_sub(40)..w.saturating_sub(5)).collect();
    (0..h).any(|y| cols.iter().filter(|x| t.get_pixel(**x, y).0.iter().copied().max().unwrap_or(255) < 45).count() as f32 >= TRACK_MIN_DARK * cols.len() as f32).then_some(dy)
}

/// The four panel lines to recognise (ally %, enemy %, outcome line, bounty) when the gate passes.
pub fn panel_crops(view: &crate::hudtext::View) -> Option<Vec<DynamicImage>> {
    let dy = panel_gate(view)?;
    Some([ALLY_PCT, ENEMY_PCT, LOWER, BOUNTY].iter().map(|b| crop(view, shifted(*b, dy))).collect())
}
/// Red glyph pixels where "RIFT LOST" is written (1920 scale), the bounty's place on other frames.
pub const LOST_AREA: (u32, u32, u32, u32) = (870, 192, 140, 34);
/// (red, teal) glyph pixels there.
pub fn outcome_pixels(view: &crate::hudtext::View) -> (usize, usize) {
    let c = crop(view, shifted(LOST_AREA, panel_offset(view).unwrap_or(0))).to_rgb8();
    let red = c.pixels().filter(|p| { let (h, s, v) = hsv(p[0], p[1], p[2]); (h < 12.0 || h > 348.0) && s > 0.5 && v > 0.55 }).count();
    let tl = c.pixels().filter(|p| { let (h, s, v) = hsv(p[0], p[1], p[2]); (140.0..=190.0).contains(&h) && s > 0.35 && v > 0.6 }).count();
    (red, tl)
}
/// "RIFT LOST" is large red text: 870-1000 red pixels on every lost panel of the recordings
/// (rec2/5/6/8/9/10/13/16), at most ~110 otherwise (an enemy portrait). Its OCR drops letters
/// ("RIOST", "RFTO"), so the colour decides when the line is not read.
pub const LOST_MIN_RED: usize = 600;
/// The panel's lines read (`lines` from [`panel_crops`], `red` from [`outcome_pixels`]):
/// (ally %, enemy %, outcome or bounty).
pub fn interpret(lines: &[String], red: usize) -> (Option<u8>, Option<u8>, Lower) {
    let get = |i: usize| lines.get(i).map(String::as_str).unwrap_or("");
    let (a, e) = (parse_pct(get(0)), parse_pct(get(1)));
    // The outcome comes from the wide line, the bounty from the crop under the icon only. The red
    // text alone needs a percent read on the same frame (the Midboss banner's red bar is no panel).
    let lower = match parse_lower(get(2)) {
        o @ (Lower::Secured | Lower::Lost | Lower::Draw) => o,
        _ if red >= LOST_MIN_RED && (a.is_some() || e.is_some()) => Lower::Lost,
        _ => match parse_lower(get(3)) { b @ Lower::Bounty(_) => b, _ => Lower::Unread },
    };
    (a, e, lower)
}

/// Pixel gate for the announcement: large cream glyphs over the centre band.
pub fn announce_gate(view: &crate::hudtext::View) -> bool {
    if view.canon.width() != 1600 || view.canon.height() != 900 { return false; }
    let c = at_1920(view, 510, 236, 900, 124).thumbnail(225, 31).to_rgb8();
    let n = c.pixels().count().max(1) as f32;
    let cream = c.pixels().filter(|p| { let (_, s, v) = hsv(p[0], p[1], p[2]); v > 0.82 && s < 0.3 }).count() as f32 / n;
    (0.06..=0.5).contains(&cream)
}
/// Announcement halves to recognise, downscaled to a 40 px line height.
pub fn announce_crops(view: &crate::hudtext::View) -> Option<Vec<DynamicImage>> {
    announce_gate(view).then(|| [ANNOUNCE_LEFT, ANNOUNCE_RIGHT].iter().map(|b| { let c = crop(view, *b); c.resize(c.width() * 40 / c.height().max(1), 40, image::imageops::FilterType::Triangle) }).collect())
}

fn letters(s: &str) -> String { s.chars().filter(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase() }
/// Whether a word of `text` is `word` with at most `edits` OCR slips.
fn has_word(text: &str, word: &str, edits: usize) -> bool {
    let t = letters(text);
    if t.len() + edits < word.len() { return false; }
    (0..=t.len().saturating_sub(word.len().saturating_sub(edits))).any(|i| {
        (word.len().saturating_sub(edits)..=word.len() + edits).any(|l| t.get(i..(i + l).min(t.len())).is_some_and(|w| crate::death::levenshtein(w, word) <= edits))
    })
}
/// "AN UNSTABLE RIFT IS OPENING..." read as two halves.
pub fn says_rift_opening(left: &str, right: &str) -> bool {
    let both = format!("{left} {right}");
    has_word(&both, "UNSTABLE", 2) || has_word(&both, "RIFT", 0) && has_word(&both, "OPENING", 2)
}

/// A percent label ("33%", "-79%-", "100%"): 0-100, None otherwise.
pub fn parse_pct(text: &str) -> Option<u8> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let Some(at) = t.find('%') else {
        // The '%' glyph read as a trailing 6 (rec17 11:30 "226" for 22%, "336" for 33%): only when
        // the digits are otherwise impossible (over 100).
        let d: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
        if d.len() != t.trim_matches(|c: char| !c.is_ascii_alphanumeric()).len() || !d.ends_with('6') || !(3..=4).contains(&d.len()) { return None; }
        let v: u32 = d.parse().ok()?;
        return (v > 100).then(|| d[..d.len() - 1].parse::<u32>().ok()).flatten().filter(|v| *v <= 100).map(|v| v as u8);
    };
    let digits: String = t[..at].chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    if digits.is_empty() || digits.len() > 3 { return None; }
    digits.parse::<u32>().ok().filter(|v| *v <= 100).map(|v| v as u8)
}

/// What the line under the icon says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lower { Bounty(u32), Secured, Lost, Draw, Unread }
/// The bounty must be the wiki's 1482 + 222 per minute from minute 13 (rec values 1482, 2592,
/// 2814, 4146, 4368, 5700, 5922, 7476, 9252); a trailing winner's comeback bonus is not shown here.
pub fn bounty_ok(v: u32) -> bool { v >= 1482 && (v - 1482) % 222 == 0 && v <= 1482 + 222 * 80 }
pub fn parse_lower(text: &str) -> Lower {
    if has_word(text, "SECURED", 2) { return Lower::Secured; }
    let l = letters(text);
    if l.contains("RIFT") && has_word(text, "LOST", 1) || has_word(text, "RIFTLOST", 1) { return Lower::Lost; }
    if has_word(text, "DRAW", 0) { return Lower::Draw; }
    // The bounty's 1 reads as i / l / I next to the '$' (rec17 "?i482").
    let digits: String = text.chars().filter_map(|c| match c { '0'..='9' => Some(c), 'i' | 'l' | 'I' | '|' => Some('1'), _ => None }).collect();
    if (4..=5).contains(&digits.len()) { if let Ok(v) = digits.parse::<u32>() { if bounty_ok(v) { return Lower::Bounty(v); } } }
    Lower::Unread
}

/// Minimap Rift sites (canonical 335x350 crop): the side-lane bridges, measured as the centre of the
/// announcement ping (rec17 11:03 / 25:11 left (48, 178); rec16 12:48 / 32:44, rec11 18:38, rec13
/// 33:21 right (270-279, 177-180)). The urn's bridge spawn (68, 175) / (259, 175) is beside them.
pub const SITES: [(f32, f32); 2] = [(48.0, 178.0), (273.0, 178.0)];
/// The announcement ping: a bright teal ring (hue ~160-170, s > 0.6) around the site for ~5 s.
/// Annulus pixels per canonical px^2 measured 150-1000 on announcements; the urn on its bridge
/// adds ~30 (it is cyan, hue > 176).
pub const PING_MIN: f32 = 120.0;
/// The ping ring is centred on the bridge: the teal pixels' centroid within this many canonical px
/// of the site (announcements measured 0.5-3.5 px off). A teammate's ping around a hero near the
/// bridge (rec17 23:49: 19 px off) and the Soul Urn's spawn ping on its bridge (rec11 / rec13 25:00:
/// 18-20 px off) are not.
pub const PING_CENTRE_PX: f32 = 9.0;
pub fn site_pings(map: &image::RgbImage) -> [f32; 2] {
    let s = map.width() as f32 / crate::minimap::CW;
    SITES.map(|(cx, cy)| {
        let (r0, r1) = (12.0 * s, 26.0 * s);
        let (mut n, mut sx, mut sy, mut all) = (0u32, 0f32, 0f32, 0u32);
        let r = (32.0 * s).ceil() as i32;
        for dy in -r..=r { for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > 32.0 * s { continue; }
            let (x, y) = ((cx * s) as i32 + dx, (cy * s) as i32 + dy);
            if x < 0 || y < 0 || x as u32 >= map.width() || y as u32 >= map.height() { continue; }
            let p = map.get_pixel(x as u32, y as u32); let (h, sat, v) = hsv(p[0], p[1], p[2]);
            if !((150.0..=176.0).contains(&h) && sat > 0.6 && v > 0.5) { continue; }
            all += 1; sx += dx as f32; sy += dy as f32;
            if d >= r0 && d <= r1 { n += 1; }
        }}
        let centred = all > 0 && (sx / all as f32).hypot(sy / all as f32) <= PING_CENTRE_PX * s;
        if centred { n as f32 / (s * s) } else { 0.0 }
    })
}
/// Normalised minimap position of a site (0 = left, 1 = right).
pub fn site_pt(k: usize) -> coach_core::Pt { coach_core::Pt { x: SITES[k].0 / crate::minimap::CW, y: SITES[k].1 / crate::minimap::CH } }

/// One frame's Rift evidence.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// The announcement text was read.
    pub announced: bool,
    /// Ping at a site (0 = left, 1 = right).
    pub ping: Option<usize>,
    /// Capture panel read: (ally %, enemy %) and the line under the icon.
    pub panel: Option<(Option<u8>, Option<u8>, Lower)>,
    /// Your minimap position, for the lane when you are at a site while the panel is up.
    pub you: Option<coach_core::Pt>,
}

/// Your marker this close to a site (minimap fractions) is at that bridge.
pub const SITE_RADIUS: f32 = 0.07;
/// Seconds after the announcement that the Rift opens (wiki: 20 s).
pub const OPEN_AFTER_SECS: u32 = 20;
/// The capture panel is only drawn near the Rift (rec17 19:18-19:44: captured with no panel on
/// screen). An announced Rift is reported open (unseen) this long after its opening, and a seen
/// panel held this long after its last read (contest up to 60 s + capture), before the Rift is
/// closed (outcome unknown) or, never seen open, forgotten.
pub const UNSEEN_OPEN_SECS: u32 = 75;
pub const PANEL_GONE_SECS: u32 = 70;
/// Progress rates and the contest count are only reported this long after a panel read.
pub const PANEL_FRESH_SECS: u32 = 12;
/// An outcome is reported this long after it was read.
pub const OUTCOME_HOLD_SECS: u32 = 120;
/// Both sides at this or more: both teams are in the point.
pub const CONTESTED_PCT: u8 = 95;
/// A minimap ping alone announces a Rift only inside its window: the first from 10:30 to 13:30
/// (effect 10-12 min + 60 s), a later one 6-8 minutes (+-30 s) after the last Rift's
/// announcement, or 2 / 3 windows later when Rifts were missed. A teammate's ping on the bridge
/// outside it is not the Rift (rec11 17:41, a minute before the real one).
pub const FIRST_WINDOW: (u32, u32) = (630, 810);
pub fn ping_in_window(anchor: Option<u32>, clock: u32) -> bool {
    match anchor {
        None => (FIRST_WINDOW.0..=FIRST_WINDOW.1).contains(&clock),
        Some(a) => (1..=3).any(|k| clock + 30 >= a + 360 * k && clock <= a + 480 * k + 30),
    }
}

#[derive(Debug, Clone, Default)]
struct Current {
    announced_at: Option<u32>, opened_at: Option<u32>, last_panel: Option<u32>,
    ally: Option<u8>, enemy: Option<u8>, prev: Option<(u32, Option<u8>, Option<u8>)>,
    bounty: Option<u32>, contested_since: Option<u32>, lane_votes: [u32; 2],
    ally_rising: bool, enemy_rising: bool, outcome: Option<(&'static str, u32)>,
    /// Panel frames with your marker read: at each site, and in total.
    you_at: [u32; 2], you_seen: u32,
}
impl Current {
    fn site(&self) -> Option<usize> { let [l, r] = self.lane_votes; if l > r { Some(0) } else if r > l { Some(1) } else { None } }
    fn est_open(&self) -> Option<u32> { self.opened_at.or(self.announced_at.map(|a| a + OPEN_AFTER_SECS)) }
}

/// Per-match Rift state from the frames' evidence. Only observed facts are reported: a phase
/// needs its text / ping / panel read; the lane needs a site ping or your marker at a site.
#[derive(Debug, Clone, Default)]
pub struct Tracker { cur: Option<Current>, history: Vec<coach_core::RiftEvent>, last_clock: Option<u32>,
    /// Announcement time of the last Rift seen in any form (est. opening - 20 s from a panel).
    anchor: Option<u32> }

impl Tracker {
    pub fn reset(&mut self) { *self = Tracker::default(); }
    fn lane(c: &Current, layout: Option<bool>) -> Option<Lane> {
        let k = c.site()? * 2;
        layout.map(|m| crate::minimap::lane_of(k, m))
    }
    /// Into the history: only a Rift whose panel or outcome was seen.
    fn close(&mut self, c: Current, clock: u32, layout: Option<bool>) {
        if c.opened_at.is_none() && c.outcome.is_none() { return; }
        let you_there = c.site().filter(|_| c.you_seen >= 2).map(|k| c.you_at[k] >= 1);
        self.history.push(coach_core::RiftEvent {
            opened_at: c.est_open(),
            ended_at: Some(c.outcome.map(|o| o.1).or(c.last_panel).unwrap_or(clock)),
            outcome: c.outcome.map(|o| o.0.to_string()), bounty: c.bounty, lane: Self::lane(&c, layout),
            side: c.site().map(|k| ["left", "right"][k].to_string()), you_there,
        });
    }
    /// Feed one HUD frame (clock in game seconds). `layout`: the minimap lane layout.
    pub fn observe(&mut self, f: &Frame, clock: u32, layout: Option<bool>) -> Option<RiftState> {
        if self.last_clock.is_some_and(|c| clock + 120 < c) { self.reset(); }
        self.last_clock = Some(clock);
        let running = self.cur.as_ref().is_some_and(|c| c.outcome.is_none_or(|o| clock < o.1 + 20));
        let ping = f.ping.filter(|_| running || ping_in_window(self.anchor, clock));
        // Rifts are 6+ minutes apart: anything within 4 minutes of the last outcome is that one.
        let last_end = self.history.last().and_then(|e| e.ended_at).max(self.cur.as_ref().and_then(|c| c.outcome.map(|o| o.1)));
        let spaced = last_end.is_none_or(|e| clock >= e + 240);
        if !running && spaced && (f.announced || f.panel.is_some() || ping.is_some()) {
            if let Some(c) = self.cur.take() { self.close(c, clock, layout); }
            self.cur = Some(Current::default());
        }
        let Some(mut c) = self.cur.take() else { return self.after() };
        if (f.announced || ping.is_some()) && c.opened_at.is_none() && c.announced_at.is_none() { c.announced_at = Some(clock); self.anchor = Some(clock); }
        if let Some(k) = ping { c.lane_votes[k] += 3; }
        if let Some((a, e, lower)) = f.panel {
            if c.opened_at.is_none() {
                // The panel is drawn only near the Rift: with the announcement seen, it opened 20 s
                // after it (wiki) even when the panel shows up later (rec10: announced 47:47, panel
                // first on screen 49:05).
                c.opened_at = Some(c.announced_at.map_or(clock, |t| (t + OPEN_AFTER_SECS).min(clock)));
                if c.announced_at.is_none() { self.anchor = Some(clock.saturating_sub(OPEN_AFTER_SECS)); }
            }
            c.last_panel = Some(clock);
            if let Lower::Bounty(v) = lower { c.bounty = Some(v); }
            // A read far below the last one is a misread (the bar under the label: rec10 "7%" between 64% and 76%).
            let a = a.filter(|x| c.ally.is_none_or(|y| *x + 30 >= y));
            let e = e.filter(|x| c.enemy.is_none_or(|y| *x + 30 >= y));
            // One side reaching 100% from 85%+ while the other is below: that side took it, even
            // when the outcome line was not read.
            let full = |x: Option<u8>, prev: Option<u8>, other: Option<u8>| x == Some(100) && prev.is_some_and(|p| p >= 85) && other.is_some_and(|o| o < 100);
            let inferred = if full(a, c.ally, e.or(c.enemy)) { Some("captured") } else if full(e, c.enemy, a.or(c.ally)) { Some("lost") } else { None };
            let outcome = match lower { Lower::Secured => Some("captured"), Lower::Lost => Some("lost"), Lower::Draw => Some("draw"), _ => inferred };
            if let Some(o) = outcome { if c.outcome.is_none() { c.outcome = Some((o, clock)); } }
            if c.outcome.is_none() {
                // Rates over the last read (percent labels only grow while a team is in the point).
                // A side not read this frame keeps its last rate; a stale previous read gives none.
                if let Some((t, pa, pe)) = c.prev {
                    let fresh = clock <= t + PANEL_FRESH_SECS;
                    if let (Some(x), Some(y)) = (a, pa) { c.ally_rising = fresh && x > y; }
                    if let (Some(x), Some(y)) = (e, pe) { c.enemy_rising = fresh && x > y; }
                }
                if a.is_some() || e.is_some() { c.prev = Some((clock, a.or(c.ally), e.or(c.enemy))); }
                if a.is_some() { c.ally = a; }
                if e.is_some() { c.enemy = e; }
                // Both in the point: both at the cap, both rising, or one held at the cap (a lone team
                // there would have claimed it) while the other rises (rec9 34:00: ours 99%, theirs 57% -> 96%).
                let capped = |x: Option<u8>, rising: bool| x.is_some_and(|v| v >= CONTESTED_PCT && v < 100) && !rising;
                let contested = c.ally.zip(c.enemy).is_some_and(|(x, y)| x >= CONTESTED_PCT && y >= CONTESTED_PCT) || c.ally_rising && c.enemy_rising
                    || capped(c.ally, c.ally_rising) && c.enemy_rising || capped(c.enemy, c.enemy_rising) && c.ally_rising;
                if contested { c.contested_since.get_or_insert(clock); } else if c.ally_rising != c.enemy_rising { c.contested_since = None; }
            }
            // You at a site while it is open: that is the Rift's bridge.
            if let Some(p) = f.you {
                c.you_seen += 1;
                for k in 0..2 { let q = site_pt(k); if (p.x - q.x).hypot(p.y - q.y) <= SITE_RADIUS { c.lane_votes[k] += 1; c.you_at[k] += 1; } }
            }
        }
        // Gone: never seen open well after its opening, a panel no longer drawn, an old outcome.
        let never_seen = c.opened_at.is_none() && c.outcome.is_none() && c.est_open().is_some_and(|o| clock > o + UNSEEN_OPEN_SECS);
        let gone = c.last_panel.is_some_and(|t| clock > t + PANEL_GONE_SECS) && c.outcome.is_none();
        let over = c.outcome.is_some_and(|o| clock > o.1 + OUTCOME_HOLD_SECS);
        if never_seen { return self.after(); }
        if gone || over { self.close(c, clock, layout); return self.after(); }
        let lane = Self::lane(&c, layout);
        let panel_fresh = c.last_panel.is_some_and(|t| clock <= t + PANEL_FRESH_SECS);
        let open_now = c.opened_at.is_some() || c.est_open().is_some_and(|o| clock >= o);
        let phase = match c.outcome {
            Some((o, _)) => o,
            None if panel_fresh && c.contested_since.is_some() => "contested",
            None if open_now => "open",
            None => "announced",
        };
        let live = c.outcome.is_none() && panel_fresh;
        let st = RiftState {
            phase: phase.into(), lane, side: c.site().map(|k| ["left", "right"][k].to_string()), seen_at: c.announced_at.or(c.opened_at),
            opened_at: c.est_open(),
            ally_progress: c.ally.filter(|_| live), enemy_progress: c.enemy.filter(|_| live),
            ally_rising: c.ally_rising && live, enemy_rising: c.enemy_rising && live,
            contested_secs: c.contested_since.filter(|_| live).map(|s| clock.saturating_sub(s)),
            bounty: c.bounty, captured_by: c.outcome.and_then(|o| match o.0 { "captured" => Some("ally".to_string()), "lost" => Some("enemy".to_string()), _ => None }),
            ended_at: c.outcome.map(|o| o.1), history: self.history.clone(),
        };
        self.cur = Some(c);
        Some(st)
    }
    /// Between Rifts: no phase, but what happened so far this match.
    fn after(&self) -> Option<RiftState> {
        (!self.history.is_empty()).then(|| RiftState { phase: "none".into(), history: self.history.clone(), ..Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_text() {
        assert_eq!(parse_pct("33%"), Some(33)); assert_eq!(parse_pct("-79%-"), Some(79)); assert_eq!(parse_pct("100%"), Some(100));
        assert_eq!(parse_pct("0%"), Some(0)); assert_eq!(parse_pct("101%"), None); assert_eq!(parse_pct("MID-BOSS"), None); assert_eq!(parse_pct("%"), None);
        assert_eq!(parse_lower("$ 1482"), Lower::Bounty(1482)); assert_eq!(parse_lower("$2592"), Lower::Bounty(2592)); assert_eq!(parse_lower("5922"), Lower::Bounty(5922));
        assert_eq!(parse_lower("1483"), Lower::Unread, "not the wiki's bounty");
        assert_eq!(parse_lower("RIFT SECURED!"), Lower::Secured); assert_eq!(parse_lower("RIFTSECUREDI"), Lower::Secured); assert_eq!(parse_lower("IFT SECURED!"), Lower::Secured);
        assert_eq!(parse_lower("RIFT LOST"), Lower::Lost); assert_eq!(parse_lower("RIFTLOST"), Lower::Lost); assert_eq!(parse_lower("RIFT L0ST"), Lower::Lost);
        assert_eq!(parse_lower("Sinner's Sacrifice"), Lower::Unread);
        assert!(says_rift_opening("AN UNSTABLE RIFT", "IS OPENING...")); assert!(says_rift_opening("AN UNSTABIE", "")); assert!(says_rift_opening("RIFT", "IS OPENNG"));
        assert!(!says_rift_opening("BOON RECEIVED", "")); assert!(!says_rift_opening("Defend the Greenwich", "Guardian"));
    }
    fn panel(a: u8, e: u8) -> Frame { Frame { panel: Some((Some(a), Some(e), Lower::Bounty(1482))), ..Default::default() } }
    #[test]
    fn rec17_first_rift_sequence() {
        // Announced 11:01 with the ping on the left bridge (layout: Green on the left), opened
        // 11:24, ours 5% -> 97%, secured 11:46; panel gone 11:53.
        let mut t = Tracker::default();
        assert!(t.observe(&Frame::default(), 650, Some(false)).is_none());
        let s = t.observe(&Frame { announced: true, ..Default::default() }, 661, Some(false)).unwrap();
        assert_eq!((s.phase.as_str(), s.lane, s.seen_at, s.opened_at), ("announced", None, Some(661), Some(681)));
        let s = t.observe(&Frame { ping: Some(0), ..Default::default() }, 663, Some(false)).unwrap();
        assert_eq!(s.lane, Some(Lane::Green));
        assert_eq!(t.observe(&Frame::default(), 675, Some(false)).unwrap().phase, "announced");
        let s = t.observe(&panel(0, 0), 684, Some(false)).unwrap();
        assert_eq!((s.phase.as_str(), s.opened_at, s.bounty), ("open", Some(681), Some(1482)), "20 s after the announcement");
        t.observe(&panel(5, 0), 687, Some(false));
        let s = t.observe(&panel(33, 0), 692, Some(false)).unwrap();
        assert!(s.ally_rising && !s.enemy_rising); assert_eq!((s.ally_progress, s.enemy_progress), (Some(33), Some(0)));
        let s = t.observe(&Frame { panel: Some((Some(100), Some(0), Lower::Secured)), ..Default::default() }, 706, Some(false)).unwrap();
        assert_eq!((s.phase.as_str(), s.captured_by.as_deref(), s.ended_at), ("captured", Some("ally"), Some(706)));
        // Held for the coach, then closed into the history.
        assert_eq!(t.observe(&Frame::default(), 713, Some(false)).unwrap().phase, "captured");
        let s = t.observe(&Frame::default(), 830, Some(false)).unwrap();
        assert_eq!(s.phase, "none"); assert_eq!(s.history.len(), 1);
        assert_eq!((s.history[0].outcome.as_deref(), s.history[0].lane, s.history[0].opened_at), (Some("captured"), Some(Lane::Green), Some(681)));
    }
    #[test]
    fn a_side_held_at_the_cap_while_the_other_rises_is_contested() {
        // rec9 33:52-34:11: ours 17 -> 56 -> 99 (held), theirs 14 -> 30 -> 57 -> 96 -> 99.
        let mut t = Tracker::default();
        for (clock, a, e) in [(2032, 17, 14), (2035, 56, 30), (2040, 99, 57), (2047, 99, 96)] {
            let s = t.observe(&panel(a, e), clock, None).unwrap();
            if clock > 2032 { assert_eq!(s.phase, "contested", "{clock}"); }
            assert!(!(s.enemy_rising && !s.ally_rising && s.ally_progress.is_some_and(|p| p < CONTESTED_PCT)), "not 'they are alone' at {clock}");
        }
    }
    #[test]
    fn contested_lost_and_unknown_end() {
        let mut t = Tracker::default();
        t.observe(&panel(8, 6), 1105, None);
        let s = t.observe(&panel(81, 64), 1115, None).unwrap();
        assert_eq!(s.phase, "contested"); assert_eq!(s.contested_secs, Some(0)); assert_eq!(s.lane, None, "lane unknown without a ping");
        let s = t.observe(&panel(99, 99), 1125, None).unwrap(); assert_eq!(s.contested_secs, Some(10));
        // The panel is only drawn near the Rift: unseen for a while it stays open, without rates.
        let s = t.observe(&Frame::default(), 1145, None).unwrap();
        assert_eq!((s.phase.as_str(), s.ally_progress, s.contested_secs), ("open", None, None));
        // rec6 18:48: the panel went without an outcome read: the Rift ended, outcome unknown.
        let s = t.observe(&Frame::default(), 1200, None).unwrap();
        assert_eq!(s.phase, "none"); assert_eq!(s.history[0].outcome, None);
        // The next one, lost.
        t.observe(&panel(12, 0), 1559, None);
        let s = t.observe(&Frame { panel: Some((Some(99), Some(100), Lower::Lost)), ..Default::default() }, 1584, None).unwrap();
        assert_eq!((s.phase.as_str(), s.captured_by.as_deref()), ("lost", Some("enemy")));
    }
    #[test]
    fn early_pings_and_lone_announcements_are_not_rifts() {
        let mut t = Tracker::default();
        assert!(t.observe(&Frame { ping: Some(0), ..Default::default() }, 601, None).is_none(), "before the first window");
        let s = t.observe(&Frame { announced: true, ..Default::default() }, 700, None).unwrap();
        assert_eq!(s.phase, "announced");
        assert_eq!(t.observe(&Frame::default(), 730, None).unwrap().phase, "open", "opened (panel not on screen)");
        // Never seen open: forgotten, nothing in the history.
        assert!(t.observe(&Frame::default(), 800, None).is_none());
        // rec11: announced 12:35 (ping); a ping on the bridge at 17:41 is outside the next window
        // (18:05-20:35), the one at 18:47 is the Rift.
        let mut t = Tracker::default();
        assert_eq!(t.observe(&Frame { ping: Some(0), ..Default::default() }, 755, Some(false)).unwrap().phase, "announced");
        assert!(t.observe(&Frame { ping: Some(1), ..Default::default() }, 1061, Some(false)).is_none());
        let s = t.observe(&Frame { ping: Some(1), ..Default::default() }, 1127, Some(false)).unwrap();
        assert_eq!((s.phase.as_str(), s.lane, s.seen_at), ("announced", Some(Lane::Yellow), Some(1127)));
    }
}
