//! Own death: the respawn countdown (death screen and the shop opened while dead) and the
//! "INCOMING DAMAGE (LAST DEATH)" panel. All coordinates are canonical 1600x900.
use crate::hudtext::{blobs, crop_padded, white};
use crate::minimap::{hsv, Blob};
use coach_perception::Bounds;
use image::DynamicImage;

/// Dark number box right of the "Respawn in:" banner on the death screen.
pub const RESPAWN_BOX: Bounds = Bounds { x: 840, y: 662, w: 88, h: 46 };
/// The same banner drawn top-left while the shop is open during a death.
pub const SHOP_RESPAWN_LABEL: Bounds = Bounds { x: 14, y: 18, w: 172, h: 40 };
pub const SHOP_RESPAWN_BOX: Bounds = Bounds { x: 186, y: 16, w: 84, h: 46 };

/// White digit glyphs in a number box: their tight bounds and count.
pub fn digit_glyphs(img: &DynamicImage, area: Bounds) -> Option<(Bounds, usize)> {
    let mut g: Vec<Blob> = blobs(img, area, white).into_iter()
        .filter(|b| { let h = b.h() as f32 / area.h as f32; (0.3..=0.85).contains(&h) && b.area >= 10 && b.w() >= 2 && b.min_y > area.y && b.max_y + 1 < area.y + area.h }).collect();
    if g.is_empty() || g.len() > 3 { return None; }
    g.sort_by_key(|b| b.min_x);
    let h = g.iter().map(|b| b.h()).max()?;
    if g.iter().any(|b| (b.h() as f32) < h as f32 * 0.8) || g.windows(2).any(|w| w[1].min_x > w[0].max_x + h) { return None; }
    let (x0, x1) = (g[0].min_x, g.last()?.max_x);
    let (y0, y1) = (g.iter().map(|b| b.min_y).min()?, g.iter().map(|b| b.max_y).max()?);
    Some((Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }, g.len()))
}

/// Central world area sampled for the revive cue (between the top bar, the damage panel and
/// the ability bar).
pub const WORLD_AREA: Bounds = Bounds { x: 380, y: 130, w: 800, h: 480 };
/// Share of coloured pixels (saturation > 0.3, value > 0.25) in the central world area.
/// A Rejuvenator revive greys the world out under the "Respawn in: 1" banner (rec8 39:17,
/// ~0.02); real death screens are red-tinted or spectate a teammate in colour (>= 0.3).
pub fn world_colour_share(img: &DynamicImage) -> f32 {
    let a = WORLD_AREA;
    if img.width() < a.x + a.w || img.height() < a.y + a.h { return 1.0; }
    let rgb = img.crop_imm(a.x, a.y, a.w, a.h).to_rgb8();
    let (mut n, mut c) = (0u32, 0u32);
    for y in (0..a.h).step_by(4) { for x in (0..a.w).step_by(4) {
        let p = rgb.get_pixel(x, y); let (_, s, v) = hsv(p[0], p[1], p[2]);
        n += 1; if s > 0.3 && v > 0.25 { c += 1; }
    } }
    c as f32 / n.max(1) as f32
}
/// Upper bound of [`world_colour_share`] for the greyed revive screen.
pub const REVIVE_MAX_COLOUR: f32 = 0.10;
/// Highest countdown a revive screen shows ("Respawn in: 1"; real deaths start at 8+ s).
pub const REVIVE_MAX_COUNT: u32 = 2;
/// A revive returns to the live HUD this soon after its death screen was first seen (rec8
/// 39:17 -> 39:22); the shortest real death measured took 10 s (rec13 6:07 -> 6:17).
pub const REVIVE_RETURN_MS: u64 = 8_000;
/// The death screen must follow a live frame this closely to judge its first moments.
pub const REVIVE_AFTER_ALIVE_MS: u64 = 10_000;

/// Rejuvenator revive detector. Every death screen starts grey for a moment, but a real one
/// shows its full countdown (43, 59, 85 ... rec8) and lasts 10 s or more; a revive shows
/// "Respawn in: 1" on the grey world, without the shop button, and the live HUD is back within
/// seconds. A frame is flagged when the countdown confirms it, else when the live HUD returns
/// after a brief grey death screen whose countdown was never above `REVIVE_MAX_COUNT`.
#[derive(Default, Clone, Debug)]
pub struct ReviveWatch { stretch: Option<ReviveStretch> }
#[derive(Clone, Copy, Debug)]
struct ReviveStretch { start: u64, grey: bool, max_read: Option<u32>, flagged: bool }
impl ReviveWatch {
    /// A death-screen frame. `first`: the previous frame was alive; `since_alive`: ms since the
    /// last live frame; `grey`: [`world_colour_share`] below `REVIVE_MAX_COLOUR`; `read`: the
    /// countdown read on this frame. Returns true when this frame shows a revive.
    pub fn dead(&mut self, first: bool, since_alive: Option<u64>, grey: bool, read: Option<u32>, now: u64) -> bool {
        let s = match (&mut self.stretch, first) {
            (Some(s), false) => { s.grey &= grey; s.max_read = s.max_read.max(read); s }
            (slot, _) => {
                let recent = since_alive.is_some_and(|d| d <= REVIVE_AFTER_ALIVE_MS);
                slot.insert(ReviveStretch { start: now, grey: grey && recent, max_read: read, flagged: false })
            }
        };
        let revive = s.grey && s.max_read.is_some_and(|r| r <= REVIVE_MAX_COUNT) && now >= s.start && now - s.start <= REVIVE_RETURN_MS;
        if revive { s.flagged = true; }
        revive
    }
    /// A live-HUD frame: ends the death screen. True when it completed a revive that no
    /// death-screen frame could confirm (countdown unread).
    pub fn alive(&mut self, now: u64) -> bool {
        let Some(s) = self.stretch.take() else { return false };
        !s.flagged && s.grey && s.max_read.is_none_or(|r| r <= REVIVE_MAX_COUNT) && now >= s.start && now - s.start <= REVIVE_RETURN_MS
    }
    /// The shop opened during the death (its respawn banner): a real death.
    pub fn interrupt(&mut self) { if let Some(s) = &mut self.stretch { s.grey = false; } }
    pub fn reset(&mut self) { self.stretch = None; }
}

/// Tight padded crop around located digits for line recognition.
pub fn digits_crop(img: &DynamicImage, b: Bounds) -> DynamicImage {
    crop_padded(img, b.x, b.y, b.x + b.w - 1, b.y + b.h - 1, (b.h / 3).max(3))
}

/// Integer of exactly `count` digits from a line read ("30" with 2 glyphs; "0" is rejected).
pub fn read_count(text: &str, count: usize) -> Option<u32> {
    let d: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    (d.len() == count && count > 0).then(|| d.parse().ok()).flatten()
}

