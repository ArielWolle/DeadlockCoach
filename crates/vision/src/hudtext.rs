//! Pixel-located personal HUD text on the canonical 1600x900 image: unspent souls (located from
//! the `$` glyph, so the number moves with the HUD), the level badge beside it and the unsecured
//! ("HAUNTED") souls line under it. Pixels decide *where* and *how many digits*; batched line
//! recognition reads the digits; a read whose length disagrees with the glyph count is rejected.
use crate::minimap::{components, hsv, Blob};
use coach_perception::Bounds;
use image::DynamicImage;

/// Connected components of `pred` inside `area` (frame coordinates, clamped).
pub fn blobs(img: &DynamicImage, area: Bounds, pred: impl Fn(u8, u8, u8) -> bool) -> Vec<Blob> {
    let (fw, fh) = (img.width(), img.height());
    if area.x >= fw || area.y >= fh { return vec![]; }
    let (w, h) = (area.w.min(fw - area.x), area.h.min(fh - area.y));
    if w < 2 || h < 2 { return vec![]; }
    let rgb = img.crop_imm(area.x, area.y, w, h).to_rgb8();
    let mask: Vec<bool> = rgb.pixels().map(|p| pred(p[0], p[1], p[2])).collect();
    components(&mask, w, h).into_iter().map(|mut b| {
        b.min_x += area.x; b.max_x += area.x; b.min_y += area.y; b.max_y += area.y; b.cx += area.x as f32; b.cy += area.y as f32; b
    }).collect()
}

