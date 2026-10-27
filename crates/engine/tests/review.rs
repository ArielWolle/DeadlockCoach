use coach_core::{GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, Pt};
use coach_engine::{review::{Catalog, ReviewCoach}, Rule};

#[test]
fn review_cards_quote_real_document_guidance() {
    let catalog: Catalog = serde_json::from_str(include_str!("../../../data/coaching/review.json")).unwrap();
    assert_eq!(catalog.schema_version, 1);
    for c in catalog.cards {
        assert!(!c.sources.is_empty(), "{}", c.id);
        for s in c.sources {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&s.path)).unwrap();
            assert!(text.contains(&s.excerpt), "{}: missing excerpt {}", c.id, s.excerpt);
            assert!(text.contains(&s.section), "{}: missing section {}", c.id, s.section);
        }
    }
}

/// Top bar: `allies_dead` of slots 0-5 dead, `unseen` alive enemies without a health bar.
fn team(allies_dead: usize, unseen: usize) -> Vec<HeroStatus> {
    (0..12).map(|i| HeroStatus { ally: i < 6, slot: i % 6, dead: Some(i < 6 && i < allies_dead),
        bar_visible: Some(!(i >= 6 && i - 6 < unseen)), ..Default::default() }).collect()
}
fn track(id: u32, ally: bool, x: f32, y: f32) -> HeroTrack { HeroTrack { id, ally, pos: Pt { x, y }, hits: 3, ..Default::default() } }
fn alive(clock: u32, hp: u32, you: Pt, lane: Option<Lane>, heroes: Vec<HeroTrack>, status: Vec<HeroStatus>, souls: u32) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(clock), screen: Some("Gameplay".into()), hp: Some(hp), max_hp: Some(2500), health_observations: 3, souls: Some(souls),
        hero_status: status, minimap: Some(MinimapState { observations: 3, you: Some(you), your_lane: lane, heroes, ..Default::default() }), ..Default::default() }
}
fn dead(clock: u32, cause: Option<&str>) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(clock), screen: Some("Dead".into()), dead: true, death_cause: cause.map(str::to_string), ..Default::default() }
}

#[test]
fn a_death_is_explained_once_from_the_seconds_before_it() {
    let r = ReviewCoach::default();
    // Alone in their half with 4 enemies unseen, 2,200 -> 0 in 8 s, holding 3,019 souls.
    let p = Pt { x: 0.2, y: 0.3 };
    for (t, hp) in [(1000, 2400), (1004, 2200), (1008, 1300), (1010, 400)] {
        assert!(r.evaluate(&alive(t, hp, p, Some(Lane::Green), vec![track(1, true, 0.6, 0.6)], team(0, 4), 3019)).is_empty());
    }
    let c = r.evaluate(&dead(1012, Some("Hyper Beam")));
    assert_eq!(c.len(), 1); let t = &c[0].text;
    assert!(t.starts_with("Why you died:"), "{t}");
    assert!(t.contains("dead in 6 seconds"), "{t}"); // 2,200 at 1004 -> last alive 1010
    assert!(t.contains("their half with 4 enemies"), "{t}");
    assert!(t.contains("Damage panel: Hyper Beam"), "{t}");
    assert!(t.contains("Next time:"), "{t}");
    // Re-offered under the same per-death key while dead (another cue may take a frame's slot).
    assert_eq!(r.evaluate(&dead(1014, Some("Hyper Beam")))[0].key, c[0].key);
}

