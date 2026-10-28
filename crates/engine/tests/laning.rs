use coach_core::{GameSnapshot,MinimapState,Lane,Pt,UrnState};
use coach_engine::{laning::{Action,Catalog,LaneCoach},Engine,Rule,RulesConfig};

fn state() -> GameSnapshot {
    let mut map=MinimapState {observations:3,you:Some(Pt{x:0.5,y:0.55}),your_lane:Some(Lane::Blue),your_progress:Some(0.45),..Default::default()};
    for l in [&mut map.green,&mut map.blue,&mut map.yellow] {l.push=Some(0.55);l.ally_troopers=4;l.enemy_troopers=2;l.ally_progress=Some(0.6);l.enemy_progress=Some(0.5);}
    GameSnapshot {game_clock_secs:Some(400),screen:Some("Gameplay".into()),hp:Some(800),max_hp:Some(1000),health_observations:3,minimap:Some(map),..Default::default()}
}
fn action(s:&GameSnapshot) -> Action {LaneCoach::default().decide(s).unwrap().action}
fn crashed(s:&mut GameSnapshot) {let l=&mut s.minimap.as_mut().unwrap().blue;l.push=Some(0.8);l.ally_progress=Some(0.8);l.enemy_troopers=0;}

#[test]
fn policy_cards_quote_real_document_guidance_without_unverified_claims() {
    let catalog:Catalog=serde_json::from_str(include_str!("../../../data/coaching/laning.json")).unwrap();
    assert_eq!(catalog.schema_version,1);assert_eq!(catalog.cards.len(),14);
    for c in catalog.cards {
        assert!(!c.sources.is_empty());assert!(c.cooldown_secs>=20);
        for source in c.sources {
            let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&source.path);
            let text=std::fs::read_to_string(path).unwrap();
            assert!(text.contains(&source.excerpt),"{}: missing excerpt {}",c.id,source.excerpt);
            assert!(text.contains(&source.section),"{}: missing section",c.id);
            assert!(!source.excerpt.contains("UNVERIFIED"));
        }
    }
}

