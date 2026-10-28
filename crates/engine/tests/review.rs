use coach_core::{DamageRow, GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, Pt};
use coach_engine::{review::{game_data, Catalog, ReviewCoach}, Rule};

#[test]
fn review_cards_quote_real_document_guidance() {
    let catalog: Catalog = serde_json::from_str(include_str!("../../../data/coaching/review.json")).unwrap();
    assert_eq!(catalog.schema_version, 1);
    let mut all: Vec<(String, Vec<coach_engine::laning::Source>)> = catalog.cards.iter().map(|c| (c.id.clone(), c.sources.clone())).collect();
    all.extend(catalog.threats.iter().map(|t| (format!("threat:{}", t.source), t.sources.clone())));
    all.extend(catalog.counters.iter().map(|c| (format!("counter:{}", c.need), c.sources.clone())));
    for (id, sources) in all {
        assert!(!sources.is_empty(), "{id}");
        for s in sources {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&s.path)).unwrap();
            assert!(text.contains(&s.excerpt), "{id}: missing excerpt {}", s.excerpt);
            assert!(text.contains(&s.section), "{id}: missing section {}", s.section);
        }
    }
}

#[test]
fn threats_and_counter_items_exist_in_the_game_data() {
    let catalog: Catalog = serde_json::from_str(include_str!("../../../data/coaching/review.json")).unwrap();
    let data = game_data();
    for t in &catalog.threats {
        if !t.hero.is_empty() { assert_ne!(data.hero_name(&t.hero), t.hero.trim_start_matches("hero_"), "unknown hero {}", t.hero); }
        if !t.counter.is_empty() { assert!(catalog.counters.iter().any(|c| c.need == t.counter), "{}", t.counter); }
    }
    for c in &catalog.counters {
        for name in &c.items {
            let item = data.item(name).unwrap_or_else(|| panic!("{name} is not in items.json"));
            assert!(item.cost > 0, "{name}");
            assert!(format!("{} {}", item.desc, item.effect).to_lowercase().contains(&c.keyword.to_lowercase()), "{name} lacks {}", c.keyword);
        }
    }
    for h in &catalog.roles.ranged { assert_ne!(data.hero_name(h), h.trim_start_matches("hero_"), "unknown hero {h}"); }
}

/// Replays real pipeline outputs through the review coach and prints every cue. Run with
/// `cargo test --release -p coach-engine --test review replay -- --ignored --nocapture`
/// (REVIEW_REPLAY=path1;path2 overrides the default recordings).
#[test]
#[ignore]
fn replay_recordings() {
    let paths = std::env::var("REVIEW_REPLAY").unwrap_or_else(|_| [
        r"C:\Users\arielwolle\Documents\Github\DeadlockCoach\frames\rec5\replay5.jsonl",
        r"C:\Users\arielwolle\Documents\Github\DeadlockCoach\frames\rec3\replay4.jsonl"].join(";"));
    for path in paths.split(';') {
        let Ok(text) = std::fs::read_to_string(path) else { println!("skip {path}"); continue };
        println!("===== {path}");
        let mut e = coach_engine::Engine::new(vec![Box::new(ReviewCoach::default())]);
        for line in text.lines() {
            let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
            let Ok(s) = serde_json::from_value::<GameSnapshot>(v["snapshot"].clone()) else { continue };
            let clock = s.game_clock_secs.unwrap_or(0);
            for c in e.step(&s) { println!("[{}:{:02}] {} :: {}", clock / 60, clock % 60, c.key, c.text); }
        }
    }
}

fn rows(r: &[(&str, u32)]) -> Vec<DamageRow> { r.iter().map(|(s, d)| DamageRow { source: s.to_string(), hits: None, damage: *d }).collect() }
fn enemy_team() -> Vec<String> { ["hero_drifter", "hero_artist", "hero_nurse", "hero_frank", "hero_doorman", "hero_chrono"].map(String::from).to_vec() }
fn panel(clock: u32, r: &[(&str, u32)], kb: &str, total: Option<u32>) -> GameSnapshot {
    let mut d = dead(clock, Some(r[0].0)); d.death_damage = rows(r); d.death_killing_blow = Some(kb.into()); d.death_total_damage = total;
    d.enemy_heroes = enemy_team(); d.hero = Some("hero_orion".into()); d
}

