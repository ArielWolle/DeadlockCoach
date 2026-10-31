//! Unstable Rift reads (crates/vision/src/rift.rs, docs/rift.md; fixtures 00610-00619 at
//! 1920x1080): the announcement text, the minimap ping on the bridge, the capture panel's
//! percentages and bounty, "RIFT SECURED!" / "RIFT LOST" (also by its red when the letters are
//! lost), the panel drawn lower under a Patron banner, and the Midboss banner that passes the pixel gate but is no Rift.
use coach_vision::{auto::AutoScanner, hudtext::View, ocr::Reader, rift, scan::Scanner};
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn reader() -> Reader { Reader::load(&root().join("data/models")).unwrap() }
fn frame(prefix: &str) -> image::DynamicImage {
    let dir = root().join("samples/frames");
    let f = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().starts_with(prefix)).unwrap();
    image::open(f.path()).unwrap()
}
/// (canonical 1600x900, full frame) of a fixture.
fn views(prefix: &str) -> (image::DynamicImage, image::DynamicImage) {
    let f = frame(prefix);
    (f.resize_exact(1600, 900, image::imageops::FilterType::Triangle), f)
}
fn panel(prefix: &str) -> Option<(Option<u8>, Option<u8>, rift::Lower)> {
    let (canon, full) = views(prefix);
    let view = View { canon: &canon, source: Some((&full, coach_perception::Bounds { x: 0, y: 0, w: full.width(), h: full.height() })) };
    let crops = rift::panel_crops(&view)?;
    let lines = reader().read_lines(&crops).unwrap();
    Some(rift::interpret(&lines, rift::outcome_pixels(&view).0))
}

#[test]
fn capture_panel_reads() {
    use rift::Lower::*;
    // rec17 11:32: ours 33% (the '%' read as a 6 on the live frame), theirs 0%, bounty 1482.
    let (a, e, l) = panel("00612").expect("panel gate");
    assert_eq!((a, e), (Some(33), Some(0))); assert_eq!(l, Bounty(1482));
    assert_eq!(panel("00613").map(|p| p.2), Some(Secured), "rec17 11:46 RIFT SECURED!");
    let (a, e, l) = panel("00614").unwrap();
    assert_eq!((a, e, l), (Some(99), Some(99), Bounty(5922)), "rec10 34:29 contested");
    assert_eq!(panel("00615").map(|p| (p.1, p.2)), Some((Some(100), Lost)), "rec16 13:24 RIFT LOST");
    assert_eq!(panel("00616").map(|p| p.2), Some(Lost), "rec6 26:24: the letters read 'RIOST', the red decides");
    let (a, e, l) = panel("00618").unwrap();
    assert_eq!((a, e, l), (Some(0), Some(43), Bounty(2592)), "rec11 19:03 they are alone in it");
    // rec17 33:03: the panel 8 px lower under the Hidden King banner; ours 9%, bounty 5922.
    let (a, _, l) = panel("00619").expect("lower panel located");
    assert_eq!((a, l), (Some(9), Bounty(5922)));
    // Midboss banner: the gate may pass, nothing is read as a Rift.
    if let Some(p) = panel("00617") { assert_eq!(p, (None, None, Unread)); }
    // No panel on the announcement frame.
    assert!(panel("00610").is_none());
}

#[test]
fn announcement_and_ping() {
    let (canon, full) = views("00610");
    let view = View { canon: &canon, source: Some((&full, coach_perception::Bounds { x: 0, y: 0, w: full.width(), h: full.height() })) };
    let l = reader().read_lines(&rift::announce_crops(&view).expect("announcement gate")).unwrap();
    assert!(rift::says_rift_opening(&l[0], &l[1]), "{l:?}");
    // The ping ring two seconds later, on the left bridge only.
    let (canon, _) = views("00611");
    let p = rift::site_pings(&coach_vision::minimap::crop_from_frame(&canon));
    assert!(p[0] >= rift::PING_MIN && p[1] < rift::PING_MIN, "{p:?}");
    let (canon, _) = views("00612");
    let p = rift::site_pings(&coach_vision::minimap::crop_from_frame(&canon));
    assert!(p[0] < rift::PING_MIN && p[1] < rift::PING_MIN, "no ping while it is open: {p:?}");
}

#[test]
fn the_scanner_reports_the_rift() {
    let mut a = AutoScanner::new(Scanner::load(&root()).unwrap(), reader());
    // The clock must be verified first (the Rift reads start at 9:50): each frame twice.
    let mut last = None;
    for (i, p) in ["00612", "00612", "00613", "00613"].iter().enumerate() { last = Some(a.scan(&frame(p), i as u64 * 1000)); if i == 1 {
        let s = last.as_ref().unwrap();
        let r = s.rift.as_ref().expect("rift state");
        assert_eq!((r.phase.as_str(), r.ally_progress, r.enemy_progress, r.bounty), ("open", Some(33), Some(0), Some(1482)), "{r:?}");
    } }
    let r = last.unwrap().rift.expect("rift state");
    assert_eq!((r.phase.as_str(), r.captured_by.as_deref()), ("captured", Some("ally")), "{r:?}");
    // The Midboss banner alone never starts a Rift.
    let mut a = AutoScanner::new(Scanner::load(&root()).unwrap(), reader());
    for i in 0..3 { let s = a.scan(&frame("00617"), i * 1000); assert!(s.rift.is_none(), "{:?}", s.rift); }
}
