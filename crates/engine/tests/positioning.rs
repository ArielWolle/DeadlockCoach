use coach_core::{GameSnapshot, HeroTrack, Lane, MinimapState, Pt};
use coach_engine::{laning::{Action, LaneCoach}, positioning::{Catalog, PositionCoach}, Engine, Priority, RulesConfig};

const ME: Pt = Pt { x: 0.5, y: 0.6 };
fn hero(id: u32, ally: bool, x: f32, y: f32) -> HeroTrack { HeroTrack { id, ally, pos: Pt { x, y }, hits: 3, ..Default::default() } }
fn snap(heroes: Vec<HeroTrack>) -> GameSnapshot {
    let map = MinimapState { observations: 3, you: Some(ME), your_lane: Some(Lane::Blue), your_progress: Some(0.5), heroes, ..Default::default() };
    GameSnapshot { game_clock_secs: Some(500), screen: Some("Gameplay".into()), hp: Some(900), max_hp: Some(1000), health_observations: 3, minimap: Some(map), ..Default::default() }
}
fn advice(s: &GameSnapshot) -> Option<String> { PositionCoach::default().assess(s).map(|a| a.id) }

#[test]
fn policy_cards_quote_real_document_guidance() {
    let catalog: Catalog = serde_json::from_str(include_str!("../../../data/coaching/positioning.json")).unwrap();
    assert_eq!(catalog.schema_version, 1);
    for c in catalog.cards {
        assert!(!c.sources.is_empty() && c.cooldown_secs >= 20, "{}", c.id);
        for source in c.sources {
            let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&source.path)).unwrap();
            assert!(text.contains(&source.excerpt), "{}: missing excerpt {}", c.id, source.excerpt);
            assert!(text.contains(&source.section), "{}: missing section {}", c.id, source.section);
        }
    }
}

#[test]
fn enemies_closing_from_another_lane_trigger_a_gank_warning() {
    let mut e = hero(1, false, 0.30, 0.55); e.velocity = Some(Pt { x: 0.03, y: 0.0 }); e.lane = Some(Lane::Green);
    let a = PositionCoach::default().assess(&snap(vec![e.clone()])).unwrap();
    assert_eq!(a.id, "gank"); assert_eq!(a.priority, Priority::High);
    assert!(a.text.contains("1 enemy") && a.text.contains("from Green lane"), "{}", a.text);
    // Moving away, unconfirmed (one sighting) or not visible: no gank warning.
    let mut away = e.clone(); away.velocity = Some(Pt { x: -0.03, y: 0.0 }); assert_eq!(advice(&snap(vec![away])), None);
    let mut once = e.clone(); once.hits = 1; assert_eq!(advice(&snap(vec![once])), None);
    let mut hidden = e; hidden.unseen_ms = 2000; hidden.velocity = None; assert_eq!(advice(&snap(vec![hidden])), None);
}

#[test]
fn a_teammate_cancels_a_lone_ganker_only_when_they_are_with_you() {
    let mut e = hero(1, false, 0.30, 0.6); e.velocity = Some(Pt { x: 0.03, y: 0.0 });
    // 1 approaching vs you + 1 ally beside you: not outnumbered, no gank call.
    assert_eq!(advice(&snap(vec![e.clone(), hero(2, true, 0.52, 0.6)])), None);
    assert_eq!(advice(&snap(vec![e])).as_deref(), Some("gank"));
}

#[test]
fn outnumbered_and_numbers_advantage_around_you() {
    let two = vec![hero(1, false, 0.55, 0.6), hero(2, false, 0.5, 0.66)];
    let a = PositionCoach::default().assess(&snap(two.clone())).unwrap();
    assert_eq!(a.id, "outnumbered"); assert!(a.text.contains("2 enemies") && a.text.contains("just you"), "{}", a.text);
    let mut team = vec![hero(1, false, 0.56, 0.6), hero(3, true, 0.47, 0.6), hero(4, true, 0.5, 0.55)];
    let a = PositionCoach::default().assess(&snap(team.clone())).unwrap();
    assert_eq!(a.id, "go_in"); assert!(a.text.contains("3 of you") && a.text.contains("1 enemy"), "{}", a.text);
    // 2v1 counts as an advantage; 3v2 does not.
    assert_eq!(advice(&snap(vec![hero(1, false, 0.56, 0.6), hero(3, true, 0.47, 0.6)])).as_deref(), Some("go_in"));
    assert_eq!(advice(&snap(vec![hero(1, false, 0.56, 0.6), hero(2, false, 0.55, 0.65), hero(3, true, 0.47, 0.6), hero(4, true, 0.5, 0.55)])), None);
    // Not with unconfirmed health, nor while an enemy near you is missing.
    let mut low = snap(team.clone()); low.health_observations = 1; assert_eq!(advice(&low), None);
    let mut gone = hero(5, false, 0.6, 0.5); gone.unseen_ms = 6000; team.push(gone);
    assert_eq!(advice(&snap(team)), None, "a missing enemy nearby blocks the commit call");
}