/// Respawn countdown: a reading is accepted when it continues the running countdown (±2 s), or
/// when two consecutive readings agree with each other as a countdown (a new death or the first
/// read). Between readings the countdown is extrapolated for at most 10 s.
#[derive(Default, Debug, Clone)]
pub struct Countdown {
    anchor: Option<(u32, u64)>,
    pending: Option<(u32, u64)>,
}
const COUNT_TOL: i64 = 2;
const COUNT_HOLD_MS: u64 = 10_000;
impl Countdown {
    fn expect(base: (u32, u64), now: u64) -> i64 { base.0 as i64 - (now.saturating_sub(base.1) / 1000) as i64 }
    /// Seconds remaining by the running countdown, when anchored by a read in the last 10 s.
    pub fn value(&self, now: u64) -> Option<u32> {
        let a = self.anchor?;
        (now >= a.1 && now - a.1 <= COUNT_HOLD_MS).then(|| Self::expect(a, now).max(0) as u32)
    }
    /// Feed one reading (or none). Returns (seconds, whether this reading started a new countdown).
    pub fn observe(&mut self, read: Option<u32>, now: u64) -> (Option<u32>, bool) {
        let Some(r) = read else { return (self.value(now), false) };
        if let Some(a) = self.anchor {
            if (r as i64 - Self::expect(a, now)).abs() <= COUNT_TOL { self.anchor = Some((r, now)); self.pending = None; return (Some(r), false); }
        }
        if let Some(p) = self.pending {
            if now > p.1 && (r as i64 - Self::expect(p, now)).abs() <= COUNT_TOL {
                // Only a jump up from a running countdown is a new death; the first anchor is not.
                let fresh = self.anchor.is_some_and(|a| r as i64 > Self::expect(a, now) + 5);
                self.anchor = Some((r, now)); self.pending = None;
                return (Some(r), fresh);
            }
        }
        self.pending = Some((r, now));
        (self.value(now), false)
    }
    /// Milliseconds since the countdown was last confirmed by a read.
    pub fn age_ms(&self, now: u64) -> Option<u64> { self.anchor.map(|(_, t)| now.saturating_sub(t)) }
    pub fn reset(&mut self) { *self = Self::default(); }
}

/// One source row of the incoming-damage panel.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRow { pub source: String, pub hits: Option<u32>, pub damage: u32, pub killing_blow: bool,
    /// Position on the panel (0 = top row).
    pub index: usize }

/// The panel is read on its own image: the canonical box below rendered at 1920x1080 scale from
/// the full-resolution source when available (its ~10 px text loses digits at 1600x900).
pub const PANEL_CANONICAL: Bounds = Bounds { x: 1217, y: 158, w: 383, h: 383 };
pub const PANEL_SIZE: u32 = 460;
/// Panel image size for a view: 1920 scale when the source has the pixels, else native 1600
/// scale (upsampling 1600 pixels merged and split digit glyphs).
pub fn panel_size(view: &crate::hudtext::View) -> u32 {
    match view.source { Some((_, ctx)) if ctx.w >= 1800 => PANEL_SIZE, _ => PANEL_CANONICAL.w }
}
/// Panel-image row geometry (1920-scale, scaled by `k` = panel width / 460): first row centre
/// y and spacing; up to 6 rows.
pub const ROW0_Y: f32 = 154.0;
pub const ROW_STEP: f32 = 38.3;
fn k_of(panel: &DynamicImage) -> f32 { panel.width() as f32 / PANEL_SIZE as f32 }
fn scaled(k: f32, x: f32, y: f32, w: f32, h: f32) -> Bounds { Bounds { x: (x * k).round() as u32, y: (y * k).round() as u32, w: (w * k).round().max(1.0) as u32, h: (h * k).round().max(1.0) as u32 } }
fn row_c(i: usize) -> f32 { ROW0_Y + ROW_STEP * i as f32 }
/// Two name crops per row (recognition of this small font is sensitive to the crop start; the
/// better dictionary match of the two wins).
pub fn name_box(i: usize, k: f32) -> Bounds { scaled(k, 195.0, row_c(i) - 10.0, 130.0, 20.0) }
/// Rows without an ability icon (world sources such as "T2 Walker") start further left.
pub fn name_box_wide(i: usize, k: f32) -> Bounds { scaled(k, 160.0, row_c(i) - 10.0, 165.0, 20.0) }
pub fn hits_box(i: usize, k: f32) -> Bounds { scaled(k, 322.0, row_c(i) - 9.0, 70.0, 18.0) }
pub fn damage_box(i: usize, k: f32) -> Bounds { scaled(k, 398.0, row_c(i) - 12.0, 54.0, 24.0) }
pub fn total_box(rows: usize, k: f32) -> Bounds { let y = row_c(rows.max(1) - 1) + 53.0; scaled(k, 138.0, y - 11.0, 185.0, 22.0) }

/// Damage-cell digit pixels. Looser than [`white`]: at 1080p the thin middle strokes of "3" and
/// "5" fall below the white threshold, which split those glyphs into two short halves that the
/// height filter dropped (rec16 37:02 read 3,142 as "142", 32:44 225 as "22"; rec13 23:09 340 /
/// 321 as "440" / "221"; rec8 27:57 501 as "O1").
pub fn panel_digit(r: u8, g: u8, b: u8) -> bool { let (_, s, v) = hsv(r, g, b); v > 0.6 && s < 0.3 }

/// Rows present on this frame: the damage cell holds 1-5 white digits (largest damage first,
/// so rows are contiguous from the top).
pub fn row_digits(panel: &DynamicImage) -> Vec<(Bounds, (usize, usize))> {
    let k = k_of(panel);
    let (hmin, hmax) = ((8.0 * k).floor() as u32, (18.0 * k).ceil() as u32);
    let mut out = vec![];
    for i in 0..6 {
        let a = damage_box(i, k);
        let g: Vec<Blob> = blobs(panel, a, panel_digit).into_iter().filter(|b| (hmin..=hmax).contains(&b.h()) && b.area as f32 >= 8.0 * k * k && b.min_y > a.y && b.max_y + 1 < a.y + a.h).collect();
        if g.is_empty() || g.len() > 5 { break; }
        let (x0, x1) = (g.iter().map(|b| b.min_x).min().unwrap(), g.iter().map(|b| b.max_x).max().unwrap());
        let (y0, y1) = (g.iter().map(|b| b.min_y).min().unwrap(), g.iter().map(|b| b.max_y).max().unwrap());
        // Touching digits merge into one blob ("75" in 175, "10" in 1,310), so each blob gives a
        // digit-count range from its width: a digit is ~0.6x as wide as tall, a "1" ~0.3x.
        // Commas are short and excluded by height.
        let (lo, hi) = g.iter().fold((0usize, 0usize), |(lo, hi), b| {
            let (w, h) = (b.w() as f32, b.h() as f32);
            (lo + if w > h * 1.25 { 2 } else { 1 }, hi + ((w + 1.0) / (h * 0.3)).floor().clamp(1.0, 4.0) as usize)
        });
        out.push((Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }, (lo, hi.min(6))));
    }
    out
}

