//! Match boundaries without the HUD match id. rec11 -> rec12: the live coach read the same id
//! (127139499) in both matches, so a reset that needed an id change never came; the previous
//! match's latches (your slot 2 = Violet, then Mirage in the new match) held until ~6:30.
//! Fixtures: 00560 = rec11 late in match A (Violet, ally slot 2, 39:09), 00561 = rec12 match B
//! at 0:22 (Grey Talon, ally slot 4), 00526 = an end-of-match Defeat banner (rec5).
use coach_core::GameSnapshot;
use coach_vision::scan::{match_boundary, own_from_votes, Boundary, BoundaryEvidence, HeroLatches};
use coach_vision::{auto::AutoScanner, ocr::Reader, scan::Scanner};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn scanner() -> AutoScanner { AutoScanner::new(Scanner::load(&root()).unwrap(), Reader::load(&root().join("data/models")).unwrap()) }
fn frame(prefix: &str) -> image::DynamicImage {
    let dir = root().join("samples/frames");
    let f = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().starts_with(prefix)).unwrap();
    image::open(f.path()).unwrap()
}

/// Match A until your hero (Violet) is latched; returns the scanner and the wall time.
fn match_a() -> (AutoScanner, u64) {
    let a = frame("00560");
    let mut auto = scanner();
    let mut last = GameSnapshot::default();
    for i in 0..6u64 { last = auto.scan(&a, i * 2000); }
    assert_eq!(last.hero.as_deref(), Some("hero_artist"), "match A: {:?}", last.ally_heroes);
    assert!(last.ally_heroes.iter().any(|h| h == "hero_artist"));
    (auto, 10_000)
}

fn assert_match_b(snaps: &[GameSnapshot]) {
    for s in snaps {
        assert_ne!(s.hero.as_deref(), Some("hero_artist"), "match A's hero in match B");
        assert_ne!(s.hero.as_deref(), Some("hero_mirage"), "match A's own slot (2) in match B");
        assert!(!s.ally_heroes.iter().any(|h| h == "hero_artist" || h == "hero_ratking"), "match A's allies: {:?}", s.ally_heroes);
    }
    let last = snaps.last().unwrap();
    assert_eq!(last.hero.as_deref(), Some("hero_orion"), "{:?}", last.ally_heroes);
    // All six allies from the portraits alone (slot 0 Billy matched Haze's amber silhouette
    // before the real top-bar capture `hero_punkgoat@topbar_amber`).
    let allies: Vec<&str> = last.ally_heroes.iter().map(|h| h.trim_start_matches("hero_")).collect();
    assert_eq!(allies, ["punkgoat", "warden", "mirage", "doorman", "orion", "nano"]);
    assert!(last.game_clock_secs.is_some_and(|c| c < 60), "{:?}", last.game_clock_secs);
}

#[test]
fn the_end_banner_then_a_start_of_match_clock_is_a_new_match_at_once() {
    let (mut auto, t) = match_a();
    let defeat = frame("00526");
    let pg = auto.scan(&defeat, t + 2000);
    assert_eq!(pg.screen.as_deref(), Some("PostGame"));
    // Minutes of post-game menus, then match B (same match id read, or none).
    let b = frame("00561");
    let snaps: Vec<GameSnapshot> = (0..4u64).map(|i| auto.scan(&b, t + 300_000 + i * 2000)).collect();
    // Reset before match B's first frame is read: no frame of match B reports match A's state.
    assert_eq!(auto.scanner().match_resets(), 1);
    assert_match_b(&snaps);
    assert_eq!(snaps[0].minimap.as_ref().and_then(|m| m.lanes_mirrored), snaps[3].minimap.as_ref().and_then(|m| m.lanes_mirrored));
}

#[test]
fn a_verified_clock_restart_is_a_new_match_without_an_end_banner() {
    // No PostGame frame captured (e.g. a recording gap): the verified clock goes back from 39 min
    // to 0:22 once two consistent verifications agree.
    let (mut auto, t) = match_a();
    let b = frame("00561");
    let snaps: Vec<GameSnapshot> = (0..6u64).map(|i| auto.scan(&b, t + 60_000 + i * 2000)).collect();
    assert_eq!(auto.scanner().match_resets(), 1);
    // The frames before the restart verified carry no clock of either match...
    let restarted = snaps.iter().position(|s| s.game_clock_secs.is_some_and(|c| c < 60)).expect("clock restart");
    // ...and from the restart on everything is match B's.
    assert_match_b(&snaps[restarted..]);
}

