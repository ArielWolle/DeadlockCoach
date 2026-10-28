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
    // Recorded 1920x1080 matches (rec4, rec5) put the icon centres 6 px lower than the earlier
    // calibration (289.6): cells cut the top of every icon and matched 2 of 132 confidently.
    let y = 295.6 + 28.2 * row as f32;
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
    // Up to a third of the letters may be wrong ("IOET" for VIOLET, "WRATH" for WRAITH on rec5's
    // small hero-name line), as long as no other hero is as close.
    let mut best: Option<(usize, &String)> = None;
    let mut tie = false;
    for (class, name) in names {
        let n: String = name.chars().filter(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
        let d = lev(&t, &n);
        if d > (n.len() / 3).max(1) { continue; }
        match best { Some(b) if d > b.0 => {} Some(b) if d == b.0 => tie = true, _ => { best = Some((d, class)); tie = false; } }
    }
    best.filter(|_| !tie).map(|b| b.1.clone())
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

/// Hero display-name line under the player name ("GREY TALON"), one per column.
pub fn hero_line_region(cx: f32) -> Region { px(cx - 34.0, 127.0, 68.0, 11.0) }
/// Horizontal extent of the K/D/A number line relative to the column centre. Measured on
/// recorded 1920x1080 matches scaled to 1600x900: rec5 K cx-19..-13, D cx-1..+4, A cx+16..+21 on
/// rows ~155..164; rec4 sits ~5 px lower with A at cx+11..+19.
/// The line recognizer misreads these 8 px digits (6 -> "G", 9 -> "1", 0 -> '"' on rec5), so they
/// are matched against glyph templates of the scoreboard font instead.
const KDA_LINE: (i32, i32) = (-30, 32);
const KDA_Y0: u32 = 150;
const KDA_Y1: u32 = 172;
pub const GLYPH_W: u32 = 8;
pub const GLYPH_H: u32 = 12;

/// Digit templates (GLYPH_W x GLYPH_H, ink 0..9), the mean glyph of each digit over the K/D/A
/// numbers of recorded scoreboards (examples/sb_digits.rs).
const DIGIT_TEMPLATES: [&str; 10] = [
    "135676423577776456766786665445776632236775210147752001477531125776422467666556764677777535777653",
    "001443000026640000377400002564000025640000256400002564000025640000256400002564000025640000245300",
    "467766315677775245555764232346641111356400013563000246420124543112566321257764434687766457877765",
    "367776423666676324444775122236750112466301235752014567530123466411113675233457754666665357776431",
    "134321112454333125544552355335524642355346423653564346646765677556667886222246640001355300013542",
    "467776525777665257654432675322216742211156433221466566632334567511123675334457645666664257766421",
    "001356531246775324676542366532215654432166445542665445656642236665311256565434654666666425677652",
    "577777654566667522223564111124530001255200013641000256310014642000256310014652000256410012552100",
    "146776513566666346533574554114644642246346643562356666534543445465211255654223555665556446766652",
    "246666313566675356544675663224666521126665211267455445772333467501124663123467522455553135664210",
];

fn templates() -> &'static Vec<Vec<f32>> {
    static T: std::sync::OnceLock<Vec<Vec<f32>>> = std::sync::OnceLock::new();
    T.get_or_init(|| DIGIT_TEMPLATES.iter().map(|s| s.chars().map(|c| c.to_digit(10).unwrap_or(0) as f32 / 9.0).collect()).collect())
}