#[test]
fn enemies_last_seen_near_you_going_missing_is_called_out() {
    let mut gone = hero(1, false, 0.6, 0.5); gone.unseen_ms = 7000;
    let a = PositionCoach::default().assess(&snap(vec![gone.clone()])).unwrap();
    assert_eq!(a.id, "missing"); assert!(a.text.contains("1 enemy") && a.text.contains("7 seconds"), "{}", a.text);
    // Not when you are back near your guardian, nor for long-gone or far-away tracks.
    let mut home = snap(vec![gone.clone()]); home.minimap.as_mut().unwrap().your_progress = Some(0.2); assert_eq!(advice(&home), None);
    let mut old = gone.clone(); old.unseen_ms = 60_000; assert_eq!(advice(&snap(vec![old])), None);
    let mut far = gone; far.pos = Pt { x: 0.1, y: 0.1 }; assert_eq!(advice(&snap(vec![far])), None);
}

#[test]
fn teammates_fighting_nearby_get_help_or_join_calls() {
    // Ally at Yellow, 0.3 away, against two enemies; nobody near you.
    let mut ally = hero(2, true, 0.8, 0.6); ally.lane = Some(Lane::Yellow);
    let fight = vec![ally.clone(), hero(5, false, 0.83, 0.62), hero(6, false, 0.78, 0.57)];
    let a = PositionCoach::default().assess(&snap(fight)).unwrap();
    assert_eq!(a.id, "help"); assert!(a.text.contains("Yellow lane") && a.text.contains("1 ally against 2 enemies"), "{}", a.text);
    let winning = vec![ally.clone(), hero(3, true, 0.82, 0.58), hero(5, false, 0.83, 0.62)];
    assert_eq!(advice(&snap(winning)).as_deref(), Some("join"));
    // Too far, or you are hurt: no rotation call.
    let mut far = ally.clone(); far.pos = Pt { x: 0.5, y: 0.05 };
    assert_eq!(advice(&snap(vec![far, hero(5, false, 0.52, 0.07)])), None);
    let mut hurt = snap(vec![ally, hero(5, false, 0.83, 0.62)]); hurt.hp = Some(300); assert_eq!(advice(&hurt), None);
}

#[test]
fn no_position_advice_without_a_confirmed_player_marker_or_while_dead() {
    let two = vec![hero(1, false, 0.55, 0.6), hero(2, false, 0.5, 0.66)];
    let mut s = snap(two.clone()); s.minimap.as_mut().unwrap().you = None; assert_eq!(advice(&s), None);
    let mut s = snap(two); s.dead = true; assert_eq!(advice(&s), None);
}

#[test]
fn lane_plan_defers_to_gank_warnings_instead_of_contradicting_them() {
    let mut e = hero(1, false, 0.30, 0.55); e.velocity = Some(Pt { x: 0.03, y: 0.0 });
    let mut s = snap(vec![e]);
    for l in [&mut s.minimap.as_mut().unwrap().blue] { l.push = Some(0.55); l.ally_troopers = 4; l.enemy_troopers = 2; l.ally_progress = Some(0.6); l.enemy_progress = Some(0.5); }
    let d = LaneCoach::default().decide(&s).unwrap();
    assert_eq!(d.action, Action::HoldWave); assert!(!d.speak); assert!(d.text.starts_with("Gank warning"));
    let mut engine = Engine::new(coach_engine::default_rules(RulesConfig::default())); engine.max_per_step = 5;
    let spoken = engine.step(&s);
    assert_eq!(spoken[0].key, "pos:gank");
    assert!(!spoken.iter().any(|c| c.key.starts_with("lane:push") || c.key.starts_with("lane:poke")), "{spoken:?}");
}
