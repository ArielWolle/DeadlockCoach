//! Plans, lead protection, heal-after-won-fight and named threats (crates/engine/src/plans.rs).
use coach_core::{GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, Pt, StructureState};
use coach_engine::{plans::{self, PlanCoach}, Cue, Engine, Priority, Rule};

const ENEMIES: [&str; 6] = ["hero_inferno", "hero_atlas", "hero_vampirebat", "hero_ghost", "hero_chrono", "hero_necro"];

fn words(t: &str) -> usize { t.split_whitespace().count() }

/// A Grey Talon snapshot with the full top bar (everyone alive, enemy bars hidden) and a minimap.
fn snap(t: u32) -> GameSnapshot {
    let mut s = GameSnapshot { game_clock_secs: Some(t), screen: Some("Gameplay".into()), hero: Some("hero_orion".into()), hp: Some(2000), max_hp: Some(2000), health_observations: 3, ..Default::default() };
    for i in 0..6 { s.hero_status.push(HeroStatus { ally: true, slot: i, hero: Some(format!("ally{i}")), dead: Some(false), bar_visible: Some(true), health_ratio: Some(1.0), ..Default::default() }); }
    for (i, h) in ENEMIES.iter().enumerate() { s.hero_status.push(HeroStatus { ally: false, slot: 6 + i, hero: Some(h.to_string()), dead: Some(false), bar_visible: Some(false), ..Default::default() }); }
    s.enemy_heroes = ENEMIES.map(String::from).to_vec();
    s.minimap = Some(MinimapState { observations: 3, you: Some(Pt { x: 0.5, y: 0.7 }), midboss_up: true, ..Default::default() });
    s
}
fn enemy<'a>(s: &'a mut GameSnapshot, hero: &str) -> &'a mut HeroStatus { s.hero_status.iter_mut().find(|h| !h.ally && h.hero.as_deref() == Some(hero)).unwrap() }
fn kill_enemies(s: &mut GameSnapshot, n: usize, respawn: u32) { for h in s.hero_status.iter_mut().filter(|h| !h.ally).take(n) { h.dead = Some(true); h.respawn_secs = Some(respawn); } }
fn kill_allies(s: &mut GameSnapshot, n: usize) { for h in s.hero_status.iter_mut().filter(|h| h.ally).take(n) { h.dead = Some(true); h.respawn_secs = Some(30); } }
fn you(s: &mut GameSnapshot, x: f32, y: f32) { s.minimap.as_mut().unwrap().you = Some(Pt { x, y }); }
fn eval(c: &PlanCoach, s: &GameSnapshot) -> Vec<Cue> { c.evaluate(s) }
fn find<'a>(cues: &'a [Cue], prefix: &str) -> Option<&'a Cue> { cues.iter().find(|c| c.key.starts_with(prefix)) }
/// Evaluate and commit like the engine does when a cue is spoken.
fn speak(c: &PlanCoach, s: &GameSnapshot, prefix: &str) -> Option<Cue> {
    let cue = eval(c, s).into_iter().find(|c| c.key.starts_with(prefix))?;
    c.spoken(&cue); Some(cue)
}

