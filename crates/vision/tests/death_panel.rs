//! The "Incoming damage (last death)" panel is received well or disregarded
//! (docs/coach-review-16-17-panel.md, fixtures 00600-00607, 1920x1080): a confirmed read of an
//! open panel, the collapsed panel, a panel that cannot be read, a panel spanning two deaths, and
//! nothing carried over from an earlier death.
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
/// Scans (fixture, ms) in order; returns every snapshot.
fn run(steps: &[(&str, u64)]) -> Vec<GameSnapshot> { let mut a = scanner(); steps.iter().map(|(p, t)| a.scan(&frame(p), *t)).collect() }
fn rows(s: &GameSnapshot) -> Vec<(&str, u32)> { s.death_damage.iter().map(|r| (r.source.as_str(), r.damage)).collect() }
fn nothing_reported(s: &GameSnapshot) -> bool { s.death_damage.is_empty() && s.death_cause.is_none() && s.death_killing_blow.is_none() && s.death_total_damage.is_none() }

#[test]
fn an_open_panel_is_reported_once_a_second_read_agrees() {
    // (Agreeing reads at least 4 s apart: the numbers count up for a moment after the death.)
    // rec16 24:08: six rows, Killing Blow on Melee Charge (the live coach never tagged it).
    let s = run(&[("00600", 0), ("00600", 4000)]);
    assert!(s[0].dead);
    assert_eq!(s[0].death_panel.as_deref(), Some("reading"));
    assert!(nothing_reported(&s[0]), "one read is not enough: {:?}", s[0].death_damage);
    let s = &s[1];
    assert_eq!(s.death_panel.as_deref(), Some("open"));
    assert_eq!(rows(s), [("Melee", 1319), ("Gun Damage", 1067), ("Gun Damage", 808), ("Ira Domini", 534), ("Ira Domini", 484), ("Melee Charge", 106)]);
    assert_eq!(s.death_killing_blow.as_deref(), Some("Melee Charge"));
    assert_eq!(s.death_total_damage, Some(6295));
    assert_eq!(s.death_cause.as_deref(), Some("Melee"));
}

#[test]
fn threes_and_fives_keep_their_digits_and_the_tag_hidden_name_is_matched() {
    // rec16 32:44: Lucky Shot 225 was read "22" (the 5 split in two), the Killing Blow row
    // "Stalk…" was dropped; the top row Melee 537 was missed live ("Royal Pestments").
    let s = run(&[("00604", 0), ("00604", 4000)]);
    let s = &s[1];
    assert_eq!(s.death_panel.as_deref(), Some("open"));
    let r = rows(s);
    assert_eq!(r[..5], [("Melee", 537), ("Royal Pestments", 451), ("Gun Damage", 444), ("Siphon Life", 417), ("Lucky Shot", 225)]);
    // The tagged bottom row "Stalk…" is Stalker or Stalker's Mark; with no Drifter in this match
    // (top bar) only Stalker is possible. Without the roster it is dropped, never guessed.
    assert!(r.len() == 5 || r[5] == ("Stalker", 109), "{r:?}");
    assert!(!r.iter().any(|x| x.0 == "Stalker's Mark") && s.death_killing_blow.as_deref() != Some("Stalker's Mark"), "{r:?}");
    assert_eq!((s.death_cause.as_deref(), s.death_total_damage), (Some("Melee"), Some(2929)));
}

#[test]
fn a_panel_that_fills_in_is_replaced_by_the_new_confirmed_read() {
    // rec16 32:44 -> 33:23: the game rewrote the panel (2,929 -> 5,963, Royal Pestments on top).
    let s = run(&[("00604", 0), ("00604", 4000), ("00607", 8000), ("00607", 12_000)]);
    assert_eq!(s[2].death_total_damage, Some(2929), "the new state needs its own second read");
    let s = &s[3];
    assert_eq!(s.death_total_damage, Some(5963));
    assert_eq!(s.death_damage.first().map(|r| (r.source.as_str(), r.damage)), Some(("Royal Pestments", 868)));
    assert_eq!(s.death_cause.as_deref(), Some("Royal Pestments"));
}

#[test]
fn two_killing_blow_tags_name_no_killing_blow() {
    // rec16 37:02, 13 s after a Rejuvenator revive: the panel spans both deaths (two tags) and its
    // top row Lurking Terrors 3,142 (the tag over its name) was dropped live.
    let s = run(&[("00603", 0), ("00603", 4000)]);
    let s = &s[1];
    assert_eq!(s.death_panel.as_deref(), Some("open"));
    assert_eq!(s.death_damage.first().map(|r| (r.source.as_str(), r.damage)), Some(("Lurking Terrors", 3142)));
    assert_eq!(s.death_total_damage, Some(5352));
    assert_eq!(s.death_killing_blow, None);
}

#[test]
fn the_collapsed_panel_is_closed_and_reports_nothing() {
    // rec16 27:52 and rec13 20:03: no panel, only your portrait tab at the right edge. The panel
    // opens by itself a few seconds into a death, so the tab counts after 6 s.
    for p in ["00601", "00602"] {
        let s = run(&[(p, 0), (p, 3000), (p, 7000), (p, 10_000)]);
        for x in &s { assert!(nothing_reported(x), "{p}: {:?}", x.death_damage); }
        let x = s.last().unwrap();
        assert!(x.dead, "{p}: {:?}", x.screen);
        assert_eq!(x.death_panel.as_deref(), Some("closed"), "{p}");
    }
    // Open panels and live frames show no tab.
    for p in ["00600", "00603", "00604", "00606"] { assert_ne!(run(&[(p, 0)])[0].death_panel.as_deref(), Some("closed"), "{p}"); }
}

#[test]
fn a_panel_that_cannot_be_read_is_disregarded() {
    // rec13 6:12: the top row "Specimen II" is not a known source, so the read is never complete:
    // nothing is reported (live said cause "Spcimen Yo"), and after 6 s it is unreadable.
    let s = run(&[("00605", 0), ("00605", 1000), ("00605", 4000), ("00605", 9000)]);
    for x in &s { assert!(nothing_reported(x), "{:?} {:?}", x.death_damage, x.death_cause); }
    assert_eq!(s[0].death_panel.as_deref(), Some("reading"));
    assert_eq!(s[3].death_panel.as_deref(), Some("unreadable"));
}

#[test]
fn nothing_carries_over_to_the_next_death() {
    // rec16: 24:08 read, alive at 24:53, then the 27:52 death with the panel collapsed.
    let s = run(&[("00600", 0), ("00600", 4000), ("00606", 45_000), ("00601", 224_000), ("00601", 231_000)]);
    assert_eq!(s[1].death_panel.as_deref(), Some("open"));
    assert!(!s[2].dead && s[2].death_panel.is_none() && nothing_reported(&s[2]));
    for x in &s[3..] { assert!(nothing_reported(x), "{:?}", x.death_damage); }
    assert_eq!(s[3].death_panel, None, "the panel may still be opening");
    assert_eq!(s[4].death_panel.as_deref(), Some("closed"));
}
