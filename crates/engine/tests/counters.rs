use coach_core::GameSnapshot;
use coach_engine::{Engine,counter_rule_from_json};
fn at(t:u32)->GameSnapshot{GameSnapshot{game_clock_secs:Some(t),..Default::default()}}
#[test]fn item_category_and_shared_item_limits_prevent_rotating_enemy_spam(){
    let data=r#"[{"when_enemy_has":"A","suggest":"B","suggest_class":"item_b","reason":"reason"},{"when_enemy_has":"C","suggest":"B","suggest_class":"item_b","reason":"reason"},{"when_enemy_has":"D","suggest":"E","suggest_class":"item_e","reason":"reason"}]"#;
    let mut e=Engine::new(vec![counter_rule_from_json(data).unwrap()]);let mut s=at(200);s.enemy_heroes=vec!["A".into()];assert_eq!(e.step(&s).len(),1);
    s.game_clock_secs=Some(210);s.enemy_heroes=vec!["D".into()];assert!(e.step(&s).is_empty());
    s.game_clock_secs=Some(199);assert!(e.step(&s).is_empty(),"a small OCR clock backstep must not clear counter cooldowns");
    s.game_clock_secs=Some(220);assert!(e.step(&s).is_empty());
    s.game_clock_secs=Some(380);s.enemy_heroes=vec!["C".into()];assert!(e.step(&s).is_empty(),"same item for another opponent is still suppressed");
    s.enemy_heroes=vec!["D".into()];assert_eq!(e.step(&s).len(),1);
}
#[test]fn rem_suggestion_explains_one_verified_item_and_respects_budget_and_danger(){
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let mut s=at(400);s.screen=Some("Gameplay".into());s.souls=Some(3500);s.enemy_heroes=vec!["hero_familiar".into()];
    let cues=rule.evaluate(&s);assert_eq!(cues.len(),1);assert!(cues[0].text.contains("Healing Nova"));assert!(cues[0].text.contains("nearby allies"));assert!(!cues[0].text.contains("Arcane Surge"));
    s.souls=Some(1600);let cue=&rule.evaluate(&s)[0];assert!(cue.text.contains("Guardian Ward"));assert!(cue.text.contains("barrier"));
    s.hp=Some(200);s.max_hp=Some(1000);assert!(rule.evaluate(&s).is_empty());
    s.hp=Some(900);s.items=vec!["upgrade_guardian_ward".into(),"upgrade_arcane_surge".into()];assert!(rule.evaluate(&s).is_empty());
    let values:serde_json::Value=serde_json::from_str(include_str!("../../../data/coaching/item-rationales.json")).unwrap();
    for r in values.as_array().unwrap(){assert!(r["source"].as_str().unwrap().starts_with("https://deadlock.wiki/"));assert!(!r["quote"].as_str().unwrap().is_empty());}
}
#[test]fn counter_tip_says_what_the_item_does_costs_and_why(){
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let mut s=at(400);s.screen=Some("Dead".into());s.souls=Some(2000);s.enemy_heroes=vec!["hero_viscous".into()];
    let c=rule.evaluate(&s);let t=&c.iter().find(|c|c.key.contains("long_range")).expect("Long Range vs Viscous").text;
    assert!(t.contains("Long Range for 1600 souls"),"{t}");
    assert!(t.contains("beyond a minimum distance"),"game tooltip: {t}");
    assert!(t.contains("percentage points more often against Viscous"),"{t}");
    // A weapon item is not suggested to a spirit build.
    s.items=vec!["upgrade_improved_spirit".into()];
    assert!(!rule.evaluate(&s).iter().any(|c|c.key.contains("long_range")));
}
#[test]fn counter_tips_wait_for_a_shop_window(){
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let mut s=at(400);s.screen=Some("Gameplay".into());s.souls=Some(3500);s.enemy_heroes=vec!["hero_familiar".into()];
    s.minimap=Some(coach_core::MinimapState{you:Some(coach_core::Pt{x:0.2,y:0.4}),..Default::default()});
    assert!(rule.evaluate(&s).is_empty(),"in lane");
    s.minimap.as_mut().unwrap().you=Some(coach_core::Pt{x:0.5,y:0.9});assert_eq!(rule.evaluate(&s).len(),1,"at base");
}