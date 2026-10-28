//! Regressions from the rec8 ground-truth review (1920x1080 fixtures 00550-00554): the clock's
//! tens-of-minutes digit, the "DELIVER THE URN" prompt, AFFLICTED and Frost Grenade slows, and the
//! Patron's health banner. Each frame is scanned twice, one second apart.
use coach_core::{GameSnapshot, UrnState};
use coach_vision::{auto::AutoScanner, ocr::Reader, scan::Scanner};
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn scanner() -> AutoScanner { AutoScanner::new(Scanner::load(&root()).unwrap(), Reader::load(&root().join("data/models")).unwrap()) }
fn frame(prefix: &str) -> image::DynamicImage {
    let dir = root().join("samples/frames");
    let f = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().starts_with(prefix)).unwrap();
    image::open(f.path()).unwrap()
}
fn twice(prefix: &str) -> GameSnapshot { let f = frame(prefix); let mut a = scanner(); a.scan(&f, 0); a.scan(&f, 1000) }

#[test]
fn the_tens_of_minutes_digit_is_kept() {
    // Live coach read 5:31 for 15:31 (frame 41) and kept it until 17:52. The leftmost canonical
    // clock box reads "5:31" on this very frame (alone it cannot tell, so the reliable verifier
    // anchors); when any box shows the tens digit, the vote prefers the reading with it.
    let s = twice("00550");
    assert_eq!(s.screen.as_deref(), Some("Gameplay"));
    assert!(matches!(s.game_clock_secs, Some(931 | 932)), "{:?}", s.game_clock_secs);
    assert_eq!(coach_vision::screen::vote_clock(&["5:31".into(), "".into(), "16:3".into(), "5:3".into()], None), Some(331));
    assert_eq!(coach_vision::screen::vote_clock(&["5:31".into(), "15:31".into()], Some(331)), Some(931), "more digits win over the expectation");
    assert_eq!(coach_vision::screen::vote_clock(&["5:31".into(), "5:31".into(), "15:31".into()], None), Some(931));
    assert_eq!(coach_vision::screen::vote_clock(&["15:31".into(), "16:31".into()], Some(931)), Some(931));
}

#[test]
fn the_deliver_the_urn_prompt_means_you_carry_it() {
    // rec8 27:27: the prompt was on screen but the minimap vase was not found (urn None).
    let s = twice("00551");
    assert_eq!(s.minimap.as_ref().and_then(|m| m.urn), Some(UrnState::CarriedByYou), "{:?}", s.minimap.as_ref().map(|m| m.urn));
    assert!(s.status_effects.iter().any(|e| e == "carrying_urn"), "{:?}", s.status_effects);
}

#[test]
fn afflicted_and_frost_slows_are_status_effects() {
    let s = twice("00552");
    assert!(s.status_effects.iter().any(|e| e == "afflicted"), "{:?}", s.status_effects);
    let s = twice("00553");
    assert!(s.status_effects.iter().any(|e| e == "slowed"), "{:?}", s.status_effects);
}

#[test]
fn the_patron_health_banner_is_read() {
    // rec8 49:34: your Hidden King at about half health while you farmed elsewhere.
    let s = twice("00554");
    let b = s.objective_banner.expect("banner");
    assert_eq!(b.name, "HIDDEN KING");
    assert!(b.health_ratio.is_some_and(|h| (0.4..=0.6).contains(&h)), "{:?}", b.health_ratio);
    assert_ne!(b.own, Some(false), "rec8 is a mirrored layout: the Hidden King is yours");
}

#[test]
fn ordinary_frames_have_no_banner_urn_carry_or_new_effects() {
    for p in ["00014", "00059", "00401", "00500", "00501", "00503", "00504", "00520", "00550"] {
        let s = twice(p);
        assert!(s.objective_banner.is_none(), "{p}: {:?}", s.objective_banner);
        assert!(!s.status_effects.iter().any(|e| ["carrying_urn", "afflicted", "slowed", "frozen"].contains(&e.as_str())), "{p}: {:?}", s.status_effects);
        assert_ne!(s.minimap.as_ref().and_then(|m| m.urn), Some(UrnState::CarriedByYou), "{p}");
    }
}
