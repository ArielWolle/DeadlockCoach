//! Crowd-control text under the crosshair ("STUNNED", "SILENCED", "OUT OF STAMINA" and the
//! smaller cause line such as "Knocked off Zipline") and named debuff badges on the left
//! ("AURA OF SUFFERING"). Pixel gating finds text-like glyph rows first; only those are read
//! (batched line recognition), and only text that matches a known status or dictionary name is
//! reported, so stray world text never becomes a status. Canonical 1600x900 coordinates.
use crate::hudtext::{blobs, crop_padded};
use crate::minimap::{hsv, Blob};
use coach_perception::Bounds;
use image::DynamicImage;

/// Large status word, left-aligned at x ~737 just below the crosshair.
pub const STATUS_AREA: Bounds = Bounds { x: 700, y: 590, w: 300, h: 48 };
/// Smaller cause line under it.
pub const CAUSE_AREA: Bounds = Bounds { x: 700, y: 632, w: 300, h: 26 };
/// "OUT OF STAMINA" banner (white on red) low in the centre, above the ability bar.
pub const STAMINA_AREA: Bounds = Bounds { x: 700, y: 722, w: 300, h: 40 };
/// Debuff badges, left middle.
pub const BADGE_AREA: Bounds = Bounds { x: 0, y: 410, w: 220, h: 115 };

/// Stat-debuff badges (salmon, dark text such as "-0.8 m" / "-70 %"), right of the buff column.
pub const SLOW_AREA: Bounds = Bounds { x: 150, y: 440, w: 112, h: 150 };
/// "DELIVER THE URN" prompt under the crosshair while you carry the Soul Urn.
pub const URN_PROMPT_AREA: Bounds = Bounds { x: 708, y: 593, w: 200, h: 39 };
/// Objective health banner, top centre: the boss name ("HIDDEN KING") over a two-phase bar.
pub const BANNER_NAME_AREA: Bounds = Bounds { x: 690, y: 118, w: 220, h: 56 };
pub const BANNER_BAR_AREA: Bounds = Bounds { x: 530, y: 158, w: 540, h: 28 };

/// Known centre texts -> tokens. "AFFLICTED" (Pocket's Affliction, rec8 39:11, 41:16, 44:08) has
/// the cause line "Affliction" under it.
const WORDS: [(&str, &str); 9] = [("STUNNED", "stunned"), ("SILENCED", "silenced"), ("ABILITIES DISABLED", "silenced"),
    ("DISARMED", "disarmed"), ("OUT OF STAMINA", "out_of_stamina"), ("KNOCKED OFF ZIPLINE", "knocked_off_zipline"),
    ("AFFLICTED", "afflicted"), ("AFFLICTION", "afflicted"), ("FROZEN", "frozen")];

/// Which crop a recognised line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Word, Cause, Badge, Slow }

fn bright(r: u8, g: u8, b: u8) -> bool { let (h, s, v) = hsv(r, g, b); v > 0.78 && (s < 0.3 || (35.0..65.0).contains(&h) && s > 0.4) }
fn grey(r: u8, g: u8, b: u8) -> bool { let (_, s, v) = hsv(r, g, b); v > 0.55 && s < 0.25 }

/// Glyph rows: blobs of plausible glyph height on one baseline, at least `min` of them, spanning
/// at most `max_gap` between neighbours. Returns the row box.
fn text_row(g: &[Blob], hmin: u32, hmax: u32, min: usize, max_gap: u32) -> Option<Bounds> {
    let mut c: Vec<&Blob> = g.iter().filter(|b| (hmin..=hmax).contains(&b.h()) && b.w() <= b.h() * 2 && b.area >= 6).collect();
    c.sort_by_key(|b| b.min_x);
    // Longest run of neighbours overlapping the run's vertical band (descenders allowed).
    let mut best: Option<(usize, usize, usize)> = None;
    let mut i = 0;
    while i < c.len() {
        let mut j = i;
        let (mut top, mut bottom) = (c[i].min_y, c[i].max_y);
        while j + 1 < c.len() && c[j + 1].min_x <= c[j].max_x + max_gap && c[j + 1].min_y + 2 <= bottom && c[j + 1].max_y >= top + 2 {
            j += 1; top = top.min(c[j].min_y); bottom = bottom.max(c[j].max_y);
            if bottom - top > hmax + hmax / 2 { break; }
        }
        if best.is_none_or(|(_, _, n)| j + 1 - i > n) { best = Some((i, j, j + 1 - i)); }
        i = j + 1;
    }
    let (a, b, n) = best?;
    if n < min { return None; }
    let r = &c[a..=b];
    let (x0, x1) = (r.iter().map(|b| b.min_x).min()?, r.iter().map(|b| b.max_x).max()?);
    let (y0, y1) = (r.iter().map(|b| b.min_y).min()?, r.iter().map(|b| b.max_y).max()?);
    Some(Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 })
}