#[test]
fn every_card_and_threat_cites_verbatim_doc_sources_and_fits_the_word_budget() {
    let cat = plans::catalog();
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let check = |id: &str, sources: &[coach_engine::laning::Source]| {
        assert!(!sources.is_empty(), "{id}: no source");
        for s in sources {
            let text = std::fs::read_to_string(docs.join(&s.path)).unwrap_or_else(|_| panic!("{id}: {}", s.path));
            assert!(text.contains(&s.excerpt), "{id}: missing excerpt {}", s.excerpt);
            assert!(!s.excerpt.contains("UNVERIFIED"));
        }
    };
    for c in &cat.cards { check(&c.id, &c.sources); }
    for t in &cat.threats { check(&t.hero, &t.sources); }
    check("jobs", &cat.jobs_sources);
    // Threat calls: <= 15 words for every role and state (rec13 review item 8).
    let threat = cat.cards.iter().find(|c| c.id == "threat").unwrap();
    for t in &cat.threats {
        for counter in t.counter.values() { for state in ["is close", "is missing"] {
            let text = threat.speech.replace("{hero}", &coach_engine::review::game_data().hero_name(&t.hero)).replace("{state}", state).replace("{danger}", &t.danger).replace("{counter}", counter);
            assert!(words(&text) <= 15, "{}: {text}", t.hero);
        } }
        assert!(t.counter.contains_key("default") && t.counter.contains_key("long_range"), "{}", t.hero);
    }
    // Reset <= 15 words; plans and lead calls <= 20 (mid / late budget), with the longest job.
    let longest = |pick: fn(&plans::Jobs) -> &String| cat.jobs.values().map(pick).max_by_key(|j| words(j)).unwrap().clone();
    for c in &cat.cards {
        let job = if c.id.starts_with("urn") { longest(|j| &j.urn) } else if c.id.starts_with("midboss") { longest(|j| &j.midboss) } else { longest(|j| &j.siege) };
        let lane = if c.speech.contains("{lane} side") { "Yellow" } else { "Yellow lane" };
        let text = c.speech.replace("{job}", &job).replace("{walk}", " Start walking now.").replace("{objective}", "the objective").replace("{hero}", "Nurse Harrow").replace("{lane}", lane);
        let budget = if c.family == "reset" { 15 } else { 20 };
        assert!(words(&text) <= budget, "{}: {} words: {text}", c.id, words(&text));
        assert!(["plan", "lead", "reset", "threat"].contains(&c.family.as_str()), "{}", c.id);
    }
}

#[test]
fn the_urn_plan_names_the_side_your_job_and_replaces_the_timer_readout() {
    let c = PlanCoach::default();
    // 9:20 = 40 s before the first urn (always Yellow), Grey Talon far on the Green side.
    let mut s = snap(560); you(&mut s, 0.15, 0.6);
    let cue = find(&eval(&c, &s), "plan:urn:600").cloned().expect("urn plan");
    assert_eq!(cue.text, "Urn in 40 on Yellow side: we contest it together. You hold back and Owl their carrier. Start walking now.");
    assert_eq!(cue.priority, Priority::Normal);
    // The second urn (15:00) is on Green; nothing outside 25-45 s.
    assert!(find(&eval(&c, &snap(500)), "plan:urn").is_none());
    let t = find(&eval(&c, &snap(860)), "plan:urn:900").unwrap().text.clone();
    assert!(t.contains("Green side") && !t.contains("walking"), "{t}");
    // The timers the plans replace are not registered twice.
    let left = plans::unplanned_timings(include_str!("../../../data/timings.json"));
    assert!(!left.contains("Soul Urn") && !left.contains("Power-ups") && left.contains("Zipline"), "{left}");
    assert_eq!(coach_engine::rules_from_timings(&left).unwrap().len(), 1);
}

#[test]
fn the_urn_plan_follows_the_numbers() {
    let c = PlanCoach::default();
    let mut s = snap(860); kill_enemies(&mut s, 3, 40);
    assert_eq!(find(&eval(&c, &s), "plan:urn").unwrap().text, "Urn in 40 on Green side, 3 of them dead: grab it before they're back.");
    let c = PlanCoach::default();
    let mut s = snap(860); kill_allies(&mut s, 2);
    assert_eq!(find(&eval(&c, &s), "plan:urn").unwrap().text, "Urn in 40, but we're 2 down: skip it and shove your wave.");
    // A hero without a role entry gets the generic job.
    let c = PlanCoach::default();
    let mut s = snap(860); s.hero = Some("hero_abrams_x".into());
    assert!(find(&eval(&c, &s), "plan:urn").unwrap().text.contains("You stay with the carrier and peel."));
}

