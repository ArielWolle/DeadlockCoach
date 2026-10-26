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