/// Red debuff badges: filled red rectangles with white text. Returns text-line boxes inside each
/// badge (a badge name can wrap to two lines). `img` is the badge area at 1920 scale.
fn badge_lines(img: &DynamicImage) -> Vec<Vec<Bounds>> {
    let red = |r: u8, g: u8, b: u8| { let (h, s, v) = hsv(r, g, b); (h < 14.0 || h > 348.0) && s > 0.55 && v > 0.45 };
    let all = Bounds { x: 0, y: 0, w: img.width(), h: img.height() };
    let mut out = vec![];
    // The badge is a red outline over a translucent (sometimes solid) red fill: the bounds of
    // the red blob are the badge whatever the fill.
    for b in blobs(img, all, red).into_iter().filter(|b| b.w() >= 70 && (16..=48).contains(&b.h()) && b.area >= 120) {
        // Text starts right of the square icon at the badge's left end.
        let inner = Bounds { x: b.min_x + b.h(), y: b.min_y, w: b.w().saturating_sub(b.h()), h: b.h() };
        let g: Vec<Blob> = blobs(img, inner, |r, g, b| { let (_, s, v) = hsv(r, g, b); v > 0.8 && s < 0.3 }).into_iter().filter(|g| g.h() >= 5 && g.h() <= 17 && g.area >= 5).collect();
        if g.len() < 4 { continue; }
        // Split by vertical centre into lines.
        let mut rows: Vec<Vec<&Blob>> = vec![];
        let mut sorted: Vec<&Blob> = g.iter().collect(); sorted.sort_by(|a, b| a.cy.total_cmp(&b.cy));
        for gl in sorted { match rows.last_mut() { Some(r) if (gl.cy - r[0].cy).abs() < 5.0 => r.push(gl), _ => rows.push(vec![gl]) } }
        let lines: Vec<Bounds> = rows.into_iter().filter(|r| r.len() >= 2).map(|r| {
            let (x0, x1) = (r.iter().map(|b| b.min_x).min().unwrap(), r.iter().map(|b| b.max_x).max().unwrap());
            let (y0, y1) = (r.iter().map(|b| b.min_y).min().unwrap(), r.iter().map(|b| b.max_y).max().unwrap());
            Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }
        }).collect();
        if !lines.is_empty() && lines.len() <= 2 { out.push(lines); }
    }
    out
}

/// Areas are rendered at 1920x1080 scale (from the source frame when available) before analysis.
fn at_1920(view: &crate::hudtext::View, a: Bounds) -> DynamicImage { view.region(a, a.w * 6 / 5, a.h * 6 / 5) }