/// The red-orange italic "Killing Blow" tag drawn over a row's name bar (panel image), from just
/// right of the ability icon to the damage cell: it sits after the visible part of the name, so
/// its position varies (rec16 24:08 x 173-385, 32:44 x 235-311; over a purple bar the tag blends
/// to hues 345-15). Normalised to 1920 scale (pixel count / k^2). Tagged rows measure 160-390, the
/// other rows 0-45 (an orange icon edge); the old narrow box (hue < 12) measured 1-27 on tags.
pub fn killing_blow_px(panel: &DynamicImage, i: usize) -> u32 {
    let k = k_of(panel);
    let a = scaled(k, 198.0, row_c(i) - 14.0, 198.0, 28.0);
    if a.x + a.w > panel.width() || a.y + a.h > panel.height() { return 0; }
    let rgb = panel.crop_imm(a.x, a.y, a.w, a.h).to_rgb8();
    (rgb.pixels().filter(|p| { let (h, s, v) = hsv(p[0], p[1], p[2]); (h < 22.0 || h > 340.0) && s > 0.55 && v > 0.6 }).count() as f32 / (k * k)) as u32
}
/// [`killing_blow_px`] at or above this is a tagged row.
pub const KILLING_BLOW_MIN_PX: u32 = 90;

/// "x18 +7%" -> 18. Only a clean "x<digits>" token counts: a glued read such as "x8+6%" (for
/// "x28 +6%") is ambiguous and yields None.
pub fn parse_hits(s: &str) -> Option<u32> {
    s.split_whitespace().find_map(|t| {
        let t = t.trim_matches(|c: char| matches!(c, '[' | ']' | '|' | '(' | ')' | '\'' | '"'));
        let d = t.strip_prefix(['x', 'X', '×'])?;
        (!d.is_empty() && d.len() <= 3 && d.chars().all(|c| c.is_ascii_digit())).then(|| d.parse().ok()).flatten()
    }).filter(|n| *n > 0)
}

/// "Total Damage: 2,248" -> 2248.
pub fn parse_total(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    // The label reads "otal Damge", "ota amg": any fragment of it, then the number after it.
    let i = ["dama", "damg", "amag", "otal", "ota "].iter().filter_map(|k| l.find(k).map(|i| i + k.len())).max()?;
    let tail = &s[i..];
    let tail = tail.find(|c: char| c.is_ascii_digit()).map(|j| &tail[j..])?;
    if tail.contains(|c: char| c.is_alphabetic()) { return None; }
    // The "Show More" button's edge reads as a trailing bar ("5,352|", "3,952 |").
    let tail = tail.trim_end().trim_end_matches(['|', ']', '[', '!', 'I', ')']).trim_end();
    let groups: Vec<&str> = tail.trim().split([',', '.']).collect();
    if groups.len() > 1 && groups[1..].iter().any(|g| g.trim().len() != 3) { return None; }
    let d: String = tail.chars().filter(|c| c.is_ascii_digit()).collect();
    (!d.is_empty() && d.len() <= 6).then(|| d.parse().ok()).flatten()
}

// ---------------------------------------------------------------- names

/// Damage-source / debuff names (data/abilities.json).
pub fn source_names() -> &'static [String] {
    static N: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    N.get_or_init(|| serde_json::from_str::<Vec<serde_json::Value>>(include_str!("../../../data/abilities.json").trim_start_matches('\u{feff}')).unwrap_or_default()
        .iter().filter_map(|v| v["name"].as_str().map(str::to_string)).collect())
}

fn key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect::<String>().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() { cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1])); }
        prev = cur;
    }
    prev[b.len()]
}

