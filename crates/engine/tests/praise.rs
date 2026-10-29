use coach_core::{GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, ObjectiveBanner, Pt, ZoneArea};
use coach_engine::{praise::{self, PraiseCoach}, Cue, Rule};

#[test]
fn praise_cards_quote_real_document_guidance() {
    let catalog = PraiseCoach::shipped();
    assert_eq!(catalog.schema_version, 1);
    assert_eq!(catalog.cards.len(), 7);
    for c in &catalog.cards {
        assert!(!c.sources.is_empty() && c.cooldown_secs >= 30 && !c.rationale.is_empty(), "{}", c.id);
        // Praise is never urgent: it waits for a calm moment.
        assert!(c.priority == "normal" || c.priority == "low", "{}", c.id);
        for s in &c.sources {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&s.path)).unwrap();
            assert!(text.contains(&s.excerpt), "{}: missing excerpt {}", c.id, s.excerpt);
            assert!(text.contains(&s.section), "{}: missing section {}", c.id, s.section);
            assert!(!s.excerpt.contains("UNVERIFIED"));
            assert!(["WIKI", "COMMUNITY", "GENERAL"].contains(&s.basis.as_str()), "{}: basis {}", c.id, s.basis);
        }
    }
}

// ---- Helpers ------------------------------------------------------------------------------

fn track(id: u32, ally: bool, x: f32, y: f32) -> HeroTrack { HeroTrack { id, ally, pos: Pt { x, y }, hits: 5, unseen_ms: 0, ..Default::default() } }
/// Top bar: the first `enemies_dead` enemy slots dead (30 s timers), `allies_dead` allied ones.
fn team(enemies_dead: usize, allies_dead: usize) -> Vec<HeroStatus> {
    (0..12).map(|i| {
        let ally = i < 6; let slot = i % 6;
        let dead = if ally { slot < allies_dead } else { slot < enemies_dead };
        HeroStatus { ally, slot, dead: Some(dead), respawn_secs: dead.then_some(30), bar_visible: Some(!dead), ..Default::default() }
    }).collect()
}
/// Alive at `hp` of 2000, at `me`, with the given minimap heroes and top bar.
fn alive(t: u32, hp: u32, me: Pt, heroes: Vec<HeroTrack>, status: Vec<HeroStatus>, souls: u32) -> GameSnapshot {
    let mut map = MinimapState { observations: 3, you: Some(me), heroes, zone_area: Some(ZoneArea::Lane), ..Default::default() };
    map.green.push = Some(0.5); map.blue.push = Some(0.5); map.yellow.push = Some(0.5);
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Gameplay".into()), hp: Some(hp), max_hp: Some(2000), health_observations: 3,
        souls: Some(souls), hero_status: status, minimap: Some(map), ..Default::default() }
}
fn dead(t: u32, souls: Option<u32>) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Dead".into()), dead: true, souls, ..Default::default() }
}
fn praises(c: &PraiseCoach, s: &GameSnapshot) -> Vec<Cue> {
    let out = c.evaluate(s);
    // The engine speaks what is offered (no other rule here); praise commits on spoken.
    for cue in &out { c.spoken(cue); }
    out
}
fn run(c: &PraiseCoach, frames: impl IntoIterator<Item = GameSnapshot>) -> Vec<(u32, Cue)> {
    frames.into_iter().flat_map(|s| { let t = s.game_clock_secs.unwrap(); praises(c, &s).into_iter().map(move |c| (t, c)) }).collect()
}
const ME_THEIR_HALF: Pt = Pt { x: 0.5, y: 0.3 };
const ME_OWN_HALF: Pt = Pt { x: 0.5, y: 0.7 };

// ---- Enemy-dead windows ---------------------------------------------------------------------

#[test]
fn a_window_used_with_a_teammate_in_their_half_is_praised_once() {
    let c = PraiseCoach::default();
    let said = run(&c, (0..12).map(|i| alive(1000 + i * 3, 1800, ME_THEIR_HALF, vec![track(1, true, 0.52, 0.31)], team(3, 0), 900)));
    assert_eq!(said.len(), 1, "{said:?}");
    let (t, cue) = &said[0];
    assert!(*t >= 1015, "after the window held for 15 s: {t}");
    assert!(cue.key.starts_with("praise:window:"), "{cue:?}");
    assert_eq!(cue.text, "Good use of the 16:40 window: with three enemies dead, you pushed into their half with your team.");
}

