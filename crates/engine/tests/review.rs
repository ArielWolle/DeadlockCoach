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

/// A death review is one short coach sentence: cause, next life, buy (docs/coach-review-13.md).
fn short(t: &str) {
    assert!(t.split_whitespace().count() <= coach_engine::review::REVIEW_WORDS, "{} words: {t}", t.split_whitespace().count());
    assert!(t.starts_with("You died because") || t.starts_with("No frames"), "{t}");
    assert!(!t.contains('{'), "{t}");
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
    // One cause, the most important: the knock-off that put him in the aura.
    assert!(t.starts_with("You died because Victor knocked you off the zipline into the fight. Next life: hop off before the fight"), "{t}");
    short(t);
}

#[test]
fn a_marked_solo_kill_by_a_repeat_killer_gets_the_mark_lesson_and_a_counter_item() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.2, y: 0.68 };
    // First death to Drifter (Bloodscent: isolated).
    r.evaluate(&alive(2300, 1500, p, Some(Lane::Yellow), vec![], team(0, 0), 800));
    r.evaluate(&panel(2310, &[("Gun Damage", 604), ("Gun Damage", 417), ("Bloodscent", 241), ("Rend", 231)], "Gun Damage", None));
    let t = &r.evaluate(&panel(2318, &[("Gun Damage", 604), ("Gun Damage", 417), ("Bloodscent", 241), ("Rend", 231)], "Gun Damage", None))[0].text;
    assert!(t.contains("You died because Drifter caught you isolated. Next life: stay within 20 meters of a teammate."), "{t}");
    short(t);
    // Respawn, marked, silenced, knocked off the zipline, solo kill holding 3,467 souls.
    for (c, hp, fx) in [(2402u32, 3199u32, vec![]), (2449, 2745, vec!["silenced", "stalkers mark"]), (2459, 1831, vec!["stunned", "knocked_off_zipline"])] {
        let mut s = alive(c, hp, p, Some(Lane::Yellow), vec![], team(0, 0), 3467);
        s.max_hp = Some(3199); s.status_effects = fx.into_iter().map(String::from).collect(); s.hero = Some("hero_orion".into());
        r.evaluate(&s);
    }
    let rows = [("Stalker's Mark", 895), ("Melee", 638), ("Rend", 559), ("Knocked off Zipline", 473), ("Spirit Burn", 214)];
    r.evaluate(&panel(2463, &rows, "Spirit Burn", Some(3332)));
    let t = &r.evaluate(&panel(2468, &rows, "Spirit Burn", Some(3332)))[0].text;
    // Knocked off the zipline is the cause; the repeat killer's counter item is the buy line.
    assert!(t.starts_with("You died because Drifter knocked you off the zipline into the fight."), "{t}");
    assert!(t.ends_with("Buy Dispel Magic now."), "{t}");
    short(t);
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
    assert_eq!(t, "You died because you fought inside their wave. Next life: fight from behind our wave.");
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
    // In their half with four missing beats the burst and being alone; the souls are the buy line.
    assert_eq!(t, "You died because you were in their half with 4 enemies missing. Next life: when enemies are missing, stay behind your wave. Spend 3,019 souls now.");
    short(t);
    // Re-offered under the same per-death key while dead (another cue may take a frame's slot).
    assert_eq!(r.evaluate(&dead(1019, Some("Hyper Beam")))[0].key, c[0].key);
}

#[test]
fn the_engine_speaks_one_review_per_death_even_with_the_dead_screen_flickering() {
    let mut e = coach_engine::Engine::new(vec![Box::new(ReviewCoach::default()), Box::new(coach_engine::UnspentSoulsRule::new(Default::default()))]);
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
    assert!(t.contains("You died because 2 teammates fell and you stayed in."), "{t}");
    // Respawn, walk out alone, die again to melee: respawn-alone and the repeated cause.
    r.evaluate(&alive(540, 2500, Pt { x: 0.3, y: 0.9 }, None, vec![], team(0, 4), 500));
    let warn = r.evaluate(&alive(550, 2500, Pt { x: 0.2, y: 0.65 }, Some(Lane::Green), vec![track(1, true, 0.8, 0.5)], team(0, 4), 500));
    assert!(warn.iter().any(|c| c.key == "review:respawn-alone"), "{warn:?}");
    r.evaluate(&alive(556, 2300, Pt { x: 0.2, y: 0.6 }, Some(Lane::Green), vec![track(1, true, 0.8, 0.5)], team(0, 4), 500));
    r.evaluate(&dead(560, Some("Melee Killing Blow")));
    let t = &r.evaluate(&dead(565, Some("Melee Killing Blow")))[0].text;
    assert!(t.contains("You died because you walked out alone 16 seconds after respawning."), "{t}"); // respawn 540 -> last alive 556
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
    assert!(c.iter().any(|c| c.key == "review:fight-lost" && c.text == "Back out: 2 teammates are down."), "{c:?}");
}