/// Ink glyphs of one number (left to right), each a GLYPH_W x GLYPH_H ink map in 0..1 scaled to
/// the glyph height and centred horizontally (so a narrow "1" stays narrow).
fn number_glyphs(luma: &image::GrayImage, cx: f32, window: (i32, i32)) -> Vec<Vec<f32>> {
    let (x0, x1) = ((cx as i32 + window.0).max(0) as u32, ((cx as i32 + window.1).max(0) as u32).min(luma.width()));
    let (y0, y1) = (KDA_Y0.min(luma.height()), KDA_Y1.min(luma.height()));
    if x1 <= x0 + 4 || y1 <= y0 + 4 { return vec![]; }
    let (w, h) = (x1 - x0, y1 - y0);
    let at = |x: u32, y: u32| luma.get_pixel(x0 + x, y0 + y)[0] as f32;
    let mut all: Vec<f32> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| at(x, y)).collect();
    all.sort_by(f32::total_cmp);
    let bg = all[all.len() / 2];
    // Columns lit over (nearly) the whole crop height are panel borders, not digits.
    let border: Vec<bool> = (0..w).map(|x| (0..h).filter(|y| at(x, *y) - bg > 25.0).count() as u32 * 10 >= h * 8).collect();
    let peak = (0..w).filter(|x| !border[*x as usize]).flat_map(|x| (0..h).map(move |y| (x, y))).map(|(x, y)| at(x, y)).fold(bg, f32::max) - bg;
    // Dim D/A digits are ~60 luma over the panel; flat or empty cells stay unread.
    if peak < 30.0 { return vec![]; }
    let ink = |x: u32, y: u32| if border[x as usize] { 0.0 } else { ((at(x, y) - bg) / peak).clamp(0.0, 1.0) };
    let on = |x: u32, y: u32| ink(x, y) > 0.45;
    // Rows of the digit band: the longest run of rows with ink, bridging one faint row (the thin
    // diagonal of a dim "7" drops below the threshold for a row).
    let rows: Vec<bool> = (0..h).map(|y| (0..w).any(|x| on(x, y))).collect();
    let (mut best, mut cur) = ((0u32, 0u32), (0u32, 0u32));
    for y in 0..h as usize {
        let lit = rows[y] || y > 0 && y + 1 < rows.len() && rows[y - 1] && rows[y + 1];
        if lit { if cur.1 == 0 { cur.0 = y as u32; } cur.1 += 1; if cur.1 > best.1 { best = cur; } } else { cur = (0, 0); }
    }
    let (gy0, gh) = best;
    if gh < 5 || gh > 13 { return vec![]; }
    let colsum: Vec<f32> = (0..w).map(|x| (gy0..gy0 + gh).map(|y| ink(x, y)).sum()).collect();
    let mut runs: Vec<(u32, u32)> = vec![];
    let mut start = None;
    for x in 0..=w {
        let ink_col = x < w && (gy0..gy0 + gh).any(|y| on(x, y));
        match (ink_col, start) { (true, None) => start = Some(x), (false, Some(s)) => { runs.push((s, x)); start = None; } _ => {} }
    }
    // Rejoin one digit split by a faint column (the thin middle of a "4" on rec4): the gap still
    // carries soft ink, while the gap between two digits is near empty, and the joined run is
    // no wider than one digit.
    // Two full-height parts are two digits ("11").
    let tall_run = |s: u32, e: u32| (gy0..gy0 + gh).filter(|y| (s..e).any(|x| on(x, *y))).count() as u32 * 10 >= gh * 7;
    let mut joined: Vec<(u32, u32)> = vec![];
    for r in runs {
        if let Some(last) = joined.last_mut() {
            let gap_ink = (last.1..r.0).map(|x| colsum[x as usize]).fold(f32::MAX, f32::min);
            if r.0 - last.1 <= 1 && gap_ink >= 1.0 && r.1 - last.0 <= gh * 8 / 10 && !(tall_run(last.0, last.1) && tall_run(r.0, r.1)) { last.1 = r.1; continue; }
        }
        joined.push(r);
    }
    let runs = joined;
    // Two digits touching after scaling: split a too-wide run at its weakest column.
    let mut split = vec![];
    for (s, e) in runs {
        if e - s >= gh * 9 / 10 + 2 {
            let m = (s + 2..e - 2).min_by(|a, b| colsum[*a as usize].total_cmp(&colsum[*b as usize])).unwrap_or((s + e) / 2);
            split.push((s, m)); split.push((m + 1, e));
        } else { split.push((s, e)); }
    }
    // Drop specks and runs cut by the crop edge that are not centred (neighbouring separators).
    // A tall run cut by the crop edge is a partial glyph: the number is not trustworthy.
    let tall_at = |s: u32, e: u32| (gy0..gy0 + gh).filter(|y| (s..e).any(|x| on(x, *y))).count() as u32 * 10 >= gh * 6;
    // Faint thin runs at the window edge are the "/" separators.
    let peak_in = |s: u32, e: u32| (s..e).flat_map(|x| (gy0..gy0 + gh).map(move |y| (x, y))).map(|(x, y)| ink(x, y)).fold(0.0, f32::max);
    split.retain(|(s, e)| !((*s == 0 || *e == w) && e - s <= 2 && peak_in(*s, *e) < 0.75));
    if split.iter().any(|(s, e)| (*s == 0 || *e == w) && tall_at(*s, *e)) { return vec![]; }
    // A short but inky fragment right beside a digit is part of it (the faint diagonal of a
    // blurred "4" left a "1"-shaped stem on rec4): unreadable rather than a wrong digit.
    let fragment = |s: u32, e: u32| !tall_at(s, e) && colsum[s as usize..e as usize].iter().sum::<f32>() >= 2.0;
    if split.iter().enumerate().any(|(i, (s, e))| fragment(*s, *e) && split.iter().enumerate().any(|(j, (s2, e2))| j != i && (s2.abs_diff(*e) <= 2 || s.abs_diff(*e2) <= 2))) { return vec![]; }
    split.retain(|(s, e)| { let tall = (gy0..gy0 + gh).filter(|y| (*s..*e).any(|x| on(x, *y))).count() as u32; tall * 10 >= gh * 6 && colsum[*s as usize..*e as usize].iter().sum::<f32>() >= 3.0 });
    split.iter().map(|(s, e)| {
        let scale = gh as f32 / GLYPH_H as f32;
        let mid = (*s + *e) as f32 / 2.0;
        (0..GLYPH_H).flat_map(|gy| (0..GLYPH_W).map(move |gx| (gx, gy))).map(|(gx, gy)| {
            let sx = mid + (gx as f32 + 0.5 - GLYPH_W as f32 / 2.0) * scale - 0.5;
            let sy = gy0 as f32 + (gy as f32 + 0.5) * scale - 0.5;
            if sx < *s as f32 - 0.5 || sx > *e as f32 - 0.5 { return 0.0; }
            // Bilinear sample of the ink map.
            let (fx, fy) = (sx.floor(), sy.floor());
            let (ax, ay) = (sx - fx, sy - fy);
            let get = |x: f32, y: f32| if x < 0.0 || y < 0.0 || x >= w as f32 || y >= h as f32 { 0.0 } else { ink(x as u32, y as u32) };
            get(fx, fy) * (1.0 - ax) * (1.0 - ay) + get(fx + 1.0, fy) * ax * (1.0 - ay) + get(fx, fy + 1.0) * (1.0 - ax) * ay + get(fx + 1.0, fy + 1.0) * ax * ay
        // Last element: run width / glyph height (a "1" is narrow, every other digit wide).
        }).chain([(*e - *s) as f32 / gh as f32]).collect()
    }).collect()
}