/// Closest dictionary name for OCR text, or None when nothing is close. Trailing garbage words
/// ("Stalkers Mark Y7") and a name cut short by the Killing Blow tag ("Mystic Sho") are tolerated.
pub fn match_name<'a>(text: &str, names: &'a [String]) -> Option<&'a str> { match_name_scored(text, names).map(|(n, _)| n) }
/// [`match_name`] with its normalised edit cost (lower is better).
pub fn match_name_scored<'a>(text: &str, names: &'a [String]) -> Option<(&'a str, f32)> { match_name_inner(text, names, false, &[]) }
/// `partial_last`: the last word is a name cut short by the Killing Blow tag ("Melee Ca"): dropping
/// it costs as much as a real word. `enemies`: this match's enemy heroes (class names), which
/// settle a clipped name shared by several sources.
fn match_name_inner<'a>(text: &str, names: &'a [String], partial_last: bool, enemies: &[String]) -> Option<(&'a str, f32)> {
    let words: Vec<String> = key(text).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
    // (cost, clipped, name) of every candidate.
    let mut cands: Vec<(f32, bool, &str)> = vec![];
    // A leading 1-2 character word may be an icon read as text ("S Spirit Burn").
    let starts = if words.first().is_some_and(|w| w.chars().count() <= 2) { 0..=1 } else { 0..=0 };
    for s in starts {
    for k in (s + 1..=words.len()).rev() {
        let q = words[s..k].join(" ");
        if q.len() < 3 { continue; }
        // Dropped words cost a little, so "Stalkers Mark" beats a one-word name and "T2 Walker"
        // beats "Walker"; a dropped real word (3+ letters) costs more than a clipped match, so
        // "Leech Ther" is Leech Therapy, not Leech.
        let dropped: f32 = words[..s].iter().chain(&words[k..]).enumerate().map(|(j, w)| if partial_last && k < words.len() && j == s + words.len() - k - 1 { 0.15 } else if w.chars().count() <= 2 || w.chars().any(|c| c.is_ascii_digit()) { 0.03 } else { 0.15 }).sum();
        for n in names {
            let nk = key(n);
            let full = levenshtein(&q, &nk) as f32 / nk.len().max(q.len()) as f32;
            // A clipped name: the query is a prefix of the name (needs 5+ characters).
            let prefix = if q.len() >= 5 && q.len() < nk.len() { levenshtein(&q, &nk[..nk.char_indices().nth(q.chars().count()).map_or(nk.len(), |(i, _)| i)]) as f32 / q.len() as f32 + 0.1 } else { 1.0 };
            // A name cut at both ends (a world source without an icon starts under the portrait:
            // "nple Guardia" for Temple Guardian): the query inside the name (needs 8+ characters).
            let inner = if q.len() >= 8 && q.len() < nk.len() && nk.is_ascii() && q.is_ascii() { (0..=nk.len() - q.len()).map(|i| levenshtein(&q, &nk[i..i + q.len()])).min().unwrap_or(usize::MAX) as f32 / q.len() as f32 + 0.15 } else { 1.0 };
            let score = full.min(prefix).min(inner);
            if score <= 0.25 { cands.push((score + dropped, prefix.min(inner) < full, n.as_str())); }
        }
    }
    }
    let best = cands.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    if !best.is_finite() { return None; }
    let mut tied: Vec<(bool, &str)> = cands.iter().filter(|c| c.0 <= best + 0.005).map(|c| (c.1, c.2)).collect();
    tied.sort_by_key(|t| t.1); tied.dedup_by_key(|t| t.1);
    if tied.len() == 1 { return Some((tied[0].1, best)); }
    // A clip shared by several names ("Spirit R": Spirit Rend, Spirit Resilience) is ambiguous.
    // Only this match's enemy team settles it: an ability of a hero not in the match cannot have
    // hit you (rec16 32:44 "Stalk…" is Abrams' Stalker, not Drifter's Stalker's Mark), and a lone
    // enemy ability beats shop items ("Bullet" vs Haze: Bullet Dance, not Bullet Armor).
    if tied.iter().all(|t| t.0) && enemies.len() >= 5 {
        let heroes = |n: &str| source_heroes(n);
        let possible: Vec<&str> = tied.iter().map(|t| t.1).filter(|n| heroes(n).is_empty() || heroes(n).iter().any(|h| enemies.contains(h))).collect();
        if let [one] = possible[..] { return Some((one, best)); }
        let owned: Vec<&str> = possible.iter().copied().filter(|n| heroes(n).iter().any(|h| enemies.contains(h))).collect();
        if let [one] = owned[..] { if possible.iter().all(|n| *n == one || source_kind(n) == Some("item")) { return Some((one, best)); } }
    }
    None
}
/// Kind ("ability", "weapon", "item", "world") and heroes of each dictionary source.
fn source_meta() -> &'static Vec<(String, String, Vec<String>)> {
    static K: std::sync::OnceLock<Vec<(String, String, Vec<String>)>> = std::sync::OnceLock::new();
    K.get_or_init(|| serde_json::from_str::<Vec<serde_json::Value>>(include_str!("../../../data/abilities.json").trim_start_matches('\u{feff}')).unwrap_or_default()
        .iter().filter_map(|v| Some((v["name"].as_str()?.to_string(), v["kind"].as_str().unwrap_or("").to_string(),
            v["heroes"].as_array().map(|h| h.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()))).collect())
}
fn source_kind(name: &str) -> Option<&'static str> { source_meta().iter().find(|m| m.0 == name).map(|m| m.1.as_str()) }
fn source_heroes(name: &str) -> &'static [String] { source_meta().iter().find(|m| m.0 == name).map_or(&[], |m| m.2.as_slice()) }
/// First line without the "Killing Blow" tag and hit counts. The tag overlaps the name, so OCR
/// glues it on ("Mystic ShoKilling Blow", "Meleegilling", "Melee kiling Bow 3"); the stem before
/// it is kept.
pub fn strip_tag(raw: &str) -> String { strip_tag_glued(raw).0 }
/// [`strip_tag`] and whether the tag was glued onto a word (the stem's last letter may then be
/// a piece of the tag: "Escalxling", "Gun Driling").
pub fn strip_tag_glued(raw: &str) -> (String, bool) {
    let mut words: Vec<String> = vec![];
    let ws: Vec<String> = raw.lines().next().unwrap_or("").split_whitespace().map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect()).collect();
    // The tag's "Blow" as read: "B", "Bl", "Blow", "BloW", "BlOSu", "Bow" (not "Bola", "Ball",
    // "Bolt" of Entangling Bola, Wrecking Ball, Vexing Bolt).
    let blow = |w: &str| { let l = w.to_lowercase(); l.len() <= 5 && (l == "b" || l.starts_with("bl") || l.starts_with("bow") || l.starts_with("biow")) };
    for (j, w) in ws.iter().enumerate() {
        if !w.is_ascii() { words.push(w.clone()); continue; }
        let l = w.to_lowercase();
        if l.is_empty() { continue; }
        let count = l.trim_start_matches(['x', '×']);
        if !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()) { break; }
        if matches!(l.as_str(), "blow" | "bow" | "blo" | "biow") { break; }
        // "…ling" right before "Blow" (or glued to it: "Lurkinling:Blow") is the tag's "Killing".
        let next_blow = ws.get(j + 1).is_some_and(|n| blow(n));
        let ling = l.find("lingb").or_else(|| l.find("ingb").filter(|i| *i > 0))
            .or_else(|| (next_blow && l.ends_with("ing")).then(|| l.rfind("ling").unwrap_or(l.len() - 3)));
        let kill = ["killing", "kiling", "gilling", "kiiling", "kilin", "illing", "iling", "iing"].iter().filter_map(|k| l.find(k)).min();
        // "Blow" glued on with what was left of "Killing" ("BorrowngBlow"): cut before the
        // leftover "…ng".
        let glued_blow = l.find("blow").or_else(|| l.find("bow")).filter(|i| *i >= 3).map(|i| {
            let s = &l[..i];
            ["killing", "kiling", "illing", "iling", "ling", "ing", "ng"].iter().find(|k| s.ends_with(*k)).map_or(i, |k| i - k.len())
        });
        if let Some(i) = [ling, kill, glued_blow].into_iter().flatten().min() {
            // At most one stray letter of "Killing" stays glued to the stem ("Stalkg", "Sho" + "K").
            let mut stem = w[..i].to_string();
            if stem.len() > 1 && stem.ends_with(['k', 'g', 'K', 'G']) { stem.pop(); }
            let glued = !stem.is_empty();
            if glued { words.push(stem); }
            return (words.join(" "), glued);
        }
        words.push(w.clone());
    }
    (words.join(" "), false)
}

/// Strip a glued "Killing Blow" tag / counts, then match against the dictionary. When the tag
/// was glued on and the stem does not match (or is ambiguous), the stem without its last letter
/// is tried ("Gun Dr" -> "Gun D": Gun Damage; "Bullet R" -> "Bullet": Bullet Dance vs Haze).
pub fn normalise_source(raw: &str) -> Option<String> { normalise_source_with(raw, &[]) }
/// [`normalise_source`] with this match's enemy heroes (class names), which settle a clipped
/// name shared by several sources.
pub fn normalise_source_with(raw: &str, enemies: &[String]) -> Option<String> { normalise_scored(raw, enemies).map(|(n, _)| n.to_string()) }
fn normalise_scored(raw: &str, enemies: &[String]) -> Option<(&'static str, f32)> {
    let (stem, glued) = strip_tag_glued(raw);
    match_name_inner(&stem, source_names(), glued, enemies).or_else(|| {
        if !glued { return None; }
        let mut s = stem.clone(); s.pop();
        let s = s.trim_end().to_string();
        (s.chars().count() >= 5).then(|| match_name_inner(&s, source_names(), false, enemies)).flatten().map(|(n, c)| (n, c + 0.05))
    })
}
/// Whether a row's name / hits text carries the "Killing Blow" tag (OCR variants: the tag spills
/// from the name box into the hits box as "low", "slow", "BloW").
pub fn mentions_killing_blow(name: &str, hits: &str) -> bool {
    let l = name.to_lowercase();
    let h = hits.trim().to_lowercase();
    ["killing", "kiling", "gilling", "kiiling", "illing", "iling"].iter().any(|k| l.contains(k)) || l.split_whitespace().any(|w| matches!(w, "blow" | "bow"))
        || ["low", "slow", "blow", "bow"].iter().any(|k| h.starts_with(k))
        // The whole tag in the hits box ("Ifling Blow": rec13 6:07, whose short death then got no
        // review because the killing blow was never tagged).
        || h.split_whitespace().any(|w| matches!(w.trim_matches(|c: char| !c.is_alphanumeric()), "blow" | "bow" | "blo"))
}