#[test]
fn a_window_spent_alone_or_on_your_own_half_is_not_praised() {
    // Alone in their half (a dive, not a team push), or on your own half (the window unused).
    let alone = PraiseCoach::default();
    assert!(run(&alone, (0..12).map(|i| alive(1000 + i * 3, 1800, ME_THEIR_HALF, vec![track(1, true, 0.8, 0.7)], team(3, 0), 900))).is_empty());
    let home = PraiseCoach::default();
    assert!(run(&home, (0..12).map(|i| alive(1000 + i * 3, 1800, ME_OWN_HALF, vec![track(1, true, 0.52, 0.7)], team(3, 0), 900))).is_empty());
    // A mid-lane front (y 0.45) is not "their half".
    let mid = PraiseCoach::default();
    assert!(run(&mid, (0..12).map(|i| alive(1000 + i * 3, 1800, Pt { x: 0.5, y: 0.45 }, vec![track(1, true, 0.52, 0.45)], team(3, 0), 900))).is_empty());
    // A one-frame flicker of two dead slots is not a window.
    let flicker = PraiseCoach::default();
    assert!(run(&flicker, (0..12).map(|i| alive(1000 + i * 3, 1800, ME_THEIR_HALF, vec![track(1, true, 0.52, 0.31)], team(if i == 3 { 2 } else { 0 }, 0), 900))).is_empty());
}

#[test]
fn their_structure_banner_during_a_window_names_the_patron() {
    let c = PraiseCoach::default();
    let said = run(&c, (0..12).map(|i| {
        let mut s = alive(1650 + i * 3, 2000, ME_OWN_HALF, vec![], team(5, 1), 6000);
        s.objective_banner = Some(ObjectiveBanner { name: "ARCHMOTHER".into(), own: Some(false), health_ratio: Some(0.7) });
        s
    }));
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].1.text.contains("with five enemies dead, your team was hitting their Patron"), "{}", said[0].1.text);
}

#[test]
fn a_lane_front_moving_up_on_your_lane_counts_with_three_dead() {
    let c = PraiseCoach::default();
    let said = run(&c, (0..12).map(|i| {
        let mut s = alive(1300 + i * 3, 1800, Pt { x: 0.5, y: 0.55 }, vec![], team(3, 0), 900);
        let m = s.minimap.as_mut().unwrap(); m.your_lane = Some(Lane::Blue); m.blue.push = Some(0.35 + 0.03 * i as f32);
        s
    }));
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].1.text.contains("you and your team pushed Blue lane up"), "{}", said[0].1.text);
}

// ---- Calm moments only ------------------------------------------------------------------------

#[test]
fn praise_waits_for_a_calm_moment() {
    let c = PraiseCoach::default();
    let window = |t: u32, hp: u32, heroes: Vec<HeroTrack>| alive(t, hp, ME_THEIR_HALF, heroes, team(3, 0), 900);
    let ally = track(1, true, 0.52, 0.31);
    // Low health, then an enemy beside you, then falling health: nothing.
    let mut said = run(&c, (0..8).map(|i| window(1000 + i * 3, 900, vec![ally.clone()])));
    said.extend(run(&c, (0..3).map(|i| window(1024 + i * 3, 1800, vec![ally.clone(), track(9, false, 0.53, 0.3)]))));
    let mut falling = window(1033, 1800, vec![ally.clone()]); falling.health_delta_ratio = Some(-0.1);
    said.extend(run(&c, [falling]));
    let mut urn = window(1036, 1800, vec![ally.clone()]); urn.status_effects = vec!["carrying_urn".into()];
    said.extend(run(&c, [urn]));
    assert!(said.is_empty(), "{said:?}");
    // Healthy, nobody close: now.
    let ok = run(&c, [window(1039, 1800, vec![ally])]);
    assert_eq!(ok.len(), 1, "{ok:?}");
}

// ---- The urn -------------------------------------------------------------------------------

