//! Hero identity regressions from rec8 (Grey Talon in ally slot 0): the urn carrier's portrait
//! (mint square backdrop, different art) read Silver, Baba carrying the urn and Bebop's turned
//! portrait read Solomon; identities latch per match and a weak stream of contrary winners
//! cannot overturn them; a misread match id mid-match no longer resets them.
use coach_vision::scan::{clock_restarted, hero_key, HeroLatches, STRONG_MARGIN};
use coach_vision::{hud, ColorLibrary};
use image::DynamicImage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }

/// The fixtures are the top 84 rows of 1920x1080 frames: paste one into a black frame.
fn frame(name: &str) -> DynamicImage {
    let part = image::open(root().join("samples/frames").join(name)).unwrap().to_rgb8();
    let mut f = image::RgbImage::new(1920, 1080);
    image::imageops::overlay(&mut f, &part, 0, 0);
    DynamicImage::ImageRgb8(f)
}

/// Best score per hero over all variants (card art, critical tint, real top-bar captures),
/// sorted best first, as the scanner scores a portrait sample.
fn ranked(img: &DynamicImage, slot: usize) -> Vec<(String, f32)> {
    let lib = |d: &str| ColorLibrary::load(&root().join("data/icons").join(d), true).unwrap();
    let (card, crit) = (lib("h_card"), lib("h_crit"));
    let cx = hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).nth(slot).copied().unwrap();
    let c = hud::crop(img, hud::portrait(cx));
    let mut best: HashMap<String, f32> = HashMap::new();
    for (n, s) in card.scores(&c).into_iter().chain(crit.scores(&c)) {
        let e = best.entry(hero_key(&n).to_string()).or_insert(f32::MIN);
        *e = e.max(s);
    }
    let mut v: Vec<(String, f32)> = best.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

#[test]
fn urn_carrier_and_turned_portraits_keep_their_heroes() {
    for (name, slot, hero) in [
        ("00540_rec8-topbar-urn-carrier.jpg", 0usize, "hero_orion"),
        ("00540_rec8-topbar-urn-carrier.jpg", 1, "hero_shiv"),
        ("00540_rec8-topbar-urn-carrier.jpg", 2, "hero_bookworm"),
        ("00540_rec8-topbar-urn-carrier.jpg", 5, "hero_bebop"),
        ("00541_rec8-topbar-baba-urn.jpg", 0, "hero_orion"),
        ("00541_rec8-topbar-baba-urn.jpg", 4, "hero_baba"),
        ("00542_rec8-topbar-bebop-turned.jpg", 5, "hero_bebop"),
    ] {
        let r = ranked(&frame(name), slot);
        assert_eq!(r[0].0, hero, "{name} slot {slot}: {:?}", &r[..3]);
        // A strong sample (it can correct a wrong latch), not a coin flip with the runner-up.
        assert!(r[0].1 - r[1].1 >= STRONG_MARGIN, "{name} slot {slot}: {:?}", &r[..3]);
    }
}

fn votes_with(slot: usize, v: &[(&str, f32)]) -> Vec<HashMap<String, f32>> {
    let mut all = vec![HashMap::new(); 12];
    for (h, n) in v { all[slot].insert(h.to_string(), *n); }
    all
}

