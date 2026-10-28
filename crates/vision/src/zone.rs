//! Zone label: the minimap's bottom-right corner names the area you stand in, in crisp
//! rotated capitals ("YORK : FACTORY", "BROADWAY : TIMES SQUARE", "HIDDEN KING : BASE").
//! The label is centred on a fixed point of the minimap (whatever its length) and drawn
//! along a fixed angle, so one rotated strip relative to the located map reads every name.
use coach_core::{Lane, ZoneArea};
use image::{DynamicImage, GenericImageView, Pixel, RgbImage};

/// Label centre in canonical minimap px (335x350 crop), measured on rec4/rec5 frames: a long
/// ("BROADWAY : TIMES SQUARE", 201,350 -> 322,281) and a short ("YORK : FACTORY", 224,335 ->
/// 295,296) label share the centre (260,316) and the baseline angle (-29.7 deg).
pub const LABEL_CENTER: (f32, f32) = (260.5, 315.5);
pub const LABEL_ANGLE_DEG: f32 = -29.7;
/// Strip size along / across the baseline (canonical px): the longest label is ~140 px.
pub const LABEL_LEN: f32 = 156.0;
pub const LABEL_HEIGHT: f32 = 15.0;

/// Sample the rotated label strip, horizontal, from `src` whose minimap occupies `map`
/// (x, y, w, h in `src` px). Points outside `src` read black (the label may run past the
/// bottom of a tight minimap crop). Output text height is ~36 px for the line recognizer.
pub fn label_strip<I: GenericImageView>(src: &I, map: (f32, f32, f32, f32)) -> DynamicImage where I::Pixel: Pixel<Subpixel = u8> {
    let (sx, sy) = (map.2 / 335.0, map.3 / 350.0);
    let out_h = 40u32;
    let k = out_h as f32 / LABEL_HEIGHT; // output px per canonical px
    let out_w = (LABEL_LEN * k) as u32;
    let (c, s) = (LABEL_ANGLE_DEG.to_radians().cos(), LABEL_ANGLE_DEG.to_radians().sin());
    let (w, h) = src.dimensions();
    let px = |x: i64, y: i64| -> [f32; 3] {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { return [0.0; 3]; }
        let p = src.get_pixel(x as u32, y as u32).to_rgb(); [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let mut out = RgbImage::new(out_w, out_h);
    for oy in 0..out_h { for ox in 0..out_w {
        // canonical offsets along (u) and across (v) the baseline
        let u = (ox as f32 + 0.5) / k - LABEL_LEN / 2.0;
        let v = (oy as f32 + 0.5) / k - LABEL_HEIGHT / 2.0;
        let (cx, cy) = (LABEL_CENTER.0 + u * c - v * s, LABEL_CENTER.1 + u * s + v * c);
        let (fx, fy) = (map.0 + cx * sx - 0.5, map.1 + cy * sy - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (ax, ay) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (p00, p10, p01, p11) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
        let mut q = [0u8; 3];
        for i in 0..3 {
            let top = p00[i] * (1.0 - ax) + p10[i] * ax;
            let bot = p01[i] * (1.0 - ax) + p11[i] * ax;
            q[i] = (top * (1.0 - ay) + bot * ay).round().clamp(0.0, 255.0) as u8;
        }
        out.put_pixel(ox, oy, image::Rgb(q));
    }}
    DynamicImage::ImageRgb8(out)
}

/// The label is "AREA : REGION" with the region in smaller capitals; the recognizer drops the
/// small colon and shrinks the small capitals, so the strip is split at the colon (two small
/// bright dots stacked vertically) and each part is read as its own line, cropped tightly to
/// its own capitals. Returns [area, region] crops, or a single crop for one-word labels
/// ("CHINATOWN", "HAUNTED LOT") and when no colon is visible; empty without label text.
pub fn label_parts(strip: &DynamicImage) -> Vec<DynamicImage> {
    use crate::minimap::Blob;
    let rgb = strip.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mask: Vec<bool> = rgb.pixels().map(|p| { let (mn, mx) = (p.0.iter().copied().min().unwrap(), p.0.iter().copied().max().unwrap()); mn > 130 && mx - mn < 70 }).collect();
    let blobs: Vec<Blob> = crate::minimap::components(&mask, w, h).into_iter().filter(|b| b.area >= 3).collect();
    let hf = h as f32;
    let dot = |b: &Blob| (b.w() as f32) <= 0.2 * hf && (b.h() as f32) <= 0.2 * hf && b.w() >= 2 && b.h() >= 2;
    let letter = |b: &Blob| (b.h() as f32) >= 0.22 * hf && (b.h() as f32) <= 0.8 * hf && b.max_y + 1 < h && b.min_y > 0;
    let mut letters: Vec<&Blob> = blobs.iter().filter(|b| letter(b)).collect();
    letters.sort_by(|a, b| a.cx.total_cmp(&b.cx));
    // Nearest letter left / right of x (by edges: neighbouring capitals often merge).
    let left_of = |x: f32| letters.iter().filter(|l| (l.max_x as f32) < x - 1.0 && x - (l.max_x as f32) < 0.9 * hf).max_by(|a, b| a.max_x.cmp(&b.max_x)).copied();
    let right_of = |x: f32| letters.iter().filter(|l| (l.min_x as f32) > x + 1.0 && (l.min_x as f32) - x < 0.9 * hf).min_by(|a, b| a.min_x.cmp(&b.min_x)).copied();
    // Colon: two dots with the same x, one above the other, large capitals on its left and
    // smaller ones on its right (a Q's tail or two serifs in "SQUARE" are not a colon).
    let mut colon = None;
    for a in blobs.iter().filter(|b| dot(b)) {
        for b in blobs.iter().filter(|b| dot(b)) {
            if b.cy <= a.cy || (a.cx - b.cx).abs() > 2.5 { continue; }
            if !(0.12 * hf..=0.45 * hf).contains(&(b.cy - a.cy)) { continue; }
            let x = (a.cx + b.cx) / 2.0;
            if let (Some(l), Some(r)) = (left_of(x), right_of(x)) {
                if l.h() as f32 >= 1.15 * r.h() as f32 { colon = Some((x, a.w().max(b.w()) as f32)); }
            }
        }
    }
    // Grow a run of letters from a start point while gaps stay below one letter height.
    let run = |from: f32, to_left: bool| -> Option<(f32, f32, u32, u32)> {
        let (mut lo, mut hi, mut top, mut bottom, mut any) = (from, from, h, 0u32, false);
        loop {
            let next = if to_left { left_of(lo) } else { right_of(hi) };
            let Some(b) = next else { break };
            lo = lo.min(b.min_x as f32); hi = hi.max(b.max_x as f32);
            top = top.min(b.min_y); bottom = bottom.max(b.max_y); any = true;
        }
        any.then_some((lo, hi, top, bottom))
    };
    // Padding never crosses `limits` (the colon), or its dots read as a letter ("E DOCKS").
    let crop = |x0: f32, x1: f32, top: u32, bottom: u32, limits: (f32, f32)| -> Option<DynamicImage> {
        let th = (bottom + 1).saturating_sub(top) as f32;
        let (pv, ph) = ((0.35 * th) as u32 + 1, 0.5 * th + 1.0);
        let (x0, x1) = ((x0 - ph).max(limits.0).max(0.0) as u32, ((x1 + ph + 1.0).min(limits.1) as u32).min(w));
        let (y0, y1) = (top.saturating_sub(pv), (bottom + pv + 1).min(h));
        (x1 > x0 + 4 && y1 > y0 + 2).then(|| strip.crop_imm(x0, y0, x1 - x0, y1 - y0))
    };
    if let Some((cx, dw)) = colon {
        let (Some(l), Some(r)) = (run(cx - dw / 2.0, true), run(cx + dw / 2.0, false)) else { return vec![] };
        let (a, b) = (cx - dw, cx + dw);
        return [crop(l.0, l.1.min(a), l.2, l.3, (0.0, a)), crop(r.0.max(b), r.1, r.2, r.3, (b, w as f32))].into_iter().flatten().collect();
    }
    // One part: the letter run through the strip centre (the label is centred on it).
    let mid = w as f32 / 2.0;
    let Some(c) = letters.iter().min_by(|a, b| (a.cx - mid).abs().total_cmp(&(b.cx - mid).abs())).copied() else { return vec![] };
    if (c.cx - mid).abs() > 0.25 * w as f32 { return vec![]; }
    let (l, r) = (run(c.min_x as f32 + 1.5, true), run(c.max_x as f32 - 1.5, false));
    let lo = l.map(|l| l.0).unwrap_or(c.min_x as f32).min(c.min_x as f32);
    let hi = r.map(|r| r.1).unwrap_or(c.max_x as f32).max(c.max_x as f32);
    let top = [Some(c.min_y), l.map(|l| l.2), r.map(|r| r.2)].into_iter().flatten().min().unwrap();
    let bottom = [Some(c.max_y), l.map(|l| l.3), r.map(|r| r.3)].into_iter().flatten().max().unwrap();
    crop(lo, hi, top, bottom, (0.0, w as f32)).into_iter().collect()
}

/// Whether every part of a zone is a known label word (accepted on a single read).
pub fn is_known(zone: &str) -> bool { zone.split(" : ").all(|p| VOCAB.contains(&p)) }

/// Zone from the recognised part lines of `label_parts`.
pub fn zone_from_lines(lines: &[String]) -> Option<String> {
    match lines {
        [a, r] => zone_from_parts(a, r),
        [one] => {
            // A colon read inside a single part ("GREENWICH:CAMPUS") still splits it.
            if let Some(z) = parse_zone(one) { return Some(z); }
            // One-word labels only when they are a known name (stray bright map pixels near
            // the strip centre otherwise read as noise words).
            let s = snap(&clean(one));
            // A region alone is the area part lost (clipped / unread).
            if let Some((r, a)) = REGION_AREA.iter().find(|(r, _)| *r == s) { return Some(format!("{a} : {r}")); }
            if VOCAB.contains(&s.as_str()) { return Some(s); }
            // Colon lost and the parts merged ("BROADWAYTIMES SOUAR"): a known area word at the
            // start still names the area; the region is kept only when it is a known name.
            let flat: String = s.chars().filter(|c| *c != ' ').collect();
            PREFIXES.iter().filter(|p| p.len() >= 4).find_map(|p| {
                let n = p.chars().filter(|c| *c != ' ').count();
                (1..=2).chain([0]).map(|e| n + 1 - e).filter(|k| *k <= flat.len()).find(|k| edit(&flat[..*k], &p.replace(' ', "")) <= 1).map(|k| {
                    let rest = snap(&flat[k..]);
                    if VOCAB.contains(&rest.as_str()) { format!("{p} : {rest}") } else { p.to_string() }
                })
            })
        }
        _ => None,
    }
}

/// Area prefixes seen on the label. Lane colours are absolute map lanes, identical for both
/// teams (see `lane_of_prefix`); bases are named after each team's patron.
const PREFIXES: [&str; 6] = ["YORK", "BROADWAY", "GREENWICH", "HIDDEN KING", "ARCHMOTHER", "MID"];

fn edit(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0]; row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (row[j] + 1).min(row[j - 1] + 1).min(prev + usize::from(a[i - 1] != b[j - 1]));
            prev = cur;
        }
    }
    row[b.len()]
}