/// The three number windows (K, D, A) of one column, found from the ink layout of the whole
/// K/D/A line: positions differ by several pixels between recordings (rec4 A "18" at cx+11..19,
/// rec5 A at cx+16..21), so fixed windows cut digits. None unless exactly three groups are seen.
fn kda_windows(luma: &image::GrayImage, cx: f32) -> Option<[(i32, i32); 3]> {
    let (lo, hi) = (KDA_LINE.0, KDA_LINE.1);
    let x0 = (cx as i32 + lo).max(0) as u32;
    let x1 = ((cx as i32 + hi).max(0) as u32).min(luma.width());
    let (y0, y1) = (KDA_Y0.min(luma.height()), KDA_Y1.min(luma.height()));
    if x1 <= x0 + 10 || y1 <= y0 + 6 { return None; }
    let (w, h) = (x1 - x0, y1 - y0);
    let at = |x: u32, y: u32| luma.get_pixel(x0 + x, y0 + y)[0] as f32;
    let mut all: Vec<f32> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| at(x, y)).collect();
    all.sort_by(f32::total_cmp);
    let bg = all[all.len() / 2];
    let border: Vec<bool> = (0..w).map(|x| (0..h).filter(|y| at(x, *y) - bg > 25.0).count() as u32 * 10 >= h * 8).collect();
    // Absolute contrast: the white K digit is ~170 over the panel, grey D/A digits only ~60.
    let lit = |x: u32, y: u32| !border[x as usize] && at(x, y) - bg > 28.0;
    let rows: Vec<bool> = (0..h).map(|y| (0..w).filter(|x| lit(*x, y)).count() >= 2).collect();
    let (mut best, mut cur) = ((0u32, 0u32), (0u32, 0u32));
    for y in 0..h as usize {
        let on = rows[y] || y > 0 && y + 1 < rows.len() && rows[y - 1] && rows[y + 1];
        if on { if cur.1 == 0 { cur.0 = y as u32; } cur.1 += 1; if cur.1 > best.1 { best = cur; } } else { cur = (0, 0); }
    }
    let (gy0, gh) = best;
    if !(5..=13).contains(&gh) { return None; }
    let mass: Vec<u32> = (0..w).map(|x| (gy0..gy0 + gh).filter(|y| lit(x, *y)).count() as u32).collect();
    // Groups: lit columns joined across gaps of up to 2 px (digits of one number sit 1-2 px apart;
    // numbers are >= 5 px apart). Specks (stray separator pixels) are dropped.
    let mut groups: Vec<(u32, u32, u32)> = vec![];
    for x in 0..w {
        if mass[x as usize] == 0 { continue; }
        match groups.last_mut() { Some(g) if x <= g.1 + 2 => { g.1 = x + 1; g.2 += mass[x as usize]; } _ => groups.push((x, x + 1, mass[x as usize])) }
    }
    groups.retain(|g| g.2 >= gh && g.1 - g.0 >= 2 || g.2 >= gh * 2);
    if groups.len() != 3 { return None; }
    let win = |g: &(u32, u32, u32)| (x0 as i32 - cx as i32 + g.0 as i32 - 2, x0 as i32 - cx as i32 + g.1 as i32 + 2);
    Some([win(&groups[0]), win(&groups[1]), win(&groups[2])])
}

