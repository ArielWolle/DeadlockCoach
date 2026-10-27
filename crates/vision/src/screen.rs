//! Screen classifier: decides which game screen a frame shows, from cheap text anchors.
//! All anchors are fixed single lines, recognized in one batch (no text-detection pass).
//! Unknown screens must produce no coaching cues.
use crate::{hud, ocr::Reader, parse, Region};
use coach_perception::track::LineBatch;
use image::DynamicImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Screen {
    /// Normal play: top bar + clock visible.
    Gameplay,
    /// Tab scoreboard held open (also has the normal clock).
    Scoreboard,
    /// Dead / spectating with the "Respawn in" banner.
    Dead,
    /// Shop open ("ESC BACK" anchor).
    Shop,
    /// Pause / Esc menu ("ESC RESUME" anchor).
    Menu,
    /// Pre-game "GAME STARTING" countdown.
    Countdown,
    /// Anything not recognised yet. Say nothing.
    Unknown,
}

impl Screen {
    /// Screens on which the top bar and clock are read.
    pub fn has_hud(self) -> bool { matches!(self, Screen::Gameplay | Screen::Scoreboard | Screen::Dead) }
}

fn px(x: f32, y: f32, w: f32, h: f32) -> Region { Region { x: x / 1600.0, y: y / 900.0, w: w / 1600.0, h: h / 900.0 } }

pub fn back_anchor() -> Region { px(10.0, 860.0, 130.0, 32.0) }
pub fn respawn_anchor() -> Region { px(675.0, 665.0, 170.0, 40.0) }
pub fn respawn_text_line() -> Region { px(680.0, 670.0, 150.0, 30.0) }
pub fn respawn_number() -> Region { px(845.0, 665.0, 80.0, 40.0) }
pub fn match_id_region() -> Region { px(1440.0, 868.0, 160.0, 16.0) }
pub fn countdown_line() -> Region { px(620.0, 490.0, 360.0, 45.0) }
/// Overlapping clock boxes. Each alone misreads some layouts (1- vs 2-digit minutes,
/// spectator top bar); their majority vote was correct on every saved fixture.
pub fn clock_lines() -> [Region; 4] { [px(775.0, 12.0, 50.0, 16.0), px(770.0, 16.0, 60.0, 20.0), px(765.0, 14.0, 70.0, 22.0), px(770.0, 12.0, 60.0, 18.0)] }
/// Scoreboard probe columns (names read reliably even when K/D/A digits do not).
pub fn scoreboard_probe() -> [f32; 4] { [hud::ALLY_CENTERS[1], hud::ALLY_CENTERS[3], hud::ENEMY_CENTERS[1], hud::ENEMY_CENTERS[4]] }

/// Number of crops produced by [`probe_crops`].
pub const PROBE_LEN: usize = 11;
/// Slot indices (0–11) of the scoreboard probe columns.
pub const PROBE_SLOTS: [usize; 4] = [1, 3, 7, 10];
/// Crops: back, respawn, countdown, 4 clock candidates, 4 scoreboard names. Names only
/// recognize reliably on their exact text rectangles; pass those once known (cached from a
/// detector-read scoreboard). Without them the name crops are placeholders.
pub fn probe_crops(img: &DynamicImage, name_rects: Option<&[coach_perception::Bounds]>) -> Vec<DynamicImage> {
    let mut crops = vec![hud::crop(img, back_anchor()), hud::crop(img, respawn_text_line()), hud::crop(img, countdown_line())];
    crops.extend(clock_lines().into_iter().map(|r| hud::crop(img, r)));
    match name_rects {
        Some(rects) if rects.len() == 4 => crops.extend(crate::scoreboard::name_crops(img, rects)),
        _ => crops.extend(scoreboard_probe().into_iter().map(|cx| hud::crop(img, crate::scoreboard::name_region(cx)))),
    }
    crops
}

