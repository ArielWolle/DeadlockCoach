//! End-of-match probe: for each frame, the Defeat/Victory banner gate and read, the post-game
//! summary's "PLAY AGAIN" button read, and the shop "BACK" anchor.
//! `cargo run --release -p coach-vision --example end_probe -- frames/rec13/img/590_*.jpg ...`
use coach_vision::{hud, ocr::Reader, screen};
fn main() {
    let rd = Reader::load(std::path::Path::new("data/models")).unwrap();
    for p in std::env::args().skip(1) {
        let raw = image::open(&p).unwrap();
        let img = if raw.width() == 1600 && raw.height() == 900 { raw } else { raw.thumbnail_exact(1600, 900) };
        let banner = rd.read_lines(&[screen::post_game_crop(&img)]).ok().and_then(|l| l.into_iter().next()).unwrap_or_default();
        let lines = rd.read_lines(&[screen::play_again_crop(&img), hud::crop(&img, screen::back_anchor())]).unwrap_or_default();
        println!("{p}\n  banner gate={} read={banner:?} post_game={}\n  summary gate={} play_again={:?} summary={}\n  back={:?}",
            screen::post_game_gate(&img), screen::says_post_game(&banner),
            screen::play_again_gate(&img), lines.first(), lines.first().is_some_and(|s| screen::says_play_again(s)), lines.get(1));
    }
}