fn carrying(t: u32, souls: u32) -> GameSnapshot {
    let mut s = alive(t, 1900, Pt { x: 0.4, y: 0.5 }, vec![], team(0, 0), souls); s.status_effects = vec!["carrying_urn".into()]; s
}
#[test]
fn a_delivered_urn_is_praised_with_its_souls() {
    // rec12 16:09-16:33: carried, souls 479 -> 2,607 by 16:46.
    let c = PraiseCoach::default();
    let mut frames: Vec<GameSnapshot> = (0..8).map(|i| carrying(969 + i * 3, 479 + i)).collect();
    frames.extend([alive(995, 1900, Pt { x: 0.2, y: 0.5 }, vec![], team(0, 0), 482), alive(1006, 1990, Pt { x: 0.28, y: 0.38 }, vec![], team(0, 0), 2607),
        alive(1011, 1990, Pt { x: 0.3, y: 0.44 }, vec![], team(0, 0), 2607)]);
    let said = run(&c, frames);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].1.text, "Urn delivered: about 2,100 souls. Walking it straight in was the right call.");
}
#[test]
fn a_small_or_late_souls_jump_after_carrying_is_not_an_urn() {
    let small = PraiseCoach::default();
    let mut frames: Vec<GameSnapshot> = (0..5).map(|i| carrying(1000 + i * 3, 500)).collect();
    frames.extend((0..4).map(|i| alive(1020 + i * 3, 1900, ME_OWN_HALF, vec![], team(0, 0), 1300)));
    assert!(run(&small, frames).is_empty(), "an 800-soul kill is not the urn");
    let late = PraiseCoach::default();
    let mut frames: Vec<GameSnapshot> = (0..5).map(|i| carrying(1000 + i * 3, 500)).collect();
    frames.extend((0..4).map(|i| alive(1080 + i * 3, 1900, ME_OWN_HALF, vec![], team(0, 0), 2500)));
    assert!(run(&late, frames).is_empty(), "a minute later is farming, not the delivery");
}

// ---- Shopping on time ------------------------------------------------------------------------

#[test]
fn souls_spent_during_a_respawn_are_praised_after_it() {
    // rec12 death 2: 6,073 souls at 14:43, 132 on the death screen, respawned at full health.
    let c = PraiseCoach::default();
    let mut frames = vec![alive(880, 1500, ME_OWN_HALF, vec![], team(0, 0), 6073), alive(883, 1500, ME_OWN_HALF, vec![], team(0, 0), 6073)];
    frames.extend([dead(890, Some(2873)), dead(900, Some(132)), dead(908, Some(132)), dead(920, None)]);
    let back = |t: u32| alive(t, 2000, Pt { x: 0.47, y: 0.8 }, vec![], team(0, 0), 132);
    frames.extend([back(928), back(934)]);
    let said = run(&c, frames);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].0 >= 928, "said after the respawn, not on the death screen");
    assert!(said[0].1.text.starts_with("Good shopping: you spent 5,941 souls on the death screen"), "{}", said[0].1.text);
}
#[test]
fn a_death_with_the_souls_kept_is_not_praised() {
    // rec12 death 3: 2,060 -> 3,693 on the death screen, 3,703 after the respawn.
    let c = PraiseCoach::default();
    let mut frames = vec![alive(1320, 1500, ME_OWN_HALF, vec![], team(0, 0), 2060)];
    frames.extend([dead(1330, Some(2060)), dead(1339, Some(3693)), dead(1350, Some(3693))]);
    frames.extend((0..4).map(|i| alive(1404 + i * 5, 2000, Pt { x: 0.16, y: 0.6 }, vec![], team(0, 0), 3703)));
    assert!(run(&c, frames).is_empty());
}
#[test]
fn buying_right_after_reaching_the_tier_is_praised_and_a_misread_is_not() {
    // rec12 6:16-6:35: 1,458 -> 1,626 (the 1,600 tier) -> 175, 175.
    let c = PraiseCoach::default();
    let at = |t: u32, souls: u32| alive(t, 1500, ME_OWN_HALF, vec![], team(0, 0), souls);
    let said = run(&c, [at(376, 1458), at(379, 1626), at(392, 175), at(395, 175), at(400, 180)]);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].1.text.starts_with("Good timing: you spent 1,400 souls 13 seconds after reaching 1,600."), "{}", said[0].1.text);
    // One low read between normal ones is a misread (rec12 3:52 read 7 souls), not a purchase.
    let m = PraiseCoach::default();
    assert!(run(&m, [at(376, 1458), at(379, 1626), at(382, 7), at(385, 1680), at(388, 1700)]).is_empty());
}

// ---- Deathless streaks -----------------------------------------------------------------------