#[test]
fn the_panel_names_the_heroes_the_crowd_control_and_a_hero_specific_lesson() {
    // rec5 death 1: Victor's aura, stunned twice, knocked off the zipline; Violet, Nurse Harrow.
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.74 };
    for (t, hp, fx) in [(1489u32, 2065u32, vec![]), (1492, 1472, vec!["Aura of Suffering"]), (1493, 1417, vec!["stunned", "Aura of Suffering"]), (1494, 1069, vec![]), (1497, 369, vec!["stunned", "Aura of Suffering"])] {
        let mut s = alive(t, hp, p, Some(Lane::Yellow), vec![track(1, true, 0.31, 0.74)], team(0, 0), 600);
        s.max_hp = Some(2065); s.status_effects = fx.into_iter().map(String::from).collect(); s.hero = Some("hero_orion".into()); s.enemy_heroes = enemy_team();
        r.evaluate(&s);
    }
    let rows = [("Aura of Suffering", 789), ("Still Life", 613), ("Lurking Terrors", 540), ("Knocked off Zipline", 332), ("Sinner's Sacrifice", 320), ("Mystic Shot", 175)];
    r.evaluate(&panel(1500, &rows, "Mystic Shot", Some(4451)));
    let t = &r.evaluate(&panel(1505, &rows, "Mystic Shot", Some(4451)))[0].text;
    assert!(t.contains("Victor did at least 789 damage with Aura of Suffering, 28% of what the panel lists"), "{t}");
    assert!(t.contains("At least 3 enemy heroes hit you: Victor, Violet and Nurse Harrow"), "{t}");
    assert!(t.contains("You were stunned twice and knocked off the zipline"), "{t}");
    assert!(t.contains("inside Victor's Aura of Suffering"), "{t}");
    assert!(t.contains("As Grey Talon, back off to max range when the Aura of Suffering badge appears"), "{t}");
    assert!(t.contains("Get off the zipline before you reach a fight"), "{t}");
    assert!(!t.contains("no clear cause"), "{t}");
}

#[test]
fn a_marked_solo_kill_by_a_repeat_killer_gets_the_mark_lesson_and_a_counter_item() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.2, y: 0.68 };
    // First death to Drifter (Bloodscent: isolated).
    r.evaluate(&alive(2300, 1500, p, Some(Lane::Yellow), vec![], team(0, 0), 800));
    r.evaluate(&panel(2310, &[("Gun Damage", 604), ("Gun Damage", 417), ("Bloodscent", 241), ("Rend", 231)], "Gun Damage", None));
    let t = &r.evaluate(&panel(2318, &[("Gun Damage", 604), ("Gun Damage", 417), ("Bloodscent", 241), ("Rend", 231)], "Gun Damage", None))[0].text;
    assert!(t.contains("At least 2 enemy heroes hit you: Drifter and another hero's gun"), "{t}");
    assert!(t.contains("you were isolated"), "{t}");
    // Respawn, marked, silenced, knocked off the zipline, solo kill holding 3,467 souls.
    for (c, hp, fx) in [(2402u32, 3199u32, vec![]), (2449, 2745, vec!["silenced", "stalkers mark"]), (2459, 1831, vec!["stunned", "knocked_off_zipline"])] {
        let mut s = alive(c, hp, p, Some(Lane::Yellow), vec![], team(0, 0), 3467);
        s.max_hp = Some(3199); s.status_effects = fx.into_iter().map(String::from).collect(); s.hero = Some("hero_orion".into());
        r.evaluate(&s);
    }
    let rows = [("Stalker's Mark", 895), ("Melee", 638), ("Rend", 559), ("Knocked off Zipline", 473), ("Spirit Burn", 214)];
    r.evaluate(&panel(2463, &rows, "Spirit Burn", Some(3332)));
    let t = &r.evaluate(&panel(2468, &rows, "Spirit Burn", Some(3332)))[0].text;
    assert!(t.contains("Drifter is the only hero named on the damage panel, with Stalker's Mark and Rend: likely a solo kill"), "{t}");
    assert!(t.contains("That is 2 deaths to Drifter"), "{t}");
    assert!(t.contains("When Stalker's Mark is on you, stand next to a teammate"), "{t}");
    assert!(t.contains("Buy Dispel Magic (3,200), you had 3,467 souls"), "{t}");
    assert!(!t.contains("first big hit"), "wrong advice against a Mark commit: {t}");
}

