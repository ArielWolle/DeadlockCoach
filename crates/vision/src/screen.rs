//! Screen classifier: decides which game screen a frame shows, from cheap OCR anchors.
//! Unknown screens must produce no coaching cues.
use crate::{hud, ocr::Reader, parse, Region};
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
pub fn respawn_text_line() -> Region { px(680.0,670.0,150.0,30.0) }
pub fn respawn_number() -> Region { px(845.0, 665.0, 80.0, 40.0) }
pub fn match_id_region() -> Region { px(1440.0, 868.0, 160.0, 16.0) }

fn has(s: &str, needle: &str) -> bool { s.to_ascii_uppercase().contains(needle) }

/// Returns the screen and the raw parsed clock (if it was read while classifying).
pub fn classify(rd: &Reader, img: &DynamicImage, hero_names: &[(String, String)]) -> (Screen, Option<u32>) {
    classify_hinted(rd,img,hero_names,None)
}
pub fn classify_hinted(rd:&Reader,img:&DynamicImage,hero_names:&[(String,String)],known_clock:Option<u32>)->(Screen,Option<u32>){
    let back = rd.read_line(&hud::crop(img, back_anchor())).unwrap_or_default();
    if has(&back, "RESUME") { return (Screen::Menu, None); }
    if has(&back, "BACK") { return (Screen::Shop, None); }
    let clock = known_clock.or_else(||rd.read(&hud::crop(img, hud::CLOCK)).ok().and_then(|s| parse::parse_clock(&s)));
    if clock.is_some() {
        if crate::scoreboard::looks_like_scoreboard(rd, img, hero_names) { return (Screen::Scoreboard, clock); }
        let resp = rd.read_line(&hud::crop(img, respawn_text_line())).unwrap_or_default();
        if has(&resp, "RESPAWN") { return (Screen::Dead, clock); }
        return (Screen::Gameplay, clock);
    }
    let mid = rd.read(&hud::crop(img, px(620.0, 490.0, 360.0, 45.0))).unwrap_or_default();
    if has(&mid, "STARTING") || has(&mid, "GAME") { return (Screen::Countdown, None); }
    (Screen::Unknown, None)
}