/// Crops to recognise for this frame (empty when no text-like pixels: the common case).
pub fn crops(view: &crate::hudtext::View) -> Vec<(Kind, DynamicImage)> {
    if view.canon.width() != 1600 || view.canon.height() != 900 { return vec![]; }
    let mut out = vec![];
    let whole = |i: &DynamicImage| Bounds { x: 0, y: 0, w: i.width(), h: i.height() };
    let word = at_1920(view, STATUS_AREA);
    if let Some(b) = text_row(&blobs(&word, whole(&word), bright), 20, 42, 5, 11) {
        out.push((Kind::Word, crop_padded(&word, b.x, b.y, b.x + b.w - 1, b.y + b.h - 1, 5)));
    }
    let cause = at_1920(view, CAUSE_AREA);
    if let Some(b) = text_row(&blobs(&cause, whole(&cause), grey), 8, 20, 8, 15) {
        out.push((Kind::Cause, crop_padded(&cause, b.x, b.y, b.x + b.w - 1, b.y + b.h - 1, 4)));
    }
    let stamina = at_1920(view, STAMINA_AREA);
    // Its pale-pink text on the red banner is less white than other HUD text.
    // (measured ~(220,110,109) on (116,10,10)).
    let pale = |r: u8, g: u8, b: u8| { let (_, s, v) = hsv(r, g, b); v > 0.75 && s < 0.62 };
    if let Some(b) = text_row(&blobs(&stamina, whole(&stamina), pale), 9, 24, 8, 14) {
        out.push((Kind::Word, crop_padded(&stamina, b.x, b.y, b.x + b.w - 1, b.y + b.h - 1, 4)));
    }
    let badges = at_1920(view, BADGE_AREA);
    for lines in badge_lines(&badges) {
        // A wrapped name is read line by line, then joined.
        for b in lines { out.push((Kind::Badge, crop_padded(&badges, b.x, b.y, b.x + b.w - 1, b.y + b.h - 1, 3))); }
        out.push((Kind::Badge, DynamicImage::new_rgb8(1, 1))); // badge separator
    }
    let slows = at_1920(view, SLOW_AREA);
    for b in slow_badges(&slows) { out.push((Kind::Slow, crop_padded(&slows, b.x, b.y, b.x + b.w - 1, b.y + b.h - 1, 2))); }
    out
}

/// Salmon stat-debuff badges ("-0.8 m" movement, "-70 %" fire rate: Kelvin's Frost Grenade,
/// rec8 46:20-46:43): filled light-red boxes ~85x30 at 1920 scale. `img` is [`SLOW_AREA`] at 1920.
fn slow_badges(img: &DynamicImage) -> Vec<Bounds> {
    let salmon = |r: u8, g: u8, b: u8| { let (h, s, v) = hsv(r, g, b); (h < 25.0 || h > 345.0) && (0.3..0.68).contains(&s) && v > 0.7 };
    let all = Bounds { x: 0, y: 0, w: img.width(), h: img.height() };
    blobs(img, all, salmon).into_iter()
        .filter(|b| (60..=115).contains(&b.w()) && (20..=42).contains(&b.h()) && b.area as f32 >= 0.45 * (b.w() * b.h()) as f32)
        .map(|b| Bounds { x: b.min_x, y: b.min_y, w: b.w(), h: b.h() }).take(3).collect()
}