/// Normalise a recognised label to "PREFIX : SUFFIX" with a known prefix, or None.
pub fn parse_zone(text: &str) -> Option<String> {
    let t: String = text.to_uppercase().chars().map(|c| if c.is_ascii_alphabetic() || c == ':' || c == ' ' || c == '\'' { c } else if c == ';' || c == '.' { ':' } else { ' ' }).collect();
    let (pre, suf) = t.split_once(':')?;
    let pre = pre.split_whitespace().collect::<Vec<_>>().join(" ");
    let suf = suf.replace(':', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    if suf.len() < 2 || pre.len() < 3 { return None; }
    let (best, d) = PREFIXES.iter().map(|p| (*p, edit(&pre, p))).min_by_key(|(_, d)| *d)?;
    // One edit for short prefixes ("MID"), two for the long ones.
    if d > if best.len() <= 4 { 1 } else { 2 } { return None; }
    Some(format!("{best} : {suf}"))
}

/// Label words seen in recorded matches; reads within a few edits snap to them.
const VOCAB: &[&str] = &["YORK", "BROADWAY", "GREENWICH", "HIDDEN KING", "ARCHMOTHER", "MID", "BASE", "DOCKS",
    "TIMES SQUARE", "FACTORY", "CANAL", "CENTRAL PARK", "PIT", "STOCK EXCHANGE", "DOWNTOWN", "UPTOWN", "BROWNSTONES",
    "CAMPUS", "PARK", "CHINATOWN", "HAUNTED LOT", "PLAZA", "THEATER"];

/// One-word labels seen only away from the lane lines.
const JUNGLE: &[&str] = &["CHINATOWN", "HAUNTED LOT", "PLAZA", "THEATER"];

/// Regions seen under exactly one area in rec2-rec5.
const REGION_AREA: &[(&str, &str)] = &[("DOCKS", "YORK"), ("FACTORY", "YORK"), ("STOCK EXCHANGE", "YORK"),
    ("TIMES SQUARE", "BROADWAY"), ("DOWNTOWN", "BROADWAY"), ("UPTOWN", "BROADWAY"),
    ("CENTRAL PARK", "GREENWICH"), ("CAMPUS", "GREENWICH"), ("BROWNSTONES", "GREENWICH"), ("PIT", "MID")];

fn snap(s: &str) -> String {
    // Spaces are not compared: the recognizer merges and splits words ("TIMESSQUARE").
    let flat = |x: &str| x.replace(' ', "");
    let best = VOCAB.iter().map(|v| (*v, edit(&flat(s), &flat(v)))).min_by_key(|(_, d)| *d);
    match best { Some((v, d)) if d <= (v.len() / 4).max(1) => v.to_string(), _ => s.to_string() }
}

fn clean(s: &str) -> String {
    s.to_uppercase().chars().map(|c| if c.is_ascii_alphabetic() { c } else { ' ' }).collect::<String>()
        .split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Zone from the separately read large-capital (area) and small-capital (region) parts.
pub fn zone_from_parts(area: &str, region: &str) -> Option<String> {
    let (a, mut r) = (clean(area), clean(region));
    // No region name starts with a one-letter word: that is a colon dot read as a letter.
    if r.len() > 2 && r.as_bytes()[1] == b' ' { r = r[2..].to_string(); }
    if a.len() < 3 || r.len() < 3 { return None; }
    let (a, r) = (snap(&a), snap(&r));
    // A clipped area word ("VAY : TIMES SQUARE") is restored from its region when that region
    // was only ever seen under one area.
    let a = if VOCAB.contains(&a.as_str()) { a } else { REGION_AREA.iter().find(|(reg, _)| *reg == r).map(|(_, ar)| ar.to_string()).unwrap_or(a) };
    Some(format!("{a} : {r}"))
}

/// Lane of a zone prefix. Measured: rec5 (mirrored layout, Yellow drawn on the left) read
/// "YORK : FACTORY" with the marker on the left (yellow) lane; rec4 / the no-urn sample (normal
/// layout, Yellow on the right) read "YORK : DOCKS" with the marker on the right (yellow) lane.
/// So the names are absolute lanes, like the colours: York = Yellow, Broadway = Blue,
/// Greenwich = Green.
pub fn lane_of_prefix(zone: &str) -> Option<Lane> {
    // Only the area word names a lane: "CANAL : YORK" is the jungle canal on the York side
    // (rec5 read it with the marker 30+ px off every lane line, carrying the urn to mid).
    match zone.split(" : ").next()? { "YORK" => Some(Lane::Yellow), "BROADWAY" => Some(Lane::Blue), "GREENWICH" => Some(Lane::Green), _ => None }
}

/// Coarse area from the label alone (lane areas need the marker to split lane / jungle).
/// Bases: the Hidden King's base belongs to the team whose map is mirrored (Yellow drawn on
/// the left), the Archmother's to the other; with an unknown layout the owner is unknown.
pub fn area_of(zone: &str, mirrored: Option<bool>) -> Option<ZoneArea> {
    // Names seen only with the marker away from every lane line (rec2-rec5): jungle.
    if JUNGLE.contains(&zone) || zone.starts_with("CANAL") { return Some(ZoneArea::Jungle); }
    let (pre, suf) = zone.split_once(" : ")?;
    let base = suf == "BASE" || pre == "HIDDEN KING" || pre == "ARCHMOTHER";
    if base {
        let hidden = match pre { "HIDDEN KING" => true, "ARCHMOTHER" => false, _ => return Some(ZoneArea::Base) };
        return Some(match mirrored { Some(m) if m == hidden => ZoneArea::OwnBase, Some(_) => ZoneArea::EnemyBase, None => ZoneArea::Base });
    }
    if pre == "MID" { return Some(ZoneArea::Mid); }
    lane_of_prefix(zone).map(|_| ZoneArea::Lane)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_labels() {
        assert_eq!(parse_zone("YORK : FACTORY").as_deref(), Some("YORK : FACTORY"));
        assert_eq!(parse_zone("BROADWAY: TIMES SQUARE").as_deref(), Some("BROADWAY : TIMES SQUARE"));
        assert_eq!(parse_zone("GREENWlCH : CENTRAL PARK").as_deref(), Some("GREENWICH : CENTRAL PARK"));
        assert_eq!(parse_zone("HIDDEN KING : BASE").as_deref(), Some("HIDDEN KING : BASE"));
        assert_eq!(parse_zone("MATCH: 126175967"), None);
        assert_eq!(parse_zone("SERVER CPU: 41%"), None);
        assert_eq!(lane_of_prefix("YORK : DOCKS"), Some(Lane::Yellow));
        assert_eq!(area_of("HIDDEN KING : BASE", Some(true)), Some(ZoneArea::OwnBase));
        assert_eq!(area_of("HIDDEN KING : BASE", Some(false)), Some(ZoneArea::EnemyBase));
        assert_eq!(area_of("MID : PIT", None), Some(ZoneArea::Mid));
    }
}
