//! Top bar, team totals, per-hero net worth, dead slots / respawn timers and the Tab scoreboard on
//! a recorded 1920x1080 match (rec5). Truth values were read by hand from the screenshots.
use coach_core::HeroStatus;
use coach_vision::{auto::AutoScanner, hud, ocr::Reader, scan::Scanner, scoreboard, team};
use image::DynamicImage;

fn root() -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn fixture(name: &str) -> DynamicImage { image::open(root().join("samples/frames").join(name)).unwrap() }
fn canonical(name: &str) -> DynamicImage { DynamicImage::ImageRgba8(fixture(name).thumbnail_exact(1600, 900).to_rgba8()) }
fn auto() -> AutoScanner { AutoScanner::new(Scanner::load(&root()).unwrap(), Reader::load(&root().join("data/models")).unwrap()) }
/// Scan a still frame `n` times, one second apart, with the full-frame scanner on the 1600x900
/// canonical image (the automatic scanner needs earlier frames to anchor a lone death screen).
fn scan_canonical(name: &str, n: u64) -> coach_core::GameSnapshot {
    let (frame, mut sc) = (canonical(name), Scanner::load(&root()).unwrap());
    let mut s = Default::default();
    for t in 0..n { s = sc.scan_image(&frame, None, t * 1000); }
    s
}
/// Scan a still frame `n` times, one second apart.
fn scan_still(name: &str, n: u64) -> coach_core::GameSnapshot {
    let (frame, mut a) = (fixture(name), auto());
    let mut s = Default::default();
    for t in 0..n { s = a.scan(&frame, t * 1000); }
    s
}
fn slot(s: &coach_core::GameSnapshot, i: usize) -> HeroStatus {
    s.hero_status.iter().find(|h| h.ally == (i < 6) && h.slot == i % 6).cloned().unwrap_or_else(|| panic!("slot {i} missing: {:?}", s.hero_status.len()))
}

// ---- Finding 1: team totals ----

#[test]
fn three_digit_team_totals_are_thousands_not_the_first_two_digits() {
    // "184" at 33:45 is 184k: 18k is far below what a team can have by then (rec5 reported 18k).
    assert_eq!(hud::team_total_candidates_at("184", Some(2025)), vec![184_000]);
    assert_eq!(hud::team_total_candidates_at("270 ", Some(2664)), vec![270_000]);
    // Early game the 2-digit reading with a misread "k" stays possible.
    assert!(hud::team_total_candidates_at("231", Some(300)).contains(&23_000));
    assert!(!hud::team_total_candidates_at("231", Some(300)).contains(&231_000));
}

#[test]
fn team_total_band_widens_with_time_since_the_last_confirmed_read() {
    // 123k before a 7-minute recording gap, 184k after it: a plausible rise for 446 s.
    let (mut prev, mut pending) = (Some(123_000), None);
    assert_eq!(coach_vision::scan::plausible(&mut prev, &mut pending, Some(184_000), 446), Some(184_000));
    // The same jump within 2 s is a leap and needs confirmation.
    let (mut prev, mut pending) = (Some(123_000), None);
    assert_eq!(coach_vision::scan::plausible(&mut prev, &mut pending, Some(184_000), 2), None);
}

#[test]
fn team_totals_after_the_recording_gap_read_in_full() {
    let s = scan_still("00506_rec5-totals-after-gap.jpg", 6);
    assert_eq!(s.ally_networth, Some(184_000));
    assert_eq!(s.enemy_networth, Some(197_000));
}

// ---- Finding 2: per-hero net worth ----

#[test]
fn per_hero_label_candidates() {
    assert_eq!(team::hero_worth_candidates("13", 1074), vec![13_000]);
    // "51k" with the small "k" read as a digit.
    assert_eq!(team::hero_worth_candidates("511", 2664), vec![51_000]);
    assert_eq!(team::hero_worth_candidates("9.9", 540), vec![9_900]);
    // "1k" at 17:54 is not a plausible earned-souls value (rec5 reported A5 = 1000).
    assert!(team::hero_worth_candidates("1k", 1074).is_empty());
}