#[test]
fn ten_minutes_without_dying_is_praised_with_the_time_dead() {
    let c = PraiseCoach::default();
    let said = run(&c, (3..=64).map(|i| alive(i * 10, 1800, ME_OWN_HALF, vec![], team(0, 0), 500)));
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].0, 630, "10 minutes after the first frame (0:30)");
    assert!(said[0].1.text.starts_with("10 minutes without dying. You've been dead 0% of the game"), "{}", said[0].1.text);
    // A death resets the streak; watched from mid-match it is unknown.
    let d = PraiseCoach::default();
    let mut frames: Vec<GameSnapshot> = (3..30).map(|i| alive(i * 10, 1800, ME_OWN_HALF, vec![], team(0, 0), 500)).collect();
    frames.extend([dead(300, None), dead(310, None)]);
    frames.extend((32..=64).map(|i| alive(i * 10, 1800, ME_OWN_HALF, vec![], team(0, 0), 500)));
    assert!(run(&d, frames).is_empty());
    let late = PraiseCoach::default();
    assert!(run(&late, (90..=160).map(|i| alive(i * 10, 1800, ME_OWN_HALF, vec![], team(0, 0), 500))).is_empty());
}

// ---- Walking out with a teammate --------------------------------------------------------------

#[test]
fn walking_out_with_a_teammate_after_dying_alone_is_praised() {
    let c = PraiseCoach::default();
    let mut frames: Vec<GameSnapshot> = (0..6).map(|i| alive(480 + i * 3, 900, Pt { x: 0.2, y: 0.45 }, vec![track(1, true, 0.8, 0.6)], team(0, 0), 500)).collect();
    frames.extend([dead(500, None), dead(510, None)]);
    frames.push(alive(530, 2000, Pt { x: 0.5, y: 0.9 }, vec![track(1, true, 0.52, 0.9)], team(0, 0), 500));
    frames.extend((0..2).map(|i| alive(540 + i * 3, 2000, Pt { x: 0.5, y: 0.7 - i as f32 * 0.02 }, vec![track(1, true, 0.53, 0.7)], team(0, 0), 500)));
    let said = run(&c, frames);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].1.text.starts_with("You walked out with a teammate this time. After a death alone"), "{}", said[0].1.text);
    // Walking out alone again: nothing to praise.
    let alone = PraiseCoach::default();
    let mut frames: Vec<GameSnapshot> = (0..6).map(|i| alive(480 + i * 3, 900, Pt { x: 0.2, y: 0.45 }, vec![track(1, true, 0.8, 0.6)], team(0, 0), 500)).collect();
    frames.extend([dead(500, None), dead(510, None)]);
    frames.extend((0..3).map(|i| alive(530 + i * 4, 2000, Pt { x: 0.5, y: 0.75 - i as f32 * 0.05 }, vec![track(1, true, 0.9, 0.4)], team(0, 0), 500)));
    assert!(run(&alone, frames).is_empty());
}

// ---- The team's lead -----------------------------------------------------------------------

#[test]
fn a_growing_team_lead_is_praised_from_reliable_totals_only() {
    let at = |t: u32, a: u32, e: u32| { let mut s = alive(t, 1800, ME_OWN_HALF, vec![], team(0, 0), 500); s.ally_networth = Some(a); s.enemy_networth = Some(e); s };
    let c = PraiseCoach::default();
    let said = run(&c, (0..=50).map(|i| { let t = 600 + i * 10; at(t, 50000 + (t - 600) * 40, 50000 + (t - 600) * 10) }));
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].1.text.starts_with("Our soul lead grew from"), "{}", said[0].1.text);
    // Totals that jump between readings (misreads) are never a story.
    let j = PraiseCoach::default();
    assert!(run(&j, (0..=50).map(|i| { let t = 600 + i * 10; at(t, if i % 2 == 0 { 70000 } else { 50000 }, 50000) })).is_empty());
    // Early totals (before 10:00) are too small to read a lead from.
    let early = PraiseCoach::default();
    assert!(run(&early, (0..=40).map(|i| { let t = 180 + i * 10; at(t, 10000 + i * 600, 10000 + i * 100) })).is_empty());
}

// ---- Two at once, and the engine's budget ----------------------------------------------------

