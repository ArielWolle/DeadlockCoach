//! Your ability bar: four round icons bottom centre (slot 4 = ultimate). Measured on rec5, rec8,
//! rec12 and rec13 (1600x900 canonical):
//! - ready: a light-grey icon disc (>= 45% of the disc near-white; the ultimate's art ~50%);
//! - cooldown: a dark disc (a lighter clock-wipe sector shows progress) with the seconds left as
//!   large white digits ("13", "1.9"), sometimes over a translucent grey disc;
//! - not castable (silenced, not learned, no charge): a dark disc without digits;
//! - faded / translucent icons (stuns, a not-yet-learned ultimate) are not classified.
//! The pixel test is cheap; only slots with digit pixels add a recognition line.
use coach_core::AbilityState;
use coach_perception::Bounds;
use image::DynamicImage;

/// Slot centres (canonical px) and the icon disc radius.
pub const CENTRES: [(f32, f32); 4] = [(686.0, 851.0), (763.0, 837.0), (837.0, 837.0), (912.0, 851.0)];
pub const RADIUS: f32 = 27.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot { Ready, Unavailable, Digits, Unclear }

/// Shares of the disc: near-white, dark; and near-white glyph pixels in the central box.
pub fn slot_shares(img: &DynamicImage, i: usize) -> (f32, f32, f32) {
    let (cx, cy) = CENTRES[i];
    let r = RADIUS;
    let (mut n, mut bright, mut dark, mut bn, mut glyph) = (0f32, 0f32, 0f32, 0f32, 0f32);
    let (w, h) = (img.width() as i32, img.height() as i32);
    for dy in -(r as i32)..=(r as i32) { for dx in -(r as i32)..=(r as i32) {
        let (x, y) = (cx as i32 + dx, cy as i32 + dy);
        if x < 0 || y < 0 || x >= w || y >= h { continue; }
        let p = image::GenericImageView::get_pixel(img, x as u32, y as u32);
        let (_, s, v) = crate::minimap::hsv(p[0], p[1], p[2]);
        let (fx, fy) = (dx as f32, dy as f32);
        if fx * fx + fy * fy <= (0.85 * r) * (0.85 * r) {
            n += 1.0;
            if v > 0.7 && s < 0.35 { bright += 1.0; }
            if v < 0.22 { dark += 1.0; }
        }
        if fx.abs() <= 0.6 * r && fy.abs() <= 0.45 * r {
            bn += 1.0;
            if v > 0.85 && s < 0.25 { glyph += 1.0; }
        }
    }}
    (bright / n.max(1.0), dark / n.max(1.0), glyph / bn.max(1.0))
}

/// Classify one slot from its shares.
pub fn classify(bright: f32, dark: f32, glyph: f32) -> Slot {
    if bright >= 0.45 { Slot::Ready }
    else if bright < 0.3 && (0.06..=0.4).contains(&glyph) { Slot::Digits }
    else if bright < 0.25 && dark >= 0.5 && glyph < 0.04 { Slot::Unavailable }
    else { Slot::Unclear }
}

/// Located cooldown digits of a slot: tight bounds of the digit glyphs, their count, and
/// whether a decimal point sits between the first two ("1.9", drawn under 10 s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Digits { pub bounds: Bounds, pub count: usize, pub decimal: bool }