#[test]
fn per_hero_networth_needs_a_repeat_and_resyncs_after_a_wrong_baseline() {
    let mut s = vec![HeroStatus::default()];
    team::apply_networth(&["32".into()], 2000, &mut s);
    assert_eq!(s[0].networth, None, "a first read needs a repeat");
    team::apply_networth(&["32".into()], 2002, &mut s);
    assert_eq!(s[0].networth, Some(32_000));
    // A failed read keeps the value briefly...
    team::apply_networth(&["".into()], 2010, &mut s);
    assert_eq!(s[0].networth, Some(32_000));
    // ...a lower value (rec5 "27" misread for "32") is held back, but two in a row replace it.
    team::apply_networth(&["21".into()], 2012, &mut s);
    assert_eq!(s[0].networth, Some(32_000));
    team::apply_networth(&["21".into()], 2014, &mut s);
    assert_eq!(s[0].networth, Some(21_000));
    // ...and a value not refreshed for over 45 s of game time expires.
    team::apply_networth(&["".into()], 2070, &mut s);
    assert_eq!(s[0].networth, None);
}

#[test]
fn all_twelve_per_hero_labels_are_read_and_own_networth_follows_your_slot() {
    let s = scan_still("00508_rec5-four-dead-labels.jpg", 6);
    let want = [38, 41, 40, 38, 47, 37, 49, 40, 47, 45, 38, 42];
    let got: Vec<Option<u32>> = (0..12).map(|i| slot(&s, i).networth).collect();
    assert_eq!(got, want.iter().map(|k| Some(k * 1000)).collect::<Vec<_>>());
}

// ---- Findings 3 and 5: dead slots and respawn timers ----

#[test]
fn dark_portraits_are_dead_by_their_respawn_timer() {
    // 40:22: A2 (14), A3 (50, dark Haze portrait), A5 (23, Silver) and E4 (7) are dead; the
    // old greyness check found only A2.
    let img = canonical("00508_rec5-four-dead-labels.jpg").to_rgb8();
    let dead: Vec<usize> = (0..12).filter(|i| team::read_bar(&img, *i).dead).collect();
    assert_eq!(dead, vec![2, 3, 5, 10]);
}

#[test]
fn death_screen_top_bar_is_realigned_and_read() {
    // 25:12, own death screen: the top bar is drawn 7 px lower; A0-A3 and E2 are dead.
    let img = canonical("00505_rec5-dead-screen-topbar-low.jpg");
    let dy = team::top_bar_offset(&img);
    assert!((6..=8).contains(&dy), "offset {dy}");
    let aligned = team::top_bar_aligned(&img, dy).to_rgb8();
    let dead: Vec<usize> = (0..12).filter(|i| team::read_bar(&aligned, *i).dead).collect();
    assert_eq!(dead, vec![0, 1, 2, 3, 8]);
    // A normal frame has no offset.
    assert_eq!(team::top_bar_offset(&canonical("00508_rec5-four-dead-labels.jpg")), 0);
}

#[test]
fn scanner_reports_dead_slots_timers_and_no_live_health_on_the_death_screen() {
    let s = scan_canonical("00505_rec5-dead-screen-topbar-low.jpg", 4);
    assert_eq!(s.screen.as_deref(), Some("Dead"));
    assert_eq!(s.hero_status.len(), 12, "top bar must be read on death screens");
    let truth = [(0, 40), (1, 53), (2, 38), (3, 21), (8, 51)];
    for i in 0..12 {
        let h = slot(&s, i);
        match truth.iter().find(|(k, _)| *k == i) {
            Some((_, t)) => {
                assert_eq!(h.dead, Some(true), "slot {i}");
                assert!(h.health_ratio.is_none(), "dead slot {i} reports health {:?}", h.health_ratio);
                if let Some(r) = h.respawn_secs { assert!(r.abs_diff(*t) <= 2, "slot {i}: {r} vs {t}"); }
            }
            None => assert_ne!(h.dead, Some(true), "slot {i} is alive"),
        }
    }
    let timed = truth.iter().filter(|(k, _)| slot(&s, *k).respawn_secs.is_some()).count();
    assert!(timed >= 3, "only {timed} timers");
}

#[test]
fn scanner_flags_all_four_dead_slots_with_their_timers() {
    let s = scan_still("00508_rec5-four-dead-labels.jpg", 4);
    for (i, t) in [(2, 14), (3, 50), (5, 23), (10, 7)] {
        let h = slot(&s, i);
        assert_eq!(h.dead, Some(true), "slot {i}");
        assert!(h.health_ratio.is_none());
        assert!(h.respawn_secs.is_some_and(|r| r.abs_diff(t) <= 2), "slot {i}: {:?} vs {t}", h.respawn_secs);
    }
    assert_eq!((0..12).filter(|i| slot(&s, *i).dead == Some(true)).count(), 4);
}