#[test]
fn a_latched_identity_survives_a_stream_of_weak_contrary_winners() {
    let mut l = HeroLatches::default();
    let mut votes = vec![HashMap::new(); 12];
    // Three confident Grey Talon samples latch slot 0.
    for _ in 0..3 {
        *votes[0].entry("hero_orion".to_string()).or_insert(0.0) += 1.0;
        l.sample(0, Some(("hero_orion", 0.2)), &votes);
    }
    assert_eq!(l.heroes()[0].as_deref(), Some("hero_orion"));
    // rec8 340-470: the urn carrier read Silver 0.62 vs Grey Talon 0.57 on every sample, long
    // enough for Silver to win any decaying vote.
    for _ in 0..150 {
        for v in votes[0].values_mut() { *v *= 0.98; }
        *votes[0].entry("hero_werewolf".to_string()).or_insert(0.0) += 1.0;
        l.sample(0, Some(("hero_werewolf", 0.05)), &votes);
    }
    assert_eq!(l.heroes()[0].as_deref(), Some("hero_orion"));
    // Isolated strong misreads (a critical-tinted frame) between correct samples do not either.
    for _ in 0..20 {
        l.sample(0, Some(("hero_werewolf", 0.15)), &votes);
        l.sample(0, Some(("hero_orion", 0.3)), &votes);
    }
    assert_eq!(l.heroes()[0].as_deref(), Some("hero_orion"));
    // Sustained strong contrary evidence does switch (a wrong early latch can be corrected).
    for _ in 0..6 { l.sample(0, Some(("hero_werewolf", 0.15)), &votes); }
    assert_eq!(l.heroes()[0].as_deref(), Some("hero_werewolf"));
}

#[test]
fn a_hero_is_in_one_slot_only() {
    let mut l = HeroLatches::default();
    // rec8 opening: ally slot 2 (Paige) was dead, slot 5 (Bebop, critical tint) voted Paige.
    let mut votes = votes_with(5, &[("hero_bookworm", 3.0)]);
    l.sample(5, Some(("hero_bookworm", 0.18)), &votes);
    assert_eq!(l.heroes()[5].as_deref(), Some("hero_bookworm"));
    // Slot 2 respawns and votes Paige: it cannot latch a hero another slot holds...
    votes[2].insert("hero_bookworm".to_string(), 3.0);
    l.sample(2, Some(("hero_bookworm", 0.27)), &votes);
    assert_eq!(l.heroes()[2], None);
    // ...until its votes are twice the holder's; the holder then latches its own next hero.
    votes[2].insert("hero_bookworm".to_string(), 6.0);
    votes[5].insert("hero_bebop".to_string(), 4.0);
    l.sample(2, Some(("hero_bookworm", 0.27)), &votes);
    assert_eq!(l.heroes()[2].as_deref(), Some("hero_bookworm"));
    assert_eq!(l.heroes()[5].as_deref(), Some("hero_bebop"));
}

#[test]
fn scoreboard_names_latch_and_switch_identities() {
    let mut l = HeroLatches::default();
    let votes = votes_with(3, &[("hero_werewolf", 4.0)]);
    l.sample(3, Some(("hero_werewolf", 0.1)), &votes);
    assert_eq!(l.heroes()[3].as_deref(), Some("hero_werewolf"));
    l.board(3, "hero_priest", &votes);
    assert_eq!(l.heroes()[3].as_deref(), Some("hero_werewolf"), "one name read is not enough");
    l.board(3, "hero_priest", &votes);
    assert_eq!(l.heroes()[3].as_deref(), Some("hero_priest"));
    // An unlatched slot takes a name at once, from another slot if needed.
    l.board(4, "hero_priest", &votes);
    assert_eq!(l.heroes()[4].as_deref(), Some("hero_priest"));
    assert_ne!(l.heroes()[3].as_deref(), Some("hero_priest"));
}

#[test]
fn a_misread_match_id_mid_match_is_not_a_new_match() {
    // rec8: verified clock 18:36 (1116 s), highest 18:34 seen; the id read 6566052 twice.
    assert!(!clock_restarted(Some(1116), Some(1114)));
    // A clock that went back by minutes is a new match (or a rewind).
    assert!(clock_restarted(Some(40), Some(1800)));
    // The very first match, and no verified clock at all.
    assert!(clock_restarted(Some(40), None));
    assert!(clock_restarted(None, None));
    // An unknown clock mid-match (death screen, overlays) is no evidence.
    assert!(!clock_restarted(None, Some(1800)));
}