/// Glyphs of the 36 K/D/A numbers (12 columns x K, D, A) on a 1600x900 frame. A column whose
/// line layout is unclear yields no glyphs (unread, never guessed).
pub fn kda_glyphs(img: &DynamicImage) -> Vec<Vec<Vec<f32>>> {
    let band = img.crop_imm(0, 0, img.width().min(1600), img.height().min(KDA_Y1 + 2)).to_luma8();
    hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).flat_map(|cx| match kda_windows(&band, cx) {
        Some(ws) => ws.map(|w| number_glyphs(&band, cx, w)),
        None => [vec![], vec![], vec![]],
    }).collect()
}

fn ncc(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len()) as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut s, mut sa, mut sb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) { s += (x - ma) * (y - mb); sa += (x - ma).powi(2); sb += (y - mb).powi(2); }
    if sa <= 0.0 || sb <= 0.0 { 0.0 } else { s / (sa * sb).sqrt() }
}

/// Best digit for one glyph: (digit, score, margin over the runner-up). The glyph is the
/// GLYPH_W x GLYPH_H ink map, optionally followed by its width ratio.
pub fn classify_glyph(g: &[f32]) -> Option<(u32, f32, f32)> {
    let g = &g[..g.len().min((GLYPH_W * GLYPH_H) as usize)];
    let mut s: Vec<(u32, f32)> = templates().iter().enumerate().filter(|(_, t)| t.len() == g.len()).map(|(d, t)| (d as u32, ncc(g, t))).collect();
    s.sort_by(|a, b| b.1.total_cmp(&a.1));
    let best = *s.first()?;
    Some((best.0, best.1, best.1 - s.get(1).map(|x| x.1).unwrap_or(0.0)))
}

