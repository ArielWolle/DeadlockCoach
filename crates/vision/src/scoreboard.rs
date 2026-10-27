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
/// Pixel-only scoreboard cue (no OCR, well under 1 ms): with Tab held, uniform blue panels sit
/// under ally portraits and olive panels under enemy portraits. Measured on saved frames: the
/// scoreboard matched 5/5 ally and 6/6 enemy columns; gameplay/death/shop/menu frames 0/5 ally.
/// The local player's column (slot 0) is skipped because its panel differs.
pub fn panel_cue(img: &DynamicImage) -> bool {
    let column = |cx: f32| {
        let (fw, fh) = (img.width() as f32 / 1600.0, img.height() as f32 / 900.0);
        let (x0, x1, y0, y1) = (((cx - 30.0) * fw) as u32, ((cx + 30.0) * fw) as u32, (232.0 * fh) as u32, (242.0 * fh) as u32);
        let (mut s, mut s2, mut n) = ([0f32; 3], 0f32, 0f32);
        let rgb = img.as_rgb8();
        for y in y0..y1.min(img.height()) { for x in x0..x1.min(img.width()) {
            let p = match rgb { Some(i) => i.get_pixel(x, y).0, None => { use image::GenericImageView; let p = img.get_pixel(x, y); [p[0], p[1], p[2]] } };
            for c in 0..3 { s[c] += p[c] as f32; }
            let l = p[0] as f32 * 0.3 + p[1] as f32 * 0.59 + p[2] as f32 * 0.11; s2 += l * l; n += 1.0;
        }}
        if n == 0.0 { return (0.0, f32::MAX); }
        let lum = s[0] / n * 0.3 + s[1] / n * 0.59 + s[2] / n * 0.11;
        ((s[2] - s[0]) / n, (s2 / n - lum * lum).max(0.0).sqrt())
    };
    let allies = hud::ALLY_CENTERS[1..].iter().filter(|cx| { let (bias, sd) = column(**cx); bias >= 12.0 && sd <= 9.0 }).count();
    let enemies = hud::ENEMY_CENTERS.iter().filter(|cx| { let (bias, sd) = column(**cx); bias <= -3.0 && sd <= 9.0 }).count();
    allies >= 4 && enemies >= 4
}

pub fn looks_like_scoreboard(rd: &Reader, img: &DynamicImage, hero_names: &[(String, String)]) -> bool {
    names(rd,img,&crate::screen::scoreboard_probe(),false,hero_names).iter().filter(|h|h.0.is_some()).count()>=3
}
/// Detector-based acquisition of the name labels: stacked into one image, one detection pass.
/// Returns each slot's hero and its exact text rectangle in frame coordinates, which callers
/// cache so later frames recognize only that rectangle (names do not change within a match).
fn names(rd:&Reader,img:&DynamicImage,centers:&[f32],big:bool,heroes:&[(String,String)])->Vec<(Option<String>,Option<coach_perception::Bounds>)>{
    let region=|cx:f32|if big{big_name_region(cx)}else{name_region(cx)};
    let mut bank=image::RgbImage::new(96,36*centers.len() as u32);
    for(i,cx)in centers.iter().enumerate(){let cell=hud::crop(img,region(*cx)).resize_exact(72,16,image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut bank,&cell,12,(i*36+10) as i64);}
    let mut result=vec![(None,None);centers.len()];
    if let Ok(lines)=rd.read_spans(&DynamicImage::ImageRgb8(bank),3){for line in lines{
        let row=((line.bounds.y+line.bounds.h/2)/36) as usize;
        let Some(slot)=result.get_mut(row) else{continue};
        if slot.0.is_some(){continue;}
        let Some(hero)=match_hero(&line.label,heroes) else{continue};
        let r=region(centers[row]);let (fw,fh)=(img.width() as f32,img.height() as f32);
        let (sx,sy)=(r.w*fw/72.,r.h*fh/16.);let b=line.bounds;
        let x=(r.x*fw+(b.x as f32-12.).max(0.)*sx) as u32;let y=(r.y*fh+(b.y as f32-(row*36+10) as f32).max(0.)*sy) as u32;
        *slot=(Some(hero),Some(coach_perception::Bounds{x,y,w:((b.w as f32*sx).ceil() as u32).max(1),h:((b.h as f32*sy).ceil() as u32).max(1)}));
    }}
    result
}
/// Cheap per-frame name check using cached exact rectangles (see [`read_with_rects`]).
pub fn name_crops(img:&DynamicImage,rects:&[coach_perception::Bounds])->Vec<DynamicImage>{
    rects.iter().map(|b|img.crop_imm(b.x.min(img.width()-1),b.y.min(img.height()-1),b.w.min(img.width()-b.x.min(img.width()-1)).max(1),b.h.min(img.height()-b.y.min(img.height()-1)).max(1))).collect()
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
    read_with_rects(rd, items, hero_names, img).0
}
/// Like [`read`], also returning each slot's exact name rectangle for cheap later checks.
pub fn read_with_rects(rd: &Reader, items: &ColorLibrary, hero_names: &[(String, String)], img: &DynamicImage) -> (Vec<PlayerRow>, Vec<Option<coach_perception::Bounds>>) {
    let mut rows = vec![];
    let centers:Vec<_>=hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).collect();
    let mut identified=names(rd,img,&centers,false,hero_names);
    if identified.iter().any(|h|h.0.is_none()){let alternative=names(rd,img,&centers,true,hero_names);for(i,h)in identified.iter_mut().enumerate(){if h.0.is_none(){*h=alternative[i].clone();}}}
    for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
        let mut row = PlayerRow { ally: i < 6,hero:identified[i].0.clone(), ..Default::default() };
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
    (rows, identified.into_iter().map(|h| h.1).collect())
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
