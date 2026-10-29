//! One coach voice (docs/coach-review-13.md items 1, 4, 9, 10, 14): one death is one event and one
//! review, fight mode, the phase cadence budget, wave / timer / last-hit phase rules and praise
//! timing, all in Engine::step.
use coach_core::{DamageRow, GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, ObjectiveBanner, Pt};
use coach_engine::{fight_text, laning::LaneCoach, review::ReviewCoach, Cue, Engine, Priority, Rule, RulesConfig, UnspentSoulsRule};
use std::cell::RefCell;

fn track(ally: bool, x: f32, y: f32) -> HeroTrack { HeroTrack { ally, pos: Pt { x, y }, hits: 5, ..Default::default() } }
fn alive(t: u32) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Gameplay".into()), hp: Some(2800), max_hp: Some(3200), health_observations: 3,
        minimap: Some(MinimapState { observations: 3, you: Some(Pt { x: 0.5, y: 0.7 }), ..Default::default() }), ..Default::default() }
}
fn dead(t: u32, respawn: Option<u32>) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Dead".into()), dead: true, respawn_secs: respawn, souls: Some(900),
        death_cause: Some("Melee".into()), death_damage: vec![DamageRow { source: "Melee".into(), hits: None, damage: 2322 }], ..Default::default() }
}
fn screen(t: u32, name: &str) -> GameSnapshot { GameSnapshot { game_clock_secs: Some(t), screen: Some(name.into()), ..Default::default() } }