#[test]
fn walker_and_guardian_plans_pick_the_pushed_lane_with_a_standing_structure() {
    let c = PlanCoach::default();
    let mut s = snap(1040);
    { let m = s.minimap.as_mut().unwrap(); m.green.push = Some(0.8); m.blue.push = Some(0.6); m.yellow.push = Some(0.3); }
    let t = find(&eval(&c, &s), "plan:walker").unwrap().text.clone();
    assert_eq!(t, "Walkers go weak at 18:00. We shove Green lane into its Walker; you shoot from max range behind the troopers.");
    // Green's Walker is already down (structures read): Blue is next.
    let c = PlanCoach::default();
    s.structures = vec![StructureState { lane: Some(Lane::Green), kind: "walker".into(), own: false, alive: false }];
    assert!(find(&eval(&c, &s), "plan:walker").unwrap().text.contains("Blue lane"));
    let c = PlanCoach::default();
    assert!(find(&eval(&c, &snap(680)), "plan:guardian").unwrap().text.starts_with("Guardians go weak at 12:00."));
}

/// Midboss seen up, then gone for 10+ s from `kill`; souls per team over the kill.
fn midboss_kill(c: &PlanCoach, kill: u32, ally: (u32, u32), enemy: (u32, u32), enemies_dead: usize, allies_dead: usize, taken_by: Option<&str>) -> Vec<(u32, Cue)> {
    let mut out = vec![];
    for t in (kill - 20..kill + 30).step_by(2) {
        let mut s = snap(t); you(&mut s, 0.5, 0.75);
        let after = t >= kill;
        s.minimap.as_mut().unwrap().midboss_up = !after;
        s.ally_networth = Some(if t + 10 >= kill { ally.1 } else { ally.0 });
        s.enemy_networth = Some(if t + 10 >= kill { enemy.1 } else { enemy.0 });
        kill_enemies(&mut s, enemies_dead, 40); kill_allies(&mut s, allies_dead);
        if after { s.midboss_taken_by = taken_by.map(String::from); }
        for cue in eval(c, &s) { if cue.key.starts_with("lead:") { c.spoken(&cue); } out.push((t, cue)); }
    }
    out
}

#[test]
fn an_enemy_midboss_is_called_from_the_snapshot_or_the_souls_swing() {
    // midboss_taken_by from the vision.
    let c = PlanCoach::default();
    let said = midboss_kill(&c, 2378, (293_000, 295_000), (247_000, 248_000), 0, 0, Some("enemy"));
    let lead: Vec<_> = said.iter().filter(|x| x.1.key.starts_with("lead:midboss")).collect();
    assert_eq!(lead.len(), 1, "{said:?}");
    assert!(lead[0].1.text.starts_with("They took Midboss: Rejuvenator for three minutes."));
    // rec13 39:38: no taker field; their souls +7k against our +2k around the kill.
    let c = PlanCoach::default();
    let said = midboss_kill(&c, 2378, (293_000, 295_000), (247_000, 254_000), 2, 3, None);
    let at = said.iter().find(|x| x.1.key.starts_with("lead:midboss")).map(|x| x.0).expect("enemy midboss call");
    assert!(at >= 2378 + 12 && at <= 2378 + 16, "said at {at}");
    // rec13 29:52: five of them dead, two of us: ours (no lead call), and its respawn is planned.
    let c = PlanCoach::default();
    let said = midboss_kill(&c, 1792, (194_000, 199_000), (172_000, 175_000), 5, 2, None);
    assert!(!said.iter().any(|x| x.1.key.starts_with("lead:midboss")), "{said:?}");
    let mut s = snap(1792 + 420 - 30); s.minimap.as_mut().unwrap().midboss_up = false;
    let cue = find(&eval(&c, &s), "plan:midboss_back").cloned().expect("respawn plan");
    assert_eq!(cue.text, "Midboss back in 30 seconds: clear your wave, group mid; you shoot from the pit edge, Snare the entrance.");
}

#[test]
fn a_midboss_window_needs_enough_of_them_dead_long_enough() {
    let c = PlanCoach::default();
    let mut s = snap(1500); kill_enemies(&mut s, 3, 40);
    assert_eq!(find(&eval(&c, &s), "plan:midboss_window").unwrap().text, "Three of them dead for 40 seconds: Midboss now, together. You shoot from the pit edge, Snare the entrance.");
    let mut s = snap(1500); kill_enemies(&mut s, 3, 20);
    assert!(find(&eval(&c, &s), "plan:midboss_window").is_none(), "20 s is too short");
    let mut s = snap(1500); kill_enemies(&mut s, 2, 40);
    assert!(find(&eval(&c, &s), "plan:midboss_window").is_none());
    let mut s = snap(1500); kill_enemies(&mut s, 3, 40); kill_allies(&mut s, 3);
    assert!(find(&eval(&c, &s), "plan:midboss_window").is_none(), "only three of us alive");
}