/// The canonical 1600x900 image plus, when available, the full-resolution source frame and the
/// source rectangle the canonical image was made from. Small text is cut from the source.
#[derive(Clone, Copy)]
pub struct View<'a> { pub canon: &'a DynamicImage, pub source: Option<(&'a DynamicImage, Bounds)> }
impl<'a> View<'a> {
    pub fn canonical(canon: &'a DynamicImage) -> Self { View { canon, source: None } }
    /// Canonical box `b` rendered at `w` x `h` pixels from the best available pixels.
    pub fn region(&self, b: Bounds, w: u32, h: u32) -> DynamicImage {
        let (img, r) = match self.source {
            Some((src, ctx)) if ctx.w > 0 && ctx.h > 0 => {
                let (sx, sy) = (ctx.w as f32 / 1600.0, ctx.h as f32 / 900.0);
                let x = (ctx.x as f32 + b.x as f32 * sx) as u32; let y = (ctx.y as f32 + b.y as f32 * sy) as u32;
                (src, Bounds { x: x.min(src.width() - 1), y: y.min(src.height() - 1), w: ((b.w as f32 * sx) as u32).max(1), h: ((b.h as f32 * sy) as u32).max(1) })
            }
            _ => (self.canon, b),
        };
        let r = Bounds { w: r.w.min(img.width() - r.x), h: r.h.min(img.height() - r.y), ..r };
        let c = img.crop_imm(r.x, r.y, r.w, r.h);
        if c.width() == w && c.height() == h { c } else { c.resize_exact(w, h, image::imageops::FilterType::CatmullRom) }
    }
}

/// Like [`blobs`] after a 3x3 dilation, which joins a glyph broken by its dark inner outline (the
/// `$` at 1080p splits into 3-4 pieces).
pub fn blobs_closed(img: &DynamicImage, area: Bounds, pred: impl Fn(u8, u8, u8) -> bool) -> Vec<Blob> {
    let (fw, fh) = (img.width(), img.height());
    if area.x >= fw || area.y >= fh { return vec![]; }
    let (w, h) = (area.w.min(fw - area.x), area.h.min(fh - area.y));
    if w < 3 || h < 3 { return vec![]; }
    let rgb = img.crop_imm(area.x, area.y, w, h).to_rgb8();
    let m: Vec<bool> = rgb.pixels().map(|p| pred(p[0], p[1], p[2])).collect();
    let mut d = m.clone();
    for y in 0..h as i64 { for x in 0..w as i64 {
        if m[(y * w as i64 + x) as usize] { continue; }
        d[(y * w as i64 + x) as usize] = (-1..=1).any(|dy| (-1..=1).any(|dx| { let (nx, ny) = (x + dx, y + dy); nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 && m[(ny * w as i64 + nx) as usize] }));
    } }
    components(&d, w, h).into_iter().map(|mut b| {
        b.min_x += area.x; b.max_x += area.x; b.min_y += area.y; b.max_y += area.y; b.cx += area.x as f32; b.cy += area.y as f32; b
    }).collect()
}

/// Mint souls text (the `$`, digits and SOULS label).
pub fn mint(r: u8, g: u8, b: u8) -> bool { let (h, s, v) = hsv(r, g, b); (125.0..=180.0).contains(&h) && s > 0.2 && v > 0.6 }
/// Souls digits: mint, the light-blue change flash, near-white, or the red "cannot afford" flash.
pub fn souls_text(r: u8, g: u8, b: u8) -> bool {
    let (h, s, v) = hsv(r, g, b);
    v > 0.6 && ((125.0..=215.0).contains(&h) && s > 0.12 || s < 0.15 && v > 0.85 || (h < 15.0 || h > 345.0) && s > 0.5 && v > 0.65)
}
/// Near-white glyphs (level badge digits, respawn digits, panel numbers).
pub fn white(r: u8, g: u8, b: u8) -> bool { let (_, s, v) = hsv(r, g, b); v > 0.78 && s < 0.22 }

/// Crop with padding, clamped to the frame.
pub fn crop_padded(img: &DynamicImage, x0: u32, y0: u32, x1: u32, y1: u32, pad: u32) -> DynamicImage {
    let x = x0.saturating_sub(pad); let y = y0.saturating_sub(pad);
    let r = (x1 + pad + 1).min(img.width()); let b = (y1 + pad + 1).min(img.height());
    img.crop_imm(x, y, r.saturating_sub(x).max(1), b.saturating_sub(y).max(1))
}

/// Where the souls number sits on this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoulsGlyphs {
    /// The `$` glyph.
    pub dollar: Bounds,
    /// Tight box around the digits (commas included), right of the `$`.
    pub digits: Bounds,
    /// Number of digit glyphs (commas excluded).
    pub count: usize,
}

fn bounds_of(b: &Blob) -> Bounds { Bounds { x: b.min_x, y: b.min_y, w: b.w(), h: b.h() } }

/// Search area for the souls row (canonical): the alive HUD draws it at y ~742-771 and the shop
/// at ~707-731; a "HAUNTED SOULS" line below shifts it up by ~15 px.
pub const SOULS_SEARCH: Bounds = Bounds { x: 30, y: 660, w: 260, h: 140 };

/// The souls search area rendered at 1920x1080 scale (exact pixels from a 1080p source): a box
/// downsample to 1600x900 merged commas into digits and broke the glyph count.
pub fn souls_area(view: &View) -> DynamicImage { view.region(SOULS_SEARCH, SOULS_SEARCH.w * 6 / 5, SOULS_SEARCH.h * 6 / 5) }

/// Locate the `$` and the digit run after it from the mint text mask, in a [`souls_area`] image.
pub fn locate_souls(img: &DynamicImage) -> Option<SoulsGlyphs> {
    let all = Bounds { x: 0, y: 0, w: img.width(), h: img.height() };
    let mut g: Vec<Blob> = blobs_closed(img, all, mint).into_iter().filter(|b| b.area >= 6 && b.h() >= 3).collect();
    g.sort_by_key(|b| b.min_x);
    // The `$` is the tallest glyph of its row (digits are 55-90% of its height) and narrow
    // (measured w/h 0.51-0.55; the teal level-badge ring beside it is ~0.8). Every candidate is
    // tried and the longest digit run wins.
    let mut best: Option<SoulsGlyphs> = None;
    for d in g.iter() {
        let dh = d.h() as f32;
        if !(18.0..=56.0).contains(&dh) || d.w() as f32 > dh * 0.68 || (d.w() as f32) < dh * 0.3 { continue; }
        let mid = (d.min_y + d.max_y) as f32 / 2.0;
        let is_digit = |b: &Blob| { let h = b.h() as f32; (0.5..=0.92).contains(&(h / dh)) && ((b.min_y + b.max_y) as f32 / 2.0 - mid).abs() <= dh * 0.2 };
        // Digits are mint at rest but flash light blue when souls change and red on a failed
        // purchase: they are read with a broader text mask in the `$` band.
        let band = Bounds { x: d.max_x + 1, y: d.min_y.saturating_sub(2), w: d.h() * 8, h: d.h() + 4 };
        let mut dg: Vec<Blob> = blobs(img, band, souls_text).into_iter().filter(|b| b.area >= 6 && b.h() >= 3).collect();
        dg.sort_by_key(|b| b.min_x);
        let mut run: Vec<&Blob> = vec![];
        let mut count = 0usize;
        let mut right = d.max_x;
        let mut pending_comma: Option<&Blob> = None;
        for b in dg.iter() {
            let gap = b.min_x as i64 - right as i64;
            if gap > (dh * 0.55) as i64 { break; }
            if is_digit(b) {
                // Merged neighbours ("23" as one blob) count by width: a digit is ~0.65x as
                // wide as tall, a "1" far narrower.
                let (w, h) = (b.w() as f32, b.h() as f32);
                count += if w > h * 1.05 { ((w / (h * 0.62)).round() as usize).max(2) } else { 1 };
                if let Some(c) = pending_comma.take() { run.push(c); }
                run.push(b); right = b.max_x;
            } else if !run.is_empty() && (b.h() as f32) < dh * 0.42 && b.min_y as f32 >= mid && b.max_y <= d.max_y + d.h() / 4 && pending_comma.is_none() {
                pending_comma = Some(b); right = b.max_x; // a thousands comma only if a digit follows
            } else if b.max_x <= right + 1 || run.is_empty() && (b.h() as f32) < dh * 0.4 {
                continue; // a fragment inside the run, or a sparkle between `$` and digits
            } else { break; }
        }
        if count == 0 || count > 6 { continue; }
        // A digit-height glyph shortly after the last digit means the run was cut (an
        // unrecognised comma): the count would be short, so say nothing rather than misread
        // "3,752" as 3.
        let last = run.last()?.max_x;
        let cut = dg.iter().any(|b| b.min_x > last && b.min_x <= last + d.h() * 4 / 5 && is_digit(b));
        let (x0, x1) = (run.first()?.min_x, run.last()?.max_x);
        let (y0, y1) = (run.iter().map(|b| b.min_y).min()?, run.iter().map(|b| b.max_y).max()?);
        let found = SoulsGlyphs { dollar: bounds_of(d), digits: Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }, count: if cut { 0 } else { count } };
        if best.is_none_or(|b| found.count > b.count) { best = Some(found); }
    }
    best.filter(|b| b.count > 0)
}