#[test]
fn endangered_personal_wave_is_prioritized_over_camps() {
    let mut s=state();let l=&mut s.minimap.as_mut().unwrap().blue;l.push=Some(0.2);l.enemy_troopers=3;l.enemy_progress=Some(0.22);
    assert_eq!(action(&s),Action::CatchWave);
    assert!(LaneCoach::default().decide(&s).unwrap().text.contains("Blue lane"));
}
#[test]
fn crashed_wave_creates_a_conditional_one_camp_window() {
    let mut s=state();crashed(&mut s);
    let d=LaneCoach::default().decide(&s).unwrap();assert_eq!(d.action,Action::Jungle);assert!(d.conditional);
    assert!(d.text.contains("if it is up"));assert!(d.missing.iter().any(|v|v.contains("Camp availability")));
    s.game_clock_secs=Some(100);assert_ne!(action(&s),Action::Jungle);
    s.minimap.as_mut().unwrap().your_lane=None;assert_ne!(action(&s),Action::Jungle);
}
#[test]
fn poke_requires_health_wave_cover_and_a_visible_nearby_opponent() {
    let mut s=state();s.minimap.as_mut().unwrap().enemies.push(Pt{x:0.54,y:0.54});
    assert_eq!(action(&s),Action::Poke);
    s.hp=None;assert_ne!(action(&s),Action::Poke);
    s.hp=Some(800);s.health_observations=1;assert_ne!(action(&s),Action::Poke);
    s.health_observations=3;s.minimap.as_mut().unwrap().enemies.clear();assert_ne!(action(&s),Action::Poke);
}
#[test]
fn push_when_wave_needs_clearing_and_player_is_behind_it() {assert_eq!(action(&state()),Action::PushWave);}
#[test]
fn a_visible_local_numbers_disadvantage_means_hold() {
    let mut s=state();s.minimap.as_mut().unwrap().enemies=vec![Pt{x:0.51,y:0.54},Pt{x:0.55,y:0.57}];
    assert_eq!(action(&s),Action::HoldWave);
}
#[test]
fn being_ahead_of_the_wave_is_not_misreported_as_low_health() {
    let mut s=state();let m=s.minimap.as_mut().unwrap();m.your_progress=Some(0.75);m.blue.ally_progress=Some(0.45);m.enemies.push(Pt{x:0.51,y:0.55});
    let d=LaneCoach::default().decide(&s).unwrap();assert_eq!(d.action,Action::BackOff);assert!(d.text.contains("ahead of your trooper"));
}
#[test]
fn low_or_falling_health_overrides_farming_and_poking() {
    let mut s=state();crashed(&mut s);s.hp=Some(200);assert_eq!(action(&s),Action::BackOff);
    s.hp=Some(800);s.health_delta_ratio=Some(-0.2);assert_eq!(action(&s),Action::BackOff);
}
#[test]
fn no_health_or_player_marker_does_not_become_a_positive_trade_or_camp_call() {
    let mut s=state();s.hp=None;crashed(&mut s);assert_eq!(action(&s),Action::LastHit);
    s.minimap.as_mut().unwrap().you=None;s.minimap.as_mut().unwrap().your_lane=None;
    assert_eq!(action(&s),Action::NeedEvidence);
    s.coaching_context.focus_lane=Some(Lane::Blue);assert_eq!(action(&s),Action::LastHit);
}
#[test]
fn objectives_and_shopping_take_precedence_over_a_jungle_detour() {
    let mut s=state();crashed(&mut s);s.souls=Some(2000);assert_eq!(action(&s),Action::Shop);
    s.souls=None;s.game_clock_secs=Some(590);assert_eq!(action(&s),Action::PrepareObjective);
    s.minimap.as_mut().unwrap().blue.push=Some(0.55);s.minimap.as_mut().unwrap().blue.enemy_troopers=2;
    assert_eq!(action(&s),Action::PrepareObjective);
}
#[test]
fn visible_grouping_near_midboss_blocks_an_extra_camp() {
    let mut s=state();crashed(&mut s);s.game_clock_secs=Some(700);
    let m=s.minimap.as_mut().unwrap();m.midboss_up=true;m.allies=vec![Pt{x:0.48,y:0.5},Pt{x:0.52,y:0.5},Pt{x:0.45,y:0.48}];
    assert_eq!(action(&s),Action::PrepareObjective);
}
#[test]
fn dead_menu_and_courier_states_do_not_get_lane_farming_orders() {
    let mut s=state();crashed(&mut s);s.dead=true;assert!(LaneCoach::default().decide(&s).is_none());
    s.dead=false;s.screen=Some("Shop".into());assert!(LaneCoach::default().decide(&s).is_none());
    s.screen=Some("Gameplay".into());s.minimap.as_mut().unwrap().urn=Some(UrnState::CarriedByYou);assert!(LaneCoach::default().decide(&s).is_none());
}
#[test]
fn low_health_can_interrupt_the_normal_gap_but_repeated_advice_is_deduplicated() {
    let mut e=Engine::new(vec![Box::new(LaneCoach::default())]);let mut s=state();
    // 8 s apart: a slow fall, not a burst (a burst is the critical call, below).
    assert!(!e.step(&s).is_empty());s.game_clock_secs=Some(408);s.hp=Some(200);
    assert!(e.step(&s).iter().any(|c|c.key=="lane:back_off"));s.game_clock_secs=Some(412);assert!(e.step(&s).is_empty());
}
#[test]fn health_warnings_are_said_once_per_low_episode_and_a_fast_fall_is_critical(){
    // rec8 44:02-44:50: "Health critical" three times at the same low.
    let mut e=Engine::new(vec![Box::new(LaneCoach::default())]);e.min_gap_secs=0;let mut s=state();
    let mut at=|t:u32,hp:u32,s:&mut GameSnapshot|{s.game_clock_secs=Some(t);s.hp=Some(hp);e.step(s).into_iter().map(|c|c.key).collect::<Vec<_>>()};
    at(400,800,&mut s);
    // 80% -> 45% in 3 s: 35% of max health lost, critical before the health itself is.
    assert_eq!(at(403,450,&mut s),["lane:critical"]);
    let mut later=vec![];for t in (406..460).step_by(4){later.extend(at(t,100+(t-406),&mut s));}
    assert!(!later.iter().any(|k|k=="lane:critical"||k=="lane:back_off"),"{later:?}");
    // Healed to 70%: a new episode can warn again.
    at(470,700,&mut s);at(480,700,&mut s);
    assert_eq!(at(490,100,&mut s),["lane:critical"]);
}
#[test]fn an_implausible_health_read_is_not_critical(){
    let mut s=state();s.hero=Some("hero_orion".into());
    s.hero_status=vec![coach_core::HeroStatus{ally:true,hero:Some("hero_orion".into()),health_ratio:Some(0.85),health_observations:5,..Default::default()}];
    // rec6 30:42: 14 / 2488 HP read while the own top-bar bar showed 85%.
    s.hp=Some(14);assert_ne!(LaneCoach::default().decide(&s).unwrap().cue_key,"lane:critical");
    s.hp=Some(0);s.hero_status.clear();assert_ne!(LaneCoach::default().decide(&s).unwrap().cue_key,"lane:critical","0 HP while alive");
    s.hp=Some(60);assert_eq!(LaneCoach::default().decide(&s).unwrap().cue_key,"lane:critical");
}
#[test]
fn shopping_reminders_do_not_send_the_player_away_from_an_uncleared_lane() {
    let mut s=state();s.souls=Some(3000);
    assert!(coach_engine::UnspentSoulsRule::new(RulesConfig::default()).evaluate(&s).is_empty());
}
#[test]
fn unlock_timers_are_one_shot_and_camps_are_not_a_fixed_respawn_clock() {
    let mut e=Engine::new(coach_engine::rules_from_timings(include_str!("../../../data/timings.json")).unwrap());
    assert!(e.step(&GameSnapshot{game_clock_secs:Some(350),..Default::default()}).iter().any(|c|c.text.contains("Zipline boost unlock")));
    for t in [710,1070] {assert!(!e.step(&GameSnapshot{game_clock_secs:Some(t),..Default::default()}).iter().any(|c|c.text.contains("Zipline")));}
    assert!(!include_str!("../../../data/timings.json").contains("Hard camps"));
}
#[test]
fn visible_ui_values_do_not_authorize_coaching_when_game_state_is_unknown(){let mut s=state();s.screen=Some("Unknown".into());s.souls=Some(3000);let mut e=Engine::new(coach_engine::default_rules(RulesConfig::default()));assert!(e.step(&s).is_empty());}