/// Lines per panel row in [`panel_crops`]: name, wide name, hits, damage digits.
pub const ROW_LINES: usize = 4;
/// Panel crops: per located row [name, wide name, hits, damage digits], then the total line.
pub fn panel_crops(panel: &DynamicImage) -> (Vec<(Bounds, (usize, usize))>, Vec<DynamicImage>) {
    let rows = row_digits(panel);
    let s = k_of(panel);
    let mut crops = vec![];
    for (k, (digits, _)) in rows.iter().enumerate() {
        for b in [name_box(k, s), name_box_wide(k, s), hits_box(k, s)] { crops.push(panel.crop_imm(b.x, b.y, b.w, b.h)); }
        crops.push(digits_crop(panel, *digits));
    }
    if !rows.is_empty() { let t = total_box(rows.len(), s); crops.push(panel.crop_imm(t.x, t.y, t.w, t.h)); }
    (rows, crops)
}

/// Rows and total from recognised panel lines (see [`panel_crops`]). A row is kept only when
/// its damage has the located digit count and its source matches the dictionary.
/// A damage cell read ("1,310", "613") whose digit count lies in the located glyph range.
/// Thousands separators must group by three; stray trailing marks ("17!", "23:") are ignored.
pub fn parse_damage(text: &str, lo: usize, hi: usize) -> Option<u32> {
    let t: String = text.trim().chars().take_while(|c| c.is_ascii_digit() || matches!(c, ',' | '.')).collect();
    let groups: Vec<&str> = t.split([',', '.']).filter(|g| !g.is_empty()).collect();
    if groups.is_empty() || groups.len() > 1 && groups[1..].iter().any(|g| g.len() != 3) { return None; }
    let d = groups.concat();
    (lo..=hi).contains(&d.len()).then(|| d.parse().ok()).flatten()
}

/// One read of the panel: the rows recognised (dictionary name and a damage with the located
/// digit count, in panel order), Total Damage, how many rows were located on screen and how many
/// carry the Killing Blow tag.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelRead { pub rows: Vec<PanelRow>, pub total: Option<u32>, pub located: usize, pub tags: usize }
/// Located rows a complete read may miss below the top row (a name hidden under the Killing
/// Blow tag, "Spirit R…", matches several dictionary names and is dropped, not guessed).
pub const PANEL_MISSING_ROWS: usize = 2;
impl PanelRead {
    /// Plausible enough to report: the top row (the biggest source: the death cause) is read,
    /// at most [`PANEL_MISSING_ROWS`] other located rows are missing, Total Damage is read and the
    /// listed damage does not exceed it (the panel lists the top six sources of the total).
    pub fn complete(&self) -> bool {
        let Some(t) = self.total else { return false };
        self.located > 0 && self.rows.first().is_some_and(|r| r.index == 0)
            && self.rows.len() + PANEL_MISSING_ROWS >= self.located && self.rows.len() >= self.located.min(2)
            && self.rows.iter().map(|r| r.damage as u64).sum::<u64>() <= t as u64
    }
    /// The same panel: rows (position, source, damage, tag) and total.
    pub fn same(&self, o: &PanelRead) -> bool {
        self.total == o.total && self.rows.len() == o.rows.len()
            && self.rows.iter().zip(&o.rows).all(|(a, b)| a.index == b.index && a.source == b.source && a.damage == b.damage && a.killing_blow == b.killing_blow)
    }
    /// Two reads of one panel state: the same total and top row, and every row both recognised
    /// agrees (source, damage, tag); a row one read missed is no disagreement.
    pub fn agrees(&self, o: &PanelRead) -> bool {
        self.total.is_some() && self.total == o.total && self.tags == o.tags
            && self.rows.first().zip(o.rows.first()).is_some_and(|(a, b)| a.index == 0 && b.index == 0 && a.source == b.source && a.damage == b.damage)
            && self.rows.iter().all(|a| o.rows.iter().find(|b| b.index == a.index).is_none_or(|b| a.source == b.source && a.damage == b.damage && a.killing_blow == b.killing_blow))
    }
    /// The rows of two agreeing reads together.
    fn merged(&self, o: &PanelRead) -> PanelRead {
        let mut rows = self.rows.clone();
        for r in &o.rows { if !rows.iter().any(|a| a.index == r.index) { rows.push(r.clone()); } }
        rows.sort_by_key(|r| r.index);
        PanelRead { rows, total: self.total, located: self.located.max(o.located), tags: self.tags }
    }
}

/// `enemies`: this match's enemy heroes as read on the top bar (empty when unknown).
pub fn parse_panel(panel: &DynamicImage, rows: &[(Bounds, (usize, usize))], lines: &[String], enemies: &[String]) -> PanelRead {
    let line = |i: usize| lines.get(i).map(String::as_str).unwrap_or("");
    let mut out = vec![];
    let mut tagged_rows: Vec<usize> = vec![];
    for (k, (_, (lo, hi))) in rows.iter().enumerate() {
        let (name, wide, hits, dmg) = (line(ROW_LINES * k), line(ROW_LINES * k + 1), line(ROW_LINES * k + 2), line(ROW_LINES * k + 3));
        let px = killing_blow_px(panel, k);
        // The tag's pixels decide; its text glued onto the name confirms a fainter tag.
        let text = mentions_killing_blow(name, hits) || mentions_killing_blow(wide, "");
        if px >= KILLING_BLOW_MIN_PX || text && px >= KILLING_BLOW_MIN_PX / 3 { tagged_rows.push(k); }
        let Some(damage) = parse_damage(dmg, *lo, *hi) else { continue };
        let best = [name, wide].into_iter().filter_map(|s| normalise_scored(s, enemies)).min_by(|a, b| a.1.total_cmp(&b.1));
        let Some(source) = best.map(|(s, _)| s.to_string()) else { continue };
        out.push(PanelRow { source, hits: if text { None } else { parse_hits(hits) }, damage, killing_blow: false, index: k });
    }
    // One killing blow. Two tagged rows: the panel covers two deaths (rec16 37:02 right after the
    // 36:44 Rejuvenator revive: Lurking Terrors and Leech Therapy both tagged, x63 / x92 hits that
    // the game replaced 30 s later with this death alone, x30 / x52): no killing blow is claimed.
    if let [k] = tagged_rows[..] { if let Some(r) = out.iter_mut().find(|r| r.index == k) { r.killing_blow = true; } }
    // Rows are sorted by damage: a value out of order is a misread (175 read as 17).
    // Keep the longest non-increasing run of damages (at most 6 rows, so O(n^2) is trivial).
    // Ties keep the larger damages: a number still counting up reads short ("23" for 233).
    let n = out.len();
    let (mut len, mut sum, mut prev) = (vec![1usize; n], out.iter().map(|r| r.damage).collect::<Vec<u32>>(), vec![usize::MAX; n]);
    for i in 0..n { for j in 0..i {
        let cand = (len[j] + 1, sum[j] + out[i].damage);
        if out[j].damage >= out[i].damage && cand > (len[i], sum[i]) { len[i] = cand.0; sum[i] = cand.1; prev[i] = j; }
    } }
    let mut keep = vec![false; n];
    let mut i = (0..n).max_by_key(|&i| (len[i], sum[i])).unwrap_or(usize::MAX);
    while i != usize::MAX { keep[i] = true; i = prev[i]; }
    let out: Vec<PanelRow> = out.into_iter().zip(keep).filter(|(_, k)| *k).map(|(r, _)| r).collect();
    let total = parse_total(line(ROW_LINES * rows.len())).filter(|t| out.iter().all(|r| r.damage <= *t));
    PanelRead { rows: out, total, located: rows.len(), tags: tagged_rows.len() }
}

