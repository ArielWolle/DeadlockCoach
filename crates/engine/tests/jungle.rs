use coach_core::{GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, Pt};
use coach_engine::{jungle::{Catalog, JungleCoach}, laning::LaneCoach, Engine, RulesConfig};

#[test]
fn jungle_cards_quote_real_document_guidance() {
    let catalog: Catalog = serde_json::from_str(include_str!("../../../data/coaching/jungle.json")).unwrap();
    assert_eq!(catalog.schema_version, 1);
    let spawns: Vec<(String, u32)> = catalog.spawns.iter().map(|s| (s.id.clone(), s.secs)).collect();
    assert_eq!(spawns, vec![("small".into(), 120), ("medium".into(), 300), ("large".into(), 480)], "wiki: 2:00 / 5:00 / 8:00");
    for c in catalog.cards {
        assert!(!c.sources.is_empty() && c.cooldown_secs >= 20, "{}", c.id);
        for s in c.sources {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&s.path)).unwrap();
            assert!(text.contains(&s.excerpt), "{}: missing excerpt {}", c.id, s.excerpt);
            assert!(text.contains(&s.section), "{}: missing section {}", c.id, s.section);
        }
    }
}

fn at(clock: u32) -> GameSnapshot { GameSnapshot { game_clock_secs: Some(clock), screen: Some("Gameplay".into()), ..Default::default() } }

#[test]
fn camp_tiers_are_announced_shortly_before_their_first_spawn() {
    let j = JungleCoach::default();
    assert!(j.assess(&at(95)).is_none());
    let a = j.assess(&at(105)).unwrap(); assert_eq!(a.id, "spawn_small"); assert!(a.text.starts_with("Small camps spawn in 15 seconds"), "{}", a.text);
    assert_eq!(j.assess(&at(285)).unwrap().id, "spawn_medium");
    let l = j.assess(&at(470)).unwrap(); assert_eq!(l.id, "spawn_large"); assert!(l.text.contains("Sinner's Sacrifice"));
    assert!(j.assess(&at(300)).is_none() && j.assess(&at(500)).is_none(), "no warning once spawned");
    let mut dead = at(105); dead.dead = true; assert!(j.assess(&dead).is_none());
}

/// You at the front of a crashed Green wave, healthy, nobody near you, 12 top-bar slots.
fn pushed(enemies: [&str; 6]) -> GameSnapshot {
    let mut map = MinimapState { observations: 3, you: Some(Pt { x: 0.8, y: 0.35 }), your_lane: Some(Lane::Green), your_progress: Some(0.75), ..Default::default() };
    map.green.push = Some(0.8); map.green.ally_troopers = 4; map.green.ally_progress = Some(0.8);
    for l in [&mut map.blue, &mut map.yellow] { l.push = Some(0.5); }
    let mut s = GameSnapshot { minimap: Some(map), hp: Some(900), max_hp: Some(1000), health_observations: 3, ..at(400) };
    s.hero_status = (0..12).map(|i| {
        let e = if i < 6 { "seen" } else { enemies[i - 6] };
        HeroStatus { ally: i < 6, slot: i % 6, dead: Some(e == "dead"), respawn_secs: (e == "dead").then_some(20),
            bar_visible: Some(e == "seen"), health_ratio: (e == "seen").then_some(1.0), ..Default::default() }
    }).collect();
    s
}

#[test]
fn invade_only_when_the_opposing_heroes_are_accounted_for() {
    let j = JungleCoach::default();
    // Lane opponent dead, everyone else visible: steal their camp.
    let a = j.assess(&pushed(["dead", "seen", "seen", "seen", "seen", "seen"])).unwrap();
    assert_eq!(a.id, "invade"); assert!(a.text.contains("one enemy is dead") && a.text.contains("leave one Haunt alive"), "{}", a.text);
    // One unseen enemy is tolerated.
    assert_eq!(j.assess(&pushed(["unseen", "seen", "seen", "seen", "seen", "seen"])).unwrap().id, "invade");
    // Missing enemies may be in that jungle: farm your own side instead.
    let u = j.assess(&pushed(["unseen", "unseen", "seen", "seen", "seen", "seen"])).unwrap();
    assert_eq!(u.id, "invade_unsafe"); assert!(u.text.contains("2 enemies are unaccounted for") && u.text.contains("own side"), "{}", u.text);
}

#[test]
fn a_lane_opponent_who_just_left_the_minimap_counts_as_unaccounted() {
    let mut s = pushed(["seen", "seen", "seen", "seen", "seen", "unseen"]);
    let mut gone = HeroTrack { id: 1, ally: false, pos: Pt { x: 0.82, y: 0.3 }, hits: 4, unseen_ms: 6000, ..Default::default() };
    s.minimap.as_mut().unwrap().heroes.push(gone.clone());
    // One enemy without a bar and one fresh missing marker may be the same hero: the larger count (1) is used.
    assert_eq!(JungleCoach::default().assess(&s).unwrap().id, "invade");
    gone.id = 2; s.minimap.as_mut().unwrap().heroes.push(gone);
    assert_eq!(JungleCoach::default().assess(&s).unwrap().id, "invade_unsafe");
}

#[test]
fn no_invade_without_a_crashed_wave_health_or_top_bar_evidence() {
    let j = JungleCoach::default();
    let ok = ["dead", "seen", "seen", "seen", "seen", "seen"];
    let mut s = pushed(ok); s.minimap.as_mut().unwrap().green.push = Some(0.5); assert!(j.invade(&s).is_none(), "wave not on their guardian");
    let mut s = pushed(ok); s.minimap.as_mut().unwrap().your_progress = Some(0.3); assert!(j.invade(&s).is_none(), "you are not up there");
    let mut s = pushed(ok); s.hp = Some(400); assert!(j.invade(&s).is_none(), "not healthy");
    let mut s = pushed(ok); s.hero_status.clear(); assert!(j.invade(&s).is_none(), "unknown enemy whereabouts");
    let mut s = pushed(ok); s.minimap.as_mut().unwrap().heroes.push(HeroTrack { id: 9, ally: false, pos: Pt { x: 0.82, y: 0.33 }, hits: 3, ..Default::default() });
    assert!(j.invade(&s).is_none(), "an enemy is right there");
}

#[test]
fn the_specific_jungle_call_replaces_the_generic_camp_plan() {
    let s = pushed(["dead", "seen", "seen", "seen", "seen", "seen"]);
    let d = LaneCoach::default().decide(&s).unwrap(); assert!(!d.speak, "{:?} {}", d.action, d.text);
    let mut e = Engine::new(coach_engine::default_rules(RulesConfig::default())); e.max_per_step = 5;
    let spoken = e.step(&s);
    assert!(spoken.iter().any(|c| c.key == "jungle:invade"), "{spoken:?}");
    assert!(!spoken.iter().any(|c| c.key == "map:farm-window" || c.key == "lane:wave_good"), "{spoken:?}");
}