/// Two recognition crops for the located digits, starting at the first digit: any part of the
/// `$` in the crop was measured to make the recognizer drop the leading digit (",757" for 3,757).
pub fn souls_crops(img: &DynamicImage, s: &SoulsGlyphs) -> [DynamicImage; 2] {
    let d = s.digits;
    let c = |left: u32, pad: u32| {
        let x0 = d.x.saturating_sub(left); let y0 = d.y.saturating_sub(pad);
        let x1 = (d.x + d.w + pad).min(img.width()); let y1 = (d.y + d.h + pad).min(img.height());
        img.crop_imm(x0, y0, x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1))
    };
    [c(2, 4), c(0, 6)]
}

/// Souls from the two crop reads: a read must have exactly the located digit count; two valid
/// reads must agree.
pub fn souls_value(lines: &[&str], count: usize) -> Option<u32> {
    let v: Vec<u32> = lines.iter().filter_map(|l| digits_matching(l, count)).collect();
    match v.as_slice() { [a] => Some(*a), [a, b] if a == b => Some(*a), _ => None }
}

/// Digits of a line read; `None` unless it has exactly `count` digits (thousands separators
/// allowed only between groups of three).
pub fn digits_matching(text: &str, count: usize) -> Option<u32> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    // Accept "1,930" / "1.930" / "1930"; strip a leading `$` or `S` read from a clipped glyph.
    let t = t.trim_start_matches(['$', 'S', 's', '§']);
    let core: String = t.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
    if core.is_empty() { return None; }
    let groups: Vec<&str> = core.split([',', '.']).filter(|g| !g.is_empty()).collect();
    if groups.len() > 1 && groups[1..].iter().any(|g| g.len() != 3) { return None; }
    let d: String = groups.concat();
    (d.len() == count).then(|| d.parse().ok()).flatten()
}