fn lead_series(c: &PlanCoach, from: u32, to: u32, lead: impl Fn(u32) -> i64) -> Vec<(u32, Cue)> {
    let mut out = vec![];
    for t in (from..to).step_by(5) {
        let mut s = snap(t);
        let e = 200_000 + (t - from) * 50;
        s.enemy_networth = Some(e); s.ally_networth = Some((e as i64 + lead(t)) as u32);
        for cue in eval(c, &s).into_iter().filter(|c| c.key.starts_with("lead:")) { c.spoken(&cue); out.push((t, cue)); }
    }
    out
}

#[test]
fn a_falling_lead_is_called_once() {
    // rec13: +46k at 36:30 thrown. Here 30k falling 100 souls a second.
    let c = PlanCoach::default();
    let said = lead_series(&c, 2000, 2400, |t| if t < 2100 { 30_000 } else { 30_000 - (t as i64 - 2100) * 80 });
    let falling: Vec<_> = said.iter().filter(|x| x.1.key.starts_with("lead:falling")).collect();
    assert_eq!(falling.len(), 1, "{said:?}");
    assert!(falling[0].1.text.starts_with("Back to our base together") && falling[0].1.text.contains("from 30k to 20k"), "{}", falling[0].1.text);
    // A steady lead is not a throw.
    let c = PlanCoach::default();
    assert!(lead_series(&c, 2000, 2400, |_| 30_000).is_empty());
}

fn fed_series(c: &PlanCoach, lead: i64) -> Vec<(u32, Cue)> {
    let mut out = vec![];
    for t in (1800..1960).step_by(5) {
        let mut s = snap(t);
        let nws = [40_000u32, 30_000, 28_000, 31_000, 29_000, 27_000]; // Infernus 1.38x the rest
        for (h, nw) in ENEMIES.iter().zip(nws) { enemy(&mut s, h).networth = Some(nw); }
        let e: u32 = nws.iter().sum();
        s.enemy_networth = Some(e); s.ally_networth = Some((e as i64 + lead) as u32);
        for cue in eval(c, &s).into_iter().filter(|c| c.key.starts_with("lead:")) { c.spoken(&cue); out.push((t, cue)); }
    }
    out
}

#[test]
fn a_fed_enemy_is_named_once_with_the_lead_when_ahead() {
    let c = PlanCoach::default();
    let said = fed_series(&c, 45_000);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].1.text, "We're 45k up but Infernus is their whole game. No fights in their base without Rejuvenator.");
    assert!(said[0].0 >= 1800 + 90, "needs 90 s of agreeing reads: {}", said[0].0);
    let c = PlanCoach::default();
    let said = fed_series(&c, 2_000);
    assert_eq!(said[0].1.text, "Infernus is fed at 40k, far above his team. Fight him only together, never alone.");
}

#[test]
fn low_health_after_a_won_fight_says_heal_first() {
    let c = PlanCoach::default();
    let fight = |hp: u32, y: f32, enemies: usize, allies: usize| { let mut s = snap(1750); you(&mut s, 0.5, y); s.hp = Some(hp); kill_enemies(&mut s, enemies, 30); kill_allies(&mut s, allies); s };
    let cue = find(&eval(&c, &fight(600, 0.35, 5, 1)), "reset:").cloned().expect("reset");
    assert_eq!(cue.text, "Fight's won, but you're at 30%. Back off and heal, then Midboss together.");
    assert_eq!(cue.priority, Priority::High, "said under 30% where only safety calls pass");
    assert!(words(&cue.text) <= 15);
    assert!(find(&eval(&c, &fight(600, 0.7, 5, 1)), "reset:").is_none(), "own half");
    assert!(find(&eval(&c, &fight(160, 0.35, 5, 1)), "reset:").is_none(), "critical: the critical-health call owns it");
    assert!(find(&eval(&c, &fight(900, 0.35, 5, 1)), "reset:").is_none(), "45% is fine");
    assert!(find(&eval(&c, &fight(600, 0.35, 2, 2)), "reset:").is_none(), "not won");
    // The marker lost mid-fight (rec13 29:17): the last position seen within 15 s counts.
    let c = PlanCoach::default();
    let mut s = fight(1800, 0.35, 5, 1); s.game_clock_secs = Some(1740); eval(&c, &s);
    let mut s = fight(600, 0.35, 5, 1); s.minimap.as_mut().unwrap().you = None;
    assert!(find(&eval(&c, &s), "reset:").is_some());
    // Once per fight.
    c.spoken(find(&eval(&c, &s), "reset:").unwrap());
    let mut s2 = s.clone(); s2.game_clock_secs = Some(1780);
    assert!(find(&eval(&c, &s2), "reset:").is_none());
}

