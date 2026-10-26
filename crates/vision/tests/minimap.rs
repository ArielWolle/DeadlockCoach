//! Regressions on map-only crops from the user's calibration recording.
use coach_core::{MinimapState, Pt, UrnState};
use coach_vision::minimap::{Analyzer, Tracker};

fn fixture(name: &str) -> image::RgbImage {
    image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../samples/minimap/{name}.png"))).unwrap().to_rgb8()
}

#[test]
fn rejects_portrait_highlight_as_urn_and_finds_bridge_vase() {
    let an = Analyzer::new();
    let early = an.analyze(&fixture("pre-urn"));
    assert!(early.urn_pos.is_none(), "a cyan portrait was mistaken for the urn");
    let bridge = an.analyze(&fixture("urn-bridge"));
    let p = bridge.urn_pos.expect("bridge vase");
    assert!((p.x - 0.204).abs() < 0.02 && (p.y - 0.50).abs() < 0.03, "{p:?}");
    assert!(bridge.green.push.unwrap() < 0.15, "visually red Green lane");
    assert!(bridge.yellow.push.unwrap() > 0.55, "visually pushed Yellow lane");
    assert!(bridge.midboss_up);
    let mut tracker = Tracker::default();
    tracker.observe(bridge.clone(), 0);
    tracker.observe(bridge.clone(), 5000);
    let st = tracker.observe(bridge, 10000);
    assert_eq!(st.urn, Some(UrnState::Ground));
    assert!(st.midboss_up && st.observations == 3);
}

#[test]
fn follows_visible_moving_vase_and_does_not_guess_delivery_from_disappearance() {
    let an = Analyzer::new();
    let moving = an.analyze(&fixture("urn-moving"));
    let p = moving.urn_pos.expect("moving urn");
    assert!((p.x - 0.52).abs() < 0.04);
    // A visible marker alone cannot say WHO is carrying it.
    assert_eq!(moving.urn, Some(UrnState::Visible));
    assert!(an.analyze(&fixture("no-urn")).urn.is_none());
}

#[test]
fn unseen_map_never_implies_midboss_dead_or_safe_farming() {
    let an = Analyzer::new();
    let empty = an.analyze(&image::RgbImage::new(335, 350));
    assert!(!empty.midboss_up && empty.urn.is_none());
    assert!(empty.green.push.is_none() && empty.blue.push.is_none() && empty.yellow.push.is_none());
    assert_eq!(empty.enemy_count, 0);
}

#[test]
fn carrier_requires_motion_with_the_same_team_on_two_frames() {
    let mut tr = Tracker::default();
    let mut m = MinimapState { urn: Some(UrnState::Visible),
        urn_pos: Some(Pt { x: 0.30, y: 0.50 }), ..Default::default() };
    m.enemies = vec![m.urn_pos.unwrap()];
    assert!(tr.observe(m.clone(), 0).urn.is_none());
    assert_eq!(tr.observe(m.clone(), 5000).urn, Some(UrnState::Visible), "mere proximity isn't possession");
    m.urn_pos = Some(Pt { x: 0.34, y: 0.50 }); m.enemies = vec![m.urn_pos.unwrap()];
    assert_eq!(tr.observe(m.clone(), 10000).urn, Some(UrnState::CarriedByEnemy));
    assert!(tr.observe(m, 60000).urn.is_none(), "old evidence must expire across a gap");
}

#[test]
fn lossless_scaled_crop_has_consistent_objective_detection() {
    let an = Analyzer::new();
    let image = fixture("urn-bridge");
    let large = image::imageops::resize(&image, 670, 700, image::imageops::FilterType::Nearest);
    let m = an.analyze(&large);
    assert!(m.midboss_up);
    assert!(m.urn_pos.is_some_and(|p| (p.x - 0.204).abs() < 0.02));
}