/// Indices of the screen-probe lines inside a [`LineBatch`]. Each line costs real
/// recognition time (~proportional to its width), so callers include only useful lines.
pub struct Probe { back: usize, respawn: usize, countdown: Option<usize>, clocks: std::ops::Range<usize>, names: Option<std::ops::Range<usize>> }
/// Add probe lines. `names`: cached exact rectangles (omitted until known, since fixed boxes
/// never recognize). `countdown`: only before the HUD is seen. `anchored`: with a verified
/// clock two candidates suffice to confirm the extrapolation.
pub fn add_probe(batch: &mut LineBatch, img: &DynamicImage, names: Option<&[coach_perception::Bounds]>, countdown: bool, anchored: bool) -> Probe {
    let back = batch.add(hud::crop(img, back_anchor()));
    let respawn = batch.add(hud::crop(img, respawn_text_line()));
    let countdown = countdown.then(|| batch.add(hud::crop(img, countdown_line())));
    let boxes = clock_lines();
    let clocks = if anchored { batch.add_all([hud::crop(img, boxes[0]), hud::crop(img, boxes[2])]) } else { batch.add_all(boxes.into_iter().map(|r| hud::crop(img, r))) };
    let names = names.filter(|r| r.len() == 4).map(|r| batch.add_all(crate::scoreboard::name_crops(img, r)));
    Probe { back, respawn, countdown, clocks, names }
}
/// Decide the screen from batched probe lines.
pub fn interpret_probe(lines: &[String], p: &Probe, hero_names: &[(String, String)], known_clock: Option<u32>, expected: Option<u32>, scoreboard_fallback: impl FnOnce() -> bool) -> (Screen, Option<u32>) {
    let line = |i: usize| lines.get(i).map(String::as_str).unwrap_or("");
    if has(line(p.back), "RESUME") { return (Screen::Menu, None); }
    if has(line(p.back), "BACK") { return (Screen::Shop, None); }
    let clock = known_clock.or_else(|| vote_clock(lines.get(p.clocks.clone()).unwrap_or(&[]), expected));
    if clock.is_some() {
        let names = p.names.clone().map_or(0, |r| r.filter(|i| crate::scoreboard::match_hero(line(*i), hero_names).is_some()).count());
        if names >= 3 || scoreboard_fallback() { return (Screen::Scoreboard, clock); }
        if has(line(p.respawn), "RESPAWN") { return (Screen::Dead, clock); }
        return (Screen::Gameplay, clock);
    }
    if p.countdown.is_some_and(|i| has(line(i), "STARTING") || has(line(i), "GAME")) { return (Screen::Countdown, None); }
    (Screen::Unknown, None)
}

/// Whether the respawn banner line reads "RESPAWN" (independent of the clock read).
pub fn probe_says_respawn(lines: &[String], p: &Probe) -> bool { lines.get(p.respawn).is_some_and(|s| has(s, "RESPAWN")) }

/// Majority vote over clock candidates; only readings with a separator are considered.
pub fn vote_clock(lines: &[String], expected: Option<u32>) -> Option<u32> {
    let parsed: Vec<Option<u32>> = lines.iter().map(|s| Some(s.trim()).filter(|s| s.contains(':')).and_then(parse::parse_clock).filter(|v| *v <= 14400)).collect();
    coach_perception::track::vote(&parsed, |v| expected.map_or(0, |e| v.abs_diff(e) as u64))
}

fn has(s: &str, needle: &str) -> bool { s.to_ascii_uppercase().contains(needle) }

/// Decide the screen from recognized probe lines (in [`probe_crops`] order).
pub fn interpret(lines: &[String], hero_names: &[(String, String)], known_clock: Option<u32>) -> (Screen, Option<u32>) {
    interpret_with(lines, hero_names, known_clock, || false)
}
/// Like [`interpret`]; `scoreboard_fallback` runs only when the clock is visible but the cheap
/// name lines did not identify a scoreboard (e.g. exact name rectangles not yet acquired).
pub fn interpret_with(lines: &[String], hero_names: &[(String, String)], known_clock: Option<u32>, scoreboard_fallback: impl FnOnce() -> bool) -> (Screen, Option<u32>) {
    let line = |i: usize| lines.get(i).map(String::as_str).unwrap_or("");
    let back = line(0);
    if has(back, "RESUME") { return (Screen::Menu, None); }
    if has(back, "BACK") { return (Screen::Shop, None); }
    let clock = known_clock.or_else(|| vote_clock(lines.get(3..7).unwrap_or(&[]), None));
    if clock.is_some() {
        let names = (7..11).filter(|i| crate::scoreboard::match_hero(line(*i), hero_names).is_some()).count();
        if names >= 3 || scoreboard_fallback() { return (Screen::Scoreboard, clock); }
        if has(line(1), "RESPAWN") { return (Screen::Dead, clock); }
        return (Screen::Gameplay, clock);
    }
    if has(line(2), "STARTING") || has(line(2), "GAME") { return (Screen::Countdown, None); }
    (Screen::Unknown, None)
}

/// Returns the screen and the raw parsed clock (if it was read while classifying).
pub fn classify(rd: &Reader, img: &DynamicImage, hero_names: &[(String, String)]) -> (Screen, Option<u32>) {
    classify_hinted(rd, img, hero_names, None)
}
pub fn classify_hinted(rd: &Reader, img: &DynamicImage, hero_names: &[(String, String)], known_clock: Option<u32>) -> (Screen, Option<u32>) {
    let mut batch = LineBatch::default();
    let probe = add_probe(&mut batch, img, None, true, false);
    let lines = batch.run(|c| rd.read_lines(c));
    interpret_probe(&lines, &probe, hero_names, known_clock, None, || crate::scoreboard::looks_like_scoreboard(rd, img, hero_names))
}