#[test]
fn a_recording_gap_is_named_and_starts_a_new_death() {
    let r = ReviewCoach::default();
    let mut d = dead(1500, Some("Still Life")); d.respawn_secs = Some(40);
    r.evaluate(&d);
    let mut d = dead(1506, Some("Aura of Suffering")); d.respawn_secs = Some(34);
    let first = r.evaluate(&d); assert!(first[0].text.starts_with("No frames before this death. You died because"), "{first:?}");
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

/// Runs `frame(t)` every 5 s over [from, to] and returns every pattern summary offered.
fn patterns(from: u32, to: u32, frame: &dyn Fn(u32) -> GameSnapshot) -> Vec<String> {
    let r = ReviewCoach::default();
    let mut out = vec![];
    for t in (from..=to).step_by(5) { out.extend(r.evaluate(&frame(t)).into_iter().filter(|c| c.key.starts_with("review:pattern")).map(|c| c.text)); }
    out
}
fn lane_frame(t: u32, souls: u32) -> GameSnapshot { alive(t, 2400, Pt { x: 0.5, y: 0.6 }, Some(Lane::Blue), vec![track(1, true, 0.52, 0.6)], team(0, 0), souls) }

#[test]
fn the_hoarding_summary_names_the_tier_that_applied() {
    // rec8 22:07: "held over 6,400 souls ... up to 5,130": 5,130 was over the 3,200 tier before
    // 20:00, never over 6,400.
    let p = patterns(1000, 1300, &|t| lane_frame(t, 5130));
    let h = p.iter().find(|t| t.contains("You held over")).unwrap_or_else(|| panic!("{p:?}"));
    assert!(h.contains("You held over 3,200 souls for") && h.contains("up to 5,130"), "{h}");
}
#[test]
fn time_in_the_shop_while_dead_counts_as_dead() {
    // rec8 42:44: "dead for 29 seconds" of an 83 s respawn spent partly in the shop.
    let p = patterns(1000, 1300, &|t| match t {
        1100..=1104 | 1155..=1184 => { let mut d = dead(t, Some("Gun Damage")); d.respawn_secs = Some(1185 - t); d }
        1105..=1154 => GameSnapshot { game_clock_secs: Some(t), screen: Some("Shop".into()), ..Default::default() },
        _ => lane_frame(t, 500),
    });
    assert!(p.iter().any(|t| t.contains("you died 1 time and were dead for 85 seconds")), "{p:?}");
}
#[test]
fn a_death_while_still_dead_counts_the_rest_of_the_timer() {
    // At 1110 (50 s into an 85 s death) the summary says 85, not 50.
    let r = ReviewCoach::default();
    let mut last = String::new();
    for t in (800..=1110).step_by(5) {
        let s = if t >= 1060 { let mut d = dead(t, Some("Gun Damage")); d.respawn_secs = Some(1145 - t); d } else { lane_frame(t, 500) };
        for c in r.evaluate(&s) { if c.key.starts_with("review:pattern") { last = c.text; } }
    }
    assert!(last.contains("were dead for 85 seconds"), "{last}");
}
#[test]
fn gameplay_frames_without_a_minimap_split_two_deaths() {
    // rec8 50:26: "1 death" for deaths at 46:43 and 49:42; 48:07-49:34 had no minimap reading.
    let p = patterns(1000, 1300, &|t| match t {
        1050..=1079 | 1150..=1179 => dead(t, Some("Gun Damage")),
        1080..=1149 => GameSnapshot { game_clock_secs: Some(t), screen: Some("Gameplay".into()), hp: Some(2500), max_hp: Some(2500), health_observations: 3, ..Default::default() },
        _ => lane_frame(t, 500),
    });
    assert!(p.iter().any(|t| t.contains("you died 2 times")), "{p:?}");
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
    assert!(t.contains("You died because you stayed in under 50% health."), "{t}");
    assert!(t.contains("shopping doesn't heal"), "{t}");
}

// Time dead = first death screen -> respawn (docs/player-profile.md "Time dead", shared with the
// player profile; checked against the respawn timers of rec4 / rec5 / rec8 / rec11).
#[test]
fn the_grey_respawn_in_1_screen_is_not_a_death() {
    // rec11 25:17: one grey "Respawn in: 1" frame between gameplay frames, then the real death at
    // 25:28 (54 s). The scoreboard death count did not change for the grey screen.
    let p = patterns(1300, 1700, &|t| match t {
        1515 => dead(t, None),
        1525..=1585 => { let mut d = dead(t, Some("Jar of Dead")); d.respawn_secs = Some(1590 - t); d }
        _ => lane_frame(t, 500),
    });
    assert!(p.iter().any(|t| t.contains("you died 1 time and were dead for 65 seconds")), "{p:?}");
    assert!(!p.iter().any(|t| t.contains("you died 2 times")), "{p:?}");
}
#[test]
fn time_dead_ends_at_the_respawn_timer_not_at_the_next_frame() {
    // The first alive frame comes a sampling gap after the respawn (rec11 10:22: respawn 10:24,
    // next frame 10:30): the gap is alive time.
    let p = patterns(1000, 1300, &|t| match t {
        1100..=1160 => { let mut d = dead(t, Some("Gun Damage")); d.respawn_secs = Some(1162 - t); d }
        _ => lane_frame(t, 500),
    });
    let dead: Vec<&String> = p.iter().filter(|t| t.contains("were dead for")).collect();
    assert!(!dead.is_empty() && dead.iter().all(|t| t.contains("you died 1 time and were dead for 62 seconds")), "{p:?}");
}
#[test]
fn dropped_timer_digits_do_not_shorten_the_rest_of_a_death() {
    // rec4 read 8, 5, 4, 2, 0 for 68-60: two reads in three here drop the tens digit, including the
    // last one (18 read as 8). Still dead at 1170 in a 1100-1188 death: 88 s.
    let p = patterns(800, 1170, &|t| if t >= 1100 {
        let mut d = dead(t, Some("Gun Damage")); let left = 1188 - t; d.respawn_secs = Some(if (t / 5) % 3 != 0 { left % 10 } else { left }); d
    } else { lane_frame(t, 500) });
    let dead: Vec<&String> = p.iter().filter(|t| t.contains("were dead for")).collect();
    assert!(!dead.is_empty() && dead.iter().all(|t| t.contains("were dead for 88 seconds")), "{dead:?}");
}

#[test]
fn a_good_five_minutes_names_what_went_well() {
    // No deaths, one purchase (2,900 -> 400 at 1100), a teammate beside you in their half while
    // three enemies were dead (1150-1180).
    let p = patterns(1000, 1300, &|t| {
        let souls = if t < 1100 { 2900 } else { 400 };
        if (1150..=1180).contains(&t) {
            alive(t, 2400, Pt { x: 0.5, y: 0.3 }, Some(Lane::Blue), vec![track(1, true, 0.52, 0.31)], (0..12).map(|i| HeroStatus { ally: i < 6, slot: i % 6, dead: Some(i >= 6 && i < 9), bar_visible: Some(true), ..Default::default() }).collect(), souls)
        } else { lane_frame(t, souls) }
    });
    let g = p.iter().find(|t| t.contains("minutes went well:")).unwrap_or_else(|| panic!("{p:?}"));
    assert!(g.contains("no deaths"), "{g}");
    assert!(g.contains("at 19:10, with 3 enemies dead, you pushed into their half"), "{g}");
    assert!(g.contains("you spent your souls on time (1 purchase)"), "{g}");
}
#[test]
fn a_critical_summary_also_says_what_went_well() {
    // Hoarding is criticised; the purchase-free window still names the fight joined with the team.
    let p = patterns(1000, 1300, &|t| {
        let mut s = lane_frame(t, 5130);
        if (1100..1130).contains(&t) { s.minimap.as_mut().unwrap().heroes.extend([track(2, true, 0.51, 0.61), track(3, false, 0.52, 0.62), track(4, false, 0.53, 0.6)]); }
        s
    });
    let h = p.iter().find(|t| t.contains("You held over")).unwrap_or_else(|| panic!("{p:?}"));
    assert!(h.contains("What went well: no deaths."), "{h}");
}
// ---- One cause per death (docs/coach-review-13.md item 2) -------------------------------------

fn named_team(dead: &[&str], enemies_dead: usize) -> Vec<HeroStatus> {
    let allies = ["hero_bebop", "hero_familiar", "hero_mirage", "hero_fencer", "hero_punkgoat", "hero_orion"];
    (0..12).map(|i| {
        let ally = i < 6;
        let hero = if ally { Some(allies[i].to_string()) } else { None };
        let d = if ally { dead.contains(&allies[i]) } else { i - 6 < enemies_dead };
        HeroStatus { ally, slot: i % 6, hero, dead: Some(d), bar_visible: Some(true), ..Default::default() }
    }).collect()
}
#[test]
fn a_won_fight_then_a_structure_at_low_health_is_the_cause() {
    // rec13 29:05-29:25: five enemies dead, 15% health, hitting their Walker.
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.3 };
    for (t, hp) in [(1740u32, 2000u32), (1745, 900), (1749, 320), (1753, 330)] {
        let mut s = alive(t, hp, p, Some(Lane::Green), vec![track(1, true, 0.31, 0.3)], named_team(&[], 5), 5579); s.hero = Some("hero_orion".into()); r.evaluate(&s);
    }
    let rows = [("Afterburn", 869), ("Walker", 1045), ("Grasping Hands", 439)];
    r.evaluate(&panel(1756, &rows, "Walker", None));
    let t = &r.evaluate(&panel(1762, &rows, "Walker", None))[0].text;
    assert!(t.starts_with("You died because you hit their Walker at 13% after the fight was won. Next life: back off and heal first"), "{t}");
    short(t);
}
#[test]
fn the_frontliner_falling_first_is_the_cause_and_the_live_exit_names_him() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.25, y: 0.35 };
    let near = vec![track(1, true, 0.27, 0.35), track(2, false, 0.3, 0.33), track(3, false, 0.22, 0.31)];
    let mut s = alive(2440, 2600, p, Some(Lane::Green), near.clone(), named_team(&[], 0), 900); s.hero = Some("hero_orion".into());
    assert!(r.evaluate(&s).is_empty());
    let mut s = alive(2445, 2000, p, Some(Lane::Green), near.clone(), named_team(&["hero_bebop"], 0), 900); s.hero = Some("hero_orion".into());
    let live = r.evaluate(&s);
    assert!(live.iter().any(|c| c.key == "review:fight-lost" && c.text == "Back out: Bebop's down."), "{live:?}");
    let mut s = alive(2449, 900, p, Some(Lane::Green), near, named_team(&["hero_bebop", "hero_familiar"], 0), 900); s.hero = Some("hero_orion".into()); r.evaluate(&s);
    r.evaluate(&dead(2452, Some("Melee")));
    let t = &r.evaluate(&dead(2458, Some("Melee")))[0].text;
    assert_eq!(t, "You died because Bebop went down first and you stayed in. Next life: when Bebop falls, back out at once.");
}
#[test]
fn alone_against_two_and_stunned_are_named() {
    let r = ReviewCoach::default();
    let p = Pt { x: 0.3, y: 0.6 };
    for (t, hp) in [(900u32, 2400u32), (904, 1500), (908, 400)] { r.evaluate(&alive(t, hp, p, Some(Lane::Blue), vec![track(1, true, 0.8, 0.6)], team(0, 0), 500)); }
    let rows = [("Grasping Hands", 700), ("Essence Bomb", 600)];
    let mut d = panel(911, &rows, "Essence Bomb", None); d.enemy_heroes = vec!["hero_necro".into(), "hero_ghost".into(), "hero_atlas".into()];
    r.evaluate(&d);
    let mut d = panel(917, &rows, "Essence Bomb", None); d.enemy_heroes = vec!["hero_necro".into(), "hero_ghost".into(), "hero_atlas".into()];
    let t = &r.evaluate(&d)[0].text;
    assert!(t.starts_with("You died because you were alone against Graves and Lady Geist. Next life: stay within reach of a teammate."), "{t}");
    short(t);
    // Stunned with a teammate beside you.
    let r = ReviewCoach::default();
    for (t, hp, fx) in [(900u32, 2400u32, vec![]), (904, 1500, vec!["stunned"]), (908, 400, vec![])] {
        let mut s = alive(t, hp, p, Some(Lane::Blue), vec![track(1, true, 0.31, 0.6)], team(0, 0), 500); s.status_effects = fx.into_iter().map(String::from).collect(); r.evaluate(&s);
    }
    let rows = [("Grasping Hands", 900)];
    let mut d = panel(911, &rows, "Grasping Hands", None); d.enemy_heroes = vec!["hero_necro".into(), "hero_ghost".into(), "hero_atlas".into()];
    r.evaluate(&d); d.game_clock_secs = Some(917);
    let t = &r.evaluate(&d)[0].text;
    assert!(t.starts_with("You died because you got stunned and Graves finished you. Next life: after the first stun, dash out at once."), "{t}");
}