#[test]
fn respawn_timer_follows_the_countdown_and_rejects_digit_drops() {
    let mut s = vec![HeroStatus::default()];
    let read = |s: &mut Vec<HeroStatus>, text: &str, ms: u64| { team::apply_respawn(&[text.to_string()], &[0], ms, s); s[0].respawn_secs };
    assert_eq!(read(&mut s, "14", 0), None, "one read is not yet a countdown");
    assert_eq!(read(&mut s, "13", 1000), Some(13));
    // rec5: "14" read as "4" -- the countdown continues instead.
    assert_eq!(read(&mut s, "4", 2000), Some(12));
    assert_eq!(read(&mut s, "", 3000), Some(11));
    assert_eq!(read(&mut s, "10", 4000), Some(10));
    assert!(s[0].respawn_observations >= 3);
}

#[test]
fn a_dropped_leading_one_is_restored_from_the_digit_glyphs() {
    // 40:22, A2 shows "14": glyph widths 8 ("1") and 13 ("4"); the recognizer reads "4".
    let img = canonical("00508_rec5-four-dead-labels.jpg");
    let w = team::timer_digit_widths(&img, hud::ALLY_CENTERS[2]);
    assert_eq!(w.len(), 2, "{w:?}");
    assert_eq!(team::check_timer_read("4", &w).as_deref(), Some("14"));
    assert_eq!(team::check_timer_read("14", &w).as_deref(), Some("14"));
    // E4 shows a single "7".
    let w = team::timer_digit_widths(&img, hud::ENEMY_CENTERS[4]);
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(team::check_timer_read("77", &w), None);
    assert_eq!(team::check_timer_read("4", &[12, 13]), None, "two wide glyphs: not a dropped 1");
}

// ---- Finding 6: scoreboard ----

const KDA_463: [[u32; 3]; 12] = [[4, 5, 6], [6, 9, 7], [6, 2, 7], [4, 9, 4], [0, 4, 7], [3, 7, 7], [7, 3, 13], [11, 2, 17], [2, 4, 12], [10, 4, 18], [2, 6, 16], [4, 4, 6]];

#[test]
fn scoreboard_kda_digits_are_read_from_glyph_templates() {
    let got = scoreboard::read_kda(&canonical("00507_rec5-scoreboard-kda.jpg"));
    let want: Vec<[Option<u32>; 3]> = KDA_463.iter().map(|r| r.map(Some)).collect();
    assert_eq!(got, want);
}

#[test]
fn hero_name_lines_tolerate_dropped_letters_but_not_ties() {
    let names: Vec<(String, String)> = [("hero_artist", "Violet"), ("hero_wraith", "Wraith"), ("hero_haze", "Haze"), ("hero_drifter", "Drifter")]
        .iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(scoreboard::match_hero("IOLT", &names).as_deref(), Some("hero_artist"));
    assert_eq!(scoreboard::match_hero("WRATH", &names).as_deref(), Some("hero_wraith"));
    assert_eq!(scoreboard::match_hero("HAZ", &names).as_deref(), Some("hero_haze"));
    assert_eq!(scoreboard::match_hero("xx", &names), None);
}

#[test]
fn scanner_fills_players_kda_heroes_and_enemy_items_from_the_scoreboard() {
    let s = scan_still("00507_rec5-scoreboard-kda.jpg", 3);
    // Tab over the death screen ("Respawn in: 28" is visible): read as a scoreboard either way.
    assert!(matches!(s.screen.as_deref(), Some("Scoreboard" | "Dead")), "{:?}", s.screen);
    assert_eq!(s.players.len(), 12);
    let (mut right, mut wrong) = (0, 0);
    for (p, t) in s.players.iter().zip(KDA_463) {
        for (g, w) in [p.kills, p.deaths, p.assists].into_iter().zip(t) { match g { Some(v) if v == w => right += 1, Some(_) => wrong += 1, None => {} } }
    }
    assert_eq!(wrong, 0);
    assert!(right >= 33, "{right}/36");
    let heroes = ["hero_orion", "hero_dynamo", "hero_wraith", "hero_haze", "hero_ratking", "hero_werewolf", "hero_drifter", "hero_artist", "hero_nurse", "hero_frank", "hero_doorman", "hero_chrono"];
    let named = s.players.iter().zip(heroes).filter(|(p, h)| p.hero.as_deref() == Some(*h)).count();
    let misnamed = s.players.iter().zip(heroes).filter(|(p, h)| p.hero.as_deref().is_some_and(|x| x != *h)).count();
    assert!(named >= 10 && misnamed == 0, "{named} named, {misnamed} wrong");
    assert!(s.enemy_items.len() >= 5, "{:?}", s.enemy_items);
}