/// A rule that offers fixed cues every step.
struct Offer(Vec<Cue>);
impl Offer { fn new(c: Vec<Cue>) -> Box<Self> { Box::new(Offer(c)) } }
impl Rule for Offer { fn evaluate(&self, _: &GameSnapshot) -> Vec<Cue> { self.0.clone() } }
/// Offers what the script says at each clock.
struct Script(Box<dyn Fn(u32) -> Vec<Cue>>);
impl Rule for Script { fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> { (self.0)(s.game_clock_secs.unwrap()) } }
/// Offers a new key each step (so cooldowns never hold it back).
struct Stream(&'static str, RefCell<u32>, Priority);
impl Rule for Stream {
    fn evaluate(&self, _: &GameSnapshot) -> Vec<Cue> { let mut n = self.1.borrow_mut(); *n += 1; vec![cue(&format!("{}:{n}", self.0), "Say it.", self.2)] }
}
fn stream(prefix: &'static str, p: Priority) -> Box<Stream> { Box::new(Stream(prefix, RefCell::new(0), p)) }
fn cue(key: &str, text: &str, priority: Priority) -> Cue { Cue { key: key.into(), text: text.into(), priority, cooldown_secs: 1 } }
fn spoken(e: &mut Engine, s: &GameSnapshot) -> Vec<String> { e.step(s).into_iter().map(|c| c.key).collect() }
fn said_at(e: &mut Engine, from: u32, to: u32) -> Vec<u32> { (from..to).step_by(5).filter(|t| !e.step(&alive(*t)).is_empty()).collect() }

// ---- 1. One death = one event ----------------------------------------------------------------

/// rec13 41:01-42:52 (live frames): Shop, single Gameplay frames without health and a Scoreboard
/// frame inside one 90 s death produced two extra reviews, "died 0 / 5 s after respawning".
fn rec13_death5() -> Vec<GameSnapshot> {
    let mut f = vec![];
    for t in [2440u32, 2449, 2452, 2461] { let mut s = alive(t); s.souls = Some(900); f.push(s); }
    let mut shop = screen(2465, "Shop"); shop.dead = true; f.push(shop);
    f.push(dead(2469, None));
    f.push(screen(2477, "Gameplay")); // flicker: no health read
    for (t, r) in [(2480, None), (2484, None), (2490, Some(64)), (2494, Some(60)), (2499, Some(55))] { f.push(dead(t, r)); }
    f.push(screen(2504, "Gameplay")); f.push(screen(2509, "Gameplay"));
    f.push(dead(2513, None));
    f.push(screen(2517, "Scoreboard"));
    for (t, r) in [(2528, None), (2537, None), (2542, None), (2545, Some(9)), (2552, Some(2))] { f.push(dead(t, r)); }
    for t in [2555u32, 2559, 2562, 2566] { f.push(screen(t, "Gameplay")); }
    f.push(alive(2571));
    f
}
#[test]
fn a_death_with_flicker_frames_is_one_death_and_one_review() {
    let mut e = Engine::new(vec![Box::new(ReviewCoach::default()), Box::new(UnspentSoulsRule::new(RulesConfig::default()))]);
    let mut reviews = vec![];
    for s in rec13_death5() {
        for c in e.step(&s) { if c.key.starts_with("death:review") { reviews.push(c.text); } }
        if (2465..2553).contains(&s.game_clock_secs.unwrap()) { assert!(e.life().dead_since.is_some(), "still dead at {:?}", s.game_clock_secs); }
    }
    assert_eq!(reviews.len(), 1, "{reviews:?}");
    assert!(!reviews[0].contains("after respawning"), "{reviews:?}");
    // The respawn is the timer running out (2552 + 2), not a flicker frame.
    assert_eq!(e.life().respawned_at, Some(2554));
    assert_eq!(e.life().last_death, Some(2465));
}
#[test]
fn a_respawn_needs_the_timer_or_two_health_reads() {
    let mut e = Engine::new(vec![]);
    e.step(&dead(1000, Some(40)));
    // One health read with 30 s left on the timer is a spectated or misread frame.
    e.step(&alive(1010)); assert!(e.life().dead_since.is_some());
    e.step(&alive(1014)); assert!(e.life().dead_since.is_some(), "two reads, but the timer says 26 s left");
    e.step(&alive(1018)); assert!(e.life().dead_since.is_none(), "three health reads in a row end it");
    assert_eq!(e.life().respawned_at, Some(1010));
    // No timer read: two health reads.
    let mut e = Engine::new(vec![]);
    e.step(&dead(2000, None)); e.step(&dead(2020, None));
    e.step(&alive(2030)); assert!(e.life().dead_since.is_some());
    e.step(&alive(2034)); assert_eq!(e.life().respawned_at, Some(2030));
    // The grey "Respawn in: 1" revive: health back within 10 s, no timer.
    let mut e = Engine::new(vec![]);
    e.step(&dead(1515, None)); e.step(&alive(1520));
    assert_eq!(e.life().respawned_at, Some(1520));
}
#[test]
fn frames_inside_a_death_reach_the_rules_as_death_frames() {
    struct Seen(std::rc::Rc<RefCell<Vec<(u32, bool)>>>);
    impl Rule for Seen { fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> { self.0.borrow_mut().push((s.game_clock_secs.unwrap(), s.dead)); vec![] } }
    let seen = std::rc::Rc::new(RefCell::new(vec![]));
    let mut e = Engine::new(vec![Box::new(Seen(seen.clone()))]);
    for s in rec13_death5() { e.step(&s); }
    let v = seen.borrow();
    for t in [2477u32, 2504, 2509, 2517] { assert!(v.contains(&(t, true)), "{t} must be a death frame: {v:?}"); }
    assert!(v.contains(&(2555, false)), "the timer ran out: alive again");
}

// ---- 3. Fight mode ---------------------------------------------------------------------------

fn fight(t: u32) -> GameSnapshot {
    let mut s = alive(t);
    s.minimap.as_mut().unwrap().heroes = vec![track(true, 0.52, 0.7), track(true, 0.48, 0.72), track(false, 0.55, 0.66), track(false, 0.5, 0.64)];
    s
}
#[test]
fn a_fight_gets_one_short_safety_call_and_no_chatter() {
    let offers = vec![
        cue("map:wave:Green", "Green wave is deep on our side. Catch it before you take camps.", Priority::Low),
        cue("timer:Soul Urn:1200", "Soul Urn in 40 seconds", Priority::Low),
        cue("praise:window:1", "Good use of the window.", Priority::Normal),
        cue("lead", "We are ahead in souls.", Priority::Normal),
        cue("pos:gank", "Fall back: 2 enemies closing from Green lane. Get to our guardian or a teammate.", Priority::High),
    ];
    let mut e = Engine::new(vec![Offer::new(offers)]);
    let c = e.step(&fight(1500));
    assert!(e.in_fight());
    assert_eq!(c.len(), 1); assert_eq!(c[0].key, "pos:gank");
    assert_eq!(c[0].text, "Fall back: 2 enemies closing from Green lane.");
    let mut later = vec![];
    for t in (1503..1530).step_by(3) { later.extend(spoken(&mut e, &fight(t))); }
    // pos:gank's cooldown (1 s here) is not what holds it back: one call per 15 s (20 s for the
    // same "back off" family), nothing but safety calls.
    assert_eq!(later, ["pos:gank"], "{later:?}");
}
#[test]
fn the_critical_exit_may_follow_after_three_seconds_but_never_a_third_call_in_30_s() {
    let mut e = Engine::new(vec![Box::new(Script(Box::new(|t| match t {
        1500 => vec![cue("lane:back_off", "Back behind cover: you're getting low.", Priority::High)],
        1502 | 1504 => vec![cue("lane:critical", "Health critical: get out now.", Priority::High)],
        _ => vec![cue("active:cc", "Escape the moment the stun ends.", Priority::High)],
    })))]);
    assert_eq!(spoken(&mut e, &fight(1500)), ["lane:back_off"]);
    assert!(spoken(&mut e, &fight(1502)).is_empty(), "2 s after the last call");
    assert_eq!(spoken(&mut e, &fight(1504)), ["lane:critical"], "the critical exit after 3 s");
    assert!(spoken(&mut e, &fight(1512)).is_empty(), "15 s gap");
    assert!(spoken(&mut e, &fight(1522)).is_empty(), "two calls in the last 30 s");
    assert_eq!(spoken(&mut e, &fight(1531)), ["active:cc"]);
}
#[test]
fn a_teammate_dying_or_health_falling_fast_is_a_fight() {
    let mut e = Engine::new(vec![]);
    let team = |dead: usize| (0..12).map(|i| HeroStatus { ally: i < 6, slot: i % 6, dead: Some(i < dead), ..Default::default() }).collect::<Vec<_>>();
    let mut s = alive(1000); s.hero_status = team(0); e.step(&s); assert!(!e.in_fight());
    let mut s = alive(1004); s.hero_status = team(1); e.step(&s); assert!(e.in_fight(), "a teammate just died");
    let mut s = alive(1020); s.hero_status = team(1); e.step(&s); assert!(!e.in_fight(), "16 s later");
    let mut s = alive(1030); s.health_delta_ratio = Some(-0.2); e.step(&s); assert!(e.in_fight(), "health falling fast");
}
#[test]
fn chatter_waits_for_the_end_of_a_siege() {
    let offers = || vec![cue("timer:Soul Urn:1200", "Soul Urn in 40 seconds", Priority::Low), cue("lead", "We are ahead in souls.", Priority::Normal)];
    let mut e = Engine::new(vec![Offer::new(offers())]);
    let mut s = alive(1500); s.objective_banner = Some(ObjectiveBanner { name: "Walker".into(), own: Some(false), health_ratio: Some(0.4) });
    assert!(spoken(&mut e, &s).is_empty(), "a structure banner is up");
    let mut e = Engine::new(vec![Offer::new(offers())]);
    assert_eq!(spoken(&mut e, &alive(1500)), ["lead"]);
}
#[test]
fn every_high_safety_card_leads_with_a_short_action() {
    // In a fight only the first sentence (<= 10 words) is said: it must carry the action.
    let mut speeches: Vec<(String, String)> = vec![];
    for c in coach_engine::active::ActiveCoach::shipped().cards.iter().filter(|c| c.priority == "high") { speeches.push((c.id.clone(), c.speech.clone())); }
    let pos: coach_engine::positioning::Catalog = serde_json::from_str(include_str!("../../../data/coaching/positioning.json")).unwrap();
    for c in pos.cards.iter().filter(|c| c.priority == "high") { speeches.push((c.id.clone(), c.speech.clone())); }
    let lane: coach_engine::laning::Catalog = serde_json::from_str(include_str!("../../../data/coaching/laning.json")).unwrap();
    for c in lane.cards.iter().filter(|c| ["critical", "back_off", "hold"].contains(&c.id.as_str())) { speeches.push((c.id.clone(), c.speech.clone())); }
    for (id, s) in speeches {
        let short = fight_text(&s);
        assert!(short.split_whitespace().count() <= 10, "{id}: {short}");
        assert!(!s.contains("Health looks low"), "{id}: robotic wording");
    }
}

// ---- 4. The cadence budget -------------------------------------------------------------------

#[test]
fn normal_calls_follow_the_phase_budget_and_state_readouts_get_half_of_it() {
    let mut e = Engine::new(vec![stream("pos:enemies_down", Priority::Normal)]);
    let early = said_at(&mut e, 100, 470);
    assert!(early.len() >= 5 && early.windows(2).all(|w| w[1] - w[0] >= 60), "laning: one per 60 s: {early:?}");
    let mut e = Engine::new(vec![stream("pos:enemies_down", Priority::Normal)]);
    let mid = said_at(&mut e, 600, 1000);
    assert!(mid.windows(2).all(|w| w[1] - w[0] >= 45) && mid.windows(2).any(|w| w[1] - w[0] < 60), "mid: one per 45 s: {mid:?}");
    let mut e = Engine::new(vec![stream("pos:missing", Priority::Normal)]);
    let state = said_at(&mut e, 600, 1000);
    assert!(state.windows(2).all(|w| w[1] - w[0] >= 90), "a state readout waits twice as long: {state:?}");
    // High safety calls bypass the budget.
    let mut e = Engine::new(vec![stream("lane:critical", Priority::High)]);
    assert!(said_at(&mut e, 600, 700).len() >= 8);
}

// ---- 5. Phase and priority -------------------------------------------------------------------

#[test]
fn wave_calls_are_low_one_per_90_s_and_never_while_grouped_forward_or_sieging() {
    let offers = vec![cue("map:wave:Green", "Green wave is deep on our side.", Priority::Low), cue("wave:overview", "Waves: Blue is coming to us.", Priority::Low)];
    let mut e = Engine::new(vec![Offer::new(offers)]);
    let said = said_at(&mut e, 1000, 1300);
    assert!(said.len() >= 2 && said.windows(2).all(|w| w[1] - w[0] >= 90), "{said:?}");
    let blocked = |f: &dyn Fn(&mut GameSnapshot)| {
        let mut e = Engine::new(vec![Offer::new(vec![cue("map:wave:Green", "Green wave is deep on our side.", Priority::Low)])]);
        let mut s = alive(1000); f(&mut s); e.step(&s).is_empty()
    };
    assert!(!blocked(&|_| {}));
    assert!(blocked(&|s| s.minimap.as_mut().unwrap().heroes = vec![track(true, 0.52, 0.7), track(true, 0.48, 0.71), track(true, 0.5, 0.78)]), "with three teammates");
    assert!(blocked(&|s| s.minimap.as_mut().unwrap().you = Some(Pt { x: 0.5, y: 0.4 })), "in their half");
    assert!(blocked(&|s| s.objective_banner = Some(ObjectiveBanner { name: "ARCHMOTHER".into(), own: Some(false), health_ratio: None })), "sieging");
}
#[test]
fn the_deep_wave_calls_are_low_priority() {
    let mut s = alive(1000); let m = s.minimap.as_mut().unwrap();
    m.green.push = Some(0.2); m.green.enemy_troopers = 3; m.green.enemy_progress = Some(0.22); m.your_lane = Some(Lane::Green);
    let c = coach_engine::map_rules::MapRule.evaluate(&s);
    assert!(c.iter().any(|c| c.key == "map:wave:Green" && c.priority == Priority::Low), "{c:?}");
    let d = LaneCoach::default().decide(&s).unwrap();
    assert_eq!(d.cue_key, "map:wave:Green"); assert_eq!(d.priority, Priority::Low);
}
#[test]
fn a_timer_readout_waits_while_an_objective_plan_covers_it() {
    let timer = || coach_engine::TimerRule(coach_engine::TimerDef { name: "Soul Urn".into(), first_spawn_secs: 1200, interval_secs: 0, warn_ahead_secs: 60 });
    let mut e = Engine::new(vec![Offer::new(vec![cue("plan:urn:1200", "Urn in 40: shove Green, then we take it.", Priority::Normal)]), Box::new(timer())]);
    let mut keys = vec![];
    for t in (1150..1200).step_by(5) { keys.extend(spoken(&mut e, &alive(t))); }
    assert!(keys.iter().any(|k| k.starts_with("plan:")), "{keys:?}");
    assert!(!keys.iter().any(|k| k.starts_with("timer:")), "{keys:?}");
    // Without a plan the timer is still said, at Low priority.
    assert_eq!(timer().evaluate(&alive(1160))[0].priority, Priority::Low);
    let mut e = Engine::new(vec![Box::new(timer())]);
    assert_eq!(spoken(&mut e, &alive(1160)), ["timer:Soul Urn:1200"]);
}
#[test]
fn last_hit_advice_ends_at_eight_minutes() {
    // Health unread on a crashed wave: the lane plan falls back to last hits.
    let lane = |t: u32| {
        let mut s = alive(t); s.hp = None; let m = s.minimap.as_mut().unwrap();
        m.your_lane = Some(Lane::Blue); m.your_progress = Some(0.45);
        m.blue.push = Some(0.8); m.blue.ally_troopers = 4; m.blue.ally_progress = Some(0.8);
        LaneCoach::default().decide(&s).unwrap()
    };
    let d = lane(470); assert_eq!(d.cue_key, "lane:last_hit"); assert!(d.speak);
    let d = lane(500); assert_eq!(d.cue_key, "lane:last_hit"); assert!(!d.speak, "9:35-14:01 in rec13 was not laning");
}

// ---- 6. Praise timing ------------------------------------------------------------------------

#[test]
fn no_praise_during_a_death_or_within_a_minute_of_the_respawn() {
    let mut e = Engine::new(vec![stream("praise:streak", Priority::Normal)]);
    let mut praised = vec![];
    for t in (1000..1200).step_by(4) {
        let s = if (1040..1080).contains(&t) { dead(t, Some(1080 - t)) } else { alive(t) };
        if !e.step(&s).is_empty() { praised.push(t); }
    }
    assert!(praised.iter().any(|t| *t < 1040), "{praised:?}");
    assert!(!praised.iter().any(|t| (1040..1140).contains(t)), "dead from 1040, back at 1080: nothing until 1140: {praised:?}");
    assert!(praised.iter().any(|t| *t >= 1140), "{praised:?}");
}
#[test]
fn a_walk_out_praise_never_follows_a_walked_out_alone_warning() {
    let mut e = Engine::new(vec![Offer::new(vec![cue("review:respawn-alone", "3 missing and you're alone. Walk out with a teammate.", Priority::Normal)]),
        Offer::new(vec![cue("praise:group:1", "You walked out with a teammate.", Priority::Normal)])]);
    assert_eq!(spoken(&mut e, &alive(2000)), ["review:respawn-alone"]);
    let mut e = Engine::new(vec![Script(Box::new(|t| if t == 2000 { vec![cue("review:respawn-alone", "x", Priority::Normal)] } else { vec![cue("praise:group:1", "You walked out with a teammate.", Priority::Normal)] })).into_box()]);
    e.step(&alive(2000));
    let later: Vec<u32> = (2045..2100).step_by(5).filter(|t| !e.step(&alive(*t)).is_empty()).collect();
    assert!(later.iter().all(|t| *t >= 2060), "{later:?}");
}
trait IntoBox { fn into_box(self) -> Box<dyn Rule>; }
impl IntoBox for Script { fn into_box(self) -> Box<dyn Rule> { Box::new(self) } }
