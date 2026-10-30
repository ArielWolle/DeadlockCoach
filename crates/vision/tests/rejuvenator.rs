//! Rejuvenator and clock regressions from coach-review-8 section D (1920x1080 fixtures
//! 00580-00583, 00554): the rec8 39:17 revive ("Respawn in: 1" on the greyed world, alive again
//! 5 s later), the Rejuvenator charge icons under a team's total naming the Midboss taker, and
//! rec13's opening clock (5:52, read 6:52 on the downsampled context before).
use coach_vision::{auto::AutoScanner, ocr::Reader, scan::Scanner};
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn scanner() -> AutoScanner { AutoScanner::new(Scanner::load(&root()).unwrap(), Reader::load(&root().join("data/models")).unwrap()) }
fn frame(prefix: &str) -> image::DynamicImage {
    let dir = root().join("samples/frames");
    let f = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().starts_with(prefix)).unwrap();
    image::open(f.path()).unwrap()
}

#[test]
fn the_rec8_revive_is_flagged_and_our_charges_name_the_taker() {
    // Recorded wall times: 39:11 (+0), 39:17 (+6.1 s), 39:22 (+11.1 s).
    let mut a = scanner();
    let (alive, revive, back) = (frame("00580"), frame("00581"), frame("00582"));
    let s = a.scan(&alive, 0);
    assert!(!s.revived && !s.dead);
    let s = a.scan(&alive, 2000);
    assert_eq!(s.midboss_taken_by.as_deref(), Some("ally"), "two charge icons under our total on two frames");
    let s = a.scan(&revive, 8058);
    assert_eq!(s.screen.as_deref(), Some("Dead"));
    assert!(s.dead && s.revived, "grey 'Respawn in: 1' right after a live frame is a revive");
    let s = a.scan(&back, 13_093);
    assert_eq!(s.screen.as_deref(), Some("Gameplay"));
    assert!(!s.dead && !s.revived, "flagged once, on the death screen");
    assert_eq!(s.midboss_taken_by.as_deref(), Some("ally"), "held after the charges were spent");
}

#[test]
fn a_revive_frame_alone_is_not_a_revive_and_real_death_screens_are_not() {
    // Without a live frame just before, the grey screen cannot be judged.
    let mut a = scanner();
    let s = a.scan(&frame("00581"), 0);
    assert!(!s.revived);
    // Real death screens (rec5 full death panel, respawn 30) after a live frame.
    let mut b = scanner();
    b.scan(&frame("00520"), 0);
    let s = b.scan(&frame("00523"), 3000);
    assert!(s.dead && !s.revived, "{:?}", s.respawn_secs);
}

#[test]
fn enemy_charges_name_the_enemy_and_ordinary_frames_name_nobody() {
    // rec8 49:34: the enemy holds the Rejuvenator (two icons under their total).
    let f = frame("00554"); let mut a = scanner(); a.scan(&f, 0);
    assert_eq!(a.scan(&f, 2000).midboss_taken_by.as_deref(), Some("enemy"));
    for p in ["00014", "00059", "00401", "00500", "00501", "00503", "00504", "00520", "00550", "00583"] {
        let f = frame(p); let mut a = scanner(); a.scan(&f, 0);
        assert_eq!(a.scan(&f, 2000).midboss_taken_by, None, "{p}");
    }
}

#[test]
fn the_rec13_opening_clock_is_read_at_full_resolution() {
    // 1920x1080: the 1600x900 context blurs the 5 into a 6 (fixed box and glyph search alike);
    // the clock jumped 7:48 -> 6:51 seventeen frames later and the first death lost its review.
    let f = frame("00583"); let mut a = scanner();
    let s = a.scan(&f, 0);
    assert_eq!(s.game_clock_secs, Some(352), "5:52");
    assert!(matches!(a.scan(&f, 1000).game_clock_secs, Some(352 | 353)));
}
