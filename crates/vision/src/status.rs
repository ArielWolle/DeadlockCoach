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

/// Known centre texts -> tokens.
const WORDS: [(&str, &str); 6] = [("STUNNED", "stunned"), ("SILENCED", "silenced"), ("ABILITIES DISABLED", "silenced"),
    ("DISARMED", "disarmed"), ("OUT OF STAMINA", "out_of_stamina"), ("KNOCKED OFF ZIPLINE", "knocked_off_zipline")];

/// Which crop a recognised line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Word, Cause, Badge }

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
    out
}

fn letters(s: &str) -> String { s.chars().filter(|c| c.is_ascii_alphabetic() || *c == ' ').collect::<String>().to_ascii_uppercase().split_whitespace().collect::<Vec<_>>().join(" ") }

/// Known centre word for a line, allowing ~20% OCR edits.
pub fn match_word(text: &str) -> Option<&'static str> {
    let t = letters(text);
    if t.len() < 5 { return None; }
    // The cause line is often cut after "Knocked off" (its last word is read separately).
    if t.len() >= 10 && crate::death::levenshtein(&t, "KNOCKED OFF") <= 2 { return Some("knocked_off_zipline"); }
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
    }
    #[test]
    fn badges_need_a_dictionary_name() {
        let k = [Kind::Badge, Kind::Badge, Kind::Badge, Kind::Badge, Kind::Badge];
        let l: Vec<String> = ["AURA OF", "SUFFERING", "", "-1.2", ""].iter().map(|s| s.to_string()).collect();
        assert_eq!(interpret(&k, &l), vec!["Aura of Suffering".to_string()]);
    }
}
