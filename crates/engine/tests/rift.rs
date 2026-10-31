//! Unstable Rift calls (crates/engine/src/rift.rs, data/coaching/rift.json, docs/rift.md).
use coach_core::{GameSnapshot, HeroStatus, Lane, MinimapState, Pt, RiftEvent, RiftState};
use coach_engine::{rift::RiftCoach, Cue, Engine, Priority, Rule};

fn team(ally_dead: usize, enemy_dead: usize) -> Vec<HeroStatus> {
    (0..12).map(|i| {
        let ally = i < 6; let slot = i % 6;
        let dead = if ally { slot >= 6 - ally_dead } else { slot < enemy_dead };
        HeroStatus { ally, slot, dead: Some(dead), bar_visible: Some(!dead), health_ratio: (!dead).then_some(0.9), ..Default::default() }
    }).collect()
}
fn snap(t: u32, rift: Option<RiftState>) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Gameplay".into()), hp: Some(2400), max_hp: Some(3000), health_observations: 3,
        ally_networth: Some(60_000), enemy_networth: Some(58_000), hero_status: team(0, 0),
        minimap: Some(MinimapState { observations: 3, you: Some(Pt { x: 0.5, y: 0.7 }), ..Default::default() }), rift, ..Default::default() }
}
fn rift(phase: &str, lane: Option<Lane>) -> RiftState {
    RiftState { phase: phase.into(), lane, side: lane.map(|_| "right".into()), seen_at: Some(1100), opened_at: Some(1120), ..Default::default() }
}
fn eval(s: &GameSnapshot) -> Vec<Cue> { RiftCoach::default().evaluate(s) }
fn speakable(c: &Cue) {
    let words = c.text.split_whitespace().count();
    let sentences = c.text.matches(['.', '!', '?']).count();
    assert!(words <= 25 && sentences <= 2 && !c.text.contains('{'), "{}", c.text);
}

#[test]
fn rift_cards_quote_real_document_guidance() {
    let c = RiftCoach::default();
    let cat = c.catalog();
    assert_eq!(cat.schema_version, 1);
    assert_eq!(cat.cards.len(), 8);
    for card in &cat.cards {
        assert!(!card.sources.is_empty() && card.cooldown_secs >= 30 && !card.rationale.is_empty() && card.family == "rift", "{}", card.id);
        for s in &card.sources {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&s.path)).unwrap();
            assert!(text.contains(&s.excerpt), "{}: missing excerpt {}", card.id, s.excerpt);
            assert!(text.contains(&s.section), "{}: missing section {}", card.id, s.section);
            assert!(!s.excerpt.contains("UNVERIFIED"));
            assert!(["WIKI", "COMMUNITY", "GENERAL"].contains(&s.basis.as_str()), "{}: basis {}", card.id, s.basis);
        }
    }
}

#[test]
fn one_heads_up_before_the_first_window() {
    assert!(eval(&snap(600, None)).is_empty(), "too early");
    let c = eval(&snap(630, None));
    assert_eq!(c.len(), 1); assert_eq!(c[0].key, "plan:rift_soon:660");
    assert_eq!(c[0].text, "Rift soon, from 11:00 on a side-lane bridge: push your wave first.");
    speakable(&c[0]);
    assert!(eval(&snap(650, None)).is_empty(), "not 10 s before");
    // The next window: 6 minutes after the last Rift's effect (opened 11:24 -> effect 10:04), announced a minute later.
    let done = RiftState { phase: "none".into(), history: vec![RiftEvent { opened_at: Some(684), ended_at: Some(706), outcome: Some("captured".into()), ..Default::default() }], ..Default::default() };
    let c = eval(&snap(994, Some(done)));
    assert_eq!(c.first().map(|c| (c.key.as_str(), c.text.as_str())), Some(("plan:rift_soon:1024", "Rift soon, from 17:04 on a side-lane bridge: push your wave first.")));
}

#[test]
fn announcement_groups_us_names_the_lane_and_the_seconds() {
    let mut s = snap(1100, Some(rift("announced", Some(Lane::Yellow))));
    s.hero_status = team(1, 0);
    let c = eval(&s);
    assert_eq!(c[0].key, "plan:rift:1100");
    assert_eq!(c[0].text, "Rift in Yellow in 20 seconds: five of us are up, group on the bridge.");
    speakable(&c[0]);
    // A teammate already on the bridge is named.
    let mut s2 = s.clone();
    s2.hero_status[0].hero = Some("hero_bebop".into()); s2.hero_status[0].position = Some(Pt { x: 0.85, y: 0.5 }); s2.hero_status[0].position_observations = 2;
    assert_eq!(eval(&s2)[0].text, "Rift in Yellow in 20 seconds: five of us are up, group with Bebop on the bridge.");
    // Lane unknown right after the text: wait for the minimap ping; then no lane, no count
    // (top bar not fully read).
    let mut s3 = snap(1100, Some(rift("announced", None))); s3.hero_status.truncate(10);
    assert!(eval(&s3).is_empty());
    s3.game_clock_secs = Some(1108);
    assert_eq!(eval(&s3)[0].text, "Rift on a side lane in 12 seconds: group on the bridge.");
    // Low on health: no call into the point.
    let mut s4 = s.clone(); s4.hp = Some(900); assert!(eval(&s4).is_empty());
    // Missed announcement: the opening says it is open.
    let c = eval(&snap(1125, Some(rift("open", Some(Lane::Green)))));
    assert_eq!(c[0].text, "The Rift is open in Green: six of us are up, group on the bridge.");
}

