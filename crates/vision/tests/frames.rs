use coach_vision::{scan::Scanner, screen::Screen};
use std::path::Path;

fn frame(prefix: &str) -> Vec<u8> {
    let dir = Path::new("../../samples/frames");
    let f = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().starts_with(prefix)).unwrap();
    std::fs::read(f.path()).unwrap()
}

fn scanner() -> Scanner { Scanner::load(Path::new("../..")).unwrap() }

/// The first reading of a clock/souls value needs a confirming second reading.
fn scan_confirmed(s: &mut Scanner, prefix: &str) -> coach_core::GameSnapshot {
    let f = frame(prefix);
    s.scan_at(&f, 0);
    s.scan_at(&f, 0)
}

#[test]
fn reads_real_frames() {
    let b = scan_confirmed(&mut scanner(), "00059");
    assert_eq!(b.game_clock_secs, Some(313));
    assert_eq!(b.souls, Some(1732));
    assert_eq!(b.ally_networth, Some(21000));
    assert_eq!(b.enemy_networth, Some(16000));
    assert!(b.items.contains(&"upgrade_pristine_emblem".to_string()), "items: {:?}", b.items);
    assert!(!b.dead);
    assert_eq!(b.hp,Some(779));assert_eq!(b.max_hp,Some(964));assert!(b.health_observations>=2);
    assert!(b.minimap.as_ref().unwrap().you.is_some());
    assert_eq!(b.minimap.as_ref().unwrap().your_lane,Some(coach_core::Lane::Blue));
}

#[test]
fn maximum_health_does_not_include_a_neighboring_stat_digit() {
    let s=scan_confirmed(&mut scanner(),"00014");
    assert_eq!(s.hp,Some(413));assert_eq!(s.max_hp,Some(769));
}

#[test]
fn classifies_every_screen() {
    let s = scanner();
    assert_eq!(s.classify(&frame("00048")), Screen::Shop);
    assert_eq!(s.classify(&frame("00014")), Screen::Gameplay);
    assert_eq!(s.classify(&frame("00184")), Screen::Scoreboard);
    assert_eq!(s.classify(&frame("00216")), Screen::Dead);
    assert_eq!(s.classify(&frame("00167")), Screen::Menu);
}

#[test]
fn reads_death_screen() {
    let snap = scanner().scan_at(&frame("00216"), 0);
    assert!(snap.dead);
    assert_eq!(snap.respawn_secs, Some(4));
    assert_eq!(snap.death_cause.as_deref(), Some("T2 Walker"));
    assert!(snap.hp.is_none()&&snap.max_hp.is_none());
}

#[test]
fn midboss_death_cause() {
    let snap = scanner().scan_at(&frame("00262"), 0);
    assert!(snap.dead);
    let cause = snap.death_cause.unwrap_or_default().to_lowercase();
    assert!(cause.contains("temple"), "cause: {cause}");
}

#[test]
fn reads_scoreboard_names() {
    let snap = scanner().scan_at(&frame("00184"), 0);
    assert_eq!(snap.players.len(), 12);
    let heroes: Vec<_> = snap.players.iter().map(|p| p.hero.clone().unwrap_or_default()).collect();
    for want in ["hero_viscous", "hero_ratking", "hero_kelvin", "hero_nurse", "hero_yamato", "hero_wraith"] {
        assert!(heroes.iter().any(|h| h == want), "missing {want} in {heroes:?}");
    }
    assert!(snap.players.iter().take(6).all(|p| p.ally) && snap.players.iter().skip(6).all(|p| !p.ally));
}

#[test]
fn menu_and_shop_yield_no_data() {
    let mut s = scanner();
    for p in ["00167", "00048"] {
        let snap = s.scan_at(&frame(p), 0);
        assert!(snap.game_clock_secs.is_none() && snap.souls.is_none() && !snap.dead, "{p}: {snap:?}");
    }
}