#[test]
fn match_boundary_rule() {
    let e = |between, gap_ms, raw, expected, verified, max| BoundaryEvidence { between, gap_ms, raw_clock: raw, expected, verified, clock_max: max };
    // rec12: PostGame/Countdown seen, then the first HUD frame reads 0:11 while the old match's
    // extrapolated clock says 48:00.
    assert_eq!(match_boundary(&e(true, Some(600_000), Some(11), Some(2880), None, Some(2410))), Boundary::Before);
    // Without a boundary screen a low raw read is not enough (the verifier decides)...
    assert_eq!(match_boundary(&e(false, Some(4000), Some(11), Some(2880), None, Some(2410))), Boundary::Same);
    // ...nor a dropped tens-of-minutes digit ("0:31" for 30:31) after a false end banner,
    assert_eq!(match_boundary(&e(true, Some(4000), Some(31), Some(1831), None, Some(1827))), Boundary::Same);
    // ...nor a clock that continues the match (a false end banner at 1:10).
    assert_eq!(match_boundary(&e(true, Some(4000), Some(70), Some(69), None, Some(65))), Boundary::Same);
    // A long gap without HUD frames and a clock minutes behind the extrapolation: another match.
    assert_eq!(match_boundary(&e(false, Some(900_000), Some(400), Some(2900), None, Some(2000))), Boundary::Before);
    // A short Esc menu mid-match with a misread: no.
    assert_eq!(match_boundary(&e(false, Some(30_000), Some(400), Some(2900), None, Some(2870))), Boundary::Same);
    // The verified clock restarted from > 2:00 to < 1:30.
    assert_eq!(match_boundary(&e(false, None, None, None, Some(22), Some(2350))), Boundary::Verified);
    // rec8: verified 18:36, highest 18:34 (the id misread twice): same match.
    assert_eq!(match_boundary(&e(false, None, None, None, Some(1116), Some(1114))), Boundary::Same);
    // Verified 7:34 for 17:34 (minute-digit misread, if it ever verified): not a match start.
    assert_eq!(match_boundary(&e(false, None, None, None, Some(454), Some(1054))), Boundary::Same);
    // Early in a match nothing tells a restart from noise.
    assert_eq!(match_boundary(&e(false, None, None, None, Some(30), Some(100))), Boundary::Same);
}

#[test]
fn a_name_confirmed_identity_is_not_taken_over_by_portraits() {
    // rec12 ally slot 0: every scoreboard read Billy, while every portrait sample of the dark
    // portrait on the amber backdrop was a strong Haze winner.
    let mut l = HeroLatches::default();
    let mut votes = vec![HashMap::new(); 12];
    votes[0].insert("hero_punkgoat".to_string(), 3.0);
    l.board(0, "hero_punkgoat", &votes);
    assert!(l.named(0));
    for _ in 0..200 {
        *votes[0].entry("hero_haze".to_string()).or_insert(0.0) += 1.0;
        l.sample(0, Some(("hero_haze", 0.2)), &votes);
    }
    assert_eq!(l.heroes()[0].as_deref(), Some("hero_punkgoat"));
    // Two contrary name reads still switch it.
    l.board(0, "hero_haze", &votes);
    l.board(0, "hero_haze", &votes);
    assert_eq!(l.heroes()[0].as_deref(), Some("hero_haze"));
}

#[test]
fn three_unanimous_backdrop_votes_name_your_slot() {
    assert_eq!(own_from_votes(&[0, 0, 0, 0, 3, 0]), Some(4));
    assert_eq!(own_from_votes(&[0, 0, 0, 1, 3, 0]), None);
    assert_eq!(own_from_votes(&[0, 0, 0, 1, 6, 0]), Some(4));
    assert_eq!(own_from_votes(&[0, 0, 2, 0, 0, 0]), None);
}