/// A movement-speed debuff badge reads "-0.8 m" (metres per second off your speed). The badge
/// icon reads as stray letters before the minus and the pale unit is often lost or misread
/// ("F-0.8 T", "Ex-0.8T%"): a negative decimal below 5 is a speed (fire-rate debuffs are
/// whole percentages: "-70 %"), as is any negative number followed by "m".
pub fn is_move_slow(text: &str) -> bool {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace(['–', '—', '~'], "-");
    let Some(at) = t.find('-') else { return false };
    let rest = &t[at + 1..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    let unit = &rest[digits.len()..];
    if !digits.starts_with(|c: char| c.is_ascii_digit()) || digits.len() > 4 { return false; }
    let decimal = digits.replace(',', ".").parse::<f32>().ok().filter(|_| digits.contains(['.', ',']));
    decimal.is_some_and(|v| v > 0.0 && v < 5.0) || unit.starts_with(['m', 'M'])
}

/// Pale-green "DELIVER THE URN" glyphs with their dark outline (rec8 236-247 scored 357-561 on
/// this measure, every other frame of rec8 at most 149). `img` is [`URN_PROMPT_AREA`] at 1920.
fn urn_prompt_score(img: &DynamicImage) -> (u32, u32) {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let green = |p: &image::Rgb<u8>| { let (_, s, v) = hsv(p[0], p[1], p[2]); p[1] >= p[0] && p[1] >= p[2] && s > 0.15 && s < 0.5 && v > 0.72 && p[1] as i32 - p[0] as i32 > 20 };
    let dark = |p: &image::Rgb<u8>| hsv(p[0], p[1], p[2]).2 < 0.3;
    let (mut score, mut rows) = (0, 0);
    for y in 0..h {
        let mut n = 0;
        for x in 0..w {
            if !green(rgb.get_pixel(x, y)) { continue; }
            n += 1;
            if (x.saturating_sub(2)..=(x + 2).min(w - 1)).any(|xx| xx != x && dark(rgb.get_pixel(xx, y))) { score += 1; }
        }
        if n > 4 { rows += 1; }
    }
    (score, rows)
}
/// The urn prompt line to recognise, when its pixels are present (rare: only while carrying).
pub fn urn_prompt(view: &crate::hudtext::View) -> Option<DynamicImage> {
    if view.canon.width() != 1600 || view.canon.height() != 900 { return None; }
    let img = at_1920(view, URN_PROMPT_AREA);
    let (score, rows) = urn_prompt_score(&img);
    (score >= 250 && (8..=30).contains(&rows)).then_some(img)
}
/// Crop of the urn prompt area (read when the minimap shows an urn).
pub fn urn_prompt_crop(view: &crate::hudtext::View) -> DynamicImage { at_1920(view, URN_PROMPT_AREA) }
/// "DELIVER THE URN" (one OCR slip allowed; the end can be covered by a world icon, rec8 247
/// "DELIVER THE [icon]RN": the pale-green prompt pixels plus "DELIVER" at this place suffice).
pub fn says_deliver_urn(text: &str) -> bool {
    // Words often run together ("DELIVERTHEURN", "DELIVEURN"): compare the word's start.
    letters(text).split(' ').any(|w| w.len() >= 6 && crate::death::levenshtein(&w.chars().take(7).collect::<String>(), "DELIVER") <= 1)
}

/// Objective banner bar: the row of the boss bar spanning the banner and its remaining
/// (not yet black) fraction. The fill is cream, grey (rec5/rec6 late game), red or pink
/// (rec6, recent damage) over the black depleted part. `img` is [`BANNER_BAR_AREA`] at 1920.
fn banner_bar(img: &DynamicImage) -> Option<f32> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    if w < 40 { return None; }
    let (x0, x1) = (w * 16 / 648, w * 632 / 648);
    // 1 = neutral fill (cream, grey: measured v ~0.30 on rec6 544), 3 = red, 2 = black ink (< 0.12).
    let class = |p: &image::Rgb<u8>| {
        let (hue, s, v) = hsv(p[0], p[1], p[2]);
        if v < 0.16 { 2 } else if v >= 0.24 && s < 0.35 { 1 } else if v >= 0.3 && s > 0.3 && (hue < 25.0 || hue > 340.0) { 3 } else { 0 }
    };
    // Per row: (neutral, red, bar pixels, first neutral x, first red x).
    let mut best: Option<(u32, u32, u32, u32, u32)> = None;
    // The bar rows (1920 scale y ~200-212); rows above and below are scene background.
    for y in (h * 10 / 34)..(h * 22 / 34).min(h) {
        let (mut fill, mut red, mut bar, mut fx, mut rx) = (0u32, 0u32, 0u32, u32::MAX, u32::MAX);
        for x in x0..x1 { match class(rgb.get_pixel(x, y)) { 1 => { fill += 1; bar += 1; fx = fx.min(x); } 3 => { red += 1; bar += 1; rx = rx.min(x); } 2 => bar += 1, _ => {} } }
        if bar as f32 >= 0.85 * (x1 - x0) as f32 && best.is_none_or(|(c, r, ..)| fill + red > c + r) { best = Some((fill, red, bar, fx, rx)); }
    }
    // The fill runs from the left; red right of a neutral fill is the trail of health just lost
    // (rec5 561, rec8 556), red left of it (or alone) is the fill itself (an enemy Patron, rec6
    // 576 / 582, whose lighter trail then follows it).
    best.map(|(c, r, b, fx, rx)| {
        let red_first = r > 0 && rx < fx;
        (if red_first { r } else { c }) as f32 / b.max(1) as f32
    })
}
/// Objective banner candidate: the name crop to recognise and the bar's remaining fraction,
/// when the bar spans the banner and the name area holds cream glyphs (cheap pixel gates).
pub fn objective_banner(view: &crate::hudtext::View) -> Option<(DynamicImage, f32)> {
    if view.canon.width() != 1600 || view.canon.height() != 900 { return None; }
    let health = banner_bar(&at_1920(view, BANNER_BAR_AREA))?;
    let name = at_1920(view, BANNER_NAME_AREA);
    let rgb = name.to_rgb8();
    let n = rgb.pixels().count().max(1) as f32;
    let cream = rgb.pixels().filter(|p| { let (_, s, v) = hsv(p[0], p[1], p[2]); v > 0.72 && s < 0.3 }).count() as f32 / n;
    (0.03..=0.6).contains(&cream).then(|| (name.resize(name.width() * 40 / name.height().max(1), 40, image::imageops::FilterType::Triangle), health))
}
/// Banner names (stylised capitals; up to 3 OCR edits).
pub fn match_banner(text: &str) -> Option<&'static str> {
    let t: String = letters(text).replace(' ', "");
    if t.len() < 6 { return None; }
    [("HIDDENKING", "HIDDEN KING"), ("ARCHMOTHER", "ARCHMOTHER")].iter()
        .map(|(k, n)| (crate::death::levenshtein(&t, k), *n)).filter(|(d, _)| *d <= 3).min_by_key(|(d, _)| *d).map(|(_, n)| n)
}
/// Whether a Patron is yours: the Hidden King's base belongs to the mirrored layout (see
/// [`crate::zone::area_of`]).
pub fn banner_is_own(name: &str, mirrored: Option<bool>) -> Option<bool> {
    let hidden = match name { "HIDDEN KING" => true, "ARCHMOTHER" => false, _ => return None };
    mirrored.map(|m| m == hidden)
}

