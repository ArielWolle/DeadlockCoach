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

/// Rows present on this frame: the damage cell holds 1-5 white digits (largest damage first,
/// so rows are contiguous from the top).
pub fn row_digits(panel: &DynamicImage) -> Vec<(Bounds, (usize, usize))> {
    let k = k_of(panel);
    let (hmin, hmax) = ((8.0 * k).floor() as u32, (18.0 * k).ceil() as u32);
    let mut out = vec![];
    for i in 0..6 {
        let a = damage_box(i, k);
        let g: Vec<Blob> = blobs(panel, a, white).into_iter().filter(|b| (hmin..=hmax).contains(&b.h()) && b.area as f32 >= 8.0 * k * k && b.min_y > a.y && b.max_y + 1 < a.y + a.h).collect();
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

/// The red italic "Killing Blow" tag drawn over a row's name bar (panel image).
/// Normalised to 1920 scale (pixel count / k^2).
pub fn killing_blow_px(panel: &DynamicImage, i: usize) -> u32 {
    let k = k_of(panel);
    let a = scaled(k, 262.0, row_c(i) - 14.0, 86.0, 28.0);
    let rgb = panel.crop_imm(a.x, a.y, a.w, a.h).to_rgb8();
    (rgb.pixels().filter(|p| { let (h, s, v) = hsv(p[0], p[1], p[2]); (h < 12.0 || h > 350.0) && s > 0.7 && v > 0.75 }).count() as f32 / (k * k)) as u32
}

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
pub fn match_name_scored<'a>(text: &str, names: &'a [String]) -> Option<(&'a str, f32)> {
    let words: Vec<String> = key(text).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
    let mut best: Option<(f32, usize, &str)> = None;
    // A leading 1-2 character word may be an icon read as text ("S Spirit Burn").
    let starts = if words.first().is_some_and(|w| w.chars().count() <= 2) { 0..=1 } else { 0..=0 };
    for s in starts {
    for k in (s + 1..=words.len()).rev() {
        let q = words[s..k].join(" ");
        if q.len() < 3 { continue; }
        for n in names {
            let nk = key(n);
            let full = levenshtein(&q, &nk) as f32 / nk.len().max(q.len()) as f32;
            // A clipped name: the query is a prefix of the name (needs 5+ characters).
            let prefix = if q.len() >= 5 && q.len() < nk.len() { levenshtein(&q, &nk[..nk.char_indices().nth(q.chars().count()).map_or(nk.len(), |(i, _)| i)]) as f32 / q.len() as f32 + 0.1 } else { 1.0 };
            let score = full.min(prefix);
            // Dropped words cost a little, so "Stalkers Mark" beats a one-word name and
            // "T2 Walker" beats "Walker".
            let cost = score + 0.03 * (words.len() - k + s) as f32;
            if score <= 0.25 && best.is_none_or(|(c, _, _)| cost < c) { best = Some((cost, k, n.as_str())); }
        }
    }
    }
    best.map(|(c, _, n)| (n, c))
}

/// First line without the "Killing Blow" tag and hit counts. The tag overlaps the name, so OCR
/// glues it on ("Mystic ShoKilling Blow", "Meleegilling", "Melee kiling Bow 3"); the stem before
/// it is kept.
pub fn strip_tag(raw: &str) -> String {
    let mut words: Vec<String> = vec![];
    for w in raw.lines().next().unwrap_or("").split_whitespace() {
        let w: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect();
        let l = w.to_lowercase();
        if l.is_empty() { continue; }
        let count = l.trim_start_matches(['x', '×']);
        if !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()) { break; }
        if matches!(l.as_str(), "blow" | "bow" | "blo" | "biow") { break; }
        if let Some(i) = ["killing", "kiling", "gilling", "kiiling", "kilin", "illing", "iling", "iing"].iter().filter_map(|k| l.find(k)).min() {
            let stem = w[..i].trim_end_matches(['k', 'g', 'i', 'l', 'K', 'G', 'I', 'L']);
            if !stem.is_empty() { words.push(stem.to_string()); }
            break;
        }
        words.push(w);
    }
    words.join(" ")
}

