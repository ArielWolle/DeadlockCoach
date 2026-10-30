//! Every rule cue carries the skill it coaches (data/coaching/skills.json, docs/player-profile.md
//! "Skills and mastery"): the browser stays quiet about mastered skills by this tag.
use coach_core::{DamageRow, GameSnapshot, MinimapState, Pt};
use coach_engine::{review::ReviewCoach, skills, Cue, Engine, Priority, Rule, RulesConfig, UnspentSoulsRule};

/// Cue keys that may have no skill: a play-pattern summary of several habits (the "went well"
/// summary) and a death review without any readable cause.
fn skill_free(c: &Cue) -> bool { c.key.starts_with("review:pattern") || c.key.starts_with("death:review") }

fn all_rules() -> Vec<Box<dyn Rule>> {
    let mut rules = coach_engine::rules_from_timings(include_str!("../../../data/timings.json")).unwrap();
    rules.extend(coach_engine::default_rules(RulesConfig::default()));
    rules.push(coach_engine::counter_rule_from_json(include_str!("../../../data/counters.json")).unwrap());
    rules
}

#[test]
fn every_card_has_a_skill() {
    // Each card of the card-driven coaches maps to a skill by its card id or by the key family the
    // module gives it (laning: lane:<id>, catch -> map:wave:*, jungle -> map:farm-window).
    let files: [(&str, &str, fn(&str) -> String); 6] = [
        ("active", include_str!("../../../data/coaching/active.json"), |id| format!("active:{id}")),
        ("laning", include_str!("../../../data/coaching/laning.json"), |id| match id { "catch" => "map:wave:Green".into(), "jungle" => "map:farm-window".into(), _ => format!("lane:{id}") }),
        ("positioning", include_str!("../../../data/coaching/positioning.json"), |id| format!("pos:{id}")),
        ("jungle", include_str!("../../../data/coaching/jungle.json"), |id| format!("jungle:{id}")),
        ("praise", include_str!("../../../data/coaching/praise.json"), |id| format!("praise:{id}")),
        ("plans", include_str!("../../../data/coaching/plans.json"), |id| match id { i if i.starts_with("urn_") => format!("plan:{i}:400"), "lead_falling" | "lead_midboss" => "lead:x".into(), "lead_fed" | "fed" => "lead:fed:hero_haze".into(), "reset" => "reset:fight".into(), "threat" => "threat:hero_haze".into(), i => format!("plan:{i}") }),
    ];
    let mut missing = vec![];
    for (module, json, key) in files {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        for c in v["cards"].as_array().unwrap() {
            let id = c["id"].as_str().unwrap();
            // Active cards say their own keys; the card id decides first.
            let k = match (module, id) {
                ("active", "base_spend") => "unspent:base".to_string(), ("active", "heal_first") => "active:heal".into(), ("active", "window") => "pos:enemies_down".into(),
                ("active", "cc_stunned" | "cc_silenced" | "cc_zipline") => "active:cc".into(), ("active", "aura" | "stalkers_mark") => format!("active:debuff:{id}"),
                ("active", "repeat_killer") => "active:repeat:hero_haze".into(), ("active", "close_deaths") => "active:close".into(), ("active", "fight_missed") => "active:fight".into(),
                ("active", "hold_base" | "wait_respawn") => "active:hold_base".into(), ("active", "base_banner") => "active:base_attack".into(),
                _ => key(id),
            };
            if skills::of(Some(&format!("{module}.{id}")), &k).is_none() { missing.push(format!("{module}.{id} ({k})")); }
        }
    }
    // Review cards that are a death cause (not text fragments of the review line).
    let v: serde_json::Value = serde_json::from_str(include_str!("../../../data/coaching/review.json")).unwrap();
    let fragments = ["review", "spend", "buy_item", "no_frames", "no_reason", "pattern_good", "pattern_balance", "live_fight_lost", "live_respawn_alone", "cc_zipline", "cc_stunned", "cc_silenced", "cc_disarmed", "cc_out_of_stamina", "counter_item", "involved"];
    for c in v["cards"].as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        if fragments.contains(&id) { continue; }
        if skills::for_card(&format!("review.{id}")).is_none() { missing.push(format!("review.{id}")); }
    }
    assert!(missing.is_empty(), "cards without a skill: {missing:?}");
    // Non-card families.
    for k in ["unspent-dead", "unspent", "lead", "behind", "timer:Soul Urn:600", "counter:hero_haze:upgrade_metal_skin", "pace:behind", "wave:overview",
        "map:urn-ground", "map:midboss", "death:heroes", "death:trooper", "review:fight-lost", "review:respawn-alone"] {
        assert!(skills::for_key(k).is_some(), "{k}");
    }
}