fn letters(s: &str) -> String { s.chars().filter(|c| c.is_ascii_alphabetic() || *c == ' ').collect::<String>().to_ascii_uppercase().split_whitespace().collect::<Vec<_>>().join(" ") }

/// Known centre word for a line, allowing ~20% OCR edits.
pub fn match_word(text: &str) -> Option<&'static str> {
    let t = letters(text);
    if t.len() < 5 { return None; }
    // The cause line is often cut after "Knocked off" (its last word is read separately).
    if t.len() >= 10 && crate::death::levenshtein(&t, "KNOCKED OFF") <= 2 { return Some("knocked_off_zipline"); }
    // "AFFLICTED" fills from the left in a dark olive that drops its first letters (rec8 41:23 "LIGTED").
    if (6..=9).contains(&t.len()) && !t.contains(' ') && (t.ends_with("LICTED") || t.ends_with("LIGTED")) { return Some("afflicted"); }
    WORDS.iter().map(|(w, tok)| (crate::death::levenshtein(&t, w) as f32 / w.len() as f32, *tok)).filter(|(d, _)| *d <= 0.2).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, t)| t)
}

/// Tokens from recognised lines (same order as [`crops`]).
pub fn interpret(kinds: &[Kind], lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut badge = String::new();
    for (k, l) in kinds.iter().zip(lines) {
        match k {
            Kind::Word | Kind::Cause => if let Some(t) = match_word(l) { if !out.iter().any(|o| o == t) { out.push(t.to_string()); } },
            Kind::Badge if l.trim().is_empty() => {
                // Badge text is the debuff's ability name, e.g. "AURA OF SUFFERING".
                if let Some(n) = crate::death::match_name(&badge, crate::death::source_names()).filter(|_| letters(&badge).len() >= 5) {
                    if !out.iter().any(|o| o == n) { out.push(n.to_string()); }
                }
                badge.clear();
            }
            Kind::Badge => { if !badge.is_empty() { badge.push(' '); } badge.push_str(l.trim()); }
            Kind::Slow => if is_move_slow(l) && !out.iter().any(|o| o == "slowed") { out.push("slowed".into()); },
        }
    }
    out
}

