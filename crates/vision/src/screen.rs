//! Screen classifier: decides which game screen a frame shows, from cheap OCR anchors.
//! Unknown screens must produce no coaching cues.
use crate::{hud, ocr::Reader, parse, Region};
use image::DynamicImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Screen {
    /// Normal play: top bar + clock visible.
    Gameplay,
    /// Shop open ("ESC BACK" anchor).
    Shop,
    /// Pre-game "GAME STARTING" countdown.
    Countdown,
    /// Anything not recognised yet (scoreboard, menus, map...). Say nothing.
    Unknown,
}

fn px(x: f32, y: f32, w: f32, h: f32) -> Region { Region { x: x / 1600.0, y: y / 900.0, w: w / 1600.0, h: h / 900.0 } }

const BACK_ANCHOR: Region = Region { x: 10.0 / 1600.0, y: 860.0 / 900.0, w: 110.0 / 1600.0, h: 32.0 / 900.0 };

fn has(s: &str, needle: &str) -> bool { s.to_ascii_uppercase().contains(needle) }

pub fn classify(rd: &Reader, img: &DynamicImage) -> Screen {
    let back = rd.read(&hud::crop(img, BACK_ANCHOR)).unwrap_or_default();
    if has(&back, "BACK") { return Screen::Shop; }
    let clock = rd.read(&hud::crop(img, hud::CLOCK)).ok().and_then(|s| parse::parse_clock(&s));
    if clock.is_some() { return Screen::Gameplay; }
    let mid = rd.read(&hud::crop(img, px(620.0, 490.0, 360.0, 45.0))).unwrap_or_default();
    if has(&mid, "STARTING") || has(&mid, "GAME") { return Screen::Countdown; }
    Screen::Unknown
}