/// Level badge digits left of the `$` (white on the teal badge), if visible.
pub fn level_glyphs(img: &DynamicImage, s: &SoulsGlyphs) -> Option<(Bounds, usize)> {
    let dh = s.dollar.h;
    let area = Bounds { x: s.dollar.x.saturating_sub(dh * 3), y: s.dollar.y.saturating_sub(dh / 2), w: dh * 3 - dh / 6, h: dh * 2 };
    let mut g: Vec<Blob> = blobs(img, area, white).into_iter()
        .filter(|b| { let h = b.h() as f32 / dh as f32; (0.45..=0.95).contains(&h) && b.w() >= 2 && b.area >= 8 }).collect();
    if g.is_empty() || g.len() > 2 { return None; }
    g.sort_by_key(|b| b.min_x);
    let gap_ok = g.windows(2).all(|w| w[1].min_x <= w[0].max_x + dh / 2 && (w[1].cy - w[0].cy).abs() <= dh as f32 * 0.2);
    if !gap_ok { return None; }
    let (x0, x1) = (g[0].min_x, g.last()?.max_x);
    let (y0, y1) = (g.iter().map(|b| b.min_y).min()?, g.iter().map(|b| b.max_y).max()?);
    Some((Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }, g.len()))
}

/// Line box of the dim red "N HAUNTED SOULS" text under the souls row, when enough reddish
/// text pixels are present (the line is absent while no souls are unsecured).
pub fn unsecured_line(img: &DynamicImage, s: &SoulsGlyphs) -> Option<Bounds> {
    let dh = s.dollar.h;
    let area = Bounds { x: s.dollar.x.saturating_sub(dh / 3), y: s.dollar.y + dh - 1, w: dh * 7, h: dh * 3 / 5 };
    if area.y + area.h > img.height() { return None; }
    let red = |r: u8, g: u8, b: u8| { let (h, sat, v) = hsv(r, g, b); (h < 22.0 || h > 340.0) && sat > 0.35 && v > 0.35 };
    let rgb = img.crop_imm(area.x, area.y, area.w, area.h).to_rgb8();
    let n = rgb.pixels().filter(|p| red(p[0], p[1], p[2])).count();
    (n >= 25 && n <= (area.w * area.h / 3) as usize).then_some(area)
}

/// "?452 HAUNTED SOULS" -> 452: the number token right before the HAUNTED label, with lookalike
/// letters mapped ("3S1" = 351, "H22" = 422). The souls icon in front is often read as "@", "?",
/// "6" or "0": a leading icon-like character of a 4-character token is dropped. Noisy by nature;
/// [`Unsecured`] reports a value only when two reads agree.
pub fn parse_unsecured(text: &str) -> Option<u32> {
    let toks: Vec<&str> = text.split_whitespace().collect();
    let k = toks.iter().position(|t| { let u = t.to_ascii_uppercase(); ["HAUNT", "AUNTED", "HAUNIED", "MAUNTED", "HAUTED"].iter().any(|w| u.contains(w)) })?;
    let mut tok = toks.get(k.checked_sub(1)?)?.trim_start_matches(['@', '?', '.', '\'', '"', ',']);
    if tok.chars().count() == 4 && matches!(tok.chars().next(), Some('6' | '0' | 'G' | 'O')) { tok = &tok[1..]; }
    if tok.is_empty() || tok.chars().count() > 3 { return None; }
    lookalike_digits(tok, tok.chars().count())
}