/// One K/D/A number from its glyphs; None unless every digit matches its template clearly.
pub fn classify_number(glyphs: &[Vec<f32>]) -> Option<u32> {
    if glyphs.is_empty() || glyphs.len() > 2 { return None; }
    let mut v = 0;
    for g in glyphs {
        let (d, score, margin) = classify_glyph(g)?;
        // rec4 through the scanner's 1915x1077 context read a faded "2" as "7" (0.77, margin
        // 0.07): a near tie stays unread.
        if !(score >= 0.75 && margin >= 0.1 || score >= 0.6 && margin >= 0.2) { return None; }
        // A blurred "4" whose faint diagonal fades matches "1" by shape (rec4); the width tells.
        if let Some(ratio) = g.get((GLYPH_W * GLYPH_H) as usize) { if (d == 1) != (*ratio < 0.5) { return None; } }
        v = v * 10 + d;
    }
    // A leading zero is not a scoreboard number.
    (glyphs.len() == 1 || v >= 10).then_some(v)
}

/// K/D/A per column (12 rows, own team first) read from the scoreboard number row.
pub fn read_kda(img: &DynamicImage) -> Vec<[Option<u32>; 3]> {
    kda_glyphs(img).chunks(3).map(|c| [classify_number(&c[0]), classify_number(&c[1]), classify_number(&c[2])]).collect()
}

/// Hero-name line crops for all twelve columns (cheap line recognition, no detector).
pub fn hero_line_crops(img: &DynamicImage) -> Vec<DynamicImage> {
    hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).map(|cx| hud::crop(img, hero_line_region(cx))).collect()
}

/// Read all 12 columns. `hero_names` = (class_name, display name).
pub fn read(rd: &Reader, items: &ColorLibrary, hero_names: &[(String, String)], img: &DynamicImage) -> Vec<PlayerRow> {
    read_with_rects(rd, items, hero_names, img).0
}
/// Like [`read`], also returning each slot's exact name rectangle for cheap later checks.
pub fn read_with_rects(rd: &Reader, items: &ColorLibrary, hero_names: &[(String, String)], img: &DynamicImage) -> (Vec<PlayerRow>, Vec<Option<coach_perception::Bounds>>) {
    read_board(rd, items, hero_names, img, true)
}

/// Hero-name line crops for a subset of columns.
pub fn hero_line_crops_for(img: &DynamicImage, slots: &[usize]) -> Vec<DynamicImage> {
    let centers: Vec<f32> = hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).collect();
    slots.iter().map(|i| hud::crop(img, hero_line_region(centers[*i]))).collect()
}

/// Full scoreboard read: per column the hero (cheap hero-name line, e.g. "GREY TALON"; on rec5
/// this matched 12/12 columns where the detector name pass matched 1-6), K/D/A (glyph templates)
/// and items. `acquire` also runs the detector name pass, which returns exact name rectangles
/// for the screen probe and fills heroes the line read missed.
pub fn read_board(rd: &Reader, items: &ColorLibrary, hero_names: &[(String, String)], img: &DynamicImage, acquire: bool) -> (Vec<PlayerRow>, Vec<Option<coach_perception::Bounds>>) {
    let centers: Vec<f32> = hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).collect();
    let lines = rd.read_lines(&hero_line_crops(img)).unwrap_or_default();
    let mut identified: Vec<(Option<String>, Option<coach_perception::Bounds>)> = (0..12).map(|i| (lines.get(i).and_then(|l| match_hero(l, hero_names)), None)).collect();
    if acquire {
        let mut detected = names(rd, img, &centers, false, hero_names);
        if detected.iter().any(|h| h.0.is_none()) { let alternative = names(rd, img, &centers, true, hero_names); for (i, h) in detected.iter_mut().enumerate() { if h.0.is_none() { *h = alternative[i].clone(); } } }
        for (mine, det) in identified.iter_mut().zip(detected) { if mine.0.is_none() { mine.0 = det.0; } mine.1 = det.1; }
    }
    let kda = read_kda(img);
    let mut rows = vec![];
    for (i, cx) in centers.iter().enumerate() {
        let k = kda.get(i).copied().unwrap_or_default();
        let mut row = PlayerRow { ally: i < 6, hero: identified[i].0.clone(), kills: k[0], deaths: k[1], assists: k[2], ..Default::default() };
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