/// The collapsed damage panel: a dark square with your hero's portrait at the right edge of the
/// death screen, where the panel's header (and its X close button) sits when open (canonical
/// 1600x900; rec16 27:49-28:44 and the 36:44 revive, rec13 19:33, rec17 26:12).
pub const PANEL_TAB: Bounds = Bounds { x: 1543, y: 208, w: 56, h: 59 };
/// Whether the collapsed-panel tab is drawn: its outer ring is near-black (>= 85 %) and its
/// centre holds a portrait (coloured or bright pixels). The open panel has the white X there
/// (no colour, ~8 % bright); gameplay and the shop have no dark square there with a portrait.
pub fn panel_tab(canon: &DynamicImage) -> bool {
    let a = PANEL_TAB;
    if canon.width() < a.x + a.w || canon.height() < a.y + a.h { return false; }
    let rgb = canon.crop_imm(a.x, a.y, a.w, a.h).to_rgb8();
    let (mut ring, mut dark, mut centre, mut col, mut bright) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for (x, y, p) in rgb.enumerate_pixels() {
        let (_, s, v) = hsv(p[0], p[1], p[2]);
        if x >= 12 && x < a.w - 12 && y >= 10 && y < a.h - 10 { centre += 1; col += u32::from(s > 0.3 && v > 0.3); bright += u32::from(v > 0.6); }
        else { ring += 1; dark += u32::from(v < 0.2); }
    }
    let share = |n: u32, d: u32| n as f32 / d.max(1) as f32;
    share(dark, ring) >= 0.85 && (share(col, centre) >= 0.05 || share(bright, centre) >= 0.15)
}

/// What the death screen shows of the damage panel (snapshot `death_panel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelState {
    /// Rows are on screen but no read has been confirmed yet (the panel fills in after a death).
    Reading,
    /// A complete read seen twice: rows, killing blow and total are reported.
    Open,
    /// Rows are on screen but no complete read agreed for [`PANEL_UNREADABLE_MS`]: disregarded.
    Unreadable,
    /// Collapsed to the portrait tab: nothing to read.
    Closed,
}
impl PanelState {
    pub fn as_str(self) -> &'static str { match self { Self::Reading => "reading", Self::Open => "open", Self::Unreadable => "unreadable", Self::Closed => "closed" } }
}
/// The collapsed tab counts as closed only this long into the death (the panel opens by itself).
pub const PANEL_TAB_GRACE_MS: u64 = 6_000;
/// Rows on screen this long (and over 3+ reads) without a confirmed read: the panel is unreadable.
pub const PANEL_UNREADABLE_MS: u64 = 8_000;
/// A read is confirmed by agreeing reads at least this far apart.
pub const PANEL_SETTLE_MS: u64 = 4_000;
/// Re-read cadence of the panel while unconfirmed, and once confirmed (it can still change: it
/// fills in after the death, rec5 2,248 -> 4,451, and rec16 37:02 replaced a two-death panel).
pub const PANEL_READ_MS: u64 = 1_000;
pub const PANEL_RECHECK_MS: u64 = 4_000;

