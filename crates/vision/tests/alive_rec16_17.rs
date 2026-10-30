//! Dead / alive top-bar regressions from rec16 and rec17 (docs/coach-review-16-17-alive.md).
//! Truth was read by hand from the screenshots: a dead hero's portrait carries its respawn
//! countdown; every other slot is alive.
//! - Phantom deaths: Shiv's portrait art has ~95 red pixels every frame; with his bar not drawn
//!   (enemy unseen, or a slid crop) the old red-pixel test read him dead (rec17 enemy 3 all game,
//!   rec16 ally 1), as it did pale or dark living portraits (Silver, Billy).
//! - Context jitter: the game context derived from the minimap box slid by up to 23 px on a
//!   full-screen capture, moving every top-bar crop (rec16 13:32, 24:12).
//! - Respawn countdowns: static art read as the same number twice is not a countdown; the fast
//!   monitor counts the full scan's timers down instead of repeating them.
use coach_core::HeroStatus;
use coach_vision::{auto::{snap_full_frame, AutoScanner}, hud, ocr::Reader, scan::Scanner, team};
use image::DynamicImage;

fn root() -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn fixture(name: &str) -> DynamicImage { image::open(root().join("samples/frames").join(name)).unwrap() }
/// The strip fixtures are the top 120 rows of 1920x1080 frames: paste into a black frame and
/// bring it to the canonical 1600x900 the scanner reads.
fn strip(name: &str) -> DynamicImage {
    let part = fixture(name).to_rgb8();
    let mut f = image::RgbImage::new(1920, 1080);
    image::imageops::overlay(&mut f, &part, 0, 0);
    DynamicImage::ImageRgb8(DynamicImage::ImageRgb8(f).thumbnail_exact(1600, 900).to_rgb8())
}
/// Slots (0-5 allies, 6-11 enemies) committed dead after two frames of the same image.
fn dead_after_two(img: &DynamicImage) -> Vec<usize> {
    let mut status: Vec<HeroStatus> = vec![];
    team::apply_bars(img, &mut status);
    team::apply_bars(img, &mut status);
    status.iter().enumerate().filter(|(_, s)| s.dead == Some(true)).map(|(i, _)| i).collect()
}
const STRIPS: [(&str, &[usize]); 8] = [
    ("00590_rec17-topbar-shiv-enemy-unseen-alive.jpg", &[]),
    ("00591_rec16-topbar-shiv-ally-critical-alive.jpg", &[]),
    ("00592_rec16-topbar-shiv-alive-3-enemies-dead.jpg", &[6, 7, 9]),
    ("00593_rec16-topbar-single-2.jpg", &[3]),
    ("00594_rec17-topbar-single-1-37-9.jpg", &[6, 9, 11]),
    ("00595_rec16-topbar-2-3-75.jpg", &[1, 6, 9]),
    ("00596_rec16-topbar-2-allies-2-enemies-dead.jpg", &[2, 3, 9, 10]),
    ("00597_rec17-topbar-no-dead-shiv-unseen.jpg", &[]),
];

#[test]
fn only_slots_showing_a_respawn_countdown_read_dead() {
    for (name, truth) in STRIPS {
        let img = strip(name);
        assert_eq!(dead_after_two(&img), truth.to_vec(), "{name}");
        let rgb = img.to_rgb8();
        for i in 0..12 {
            let r = team::read_bar(&rgb, i);
            // The old evidence is still there on Shiv's slot: plenty of loose red pixels, no digits.
            if name.starts_with("00590") && i == 9 { assert!(r.timer >= 55 && r.digits < team::DIGITS_UNSURE, "{r:?}"); }
            assert!(!r.unsure, "{name} slot {i}: {r:?}");
        }
    }
}

#[test]
fn respawn_digits_of_the_dead_slots_read_right() {
    let reader = Reader::load(&root().join("data/models")).unwrap();
    for (name, slots, want) in [("00595_rec16-topbar-2-3-75.jpg", vec![1usize, 6, 9], vec!["2", "3", "75"]),
        ("00594_rec17-topbar-single-1-37-9.jpg", vec![6, 9, 11], vec!["1", "37", "9"]),
        ("00596_rec16-topbar-2-allies-2-enemies-dead.jpg", vec![2, 3, 9, 10], vec!["73", "60", "7", "78"])] {
        let img = strip(name);
        let lines = reader.read_lines(&team::respawn_crops(&img, &slots)).unwrap();
        assert_eq!(team::checked_respawn_lines(&img, &lines, &slots), want, "{name}");
    }
}