/// White digit glyphs (~18 px tall canonical) in the centre of slot `i`. The fixed box read the
/// clock-wipe and icon art along with the digits ("11" -> "7", "1.9" -> "129" on rec13).
pub fn locate_digits(img: &DynamicImage, i: usize) -> Option<Digits> {
    let (cx, cy) = CENTRES[i];
    let area = Bounds { x: (cx - 0.8 * RADIUS) as u32, y: (cy - 0.62 * RADIUS) as u32, w: (1.6 * RADIUS) as u32, h: (1.24 * RADIUS) as u32 };
    let all = crate::hudtext::blobs(img, area, crate::hudtext::white);
    let inside = |b: &crate::minimap::Blob| b.min_x > area.x && b.min_y > area.y && b.max_x + 1 < area.x + area.w && b.max_y + 1 < area.y + area.h;
    let tall = |b: &&crate::minimap::Blob| (10..=26).contains(&b.h()) && b.w() >= 2 && b.area >= 12 && inside(b);
    let mut g: Vec<_> = all.iter().filter(tall).filter(|b| b.w() <= 16).copied().collect();
    // Touching digits merge into one blob (the box-filtered context joined rec13 180's "23");
    // a wide blob is taken only alone (clock-wipe and icon-art blobs are wide too).
    if g.is_empty() { g = all.iter().filter(tall).filter(|b| b.w() <= 30).copied().collect(); if g.len() != 1 { return None; } }
    if g.is_empty() || g.len() > 3 { return None; }
    g.sort_by_key(|b| b.min_x);
    let h = g.iter().map(|b| b.h()).max()?;
    if g.iter().any(|b| (b.h() as f32) < h as f32 * 0.8) || g.windows(2).any(|w| w[1].min_x > w[0].max_x + h) { return None; }
    // A blob wider than its height holds two digits, wider than 1.6x three.
    let count: usize = g.iter().map(|b| { let r = b.w() as f32 / b.h() as f32; if r > 1.6 { 3 } else if r > 1.0 { 2 } else { 1 } }).sum();
    if count > 3 { return None; }
    let (x0, x1) = (g[0].min_x, g.last()?.max_x);
    let (y0, y1) = (g.iter().map(|b| b.min_y).min()?, g.iter().map(|b| b.max_y).max()?);
    // The point: a small blob on the baseline between the first two digits.
    let decimal = g.len() == 2 && all.iter().any(|b| b.h() <= 6 && b.w() <= 6 && b.area >= 2
        && b.cx > g[0].max_x as f32 - 1.0 && b.cx < g[1].min_x as f32 + 1.0 && b.cy > (y0 + y1) as f32 / 2.0);
    Some(Digits { bounds: Bounds { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }, count, decimal })
}

/// Per-slot classes (with located digits), or None when fewer than three slots look like the
/// ability bar (HUD hidden, faded, covered). A cooldown disc whose digits are not located is
/// not castable now (Unavailable).
pub fn slots(img: &DynamicImage) -> Option<[(Slot, Option<Digits>); 4]> {
    if img.width() != 1600 || img.height() != 900 { return None; }
    let s: [(Slot, Option<Digits>); 4] = std::array::from_fn(|i| {
        let (b, d, g) = slot_shares(img, i);
        match classify(b, d, g) {
            Slot::Digits => match locate_digits(img, i) { Some(dg) => (Slot::Digits, Some(dg)), None if d >= 0.3 => (Slot::Unavailable, None), None => (Slot::Unclear, None) },
            c => (c, None),
        }
    });
    (s.iter().filter(|c| c.0 != Slot::Unclear).count() >= 3).then_some(s)
}

/// Crop of located digits for line recognition: 8 px padding, upscaled 2x (bicubic), then
/// binarised (near-white glyph pixels white, everything else black). On 23 measured cooldown
/// digits (rec5/6/8/11/12/13) this read all correctly; the plain crop misread 6 ("5" -> "6",
/// "8" -> "88", "4" -> "A", "37" -> "337": the clock-wipe and icon art around the digits).
pub fn digits_crop(img: &DynamicImage, d: &Digits) -> DynamicImage { digits_crop_padded(img, d, 8) }
/// The retry crop (5 px padding) when the first read's digit count did not match: a lone digit
/// is sometimes read doubled at 8 px ("3" -> "33", rec13 310).
pub fn digits_retry_crop(img: &DynamicImage, d: &Digits) -> DynamicImage { digits_crop_padded(img, d, 5) }
fn digits_crop_padded(img: &DynamicImage, d: &Digits, pad: u32) -> DynamicImage {
    let b = d.bounds;
    let p = Bounds { x: b.x.saturating_sub(pad), y: b.y.saturating_sub(pad), w: b.w + 2 * pad, h: b.h + 2 * pad };
    let mut g = crate::hudtext::View::canonical(img).region(p, p.w * 2, p.h * 2).to_rgb8();
    for px in g.pixels_mut() { *px = if crate::hudtext::white(px[0], px[1], px[2]) { image::Rgb([255, 255, 255]) } else { image::Rgb([0, 0, 0]) }; }
    DynamicImage::ImageRgb8(g)
}