#[test]
fn a_missing_diver_is_named_with_your_counter_play_once_per_three_minutes() {
    // rec13 22:42: Graves without a health bar, you on Green in their half, the rest seen.
    let c = PlanCoach::default();
    let mut s3 = snap(1362); you(&mut s3, 0.3, 0.3);
    for h in s3.hero_status.iter_mut().filter(|h| !h.ally) { h.bar_visible = Some(true); }
    enemy(&mut s3, "hero_necro").bar_visible = Some(false);
    for t in (1350..1360).step_by(2) { let mut s2 = s3.clone(); s2.game_clock_secs = Some(t); assert!(find(&eval(&c, &s2), "threat:").is_none(), "missing under 10 s at {t}"); }
    let cue = speak(&c, &s3, "threat:").expect("Graves");
    assert_eq!(cue.text, "Graves is missing: her Grasping Hands root you. Walk behind a teammate, not on ziplines.");
    assert!(words(&cue.text) <= 15);
    // Not again for three minutes.
    let mut s4 = s3.clone(); s4.game_clock_secs = Some(1362 + 120);
    assert!(find(&eval(&c, &s4), "threat:hero_necro").is_none());
    // Not in your own half, and not before 10:00.
    let c = PlanCoach::default();
    for t in (1340..1366).step_by(2) { let mut s5 = s3.clone(); s5.game_clock_secs = Some(t); you(&mut s5, 0.3, 0.7); assert!(find(&eval(&c, &s5), "threat:").is_none()); }
    let c = PlanCoach::default();
    for t in (500..520).step_by(2) { let mut s5 = s3.clone(); s5.game_clock_secs = Some(t); assert!(find(&eval(&c, &s5), "threat:").is_none()); }
}

#[test]
fn a_close_threat_needs_identity_evidence() {
    let c = PlanCoach::default();
    let mut s = snap(400); you(&mut s, 0.5, 0.6);
    // Lady Geist's bar is visible and one enemy ring is near you; she is the only enemy seen.
    enemy(&mut s, "hero_ghost").bar_visible = Some(true);
    s.minimap.as_mut().unwrap().heroes = vec![HeroTrack { id: 1, ally: false, pos: Pt { x: 0.52, y: 0.55 }, unseen_ms: 0, hits: 3, ..Default::default() }];
    let cue = find(&eval(&c, &s), "threat:").cloned().expect("Geist");
    assert_eq!(cue.text, "Lady Geist is close: her Essence Bomb blows after arming. Keep moving at max range.");
    // Two enemies seen, one ring near: not known which one.
    let c = PlanCoach::default();
    enemy(&mut s, "hero_inferno").bar_visible = Some(true);
    assert!(find(&eval(&c, &s), "threat:").is_none());
    // A read top-bar position near you names the hero.
    enemy(&mut s, "hero_inferno").position = Some(Pt { x: 0.55, y: 0.6 }); enemy(&mut s, "hero_inferno").position_observations = 2;
    enemy(&mut s, "hero_ghost").position = Some(Pt { x: 0.9, y: 0.1 }); enemy(&mut s, "hero_ghost").position_observations = 2;
    assert_eq!(find(&eval(&c, &s), "threat:").unwrap().key, "threat:hero_inferno");
    // Another hero: the generic counter-play.
    let c = PlanCoach::default();
    s.hero = Some("hero_bebop".into());
    assert!(find(&eval(&c, &s), "threat:").unwrap().text.ends_with("Break line of sight early."));
}

