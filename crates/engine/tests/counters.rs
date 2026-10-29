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
fn dead_with(t:u32,rows:&[(&str,u32)],souls:u32)->GameSnapshot{
    let mut s=at(t);s.screen=Some("Dead".into());s.dead=true;s.souls=Some(souls);s.hero=Some("hero_orion".into());
    s.enemy_heroes=["hero_synth","hero_yamato","hero_haze","hero_kelvin","hero_deadpack","hero_magician"].map(String::from).to_vec();
    s.death_damage=rows.iter().map(|(src,d)|coach_core::DamageRow{source:src.to_string(),hits:None,damage:*d}).collect();s
}
#[test]fn a_ranged_hero_is_never_offered_a_melee_item(){
    // rec8 34:54: Lifestrike against Yamato for Grey Talon.
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let mut s=at(2000);s.screen=Some("Dead".into());s.souls=Some(5000);s.enemy_heroes=vec!["hero_yamato".into()];
    s.items=vec!["upgrade_tech_damage_pulse".into(),"upgrade_restorative_locket".into()];
    s.hero=Some("hero_haze".into());
    assert!(rule.evaluate(&s).iter().any(|c|c.key.contains("boxing_glove")),"a melee-capable hero may get it");
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    s.hero=Some("hero_orion".into());
    assert!(!rule.evaluate(&s).iter().any(|c|c.key.contains("boxing_glove")),"{:?}",rule.evaluate(&s));
}
#[test]fn the_damage_that_killed_you_picks_the_counter(){
    // rec8 46:51: Pocket's Affliction topped the panel; 50:14: Haze's Bullet Dance.
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let c=rule.evaluate(&dead_with(2810,&[("Affliction",2172),("Spirit Armor",1104),("Frost Grenade",1017)],3580));
    assert_eq!(c[0].key,"counter:hero_synth:upgrade_spirit_bubble","{c:?}");
    assert!(c[0].text.starts_with("Pocket's Affliction did 2,172 damage last death. Get Spirit Shielding for 1,600 souls"),"{}",c[0].text);
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let c=rule.evaluate(&dead_with(2990,&[("Bullet Dance",2017),("Fixation",932),("Capacitor",398)],4474));
    assert_eq!(c[0].key,"counter:hero_haze:upgrade_weapon_shielding","{c:?}");
    // Once panels name who hurts you, a hero who did not is not countered (rec8: Metal Skin vs Deadman Danny).
    assert!(!c.iter().any(|c|c.key.starts_with("counter:hero_deadpack")),"{c:?}");
}
#[test]fn the_same_item_is_suggested_once_per_match(){
    let mut e=Engine::new(vec![counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap()]);
    let mut s=at(1000);s.screen=Some("Dead".into());s.souls=Some(2000);s.enemy_heroes=vec!["hero_viscous".into()];
    assert!(e.step(&s).iter().any(|c|c.key.contains("long_range")));
    s.game_clock_secs=Some(2900);
    assert!(!e.step(&s).iter().any(|c|c.key.contains("long_range")),"rec8 said Long Range vs Pocket at 17:18 and 48:20");
}
#[test]fn counter_tips_wait_for_a_shop_window(){
    let rule=counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap();
    let mut s=at(400);s.screen=Some("Gameplay".into());s.souls=Some(3500);s.enemy_heroes=vec!["hero_familiar".into()];
    s.minimap=Some(coach_core::MinimapState{you:Some(coach_core::Pt{x:0.2,y:0.4}),..Default::default()});
    assert!(rule.evaluate(&s).is_empty(),"in lane");
    s.minimap.as_mut().unwrap().you=Some(coach_core::Pt{x:0.5,y:0.9});assert_eq!(rule.evaluate(&s).len(),1,"at base");
}