#[test]
fn two_good_things_waiting_share_one_call() {
    let c = PraiseCoach::default();
    // A window is used while low on health (not calm), then the urn is delivered: one call later.
    let mut frames: Vec<GameSnapshot> = (0..8).map(|i| alive(1000 + i * 3, 900, ME_THEIR_HALF, vec![track(1, true, 0.52, 0.31)], team(3, 0), 500)).collect();
    frames.extend((0..3).map(|i| { let mut s = carrying(1024 + i * 3, 500); s.hp = Some(900); s }));
    frames.extend([alive(1036, 900, ME_OWN_HALF, vec![], team(0, 0), 2400), alive(1039, 900, ME_OWN_HALF, vec![], team(0, 0), 2400),
        alive(1042, 1900, ME_OWN_HALF, vec![], team(0, 0), 2400), alive(1045, 1900, ME_OWN_HALF, vec![], team(0, 0), 2400)]);
    let said = run(&c, frames);
    assert_eq!(said.len(), 1, "{said:?}");
    let t = &said[0].1.text;
    assert!(t.starts_with("Urn delivered: about 1,900 souls. Also, good use of the 16:40 window"), "{t}");
    assert!(!t.contains("right call"), "a shared call keeps only the observations: {t}");
}

#[test]
fn the_engine_allows_three_praise_calls_per_ten_minutes_a_minute_apart() {
    assert!(praise::budget_allows("praise:urn:1", &[], None, 100));
    assert!(!praise::budget_allows("praise:urn:1", &[100], Some(100), 150), "a minute apart");
    assert!(praise::budget_allows("praise:urn:1", &[100], Some(100), 160));
    assert!(!praise::budget_allows("praise:urn:1", &[100, 200, 300], Some(300), 690), "three in ten minutes");
    assert!(praise::budget_allows("praise:urn:1", &[100, 200, 300], Some(300), 700));
    // The farm-pace compliment waits for the gap but does not use the budget; other cues are free.
    assert!(!praise::budget_allows("pace:on-track", &[100], Some(100), 130));
    assert!(praise::budget_allows("pace:on-track", &[100, 200, 300], Some(300), 400));
    assert!(praise::budget_allows("lane:critical", &[100], Some(100), 101));
}

/// rec12 (a 12/3/11 Grey Talon win): every praise call, checked against the recording (base push
/// 13:35-14:38, death-screen shopping 6,073 -> 132, urn 16:09-16:33 then 479 -> 2,607 souls, the
/// closing push on the Patron from 27:17, a team lead growing to +43k with ~19% of it yours).
/// Run with `cargo test -p coach-engine --test praise -- --ignored` (needs the local recording).
#[test]
#[ignore]
fn rec12_praise_calls() {
    let path = std::env::var("PRAISE_REPLAY").unwrap_or_else(|_| r"C:\Users\arielwolle\Documents\Github\DeadlockCoach\frames\rec12\replay1.jsonl".into());
    let Ok(text) = std::fs::read_to_string(&path) else { println!("skip {path}"); return };
    let mut rules = coach_engine::rules_from_timings(include_str!("../../../data/timings.json")).unwrap();
    rules.extend(coach_engine::default_rules(coach_engine::RulesConfig::default()));
    rules.push(coach_engine::counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap());
    let mut e = coach_engine::Engine::new(rules);
    let mut said: Vec<(u32, Cue)> = vec![];
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Ok(s) = serde_json::from_value::<GameSnapshot>(v["snapshot"].clone()) else { continue };
        let t = s.game_clock_secs.unwrap_or(0);
        said.extend(e.step(&s).into_iter().filter(|c| c.key.starts_with("praise:")).map(|c| (t, c)));
    }
    for (t, c) in &said { println!("{}:{:02} [{}] {}", t / 60, t % 60, c.key, c.text); }
    let expect: [(u32, &str, &str); 6] = [
        (407, "praise:spent_tier:", "you spent 1,400 souls 13 seconds after reaching 1,600"),
        (706, "praise:window:679", "with two enemies dead you pushed into their half with your team"),
        (943, "praise:window:815", "with five enemies dead you pushed into their base with your team. Also, good shopping: you spent 5,941 souls during your respawn"),
        (1011, "praise:urn:", "Urn delivered: about 2,100 souls"),
        (1548, "praise:lead:", "You hold 18% of your team's souls"),
        (1687, "praise:window:1637", "with five enemies dead, your team was hitting their Patron"),
    ];
    assert_eq!(said.len(), expect.len(), "{said:?}");
    for ((t, c), (et, key, text)) in said.iter().zip(expect) {
        assert_eq!(*t, et, "{c:?}"); assert!(c.key.starts_with(key), "{c:?}"); assert!(c.text.contains(text), "{c:?}");
    }
}