#[test]
fn a_panel_without_named_heroes_is_reported_not_called_unclear() {
    let r = ReviewCoach::default();
    r.evaluate(&alive(900, 2000, Pt { x: 0.5, y: 0.6 }, Some(Lane::Blue), vec![], team(0, 0), 300));
    let rows = [("Trooper", 700), ("Gun Damage", 300)];
    let mut d = dead(905, Some("Trooper")); d.death_damage = super_rows(&rows);
    r.evaluate(&d);
    let mut d = dead(913, Some("Trooper")); d.death_damage = super_rows(&rows);
    let t = &r.evaluate(&d)[0].text;
    assert!(t.contains("The damage panel shows Trooper as the biggest source (700)"), "{t}");
    assert!(t.contains("Troopers did 700 damage: you were fighting inside their wave"), "{t}");
    assert!(!t.contains("no clear cause"), "{t}");
}
fn super_rows(r: &[(&str, u32)]) -> Vec<DamageRow> { rows(r) }

#[test]
fn the_pattern_names_a_repeat_killer_crowd_control_and_close_range_deaths() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.7 };
    let mut said = vec![];
    let drifter = [("Stalker's Mark", 900), ("Rend", 560), ("Melee", 600)];
    for t in (1000..=1300).step_by(2) {
        let death = (1100..1120).contains(&t) || (1260..1300).contains(&t);
        let s = if death {
            let mut d = panel(t, &drifter, "Rend", Some(2100));
            d.souls = Some(500);
            d
        } else {
            let mut s = alive(t, 2400, p, Some(Lane::Yellow), vec![track(1, true, 0.31, 0.7)], team(0, 0), 500);
            s.hero = Some("hero_orion".into()); s.enemy_heroes = enemy_team();
            if t == 1096 || t == 1256 { s.status_effects = vec!["stunned".into()]; }
            s
        };
        said.extend(r.evaluate(&s));
    }
    let p = said.iter().find(|c| c.key == "review:pattern:death:2").unwrap_or_else(|| panic!("{said:?}"));
    assert!(p.text.contains("Drifter killed you 2 times in the last"), "{}", p.text);
    assert!(p.text.contains("Buy Dispel Magic (3,200)"), "{}", p.text);
    assert!(p.text.contains("crowd-controlled before both deaths (stunned)"), "{}", p.text);
    assert!(p.text.contains("Both deaths were at close range (Stalker's Mark, Rend and Melee)"), "{}", p.text);
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
    assert!(r.evaluate(&dead(1012, Some("Sticky"))).is_empty(), "waits for the damage panel to fill in");
    let c = r.evaluate(&dead(1017, Some("Hyper Beam")));
    assert_eq!(c.len(), 1); let t = &c[0].text;
    assert!(t.starts_with("Why you died:"), "{t}");
    assert!(t.contains("from 96% to dead in 10 seconds"), "{t}"); // healthiest reading 1000 -> last alive 1010
    assert!(t.contains("their half with 4 enemies"), "{t}");
    assert!(t.contains("Damage panel: Hyper Beam"), "{t}");
    assert!(t.contains("Next time:"), "{t}");
    // Re-offered under the same per-death key while dead (another cue may take a frame's slot).
    assert_eq!(r.evaluate(&dead(1019, Some("Hyper Beam")))[0].key, c[0].key);
}

