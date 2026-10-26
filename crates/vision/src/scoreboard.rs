//! Tab scoreboard reader. Columns line up with the top-bar portraits (hud::ALLY_CENTERS /
//! ENEMY_CENTERS); each shows hero name, K/D/A, health and the player's items (2 per row).
use crate::{hud, ocr::Reader, ColorLibrary, Region};
use coach_core::PlayerRow;
use image::DynamicImage;

fn px(x: f32, y: f32, w: f32, h: f32) -> Region { Region { x: x / 1600.0, y: y / 900.0, w: w / 1600.0, h: h / 900.0 } }

pub fn name_region(cx: f32) -> Region { px(cx - 36.0, 120.0, 72.0, 14.0) }
pub fn big_name_region(cx: f32) -> Region { px(cx - 36.0, 104.0, 72.0, 16.0) }

pub const ITEM_ROWS: usize = 6;

/// Centre of item cell (col 0/1, row 0..ITEM_ROWS) in 1600x900 space.
pub fn item_cell(cx: f32, col: usize, row: usize) -> Region {
    let x = if col == 0 { cx - 12.0 } else { cx + 16.5 };
    let y = 289.6 + 28.2 * row as f32;
    px(x - 10.0, y - 10.0, 20.0, 20.0)
}

/// Quick check used by the screen classifier: do the probe columns show hero names?
/// (K/D/A digits are too small for the OCR model, but names read reliably.)
pub fn looks_like_scoreboard(rd: &Reader, img: &DynamicImage, hero_names: &[(String, String)]) -> bool {
    let mut matches=0;
    for (i,cx) in [hud::ALLY_CENTERS[1],hud::ALLY_CENTERS[3],hud::ENEMY_CENTERS[1],hud::ENEMY_CENTERS[4]].iter().enumerate(){
        if rd.read(&hud::crop(img,name_region(*cx))).ok().and_then(|s|match_hero(&s,hero_names)).is_some(){matches+=1;}
        if matches>=3{return true;}if matches+3-i<3{return false;}
    }false
}
/// Normalised Levenshtein-ish match of OCR text against hero display names.
pub fn match_hero(text: &str, names: &[(String, String)]) -> Option<String> {
    let t: String = text.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
    if t.len() < 3 { return None; }
    let mut best: Option<(usize, &String)> = None;
    for (class, name) in names {
        let n: String = name.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
        let d = lev(&t, &n);
        if d <= (n.len() / 4).max(1) && best.map(|b| d < b.0).unwrap_or(true) { best = Some((d, class)); }
    }
    best.map(|b| b.1.clone())
}

fn lev(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + (a[i - 1] != b[j - 1]) as usize);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Read all 12 columns. `hero_names` = (class_name, display name).
pub fn read(rd: &Reader, items: &ColorLibrary, hero_names: &[(String, String)], img: &DynamicImage) -> Vec<PlayerRow> {
    let mut rows = vec![];
    for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
        let mut row = PlayerRow { ally: i < 6, ..Default::default() };
        for r in [name_region(*cx), big_name_region(*cx)] {
            if row.hero.is_none() {
                row.hero = rd.read(&hud::crop(img, r)).ok().and_then(|s| match_hero(&s, hero_names));
            }
        }
        for r in 0..ITEM_ROWS {
            for c in 0..2 {
                let cell = hud::crop(img, item_cell(*cx, c, r));
                if hud::contrast(&cell) < 14.0 { continue; }
                if let Some((name, score, margin)) = items.identify(&cell) {
                    if score >= 0.72 && margin >= 0.05 && !name.contains("non_player") { row.items.push(name); }
                }
            }
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fuzzy_hero() {
        let n = vec![("hero_a".to_string(), "Nurse Harrow".to_string()), ("hero_b".to_string(), "Viscous".to_string())];
        assert_eq!(match_hero("NURSE HARROW", &n).as_deref(), Some("hero_a"));
        assert_eq!(match_hero("Viscou5", &n).as_deref(), Some("hero_b"));
        assert_eq!(match_hero("xx", &n), None);
    }
}