#[test]fn gank_requires_matched_critical_target_and_confirmed_support(){
    let mut s=state();crashed(&mut s);
    s.hero_status=vec![coach_core::HeroStatus{ally:false,hero:Some("target".into()),critical:Some(true),observations:3,position:Some(Pt{x:0.7,y:0.5}),position_observations:3,lane:Some(Lane::Yellow),..Default::default()},
        coach_core::HeroStatus{ally:true,critical:Some(false),observations:3,position:Some(Pt{x:0.72,y:0.5}),position_observations:3,..Default::default()}];
    assert_eq!(action(&s),Action::Gank);
    s.hero_status[0].position=None;assert_ne!(action(&s),Action::Gank);
    s.hero_status[0].position=Some(Pt{x:0.7,y:0.5});s.hero_status[1].critical=None;assert_ne!(action(&s),Action::Gank);
    s.hero_status[1].critical=Some(true);assert_ne!(action(&s),Action::Gank);
}
#[test]fn health_warning_interrupts_immediately_and_starts_before_critical_hp(){
    let mut e=Engine::new(vec![Box::new(LaneCoach::default())]);let mut s=state();assert!(!e.step(&s).is_empty());
    s.game_clock_secs=Some(408);s.hp=Some(440);assert_eq!(e.step(&s)[0].key,"lane:back_off");
}
#[test]fn critical_health_is_called_on_one_reading_and_preempts_the_gap(){
    let mut e=Engine::new(vec![Box::new(LaneCoach::default())]);let mut s=state();assert!(!e.step(&s).is_empty());
    s.game_clock_secs=Some(401);s.hp=Some(60);s.health_observations=1;
    let c=e.step(&s);assert_eq!(c[0].key,"lane:critical");assert!(c[0].text.contains("critical"));
}
#[test]fn low_health_is_not_told_to_back_off_trades_while_three_enemies_are_dead(){
    let mut s=state();s.hp=Some(300);
    s.hero_status=(0..12).map(|i|coach_core::HeroStatus{ally:i<6,slot:i%6,dead:Some(!(i<6)&&i<9),..Default::default()}).collect();
    let d=LaneCoach::default().decide(&s).unwrap();assert_eq!(d.cue_key,"lane:back_off");assert!(!d.speak);
    // Unknown position is not safe: the warning stays.
    s.minimap.as_mut().unwrap().you=None;assert!(LaneCoach::default().decide(&s).unwrap().speak);
    // Critical health still preempts.
    s.hp=Some(100);assert_eq!(LaneCoach::default().decide(&s).unwrap().cue_key,"lane:critical");
}