// ---- docs/coach-review-8.md D6, D8 ----------------------------------------------------------

fn r8_team() -> Vec<String> { ["hero_kelvin", "hero_yamato", "hero_synth", "hero_haze", "hero_chrono", "hero_frank"].map(String::from).to_vec() }
fn r8_death(r: &ReviewCoach, from: u32, panel_rows: &[(&str, u32)]) -> String {
    let p = Pt { x: 0.5, y: 0.55 };
    for (t, hp) in [(from, 2300), (from + 4, 2300), (from + 8, 1200), (from + 11, 300)] {
        let mut s = alive(t, hp, p, Some(Lane::Blue), vec![track(1, true, 0.52, 0.56)], team(0, 0), 500); s.hero = Some("hero_orion".into()); s.enemy_heroes = r8_team(); r.evaluate(&s);
    }
    let mut out = String::new();
    for t in from + 13..from + 30 { let mut d = dead(t, Some(panel_rows[0].0)); d.death_damage = rows(panel_rows); d.enemy_heroes = r8_team(); d.hero = Some("hero_orion".into());
        if let Some(c) = r.evaluate(&d).into_iter().find(|c| c.key.starts_with("death:review")) { out = c.text; } }
    let mut back = alive(from + 40, 2500, p, Some(Lane::Blue), vec![], team(0, 0), 500); back.hero = Some("hero_orion".into()); r.evaluate(&back);
    out
}
#[test]
fn the_death_review_names_the_killing_ability_and_its_counter() {
    // rec8 42:05: Kelvin's Arctic Beam 1,342 of 2,670 (the review said "Kelvin and Yamato focused you").
    let t = r8_death(&ReviewCoach::default(), 2500, &[("Arctic Beam", 1342), ("Frost Grenade", 396), ("Flying Slash", 277), ("Gun Damage", 153)]);
    assert!(t.starts_with("You died because Kelvin's Arctic Beam froze you in place. Next life: break line of sight at the first tick of Kelvin's beam."), "{t}");
    short(&t);
    // rec8 49:42: Haze's Bullet Dance (2.0k) -> the Snare and Metal Skin answer.
    let t = r8_death(&ReviewCoach::default(), 2970, &[("Bullet Dance", 2021), ("Gun Damage", 400)]);
    assert!(t.starts_with("You died because Haze's Bullet Dance caught you. Next life: stay with the team and Snare her landing; Metal Skin answers Bullet Dance."), "{t}");
    // A burst by nobody named: lower case mid-sentence (rec8 21:10 "You died because They burst you").
    let t = r8_death(&ReviewCoach::default(), 1230, &[("Gun Damage", 2200)]);
    assert!(!t.contains("They") && !t.contains("because They"), "{t}");
}
#[test]
fn a_repeat_killer_is_folded_into_the_one_death_review() {
    // rec8 47:38: "Pocket's Affliction topped 2 of your deaths" as a second death-screen message.
    let r = ReviewCoach::default();
    r8_death(&r, 2700, &[("Affliction", 2200), ("Frost Grenade", 300)]);
    let t = r8_death(&r, 2800, &[("Affliction", 2210), ("Frost Grenade", 310)]);
    assert!(t.contains("Pocket") && t.contains("Affliction") && (t.contains("topped 2 of your deaths") || t.contains("killed you 2 times")), "{t}");
    short(&t);
    // In the engine the active coach's repeat-killer message stays quiet after that review.
    let mut e = coach_engine::Engine::new(coach_engine::default_rules(coach_engine::RulesConfig::default()));
    let mut keys = vec![];
    for (from, dmg) in [(2700u32, 2200u32), (2800, 2210)] {
        let p = Pt { x: 0.5, y: 0.55 };
        for (t, hp) in [(from, 2300), (from + 4, 2300), (from + 8, 1200), (from + 11, 300)] {
            let mut s = alive(t, hp, p, Some(Lane::Blue), vec![], team(0, 0), 500); s.hero = Some("hero_orion".into()); s.enemy_heroes = r8_team(); keys.extend(e.step(&s).into_iter().map(|c| c.key));
        }
        for t in from + 13..from + 40 { let mut d = dead(t, Some("Affliction")); d.death_damage = rows(&[("Affliction", dmg), ("Frost Grenade", 300)]); d.enemy_heroes = r8_team(); d.hero = Some("hero_orion".into()); d.respawn_secs = Some(from + 45 - t);
            keys.extend(e.step(&d).into_iter().map(|c| c.key)); }
        for t in from + 46..from + 60 { let mut s = alive(t, 2500, p, Some(Lane::Blue), vec![], team(0, 0), 500); s.hero = Some("hero_orion".into()); keys.extend(e.step(&s).into_iter().map(|c| c.key)); }
    }
    assert_eq!(keys.iter().filter(|k| k.starts_with("death:review")).count(), 2, "{keys:?}");
    assert!(!keys.iter().any(|k| k.starts_with("active:repeat") || k == "active:close"), "{keys:?}");
}
fn slots(allies_dead: usize, enemies_dead: usize) -> Vec<HeroStatus> {
    (0..12).map(|i| { let ally = i < 6; let slot = i % 6; HeroStatus { ally, slot, dead: Some(if ally { slot < allies_dead } else { slot < enemies_dead }), bar_visible: Some(true), ..Default::default() } }).collect()
}
#[test]
fn fight_lost_is_only_called_when_we_lost_the_trade() {
    let p = Pt { x: 0.5, y: 0.5 };
    let foe = vec![track(9, false, 0.55, 0.5)];
    let run = |steps: &[(u32, usize, usize)]| { let r = ReviewCoach::default(); let mut out = vec![];
        for &(t, a, e) in steps { out.extend(r.evaluate(&alive(t, 2000, p, Some(Lane::Blue), foe.clone(), slots(a, e), 500)).into_iter().filter(|c| c.key == "review:fight-lost")); } out };
    // rec8 13:51: three of theirs and three of ours: an even trade.
    assert!(run(&[(800, 0, 0), (805, 0, 2), (810, 1, 3), (815, 3, 3)]).is_empty());
    // rec8 33:40: 2-for-2 within the minute.
    assert!(run(&[(800, 0, 0), (810, 0, 1), (840, 0, 2), (850, 2, 2)]).is_empty());
    // Lost: two of ours, none of theirs; or two down now after their dead respawned (rec13 13:40).
    let c = run(&[(800, 0, 0), (806, 2, 0)]);
    assert_eq!(c.len(), 1); assert_eq!(c[0].text, "Back out: 2 teammates are down.");
    assert_eq!(run(&[(760, 0, 0), (770, 0, 2), (790, 0, 0), (800, 0, 0), (806, 2, 0)]).len(), 1);
}