/// Seconds from the recognised digits: exactly `count` digits; with a decimal point (located,
/// or read between two digits) the value is d.d rounded up ("1.9" -> 2, "0.6" -> 1).
pub fn cooldown_secs(text: &str, d: &Digits) -> Option<u32> {
    let t: String = text.trim().chars().map(|c| match c { 'O' | 'o' => '0', 'l' | 'I' | '|' => '1', c => c }).filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let point = t.trim_matches('.').contains('.');
    let t: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if t.len() != d.count { return None; }
    let v: u32 = t.parse().ok()?;
    let secs = if (d.decimal || point) && d.count == 2 { v.div_ceil(10) } else { v };
    (secs <= 300).then_some(secs)
}

/// Snapshot entries from slot classes and the digit reads (`texts[i]`: the recognised texts of
/// a Digits slot's crops, see [`digits_crop`]). A Digits slot none of whose texts matches its
/// glyph count has no seconds.
pub fn states(slots: &[(Slot, Option<Digits>); 4], texts: &[Vec<String>; 4]) -> Vec<AbilityState> {
    slots.iter().enumerate().filter_map(|(i, (s, dg))| {
        let slot = i as u8 + 1;
        match s {
            Slot::Ready => Some(AbilityState { slot, ready: Some(true), cooldown_secs: None }),
            Slot::Unavailable => Some(AbilityState { slot, ready: Some(false), cooldown_secs: None }),
            Slot::Digits => Some(AbilityState { slot, ready: Some(false), cooldown_secs: dg.as_ref().and_then(|d| texts[i].iter().find_map(|t| cooldown_secs(t, d))) }),
            Slot::Unclear => None,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cooldown_text() {
        let d = |count, decimal| Digits { bounds: Bounds { x: 0, y: 0, w: 1, h: 1 }, count, decimal };
        assert_eq!(cooldown_secs("13", &d(2, false)), Some(13));
        assert_eq!(cooldown_secs("1.9", &d(2, true)), Some(2));
        assert_eq!(cooldown_secs("19", &d(2, true)), Some(2), "a point the recogniser dropped");
        assert_eq!(cooldown_secs("0.6", &d(2, true)), Some(1));
        assert_eq!(cooldown_secs("1.0 ", &d(2, false)), Some(1), "a point read but not located");
        assert_eq!(cooldown_secs("13.", &d(2, false)), Some(13), "a trailing dot is no decimal point");
        assert_eq!(cooldown_secs(" 43 ", &d(2, false)), Some(43));
        assert_eq!(cooldown_secs("l1", &d(2, false)), Some(11));
        assert_eq!(cooldown_secs("7", &d(2, false)), None, "digit count must match the glyphs");
        assert_eq!(cooldown_secs("", &d(1, false)), None);
        assert_eq!(cooldown_secs("129", &d(3, false)), Some(129));
        assert_eq!(cooldown_secs("999", &d(3, false)), None);
    }
    #[test]
    fn classes_from_measured_shares() {
        // rec13 frame 60: ready, "13", ready, ready ultimate; rec8 100: dark without digits.
        assert_eq!(classify(0.64, 0.03, 0.28), Slot::Ready);
        assert_eq!(classify(0.08, 0.72, 0.14), Slot::Digits);
        assert_eq!(classify(0.50, 0.15, 0.05), Slot::Ready);
        assert_eq!(classify(0.00, 0.64, 0.00), Slot::Unavailable);
        // rec12 200: "17" over a translucent disc; a faded ultimate.
        assert_eq!(classify(0.09, 0.10, 0.12), Slot::Digits);
        assert_eq!(classify(0.00, 0.00, 0.00), Slot::Unclear);
    }
}
