//! Regression tests for rec5 HUD findings (1920x1080 fixtures 00505-00511): souls, level,
//! screen classes (shop alive / dead, post-game), own respawn timer, death panel and status
//! effects. Each frame is scanned twice, one second apart (first values need a confirming read).
use coach_core::GameSnapshot;
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
fn souls_are_read_right_of_the_dollar_glyph_with_level() {
    // Finding 1: this frame (and 8 others in rec5) had no souls value though 2,809 is readable.
    let s = twice("00520");
    assert_eq!(s.screen.as_deref(), Some("Gameplay"));
    assert_eq!(s.souls, Some(2809));
    assert_eq!(s.souls_age_ms, Some(0), "read on this frame, not held");
    assert_eq!(s.level, Some(17));
}

#[test]
fn shop_open_while_alive_is_a_shop_with_souls() {
    // Finding 3: rec5 frames 138-140, 193, 247, 261-263, 426-427 were Unknown.
    let s = twice("00521");
    assert_eq!(s.screen.as_deref(), Some("Shop"));
    assert!(!s.dead);
    assert_eq!(s.souls, Some(1930));
    assert_eq!(s.level, Some(24));
    assert!(s.respawn_secs.is_none());
}

#[test]
fn shop_open_while_dead_keeps_the_death_and_reads_the_timer() {
    // Findings 2+3: rec5 frame 501 shows "Respawn in: 60" over the shop.
    let s = twice("00525");
    assert_eq!(s.screen.as_deref(), Some("Shop"));
    assert!(s.dead);
    assert!(matches!(s.respawn_secs, Some(59 | 60)), "{:?}", s.respawn_secs);
}

#[test]
fn respawn_digits_are_not_dropped() {
    // Finding 2: "30" was read as 0 (frame 311).
    let s = twice("00524");
    assert!(s.dead);
    assert!(matches!(s.respawn_secs, Some(29 | 30)), "{:?}", s.respawn_secs);
    assert!(s.respawn_age_ms.is_some_and(|a| a <= 1000));
}

#[test]
fn death_panel_rows_killing_blow_and_total() {
    // Finding 4: the complete panel (frame 300) has six rows, top "Aura of Suffering 789".
    let s = twice("00523");
    assert!(s.dead);
    assert_eq!(s.death_cause.as_deref(), Some("Aura of Suffering"));
    assert_eq!(s.death_total_damage, Some(4451));
    assert_eq!(s.death_killing_blow.as_deref(), Some("Mystic Shot"));
    let top = s.death_damage.first().expect("rows");
    assert_eq!((top.source.as_str(), top.damage), ("Aura of Suffering", 789));
    assert!(s.death_damage.len() >= 5, "{:?}", s.death_damage);
    // Every reported row is a dictionary name with a damage in panel order.
    assert!(s.death_damage.windows(2).all(|w| w[0].damage >= w[1].damage), "{:?}", s.death_damage);
    for r in &s.death_damage { assert!(coach_vision::death::source_names().contains(&r.source), "{r:?}"); }
}

#[test]
fn stunned_and_debuff_badge_are_status_effects() {
    // Finding 5: frame 287 shows STUNNED (cause line "Knocked off Zipline") and an Aura badge.
    let s = twice("00522");
    assert!(s.status_effects.iter().any(|e| e == "stunned"), "{:?}", s.status_effects);
    assert!(s.status_effects.iter().any(|e| e == "Aura of Suffering"), "{:?}", s.status_effects);
}

#[test]
fn ordinary_frames_have_no_status_effects() {
    // False positives are worse than misses.
    for p in ["00014", "00059", "00401", "00500", "00501", "00503", "00504", "00520", "00521"] {
        let s = twice(p);
        assert!(s.status_effects.is_empty(), "{p}: {:?}", s.status_effects);
    }
}

#[test]
fn the_defeat_banner_is_post_game() {
    // Finding 3: frames 570-572 were Gameplay/Unknown.
    let s = twice("00526");
    assert_eq!(s.screen.as_deref(), Some("PostGame"));
    assert!(s.game_clock_secs.is_none() && s.souls.is_none());
}