/// Level badge: `state` = (accepted level, last raw read). Levels never go down within a match.
/// A read continues the accepted level (+0..2), or, first / after a gap, needs two identical reads
/// in a row and must not be lower. Returns the level read on this frame, if accepted.
pub fn accept_level(state: &mut (Option<u32>, Option<u32>), read: Option<u32>) -> Option<u32> {
    let r = read.filter(|v| (1..=60).contains(v));
    let ok = r.is_some_and(|v| match state.0 {
        Some(l) => (l..=l + 2).contains(&v) || v > l && state.1 == Some(v),
        None => state.1 == Some(v),
    });
    if r.is_some() { state.1 = r; }
    if ok { state.0 = r; r } else { None }
}

/// Unsecured souls: a read is reported when the previous read (within 6 s) is within 25 of it.
#[derive(Default, Debug, Clone)]
pub struct Unsecured { last: Option<(u32, u64)> }
impl Unsecured {
    pub fn observe(&mut self, read: Option<u32>, now: u64) -> Option<u32> {
        let r = read?;
        let ok = self.last.is_some_and(|(v, t)| now >= t && now - t <= 6000 && v.abs_diff(r) <= 25);
        self.last = Some((r, now));
        ok.then_some(r)
    }
}

/// Digits of a small line read where OCR swaps lookalike letters ("3S1" = 351, "4O4" = 404).
pub fn lookalike_digits(text: &str, count: usize) -> Option<u32> {
    let d: String = text.chars().filter(|c| !c.is_whitespace()).map(|c| match c { 'S' | 's' => '5', 'O' | 'o' | 'D' | 'Q' => '0', 'G' => '6', 'B' => '8', 'I' | 'l' | '|' => '1', 'Z' => '2', 'H' | 'U' => '4', 'T' => '7', c => c }).filter(|c| c.is_ascii_digit()).collect();
    (d.len() == count && count > 0).then(|| d.parse().ok()).flatten()
}



#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digits_must_match_the_glyph_count() {
        assert_eq!(digits_matching("2,809", 4), Some(2809));
        assert_eq!(digits_matching("$531", 3), Some(531));
        assert_eq!(digits_matching("3757", 4), Some(3757));
        assert_eq!(digits_matching("53", 3), None, "dropped digit");
        assert_eq!(digits_matching("35,31", 4), None, "bad grouping");
        assert_eq!(digits_matching("", 1), None);
    }
    #[test]
    fn level_never_goes_down() {
        let mut s = (None, None);
        assert_eq!(accept_level(&mut s, Some(17)), None, "first read needs a repeat");
        assert_eq!(accept_level(&mut s, Some(17)), Some(17));
        assert_eq!(accept_level(&mut s, Some(18)), Some(18));
        assert_eq!(accept_level(&mut s, Some(9)), None, "29 with a hidden digit");
        assert_eq!(accept_level(&mut s, Some(9)), None, "a repeated lower read is still rejected");
        assert_eq!(accept_level(&mut s, Some(29)), None, "a jump after a gap needs a repeat");
        assert_eq!(accept_level(&mut s, Some(29)), Some(29));
    }
    #[test]
    fn small_digits_map_lookalike_letters() {
        assert_eq!(lookalike_digits("3S1", 3), Some(351));
        assert_eq!(lookalike_digits("4O4", 3), Some(404));
        assert_eq!(lookalike_digits("1B5", 3), Some(185));
        assert_eq!(lookalike_digits("6489", 3), None);
    }
    #[test]
    fn unsecured_line_needs_its_label() {
        assert_eq!(parse_unsecured("?452 HAUNTED SOULS"), Some(452));
        assert_eq!(parse_unsecured("@ 3S1 HAUNTED SOULS"), Some(351));
        assert_eq!(parse_unsecured("0495 HAUNTED SOULS"), Some(495), "icon glued as a digit");
        assert_eq!(parse_unsecured("H22 HAUNTED SOULS"), Some(422));
        let mut u = Unsecured::default();
        assert_eq!(u.observe(Some(452), 0), None);
        assert_eq!(u.observe(Some(446), 1000), Some(446));
        assert_eq!(u.observe(Some(46), 2000), None);
        assert_eq!(parse_unsecured("220 MAUNTED SOULS"), Some(220));
        assert_eq!(parse_unsecured("HAUNTED SOULS"), None);
        assert_eq!(parse_unsecured("131"), None);
    }
}