#[test]
fn the_engine_speaks_one_review_per_death_even_with_the_dead_screen_flickering() {
    let mut e = coach_engine::Engine::new(vec![Box::new(ReviewCoach::default()), Box::new(coach_engine::UnspentSoulsRule(Default::default()))]);
    e.min_gap_secs = 0;
    let p = Pt { x: 0.2, y: 0.3 };
    for (t, hp) in [(1000, 2400), (1004, 2200), (1008, 400)] { e.step(&alive(t, hp, p, Some(Lane::Green), vec![], team(0, 4), 3019)); }
    let mut d = dead(1012, Some("Hyper Beam")); d.souls = Some(3019);
    let mut reviews = e.step(&d).iter().filter(|c| c.key.starts_with("death:review")).count();
    for t in [1017u32, 1020, 1024, 1030, 1040] {
        let mut s = if t == 1020 { GameSnapshot { game_clock_secs: Some(t), screen: Some("Unknown".into()), ..Default::default() } } else { dead(t, Some("Hyper Beam")) };
        s.souls = Some(3019);
        reviews += e.step(&s).iter().filter(|c| c.key.starts_with("death:review")).count();
    }
    assert_eq!(reviews, 1, "spoken once");
}

#[test]
fn teammates_falling_and_walking_out_alone_after_respawn_are_named() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.6 };
    r.evaluate(&alive(500, 2400, p, Some(Lane::Blue), vec![track(1, true, 0.31, 0.6)], team(0, 1), 500));
    r.evaluate(&alive(505, 1500, p, Some(Lane::Blue), vec![track(1, true, 0.31, 0.6)], team(2, 1), 500));
    r.evaluate(&dead(508, Some("Melee")));
    let t = &r.evaluate(&dead(513, Some("Melee")))[0].text;
    assert!(t.contains("2 teammates died just before you"), "{t}");
    // Respawn, walk out alone, die again to melee: respawn-alone and the repeated cause.
    r.evaluate(&alive(540, 2500, Pt { x: 0.3, y: 0.9 }, None, vec![], team(0, 4), 500));
    let warn = r.evaluate(&alive(550, 2500, Pt { x: 0.2, y: 0.65 }, Some(Lane::Green), vec![track(1, true, 0.8, 0.5)], team(0, 4), 500));
    assert!(warn.iter().any(|c| c.key == "review:respawn-alone"), "{warn:?}");
    r.evaluate(&alive(556, 2300, Pt { x: 0.2, y: 0.6 }, Some(Lane::Green), vec![track(1, true, 0.8, 0.5)], team(0, 4), 500));
    r.evaluate(&dead(560, Some("Melee Killing Blow")));
    let t = &r.evaluate(&dead(565, Some("Melee Killing Blow")))[0].text;
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

#[test]
fn a_recording_gap_is_named_and_starts_a_new_death() {
    let r = ReviewCoach::default();
    let mut d = dead(1500, Some("Still Life")); d.respawn_secs = Some(40);
    r.evaluate(&d);
    let mut d = dead(1506, Some("Aura of Suffering")); d.respawn_secs = Some(34);
    let first = r.evaluate(&d); assert!(first[0].text.contains("No screenshots were recorded"), "{first:?}");
    assert!(first[0].text.contains("Aura of Suffering"), "the latest panel reading wins: {first:?}");
    // A misread respawn timer jumping up is not a new death...
    let mut d = dead(1512, Some("Aura of Suffering")); d.respawn_secs = Some(45);
    assert_eq!(r.evaluate(&d)[0].key, first[0].key);
    // ...but seven minutes later, still on a death screen (nothing recorded between), it is.
    let mut d = dead(1972, Some("Pulse Grenade")); d.respawn_secs = Some(46);
    r.evaluate(&d);
    let mut d = dead(1978, Some("Pulse Grenade")); d.respawn_secs = Some(40);
    let second = r.evaluate(&d);
    assert!(second[0].key != first[0].key, "{second:?}");
}

#[test]
fn going_back_in_at_half_health_is_called_out() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.6 };
    for (t, hp) in [(2280, 1200), (2295, 1150), (2310, 1250), (2318, 900), (2322, 150)] {
        r.evaluate(&alive(t, hp, p, Some(Lane::Blue), vec![track(1, true, 0.32, 0.6)], team(0, 1), 800));
    }
    r.evaluate(&dead(2324, Some("Gun Damage")));
    let t = &r.evaluate(&dead(2329, Some("Gun Damage")))[0].text;
    assert!(t.contains("under 50% health for the 30 seconds before"), "{t}");
    assert!(t.contains("shopping does not heal"), "{t}");
}