#[test]
fn the_engine_speaks_one_review_per_death_even_with_the_dead_screen_flickering() {
    let mut e = coach_engine::Engine::new(vec![Box::new(ReviewCoach::default()), Box::new(coach_engine::UnspentSoulsRule(Default::default()))]);
    e.min_gap_secs = 0;
    let p = Pt { x: 0.2, y: 0.3 };
    for (t, hp) in [(1000, 2400), (1004, 2200), (1008, 400)] { e.step(&alive(t, hp, p, Some(Lane::Green), vec![], team(0, 4), 3019)); }
    let mut d = dead(1012, Some("Hyper Beam")); d.souls = Some(3019);
    let first = e.step(&d);
    assert!(first[0].key.starts_with("death:review"), "the review beats the dead-with-souls reminder: {first:?}");
    let mut reviews = 0;
    for t in [1016u32, 1020, 1024, 1030, 1040] {
        let mut s = if t == 1020 { GameSnapshot { game_clock_secs: Some(t), screen: Some("Unknown".into()), ..Default::default() } } else { dead(t, Some("Hyper Beam")) };
        s.souls = Some(3019);
        reviews += e.step(&s).iter().filter(|c| c.key.starts_with("death:review")).count();
    }
    assert_eq!(reviews, 0, "spoken once");
}

#[test]
fn teammates_falling_and_walking_out_alone_after_respawn_are_named() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.6 };
    r.evaluate(&alive(500, 2400, p, Some(Lane::Blue), vec![track(1, true, 0.31, 0.6)], team(0, 1), 500));
    r.evaluate(&alive(505, 1500, p, Some(Lane::Blue), vec![track(1, true, 0.31, 0.6)], team(2, 1), 500));
    let t = &r.evaluate(&dead(508, Some("Melee")))[0].text;
    assert!(t.contains("2 teammates died just before you"), "{t}");
    // Respawn, walk out alone, die again to melee: respawn-alone and the repeated cause.
    r.evaluate(&alive(540, 2500, Pt { x: 0.3, y: 0.9 }, None, vec![], team(0, 4), 500));
    let warn = r.evaluate(&alive(550, 2500, Pt { x: 0.2, y: 0.65 }, Some(Lane::Green), vec![track(1, true, 0.8, 0.5)], team(0, 4), 500));
    assert!(warn.iter().any(|c| c.key == "review:respawn-alone"), "{warn:?}");
    r.evaluate(&alive(556, 2300, Pt { x: 0.2, y: 0.6 }, Some(Lane::Green), vec![track(1, true, 0.8, 0.5)], team(0, 4), 500));
    let t = &r.evaluate(&dead(560, Some("Melee Killing Blow")))[0].text;
    assert!(t.contains("alone 16 seconds after respawning"), "{t}"); // respawn 540 -> last alive 556
    assert!(t.contains("2 deaths to melee hits"), "{t}");
}

#[test]
fn the_play_pattern_reports_jungle_time_and_missed_fights() {
    let r = ReviewCoach::default();
    let mut said = vec![];
    for t in (0..=300).step_by(5) {
        // Jungle all along; from 100 s, a 2v2 at the far side twice, separated by quiet time.
        let fight = (100..130).contains(&t) || (200..230).contains(&t);
        let mut h = vec![track(1, true, 0.8, 0.3)];
        if fight { h.extend([track(2, true, 0.82, 0.3), track(3, false, 0.8, 0.32), track(4, false, 0.83, 0.33)]); }
        said.extend(r.evaluate(&alive(1000 + t, 2400, Pt { x: 0.3, y: 0.55 }, None, h, team(0, 0), 500)));
    }
    let p = said.iter().find(|c| c.key.starts_with("review:pattern")).expect("pattern after five minutes");
    assert!(p.text.contains("fought 2 times without you"), "{}", p.text);
    assert!(p.text.contains("100% of the last") && p.text.contains("in the jungle"), "{}", p.text);
}

#[test]
fn fight_lost_is_called_live_when_two_teammates_fall_beside_you() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.5, y: 0.5 };
    let foe = vec![track(9, false, 0.55, 0.5)];
    assert!(r.evaluate(&alive(700, 2000, p, Some(Lane::Blue), foe.clone(), team(0, 0), 500)).is_empty());
    let c = r.evaluate(&alive(706, 1800, p, Some(Lane::Blue), foe, team(2, 0), 500));
    assert!(c.iter().any(|c| c.key == "review:fight-lost" && c.text.contains("2 teammates just died")), "{c:?}");
}
