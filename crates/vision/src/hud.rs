//! HUD layout (fractions of the frame, measured on 16:9 captures) and readers.
use crate::{Library, Region};
use image::DynamicImage;

const W: f32 = 1600.0;
const H: f32 = 900.0;

fn px(x: f32, y: f32, w: f32, h: f32) -> Region {
    Region { x: x / W, y: y / H, w: w / W, h: h / H }
}

pub const ALLY_CENTERS: [f32; 6] = [310.0, 383.0, 457.0, 530.0, 603.0, 677.0];
pub const ENEMY_CENTERS: [f32; 6] = [920.0, 994.0, 1067.0, 1140.0, 1213.0, 1286.0];

pub fn portrait(cx: f32) -> Region { px(cx - 26.0, 8.0, 52.0, 52.0) }
pub fn networth_label(cx: f32) -> Region { px(cx - 24.0, 80.0, 48.0, 18.0) }
pub const CLOCK: Region = Region { x: 0.47, y: 0.01, w: 0.06, h: 0.035 };
pub const SOULS: Region = Region { x: 0.04, y: 0.80, w: 0.09, h: 0.06 };
pub const ALLY_TOTAL: Region = Region { x: 728.0 / W, y: 40.0 / H, w: 44.0 / W, h: 20.0 / H };
pub const ENEMY_TOTAL: Region = Region { x: 832.0 / W, y: 40.0 / H, w: 44.0 / W, h: 20.0 / H };

/// 12 inventory slots: (col, row) -> region. Columns 0..6, rows 0..2.
pub fn item_slot(col: usize, row: usize) -> Region {
    let cx = 44.0 + col as f32 * 51.0;
    let cy = 810.0 + row as f32 * 50.0;
    px(cx - 20.0, cy - 20.0, 40.0, 40.0)
}

pub fn crop(img: &DynamicImage, r: Region) -> DynamicImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    img.crop_imm((r.x * w) as u32, (r.y * h) as u32, ((r.w * w) as u32).max(1), ((r.h * h) as u32).max(1))
}

/// Standard deviation of luma: near 0 means an empty / flat slot.
pub fn contrast(img: &DynamicImage) -> f32 {
    let g = img.to_luma8();
    let n = (g.width() * g.height()) as f32;
    let m = g.pixels().map(|p| p[0] as f32).sum::<f32>() / n;
    (g.pixels().map(|p| (p[0] as f32 - m).powi(2)).sum::<f32>() / n).sqrt()
}

/// Identify an icon in a region; None if too flat or low confidence.
pub fn identify(img: &DynamicImage, r: Region, lib: &Library, min_score: f32, min_margin: f32) -> Option<String> {
    let c = crop(img, r);
    if contrast(&c) < 18.0 { return None; }
    let (name, score, margin) = lib.identify(&c.to_luma8())?;
    (score >= min_score && margin >= min_margin).then_some(name)
}

/// "2.6k" / "21k" / "950" -> souls. Tolerates OCR noise after the number ("2.6h", "3.71").
pub fn parse_networth(s: &str) -> Option<u32> {
    let s = s.trim();
    let mut int = String::new();
    let mut frac: Option<char> = None;
    let mut seen_dot = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if seen_dot { frac = Some(c); break; } else { int.push(c); }
        } else if c == '.' && !int.is_empty() && !seen_dot { seen_dot = true; }
        else if !int.is_empty() { break; }
    }
    let n: u32 = int.parse().ok()?;
    let has_k = s.chars().any(|c| c == 'k' || c == 'K');
    if seen_dot || has_k {
        Some(n * 1000 + frac.and_then(|c| c.to_digit(10)).unwrap_or(0) * 100)
    } else { Some(n) }
}
/// Team totals are shown as "21k"; the small "k" is often lost, so bare numbers < 1000 are thousands.
pub fn parse_team_total(s: &str) -> Option<u32> {
    let v = parse_networth(s)?;
    Some(if v < 1000 { v * 1000 } else { v })
}

/// Possible readings of a team total such as "21k", "9.2k" or the noisy "231" (= "23k" with the
/// small "k" read as 1). The caller picks the candidate closest to the previous value.
pub fn team_total_candidates(s: &str) -> Vec<u32> {
    let s = s.trim();
    let mut c = vec![];
    if let Some(v) = parse_networth(s) { if s.contains('.') { c.push(v); } }
    let digits: String = s.chars().take_while(|ch| !ch.is_alphabetic() || *ch == 'k').filter(|ch| ch.is_ascii_digit()).collect();
    if !s.contains('.') && !digits.is_empty() && digits.len() <= 3 {
        c.push(digits.parse::<u32>().unwrap_or(0) * 1000);
        if digits.len() == 3 {
            c.push(digits[..2].parse::<u32>().unwrap_or(0) * 1000);
            c.push(digits[..1].parse::<u32>().unwrap_or(0) * 1000 + digits[1..2].parse::<u32>().unwrap_or(0) * 100);
        }
    }
    c.retain(|v| *v >= 500 && *v <= 400_000);
    c
}

/// Plausible team net worth at `clock` seconds. Recorded matches ran 70-120 souls/s per team
/// (rec5: 86k at 17:54, 184k at 33:45, 312k at 44:24); the band is ~3x wider on both sides, so it
/// only removes the wrong decimal interpretations ("184" as 18k at 33:45, "231" as 231k at 5:00).
pub fn team_total_range(clock: u32) -> (u32, u32) { ((clock * 30).saturating_sub(3000).max(500), clock * 220 + 15000) }

/// [`team_total_candidates`] restricted to [`team_total_range`] when the clock is known.
pub fn team_total_candidates_at(s: &str, clock: Option<u32>) -> Vec<u32> {
    let mut c = team_total_candidates(s);
    if let Some(t) = clock { let (lo, hi) = team_total_range(t); c.retain(|v| *v >= lo && *v <= hi); }
    c
}

/// Choose the candidate nearest `prev`; without history only an unambiguous reading is taken.
pub fn pick_total(cands: &[u32], prev: Option<u32>) -> Option<u32> {
    match (prev, cands) {
        (Some(p), c) if !c.is_empty() => c.iter().copied().min_by_key(|v| v.abs_diff(p)),
        (None, [only]) => Some(*only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn networth() {
        assert_eq!(parse_networth("2.6k"), Some(2600));
        assert_eq!(parse_networth("21k"), Some(21000));
        assert_eq!(parse_networth("950"), Some(950));
        assert_eq!(parse_networth("x"), None);
        assert_eq!(parse_networth("2.6h"), Some(2600));
        assert_eq!(parse_networth("3.71"), Some(3700));
        let c = team_total_candidates("231");
        assert_eq!(pick_total(&c, Some(22000)), Some(23000));
        assert_eq!(pick_total(&team_total_candidates("921"), Some(9000)), Some(9200));
        assert_eq!(pick_total(&team_total_candidates("21k"), None), Some(21000));
    }
}