/// Convenience: gate, recognise and interpret one frame (tools and tests).
pub fn read(reader: &crate::ocr::Reader, view: &crate::hudtext::View) -> Vec<String> {
    let c = crops(view);
    if c.is_empty() { return vec![]; }
    let (kinds, imgs): (Vec<Kind>, Vec<DynamicImage>) = c.into_iter().unzip();
    let lines = reader.read_lines(&imgs).unwrap_or_default();
    interpret(&kinds, &lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_known_words_become_statuses() {
        assert_eq!(match_word("STUNNED"), Some("stunned"));
        assert_eq!(match_word("STUNNEDI"), Some("stunned"));
        assert_eq!(match_word("SLENCED"), Some("silenced"));
        assert_eq!(match_word("Knocked off Zipline"), Some("knocked_off_zipline"));
        assert_eq!(match_word("Knocked off"), Some("knocked_off_zipline"));
        assert_eq!(match_word("OUT OF STAMINA"), Some("out_of_stamina"));
        assert_eq!(match_word("Still Life"), None);
        assert_eq!(match_word("RAIN OF ARROWS"), None);
        assert_eq!(match_word("STUN"), None);
        assert_eq!(match_word("AFFLICTED"), Some("afflicted"));
        assert_eq!(match_word("AFFLICTEDI"), Some("afflicted"));
        assert_eq!(match_word("Affliction"), Some("afflicted"));
    }
    #[test]
    fn slow_badges_and_banner_names() {
        assert!(is_move_slow("-0.8 m")); assert!(is_move_slow("-1.2m")); assert!(is_move_slow("–0,8 m"));
        assert!(!is_move_slow("-70 %")); assert!(!is_move_slow("+1.5 m")); assert!(!is_move_slow("-m")); assert!(!is_move_slow("-20"));
        // Live reads of rec8 Frost Grenade badges (icon letters before, pale unit misread).
        for s in ["F-0.8 T", "Ex-0.8T%", "ER-1.0 mE", "1-10 m"] { assert!(is_move_slow(s), "{s}"); }
        for s in ["@70", "[35]", "2-703", "1-05", "0.", "10.5"] { assert!(!is_move_slow(s), "{s}"); }
        assert_eq!(match_word("LIGTED"), Some("afflicted"));
        assert_eq!(match_word("SELECTED"), None);
        let k = [Kind::Slow, Kind::Slow];
        assert_eq!(interpret(&k, &["-70 %".into(), "-0.8 m".into()]), vec!["slowed".to_string()]);
        assert_eq!(match_banner("HiDDEN KinG"), Some("HIDDEN KING"));
        assert_eq!(match_banner("ARCHMOTHER"), Some("ARCHMOTHER"));
        assert_eq!(match_banner("Quinnwils2"), None);
        assert_eq!(banner_is_own("HIDDEN KING", Some(true)), Some(true));
        assert_eq!(banner_is_own("ARCHMOTHER", Some(true)), Some(false));
        assert_eq!(banner_is_own("ARCHMOTHER", None), None);
        assert!(says_deliver_urn("DELIVER THE URN")); assert!(says_deliver_urn("DELIVER THE RN")); assert!(says_deliver_urn("DELIVFR THE URN"));
        assert!(!says_deliver_urn("RAIN OF ARROWS")); assert!(!says_deliver_urn("DELI"));
    }
    #[test]
    fn badges_need_a_dictionary_name() {
        let k = [Kind::Badge, Kind::Badge, Kind::Badge, Kind::Badge, Kind::Badge];
        let l: Vec<String> = ["AURA OF", "SUFFERING", "", "-1.2", ""].iter().map(|s| s.to_string()).collect();
        assert_eq!(interpret(&k, &l), vec!["Aura of Suffering".to_string()]);
    }
}