/// Panel reads of one death. Rows are reported only from a complete read ([`PanelRead::complete`])
/// that a second read agrees with (same rows, damages, tag and total): the numbers count up for a
/// moment after the death, and a misread row is never reported. A later confirmed read replaces
/// it (the panel filled in or changed). Nothing carries over to another death.
#[derive(Default, Debug, Clone)]
pub struct Panel {
    pub rows: Vec<PanelRow>,
    pub total: Option<u32>,
    /// A read was confirmed (rows / total are reportable).
    pub confirmed: bool,
    /// The last complete read, how many reads in a row agreed with it, and when the first of
    /// them was made.
    pending: Option<(PanelRead, u8, u64)>,
    /// Reads with rows on screen, and the first one's time.
    reads: u32, first_read: Option<u64>,
    /// The collapsed tab was seen with no rows on screen; the first death-screen frame of this death.
    closed: bool, since: Option<u64>,
    /// Hit-count reads per source (row index, source) -> (value, agreeing reads). The small
    /// "x15" badge misreads ("x5"), so a count is reported only once two reads agree.
    hits: Vec<((usize, String), (u32, u8))>,
}
impl Panel {
    /// Merge one read. Returns true when it changed the reported rows.
    pub fn update(&mut self, mut read: PanelRead, now: u64) -> bool {
        if read.located == 0 { return false; }
        self.reads += 1; self.first_read.get_or_insert(now); self.closed = false;
        // Key: (n-th row with this source, source); a source can appear twice ("Gun Damage").
        let keys = |rows: &[PanelRow]| -> Vec<(usize, String)> { rows.iter().enumerate().map(|(i, r)| (rows[..i].iter().filter(|o| o.source == r.source).count(), r.source.clone())).collect() };
        for (key, r) in keys(&read.rows).into_iter().zip(read.rows.iter_mut()) {
            let Some(h) = r.hits.take() else { continue };
            match self.hits.iter_mut().find(|(k, _)| *k == key) {
                Some((_, (v, n))) if *v == h => *n = n.saturating_add(1),
                Some((_, slot)) => *slot = (h, 1),
                None => self.hits.push((key, (h, 1))),
            }
        }
        let mut changed = false;
        if read.complete() {
            let (read, n, t0) = match &self.pending { Some((p, n, t0)) if p.agrees(&read) => (p.merged(&read), n + 1, *t0), _ => (read, 1, now) };
            let current = PanelRead { rows: self.rows.clone(), total: self.total, ..Default::default() };
            // Agreeing reads over PANEL_SETTLE_MS: the numbers have stopped counting up (rec16
            // 24:03 showed 3,468 for 3 s, then 6,295).
            if n >= 2 && now >= t0 + PANEL_SETTLE_MS && !(self.confirmed && current.same(&read)) {
                self.rows = read.rows.clone(); self.total = read.total; self.confirmed = true; changed = true;
            }
            self.pending = Some((read, n.min(9), t0));
        }
        let ks = keys(&self.rows);
        for (key, r) in ks.into_iter().zip(self.rows.iter_mut()) {
            r.hits = self.hits.iter().find(|(k, (_, n))| *k == key && *n >= 2).map(|(_, (v, _))| *v);
        }
        changed
    }
    /// A death-screen frame without rows on screen showing the collapsed tab.
    /// The tab counts once the death is [`PANEL_TAB_GRACE_MS`] old: the panel opens by itself a
    /// few seconds into a death (rec8 20:40 / 49:42: tab on the first frame, rows 5 s later).
    pub fn saw_tab(&mut self, now: u64) {
        self.seen(now);
        if !self.confirmed && self.since.is_some_and(|t| now >= t + PANEL_TAB_GRACE_MS) { self.closed = true; }
    }
    /// A death-screen frame of this death (the tab grace period counts from the first).
    pub fn seen(&mut self, now: u64) { self.since.get_or_insert(now); }
    /// What to report on this frame (None: nothing seen of the panel in this death).
    pub fn state(&self, now: u64) -> Option<PanelState> {
        if self.confirmed { Some(PanelState::Open) }
        else if self.closed { Some(PanelState::Closed) }
        else if self.reads >= 3 && self.first_read.is_some_and(|t| now >= t + PANEL_UNREADABLE_MS) { Some(PanelState::Unreadable) }
        else if self.reads > 0 { Some(PanelState::Reading) }
        else { None }
    }
    /// Re-read interval for this panel.
    /// Every second until confirmed, and again whenever the last read differs from the
    /// confirmed panel (it is changing: confirm the new state quickly).
    pub fn cadence_ms(&self) -> u64 {
        let settled = self.confirmed && self.pending.as_ref().is_some_and(|(p, n, _)| *n >= 2 && p.total == self.total && p.rows.len() == self.rows.len());
        if settled { PANEL_RECHECK_MS } else { PANEL_READ_MS }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn killing_blow_tag_in_the_hits_box() {
        assert!(mentions_killing_blow("Essence Bomb ", "Ifling Blow"), "rec13 6:07");
        assert!(mentions_killing_blow("Melee killing B", "low+5% "));
        for h in ["x18 +7%", "+23% ", "x", "S", "", "x+5%"] { assert!(!mentions_killing_blow("Rake", h), "{h}"); }
    }
    #[test]
    fn revive_watch_separates_revives_from_real_deaths() {
        // rec8 39:17: grey "Respawn in: 1" 6 s after the last live frame, live again at 39:22.
        let mut w = ReviveWatch::default();
        assert!(w.dead(true, Some(6000), true, Some(1), 0));
        assert!(!w.alive(5000), "already flagged on the death screen");
        // Countdown unread: the quick return to the live HUD confirms it.
        assert!(!w.dead(true, Some(6000), true, None, 10_000));
        assert!(w.alive(15_000));
        // Real deaths: grey first frame with the full countdown (rec8 17:00: 43) ...
        assert!(!w.dead(true, Some(2000), true, Some(43), 20_000)); assert!(!w.dead(false, None, false, Some(38), 25_000)); assert!(!w.alive(60_000));
        // ... a coloured red-tinted screen (rec13 6:07: 8), or a return too late to be a revive.
        assert!(!w.dead(true, Some(2000), false, Some(8), 70_000)); assert!(!w.alive(80_000));
        assert!(!w.dead(true, Some(2000), true, None, 90_000)); assert!(!w.alive(100_000));
        // A grey screen long after the last live frame (recording gap), or the shop opened.
        assert!(!w.dead(true, Some(40_000), true, Some(1), 110_000)); assert!(!w.alive(112_000));
        assert!(!w.dead(true, Some(2000), true, None, 120_000)); w.interrupt(); assert!(!w.alive(124_000));
    }
    #[test]
    fn countdown_rejects_dropped_and_doubled_digits() {
        let mut c = Countdown::default();
        assert_eq!(c.observe(Some(46), 0).0, None, "first read needs a second");
        assert_eq!(c.observe(Some(37), 9_000), (Some(37), false));
        assert_eq!(c.observe(Some(35), 11_000).0, Some(35));
        assert_eq!(c.observe(Some(0), 14_000).0, Some(32), "30 read as 0 -> countdown");
        assert_eq!(c.observe(Some(6), 18_000).0, Some(28), "26 read as 6 -> countdown");
        assert_eq!(c.observe(Some(26), 20_000).0, Some(26));
        assert_eq!(c.observe(Some(16), 30_000).0, Some(16));
        assert_eq!(c.observe(Some(77), 39_000).0, Some(7), "7 read as 77");
        assert_eq!(c.observe(None, 40_000).0, Some(6), "extrapolated between reads");
        assert_eq!(c.observe(None, 60_000).0, None, "no read for >10 s: unknown");
    }
    #[test]
    fn a_new_death_restarts_after_two_agreeing_reads() {
        let mut c = Countdown::default();
        c.observe(Some(20), 0); c.observe(Some(18), 2000);
        assert_eq!(c.observe(Some(76), 4000).0, Some(16), "one jump up is not trusted yet");
        assert_eq!(c.observe(Some(74), 6000), (Some(74), true));
    }
    #[test]
    fn respawn_digits_must_match_glyphs() {
        assert_eq!(read_count("30", 2), Some(30));
        assert_eq!(read_count("0", 2), None);
        assert_eq!(read_count("77", 1), None);
    }
    #[test]
    fn panel_names_are_normalised() {
        for (raw, want) in [("Aura of suffering", "Aura of Suffering"), ("Stalkers Mark Y7", "Stalker's Mark"), ("Stalkers MarkX7", "Stalker's Mark"),
            ("Mystic ShoKilling Blow", "Mystic Shot"), ("Gun DaKilling Blow", "Gun Damage"), ("Knocked off Zipline", "Knocked off Zipline"),
            ("Still Life", "Still Life"), ("Sinners Sacrifice", "Sinner's Sacrifice"), ("Melee kiling Bow 3", "Melee"), ("Lurking Terrors", "Lurking Terrors"),
            ("Mystic shotiling B", "Mystic Shot"), ("Pulse Grenilling B", "Pulse Grenade"), ("SSpirit Burn killing", "Spirit Burn"), ("Aura of suffering I", "Aura of Suffering"),
            ("'Stalker's Mark ", "Stalker's Mark"), ("Knocked off Ziplin", "Knocked off Zipline"), ("Lurking Terrr", "Lurking Terrors")] {
            assert_eq!(normalise_source(raw).as_deref(), Some(want), "{raw}");
        }
        assert_eq!(normalise_source("qwzx"), None);
    }
    #[test]
    fn panel_parsers() {
        assert_eq!(parse_hits("x18 +7%"), Some(18));
        assert_eq!(parse_hits("+7%"), None);
        assert_eq!(parse_hits("[X18 +7%"), Some(18));
        assert_eq!(parse_hits("x8+6%"), None, "glued: ambiguous");
        assert_eq!(parse_damage("1,310", 3, 5), Some(1310));
        assert_eq!(parse_damage("17!", 2, 3), Some(17));
        assert_eq!(parse_damage("90", 3, 4), None, "a dropped digit");
        assert_eq!(parse_damage("1,04", 3, 5), None, "bad grouping");
        assert_eq!(parse_total("Total Damage: 2,248"), Some(2248));
        assert_eq!(parse_total("Total Damage: 4.451"), Some(4451));
        assert_eq!(parse_total("otal Damge: 4,451"), Some(4451));
        assert_eq!(parse_total("ota amg: 1,04 |"), None, "cut digits");
    }
    #[test]
    fn tag_hidden_names_are_matched_or_dropped_never_guessed() {
        // OCR of rows whose name the Killing Blow tag covers (rec16, rec8, rec13 1080p).
        for (raw, want) in [("Lurkinling:Blow ", Some("Lurking Terrors")), ("Leech Therling B", Some("Leech Therapy")),
            ("Gun Driling Blow |", Some("Gun Damage")), ("Melee Chailling Bl", Some("Melee Charge")),
            // A clip shared by several names is never guessed without the enemy team: "Spirit R…"
            // (Spirit Rend / Resilience), "Escal…" (Exposure / Resilience), "Bullet", "Arctic",
            // "Stalk…", "Borrow…".
            ("SSpirit iling Blow", None), ("Escalxling BlOSu", None), ("Bullet Rling Blow |", None), ("Arctic ling Blow ", None),
            ("stalkgling Blow ", None), ("Borrowiiling:Blow", None), ("BorrowngBlow", None),
            // Real names ending in "-ing" before a B-word are not the tag.
            ("Entangling Bola", Some("Entangling Bola")), ("Wrecking Ball", Some("Wrecking Ball"))] {
            assert_eq!(normalise_source(raw).as_deref(), want, "{raw}");
        }
        // With the enemy team: an ability of a hero not in the match is impossible, a lone enemy
        // ability beats shop items; items alone stay ambiguous.
        let team = |h: &[&str]| h.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let rec16 = team(&["hero_priest", "hero_gigawatt", "hero_ratking", "hero_nurse", "hero_hornet", "hero_atlas"]);
        let rec8 = team(&["hero_kelvin", "hero_yamato", "hero_synth", "hero_haze", "hero_chrono", "hero_frank"]);
        assert_eq!(normalise_source_with("stalkgling Blow ", &rec16).as_deref(), Some("Stalker"), "no Drifter in rec16");
        assert_eq!(normalise_source_with("Bullet Rling Blow |", &rec8).as_deref(), Some("Bullet Dance"));
        assert_eq!(normalise_source_with("Arctic ling Blow ", &rec8).as_deref(), Some("Arctic Beam"));
        assert_eq!(normalise_source_with("Bullet Rling Blow |", &rec16).as_deref(), None, "no Haze: items only");
        assert_eq!(normalise_source_with("SSpirit iling Blow", &rec8).as_deref(), None);
    }
    fn row(s: &str, d: u32, i: usize) -> PanelRow { PanelRow { source: s.into(), hits: None, damage: d, killing_blow: false, index: i } }
    fn read(rows: Vec<PanelRow>, total: Option<u32>, located: usize) -> PanelRead { PanelRead { rows, total, located, tags: 0 } }
    #[test]
    fn a_panel_is_reported_after_two_agreeing_complete_reads() {
        let full = || vec![row("Aura of Suffering", 789, 0), row("Still Life", 613, 1), row("Mystic Shot", 175, 2)];
        let mut p = Panel::default();
        // Filling in: the numbers count up, each read differs.
        assert!(!p.update(read(vec![row("Still Life", 613, 0)], Some(2248), 3), 0));
        assert!(!p.update(read(full(), Some(4451), 3), 1000));
        assert_eq!(p.state(1000), Some(PanelState::Reading));
        assert!(p.rows.is_empty(), "one read is not enough");
        assert!(!p.update(read(full(), Some(4451), 3), 3000), "agreeing reads 2 s apart: may still be counting up");
        assert!(p.update(read(full(), Some(4451), 3), 5000));
        assert_eq!(p.state(5000), Some(PanelState::Open));
        assert_eq!((p.rows[0].source.as_str(), p.total), ("Aura of Suffering", Some(4451)));
        // A later panel (filled in further, or replaced) needs its own agreeing reads.
        let more = || vec![row("Aura of Suffering", 1200, 0), row("Still Life", 613, 1), row("Mystic Shot", 175, 2)];
        assert!(!p.update(read(more(), Some(5000), 3), 8000));
        assert!(p.update(read(more(), Some(5000), 3), 12_000));
        assert_eq!(p.rows[0].damage, 1200);
    }
    #[test]
    fn partial_or_implausible_reads_are_disregarded() {
        // The top row unread (its name under the tag), too many rows missing, no total, or rows
        // adding up to more than the total: never complete, so nothing is reported.
        assert!(!read(vec![row("Escalating Exposure", 761, 1), row("Spirit Burn", 647, 2)], Some(5352), 6).complete(), "top row missing");
        assert!(!read(vec![row("Melee", 537, 0), row("Gun Damage", 444, 2), row("Siphon Life", 417, 3)], Some(2929), 6).complete(), "3 of 6 rows missing");
        assert!(!read(vec![row("Melee", 537, 0), row("Royal Pestments", 451, 1)], None, 2).complete(), "no total");
        assert!(!read(vec![row("Melee", 2537, 0), row("Royal Pestments", 451, 1)], Some(2929), 2).complete(), "rows exceed the total");
        assert!(read(vec![row("Melee", 537, 0), row("Royal Pestments", 451, 1), row("Gun Damage", 444, 2), row("Siphon Life", 417, 3), row("Lucky Shot", 225, 4)], Some(2929), 6).complete());
        let mut p = Panel::default();
        let partial = || read(vec![row("Escalating Exposure", 761, 1)], Some(5352), 6);
        for t in [0, 2000, 4000, 8000] { p.update(partial(), t); }
        assert!(p.rows.is_empty());
        assert_eq!(p.state(8000), Some(PanelState::Unreadable));
    }
    #[test]
    fn the_collapsed_tab_is_closed_until_rows_appear() {
        let mut p = Panel::default();
        assert_eq!(p.state(0), None);
        p.seen(0); p.saw_tab(2000);
        assert_eq!(p.state(2000), None, "the panel is still opening");
        p.saw_tab(6000);
        assert_eq!(p.state(6000), Some(PanelState::Closed));
        // Opened by the player: rows on screen.
        p.update(read(vec![row("Melee", 537, 0)], Some(2929), 1), 9000);
        assert_eq!(p.state(9000), Some(PanelState::Reading));
    }
    #[test]
    fn hit_counts_need_two_agreeing_reads() {
        let r = |h| read(vec![PanelRow { source: "Aura of Suffering".into(), hits: h, damage: 426, killing_blow: false, index: 0 }], Some(2248), 1);
        let mut p = Panel::default();
        p.update(r(Some(5)), 0);
        p.update(r(Some(15)), 4000);
        assert_eq!(p.rows[0].hits, None, "x15 read once as x5");
        p.update(r(Some(15)), 8000);
        assert_eq!(p.rows[0].hits, Some(15));
    }
}