#[test]
fn a_context_that_nearly_fills_a_16_9_frame_is_the_frame() {
    // Contexts measured on rec16 (full-screen 1920x1080 capture).
    for (x, y, w, h) in [(8., 5., 1910., 1074.), (23., 15., 1891., 1064.), (4., 3., 1915., 1077.), (15., 0., 1905., 1072.), (-6., -2., 1930., 1085.)] {
        assert_eq!(snap_full_frame(1920, 1080, x, y, w, h), (0., 0., 1920., 1080.), "{x},{y},{w},{h}");
    }
    // A real sub-window (letterboxed or windowed game) keeps its context.
    assert_eq!(snap_full_frame(1920, 1080, 160., 90., 1600., 900.), (160., 90., 1600., 900.));
    assert_eq!(snap_full_frame(2560, 1080, 320., 0., 1920., 1080.), (320., 0., 1920., 1080.));
}

#[test]
fn jittered_minimap_context_keeps_living_teammates_alive() {
    let mut a = AutoScanner::new(Scanner::load(&root()).unwrap(), Reader::load(&root().join("data/models")).unwrap());
    // 13:32: nobody dead (Shiv in ally slot 1 is alive at 59% health).
    let f = fixture("00598_rec16-context-jitter-gameplay-13-32.jpg");
    let mut s = Default::default();
    for t in 0..3 { s = a.scan(&f, t * 1000); }
    let ctx = a.regions().into_iter().find(|r| r.id == "game_context").expect("context").bounds;
    assert_eq!((ctx.x, ctx.y, ctx.w, ctx.h), (0, 0, 1920, 1080));
    let dead = |s: &coach_core::GameSnapshot| s.hero_status.iter().filter(|h| h.dead == Some(true)).map(|h| (h.ally, h.slot)).collect::<Vec<_>>();
    assert_eq!(s.hero_status.len(), 12);
    assert_eq!(dead(&s), vec![], "13:32");
    // 24:12 death screen: Silver, Paige, you (slot 4) and Billy dead; Shiv and Mina alive;
    // Seven (enemy 3) dead.
    let f = fixture("00599_rec16-context-jitter-death-24-12.jpg");
    for t in 3..6 { s = a.scan(&f, t * 1000); }
    assert_eq!(dead(&s), vec![(true, 0), (true, 3), (true, 4), (true, 5), (false, 3)], "24:12");
}

#[test]
fn a_number_that_does_not_count_down_is_not_a_respawn_timer() {
    let mut st = vec![HeroStatus { dead: Some(true), ..Default::default() }];
    // Shiv's living portrait read "29" on consecutive frames 3 s apart.
    team::apply_respawn(&["29".into()], &[0], 10_000, &mut st);
    team::apply_respawn(&["29".into()], &[0], 13_000, &mut st);
    team::apply_respawn(&["29".into()], &[0], 16_000, &mut st);
    assert_eq!(st[0].respawn_secs, None);
    // A real countdown 3 s apart is confirmed.
    team::apply_respawn(&["40".into()], &[0], 20_000, &mut st);
    team::apply_respawn(&["37".into()], &[0], 23_000, &mut st);
    assert_eq!(st[0].respawn_secs, Some(37));
}

#[test]
fn fast_monitor_counts_full_scan_timers_down() {
    let mut st = vec![HeroStatus { dead: Some(true), respawn_secs: Some(14), ..Default::default() }, HeroStatus { dead: Some(true), respawn_secs: Some(3), ..Default::default() }];
    team::age_respawn(&mut st, 4);
    assert_eq!(st.iter().map(|s| s.respawn_secs).collect::<Vec<_>>(), vec![Some(10), None]);
    // Slot centres line up with the calibrated portraits (a guard for the fixtures above).
    assert_eq!(hud::ALLY_CENTERS[0], 310.0); assert_eq!(hud::ENEMY_CENTERS[0], 920.0);
}