fn dead(t: u32, souls: u32) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Dead".into()), dead: true, respawn_secs: Some(30), souls: Some(souls),
        death_cause: Some("Melee".into()), death_damage: vec![DamageRow { source: "Melee".into(), hits: None, damage: 2322 }], ..Default::default() }
}
fn alive(t: u32, souls: u32, y: f32) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Gameplay".into()), souls: Some(souls), hp: Some(2800), max_hp: Some(3200), health_observations: 3,
        minimap: Some(MinimapState { observations: 3, you: Some(Pt { x: 0.5, y }), ..Default::default() }), ..Default::default() }
}

#[test]
fn shop_reminders_carry_their_skill_and_serialize_it() {
    let r = UnspentSoulsRule::new(RulesConfig::default());
    let mut cue = None;
    for t in 1200..1215 { if let Some(c) = r.evaluate(&dead(t, 6000)).into_iter().next() { cue = Some(c); break; } }
    let c = cue.expect("a death-screen reminder");
    assert_eq!((c.key.as_str(), c.skill.as_deref()), ("unspent-dead", Some("shop_on_death")));
    let json = serde_json::to_value(&c).unwrap();
    assert_eq!(json["skill"], "shop_on_death");
    // Older payloads without the field still parse.
    let old: Cue = serde_json::from_str(r#"{"key":"lead","text":"x","priority":"Normal","cooldown_secs":1}"#).unwrap();
    assert_eq!(old.skill, None);
    assert_eq!(Cue::new("lead", "x", Priority::Normal, 1).skill.as_deref(), Some("throw_awareness"));
}

#[test]
fn a_death_review_names_the_skill_of_its_cause() {
    let mut e = Engine::new(vec![Box::new(ReviewCoach::default())]);
    let mut review = None;
    for t in (1000..1060).step_by(3) { let mut s = alive(t, 500, 0.4); s.hp = Some(3000); e.step(&s); }
    for t in (1060..1090).step_by(2) { for c in e.step(&dead(t, 500)) { if c.key.starts_with("death:review") { review = Some(c); } } }
    let c = review.expect("a death review");
    assert!(c.skill.is_some(), "{c:?}");
    assert!(skills::catalog().skills.iter().any(|s| Some(&s.id) == c.skill.as_ref()));
}

/// The recorded sample sessions (and the live recordings when present, SKILL_REPLAY=path;path):
/// every cue any rule offers has a skill, except the multi-habit summaries.
#[test]
fn recorded_sessions_give_every_cue_a_skill() {
    let mut files = vec![concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/session2.jsonl").to_string(), concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/session2-minimap.jsonl").to_string()];
    if let Ok(extra) = std::env::var("SKILL_REPLAY") { files.extend(extra.split(';').map(str::to_string)); }
    let (mut n, mut free) = (0, 0);
    let mut missing: Vec<String> = vec![];
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { println!("skip {f}"); continue };
        let rules = all_rules();
        let mut e = Engine::new(all_rules());
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let v = if v.get("snapshot").is_some() { v["snapshot"].clone() } else { v };
            let Ok(s) = serde_json::from_value::<GameSnapshot>(v) else { continue };
            let offered: Vec<Cue> = rules.iter().flat_map(|r| r.evaluate(&s)).chain(e.step(&s)).collect();
            for c in offered {
                n += 1;
                if c.skill.is_none() { if skill_free(&c) { free += 1; } else if !missing.contains(&c.key) { missing.push(c.key.clone()); } }
            }
        }
    }
    println!("{n} cues, {free} skill-free summaries");
    assert!(missing.is_empty(), "cues without a skill: {missing:?}");
}