#[test]
fn plans_register_in_the_default_engine_and_degrade_without_the_top_bar() {
    // No top bar, no minimap: objective timing plans still speak, nothing else is invented.
    let mut s = GameSnapshot { game_clock_secs: Some(560), screen: Some("Gameplay".into()), ..Default::default() };
    let c = PlanCoach::default();
    let cues = eval(&c, &s);
    assert_eq!(cues.len(), 1, "{cues:?}");
    assert!(cues[0].text.starts_with("Urn in 40 on Yellow side: we contest it together."));
    s.dead = true;
    assert!(PlanCoach::default().evaluate(&s).is_empty(), "no plan on the death screen");
    let mut e = Engine::new(coach_engine::default_rules(coach_engine::RulesConfig::default()));
    e.max_per_step = 20; e.min_gap_secs = 0;
    let s = snap(560);
    assert!(e.step(&s).iter().any(|c| c.key == "plan:urn:600"));
}

// ---- docs/coach-review-8.md D2, D3, D10 -----------------------------------------------------

#[test]
fn our_midboss_gives_one_rejuvenator_push_call_to_a_reachable_target() {
    // rec8 38:41 and rec13 30:13: our Midboss, no plan.
    let c = PlanCoach::default();
    let said = midboss_kill(&c, 1800, (194_000, 199_000), (172_000, 173_000), 0, 0, Some("ally"));
    let first = said.iter().find(|x| x.1.key.starts_with("plan:rejuv")).expect("rejuvenator plan");
    assert_eq!(first.1.text, "All five push their base now: we have Rejuvenator for three minutes.");
    assert_eq!(first.1.priority, Priority::High);
    assert!(words(&first.1.text) <= 20);
    c.spoken(&first.1.clone());
    let mut s = snap(1840); you(&mut s, 0.5, 0.6);
    assert!(find(&eval(&c, &s), "plan:rejuv").is_none(), "once per kill");
    // Their Shrines when a lane is open to them; it lasts three minutes.
    let c = PlanCoach::default();
    midboss_kill(&c, 1800, (194_000, 199_000), (172_000, 173_000), 0, 0, Some("ally"));
    let mut s = snap(1900);
    s.structures = ["guardian", "walker", "guardian"].iter().map(|k| StructureState { lane: Some(Lane::Blue), kind: k.to_string(), own: false, alive: false })
        .chain(std::iter::once(StructureState { lane: None, kind: "shrine".into(), own: false, alive: true })).collect();
    assert_eq!(find(&eval(&c, &s), "plan:rejuv").unwrap().text, "All five push their Shrines now: we have Rejuvenator for 80 seconds.");
    assert!(find(&eval(&c, &snap(1950)), "plan:rejuv").is_none(), "under 45 s left");
    // Their Midboss: no push call.
    let c = PlanCoach::default();
    let said = midboss_kill(&c, 1800, (194_000, 195_000), (172_000, 178_000), 0, 0, Some("enemy"));
    assert!(!said.iter().any(|x| x.1.key.starts_with("plan:rejuv")));
}
#[test]
fn a_late_taker_read_is_the_same_midboss_kill() {
    // rec13: "They took Midboss" at 39:50 (souls swing) and again at 40:38 (the taker read later).
    let c = PlanCoach::default();
    let mut said = midboss_kill(&c, 2378, (293_000, 295_000), (247_000, 254_000), 2, 3, None);
    for t in (2410..2470).step_by(4) { let mut s = snap(t); s.minimap.as_mut().unwrap().midboss_up = false; s.midboss_taken_by = Some("enemy".into());
        for cue in eval(&c, &s) { if cue.key.starts_with("lead:") { c.spoken(&cue); } said.push((t, cue)); } }
    assert_eq!(said.iter().filter(|x| x.1.key.starts_with("lead:midboss")).count(), 1, "{said:?}");
}
#[test]
fn the_fed_call_never_says_without_rejuvenator_while_we_hold_it() {
    // rec13 32:18: "No fights in their base without Rejuvenator" two minutes after our Midboss.
    let c = PlanCoach::default();
    let mut said = vec![];
    for t in (1800..1960).step_by(5) {
        let mut s = snap(t); s.midboss_taken_by = Some("ally".into());
        let nws = [40_000u32, 30_000, 28_000, 31_000, 29_000, 27_000];
        for (h, nw) in ENEMIES.iter().zip(nws) { enemy(&mut s, h).networth = Some(nw); }
        let e: u32 = nws.iter().sum(); s.enemy_networth = Some(e); s.ally_networth = Some(e + 45_000);
        for cue in eval(&c, &s).into_iter().filter(|c| c.key.starts_with("lead:fed")) { c.spoken(&cue); said.push(cue.text); }
    }
    assert_eq!(said, ["We're 45k up with our Rejuvenator, but Infernus is their whole game: push only together."]);
}
#[test]
fn a_throw_is_called_within_seconds_with_the_newest_numbers() {
    // rec8 41:07: 38k -> 27k in seconds, called at 43:01 ("39k to 6k"); rec13 42:08 "46k to 35k" at 10k.
    let c = PlanCoach::default();
    let said = lead_series(&c, 2400, 2600, |t| if t < 2460 { 38_000 } else if t < 2470 { 30_000 } else if t < 2520 { 20_000 } else { 8_000 });
    let falling: Vec<_> = said.iter().filter(|x| x.1.key.starts_with("lead:falling")).collect();
    assert_eq!(falling.len(), 1, "one call per throw: {said:?}");
    assert_eq!(falling[0].0, 2470, "10 s after the drop began");
    assert_eq!(falling[0].1.text, "Back to our base together, nobody fights alone: our lead fell from 38k to 20k.");
    assert_eq!(falling[0].1.priority, Priority::High);
    // In their half: get out and regroup; our structure under attack is named.
    let c = PlanCoach::default();
    let mut out = vec![];
    for t in (2400..2500).step_by(5) {
        let mut s = snap(t); you(&mut s, 0.5, 0.3);
        let e = 200_000 + (t - 2400) * 50; s.enemy_networth = Some(e); s.ally_networth = Some(e + if t < 2460 { 38_000 } else { 26_000 });
        out.extend(eval(&c, &s).into_iter().filter(|c| c.key.starts_with("lead:falling")).map(|c| c.text));
    }
    assert_eq!(out.first().map(String::as_str), Some("Out of their base and regroup: our lead fell from 38k to 26k."));
    // A one-read spike is not the peak (rec8 34:18 read +38k once between +33k reads).
    let c = PlanCoach::default();
    let said = lead_series(&c, 2000, 2200, |t| if t == 2050 { 45_000 } else { 33_000 });
    assert!(said.is_empty(), "{said:?}");
}
#[test]
fn the_same_missing_threat_waits_six_minutes_and_is_said_shorter() {
    // rec13: "Abrams is missing: his Shoulder Charge stuns on walls. Snare at your feet." four times.
    let c = PlanCoach::default();
    let missing = |t: u32| { let mut s = snap(t); you(&mut s, 0.3, 0.3); for h in s.hero_status.iter_mut().filter(|h| !h.ally) { h.bar_visible = Some(true); } enemy(&mut s, "hero_necro").bar_visible = Some(false); s };
    let seen = |t: u32| { let mut s = missing(t); enemy(&mut s, "hero_necro").bar_visible = Some(true); s };
    eval(&c, &missing(1350));
    speak(&c, &missing(1362), "threat:").expect("first call");
    eval(&c, &seen(1580));
    for t in (1585..1610).step_by(5) { assert!(find(&eval(&c, &missing(t)), "threat:hero_necro").is_none(), "{t}: 4 minutes after the first"); }
    eval(&c, &seen(1700));
    let mut again = None;
    for t in (1705..1740).step_by(5) { if let Some(cue) = speak(&c, &missing(t), "threat:hero_necro") { again = Some((t, cue)); break; } }
    let (t, cue) = again.expect("said again after six minutes");
    assert!(t >= 1362 + 360, "{t}");
    assert_eq!(cue.text, "Graves is missing again. Walk behind a teammate, not on ziplines.");
}