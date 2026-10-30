//! Game-state fields of the snapshot: match result (end banner), lane structures (minimap
//! diamonds) and your ability bar. Fixtures: samples/frames 00508-00574, samples/minimap rec13-*.
//! The Midboss taker is covered by unit tests in `objectives.rs` (it needs a timeline).
use coach_core::GameSnapshot;
use coach_vision::structures::{self, Read, SITES};
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
/// Abilities as "R" (ready), seconds, or "X" (not castable, no seconds).
fn bar(s: &GameSnapshot) -> Vec<String> {
    s.abilities.iter().map(|a| match (a.ready, a.cooldown_secs) { (Some(true), _) => "R".into(), (_, Some(c)) => c.to_string(), _ => "X".into() }).collect()
}

#[test]
fn banner_word_names_the_result() {
    use coach_vision::screen::post_game_result;
    assert_eq!(post_game_result("Victory"), Some("win"));
    assert_eq!(post_game_result("VICT0RY"), Some("win"));
    assert_eq!(post_game_result("Defeat"), Some("loss"));
    assert_eq!(post_game_result("Defeot"), Some("loss"));
    assert_eq!(post_game_result("Delay"), None);
    assert_eq!(post_game_result(""), None);
}

#[test]
fn defeat_banner_over_death_screen_and_scoreboard_is_a_loss() {
    // rec13 590 / 591: the match ended while dead / holding Tab.
    for p in ["00570", "00571"] {
        let s = twice(p);
        assert_eq!(s.screen.as_deref(), Some("PostGame"), "{p}");
        assert_eq!(s.match_result.as_deref(), Some("loss"), "{p}");
    }
    let s = twice("00526"); // rec5 Defeat
    assert_eq!(s.match_result.as_deref(), Some("loss"));
}

#[test]
fn victory_banner_is_a_win() {
    // rec12 592: the end of a won match.
    let s = twice("00574");
    assert_eq!(s.screen.as_deref(), Some("PostGame"));
    assert_eq!(s.match_result.as_deref(), Some("win"));
}

#[test]
fn post_game_summary_keeps_the_banner_result_and_gameplay_has_none() {
    let mut a = scanner();
    let (banner, summary) = (frame("00570"), frame("00572"));
    a.scan(&banner, 0);
    assert_eq!(a.scan(&banner, 1000).match_result.as_deref(), Some("loss"));
    // The PLAY AGAIN summary has no Victory / Defeat word of its own.
    a.scan(&summary, 5000);
    let s = a.scan(&summary, 6000);
    assert_eq!(s.screen.as_deref(), Some("PostGame"));
    assert_eq!(s.match_result.as_deref(), Some("loss"));
    // A fresh scanner on the summary alone does not guess.
    assert_eq!(twice("00572").match_result, None);
    assert_eq!(twice("00520").match_result, None);
}

fn minimap(name: &str) -> image::RgbImage { image::open(root().join("samples/minimap").join(name)).unwrap().to_rgb8() }
fn site(own: bool, kind: structures::Kind, side: Option<usize>) -> usize {
    SITES.iter().position(|s| s.own == own && s.kind == kind && s.side == side).unwrap()
}

#[test]
fn every_structure_stands_early() {
    // rec13 frame 40 (~4:40): all 24 diamonds drawn.
    let r = structures::read(&minimap("rec13-040-structures-standing.png")).expect("a map frame");
    let alive = r.iter().filter(|x| **x == Read::Alive).count();
    assert!(r.iter().all(|x| *x != Read::Gone), "{r:?}");
    assert!(alive >= 22, "{alive} of 24 read standing: {r:?}");
}

#[test]
fn fallen_structures_are_gone_late() {
    // rec13 frame 520 (36:40, a loss after a long comeback): every enemy lane structure fell,
    // their Shrines and Patron stand; you kept your right Base Guardian, Shrines and Patron.
    use structures::Kind::*;
    let r = structures::read(&minimap("rec13-520-structures-late.png")).expect("a map frame");
    let standing = [site(false, Shrine, None), site(false, Patron, None), site(true, BaseGuardian, Some(2)), site(true, Shrine, None), site(true, Patron, None)];
    let mut decisive = 0;
    for (i, x) in r.iter().enumerate() {
        let truth_alive = standing.contains(&i) || (SITES[i].kind == Shrine);
        match x {
            Read::Alive => { decisive += 1; assert!(truth_alive, "site {i} {:?} read standing", SITES[i]); }
            Read::Gone => { decisive += 1; assert!(!truth_alive, "site {i} {:?} read gone", SITES[i]); }
            Read::Unclear => {}
        }
    }
    assert!(decisive >= 18, "only {decisive} sites decided: {r:?}");
}

#[test]
fn no_map_reads_no_structures() {
    // Black / menu frames: the Patrons are not drawn, so nothing is read (no false "destroyed").
    assert!(structures::read(&image::RgbImage::new(402, 420)).is_none());
    let mut t = structures::Tracker::default();
    for k in 0..5 { t.observe(structures::read(&image::RgbImage::new(402, 420)).as_ref(), k * 4000); }
    assert!(t.states(20_000, Some(false)).is_empty());
}

#[test]
fn snapshot_lists_structures_with_lane_kind_and_team() {
    // A live frame with the minimap: structures are listed (needs two Alive reads).
    let f = frame("00553");
    let mut a = scanner();
    a.scan(&f, 0);
    let s = a.scan(&f, 4000);
    assert!(!s.structures.is_empty());
    for st in &s.structures {
        assert!(["guardian", "walker", "shrine", "patron"].contains(&st.kind.as_str()), "{st:?}");
        assert_eq!(st.lane.is_none(), matches!(st.kind.as_str(), "shrine" | "patron"), "{st:?}");
    }
    assert!(s.structures.iter().any(|x| x.own && x.kind == "patron" && x.alive));
    assert!(s.structures.iter().any(|x| !x.own && x.kind == "patron" && x.alive));
}

#[test]
fn ability_bar_ready_and_cooldowns() {
    // Verified against the frames; slot 4 is the ultimate. Decimals round up ("1.0" -> 1).
    assert_eq!(bar(&twice("00553")), ["R", "1", "9", "51"]); // rec8: 1.0, 9, 51
    assert_eq!(bar(&twice("00560")), ["1", "R", "19", "R"]); // rec11: 0.7 (slot 2 lit: active)
    assert_eq!(bar(&twice("00550")), ["4", "R", "5", "R"]); // rec8 15:31
    assert_eq!(bar(&twice("00554")), ["R", "4", "R", "R"]); // rec8
    assert_eq!(bar(&twice("00508")), ["R", "R", "R", "63"]); // rec5
    assert_eq!(bar(&twice("00531")), ["R", "R", "R", "37"]); // rec6
    assert_eq!(bar(&twice("00551")), ["R", "R", "R", "8"]); // rec8
    assert_eq!(bar(&twice("00520")), ["R", "R", "R", "R"]);
}

#[test]
fn ability_bar_is_omitted_without_the_live_hud() {
    for p in ["00505", "00521", "00570", "00572"] { // death screen, shop, banner, summary
        assert!(twice(p).abilities.is_empty(), "{p}");
    }
}