/// Strip a glued "Killing Blow" tag / counts, then match against the dictionary.
pub fn normalise_source(raw: &str) -> Option<String> {
    match_name(&strip_tag(raw), source_names()).map(str::to_string)
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

pub fn parse_panel(panel: &DynamicImage, rows: &[(Bounds, (usize, usize))], lines: &[String]) -> (Vec<PanelRow>, Option<u32>) {
    let line = |i: usize| lines.get(i).map(String::as_str).unwrap_or("");
    let mut out = vec![];
    let mut kb: Vec<(usize, u32)> = vec![];
    for (k, (_, (lo, hi))) in rows.iter().enumerate() {
        let (name, wide, hits, dmg) = (line(ROW_LINES * k), line(ROW_LINES * k + 1), line(ROW_LINES * k + 2), line(ROW_LINES * k + 3));
        let px = killing_blow_px(panel, k);
        let tagged = px >= 15 || mentions_killing_blow(name, hits) || mentions_killing_blow(wide, "");
        let Some(damage) = parse_damage(dmg, *lo, *hi) else { continue };
        let best = [name, wide].into_iter().filter_map(|s| match_name_scored(&strip_tag(s), source_names())).min_by(|a, b| a.1.total_cmp(&b.1));
        let Some(source) = best.map(|(s, _)| s.to_string()) else { continue };
        if tagged { kb.push((out.len(), px)); }
        out.push(PanelRow { source, hits: if mentions_killing_blow("", hits) { None } else { parse_hits(hits) }, damage, killing_blow: false, index: k });
    }
    // One killing blow: the strongest tagged row.
    if let Some((i, _)) = kb.iter().max_by_key(|(_, px)| *px) { out[*i].killing_blow = true; }
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
    (out, total)
}

/// How long Total Damage must stay unchanged before the panel counts as complete (rec5: the
/// total went 2,248 -> 4,451 over ~10 s after the death, with equal totals 2 s apart).
pub const PANEL_SETTLE_MS: u64 = 8_000;

/// Accumulates panel reads during one death: re-read until Total Damage stops changing.
#[derive(Default, Debug, Clone)]
pub struct Panel {
    pub rows: Vec<PanelRow>,
    pub total: Option<u32>,
    /// When the current total was first read.
    total_since: Option<u64>,
    /// The total has not changed for [`PANEL_SETTLE_MS`] with full reads: the panel is complete.
    pub stable: bool,
    /// Hit-count reads per source (row index, source) -> (value, agreeing reads). The small
    /// "x15" badge misreads ("x5"), so a count is reported only once two reads agree.
    hits: Vec<((usize, String), (u32, u8))>,
}
impl Panel {
    /// Merge one read. Returns true when it changed the rows.
    pub fn update(&mut self, mut rows: Vec<PanelRow>, total: Option<u32>, now: u64) -> bool {
        if rows.is_empty() { return false; }
        // Key: (n-th row with this source, source); a source can appear twice ("Gun Damage").
        let keys = |rows: &[PanelRow]| -> Vec<(usize, String)> { rows.iter().enumerate().map(|(i, r)| (rows[..i].iter().filter(|o| o.source == r.source).count(), r.source.clone())).collect() };
        for (key, r) in keys(&rows).into_iter().zip(rows.iter_mut()) {
            let Some(h) = r.hits.take() else { continue };
            match self.hits.iter_mut().find(|(k, _)| *k == key) {
                Some((_, (v, n))) if *v == h => *n = n.saturating_add(1),
                Some((_, slot)) => *slot = (h, 1),
                None => self.hits.push((key, (h, 1))),
            }
        }
        if total.is_some() && total == self.total && rows.len() >= self.rows.len() && self.total_since.is_some_and(|t| now >= t + PANEL_SETTLE_MS) { self.stable = true; }
        if total.is_some() && total != self.total { self.total_since = Some(now); }
        // A later, larger total (the panel kept filling in) replaces the rows; a read with fewer
        // recognised rows never replaces a fuller one with the same total.
        // Same total: the fuller read wins (more rows, then more damage: numbers count up).
        let fuller = (rows.len(), rows.iter().map(|r| r.damage).sum::<u32>()) > (self.rows.len(), self.rows.iter().map(|r| r.damage).sum::<u32>());
        let better = match (total, self.total) { (Some(t), Some(o)) => t > o || (t == o && fuller), (Some(_), None) => true, (None, None) => fuller, (None, Some(_)) => false };
        if better { self.rows = rows; self.total = total.or(self.total); }
        let ks = keys(&self.rows);
        for (key, r) in ks.into_iter().zip(self.rows.iter_mut()) {
            r.hits = self.hits.iter().find(|(k, (_, n))| *k == key && *n >= 2).map(|(_, (v, _))| *v);
        }
        better
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
    fn panel_waits_for_the_total_to_settle() {
        let row = |s: &str, d| PanelRow { source: s.into(), hits: None, damage: d, killing_blow: false, index: 0 };
        let mut p = Panel::default();
        p.update(vec![row("Still Life", 613)], Some(2248), 0);
        p.update(vec![row("Still Life", 613)], Some(2248), 2000);
        assert!(!p.stable, "equal totals 2 s apart: the panel may still be filling in");
        p.update(vec![row("Aura of Suffering", 789), row("Still Life", 613)], Some(4451), 6000);
        assert_eq!(p.rows[0].source, "Aura of Suffering");
        p.update(vec![row("Aura of Suffering", 789)], Some(4451), 8000);
        assert_eq!(p.rows.len(), 2, "a partial read does not replace a fuller one");
        p.update(vec![row("Aura of Suffering", 789), row("Still Life", 613)], Some(4451), 14_000);
        assert!(p.stable);
        assert_eq!(p.rows[0].hits, None);
    }
    #[test]
    fn hit_counts_need_two_agreeing_reads() {
        let row = |h| PanelRow { source: "Aura of Suffering".into(), hits: h, damage: 426, killing_blow: false, index: 0 };
        let mut p = Panel::default();
        p.update(vec![row(Some(5))], Some(2248), 0);
        assert_eq!(p.rows[0].hits, None, "x15 read once as x5");
        p.update(vec![row(Some(15))], Some(2248), 2000);
        assert_eq!(p.rows[0].hits, None);
        p.update(vec![row(Some(15))], Some(2248), 4000);
        assert_eq!(p.rows[0].hits, Some(15));
    }
}