#[test]
fn behind_we_all_go_and_with_two_dead_we_skip() {
    let mut s = snap(1100, Some(rift("announced", Some(Lane::Yellow))));
    s.ally_networth = Some(50_000); s.enemy_networth = Some(58_000);
    let c = eval(&s);
    assert_eq!((c[0].key.as_str(), c[0].text.as_str()), ("plan:rift:1100", "We're behind, so we capture the Rift faster: all go Yellow."));
    let mut s = snap(1100, Some(rift("announced", Some(Lane::Yellow))));
    s.hero_status = team(3, 0);
    let c = eval(&s);
    assert_eq!((c[0].key.as_str(), c[0].text.as_str()), ("rift:skip:1100", "Skip the Rift: three of us are dead, farm safe."));
    s.hero_status = team(2, 0);
    assert_eq!(eval(&s)[0].text, "Skip the Rift: two of us are dead, farm safe.");
    // As many of theirs dead: no skip.
    s.hero_status = team(2, 2);
    assert_eq!(eval(&s)[0].key, "plan:rift:1100");
    // One dead: still a go.
    s.hero_status = team(1, 0);
    assert_eq!(eval(&s)[0].key, "plan:rift:1100");
}

#[test]
fn while_open_contested_and_after() {
    let mut r = rift("open", Some(Lane::Yellow)); r.enemy_rising = true; r.enemy_progress = Some(43); r.ally_progress = Some(0);
    let keys: Vec<String> = eval(&snap(1130, Some(r.clone()))).into_iter().map(|c| c.key).collect();
    assert!(keys.contains(&"plan:rift_stop:1100".to_string()), "{keys:?}");
    let stop = eval(&snap(1130, Some(r))).into_iter().find(|c| c.key == "plan:rift_stop:1100").unwrap();
    assert_eq!(stop.text, "They're alone on the Rift: step in to stop the capture.");
    let mut r = rift("contested", Some(Lane::Yellow)); r.contested_secs = Some(50);
    let c = eval(&snap(1180, Some(r)));
    let d = c.iter().find(|c| c.key == "plan:rift_orbs:1100").unwrap();
    assert_eq!((d.text.as_str(), d.priority), ("The Rift draws in 10 seconds: grab the soul orbs.", Priority::High));
    let mut r = rift("captured", Some(Lane::Yellow)); r.ended_at = Some(1140);
    assert_eq!(eval(&snap(1145, Some(r.clone())))[0].text, "Rift Troopers in Yellow: push behind them.");
    assert!(eval(&snap(1200, Some(r))).is_empty(), "too late to push behind them");
    let mut r = rift("lost", None); r.ended_at = Some(1140);
    let c = eval(&snap(1145, Some(r)));
    assert_eq!((c[0].key.as_str(), c[0].text.as_str()), ("rift:defend:1100", "Their Rift Troopers are coming down the Rift's lane: defend at the Guardian, don't fight them alone."));
    speakable(&c[0]);
    // Dead or in a menu: nothing.
    let mut s = snap(1100, Some(rift("announced", Some(Lane::Yellow)))); s.dead = true; assert!(eval(&s).is_empty());
}

#[test]
fn the_engine_says_each_once_and_never_under_a_hold_call() {
    let mut e = Engine::new(vec![Box::new(RiftCoach::default())]);
    let s = snap(1100, Some(rift("announced", Some(Lane::Yellow))));
    assert_eq!(e.step(&s).first().map(|c| c.key.clone()), Some("plan:rift:1100".into()));
    let mut s2 = s.clone(); s2.game_clock_secs = Some(1160);
    assert!(e.step(&s2).is_empty(), "said once per Rift");
    // Three of us dead: the go call is blocked by the engine's contradiction guard, the skip passes.
    let mut e = Engine::new(vec![Box::new(RiftCoach::default())]);
    let mut s = snap(1100, Some(rift("announced", Some(Lane::Yellow)))); s.hero_status = team(3, 0);
    assert_eq!(e.step(&s).first().map(|c| c.key.clone()), Some("rift:skip:1100".into()));
    assert!(coach_engine::is_go_call("plan:rift:1100") && coach_engine::is_go_call("plan:rift_soon:660") && !coach_engine::is_go_call("rift:skip:1100"));
}
