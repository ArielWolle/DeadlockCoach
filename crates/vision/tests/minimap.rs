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
fn frame_map(name:&str)->image::RgbImage{
    let img=image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/frames").join(name)).unwrap();
    coach_vision::minimap::crop_from_frame(&img)
}
/// Every labelled ring is found within 6 px and nothing else is reported.
fn assert_heroes(m:&MinimapState,allies:&[(f32,f32)],enemies:&[(f32,f32)],what:&str){
    for (found,want,team) in [(&m.allies,allies,"ally"),(&m.enemies,enemies,"enemy")]{
        let px:Vec<(f32,f32)>=found.iter().map(|p|(p.x*335.0,p.y*350.0)).collect();
        for w in want{assert!(px.iter().any(|p|(p.0-w.0).hypot(p.1-w.1)<=6.0),"{what}: missed {team} at {w:?}; found {px:?}");}
        assert_eq!(px.len(),want.len(),"{what}: {team} count; found {px:?}");
    }
}
// Hand-labelled ring centres (canonical minimap px) from zoomed fixture crops.
const F49:(&[(f32,f32)],&[(f32,f32)])=(&[(144.,244.),(182.,271.),(47.,199.)],&[(274.,195.),(50.,190.)]);
const F59:(&[(f32,f32)],&[(f32,f32)])=(&[(42.,171.),(51.,216.),(162.,192.),(277.,176.),(289.,176.)],&[(166.,137.),(36.,177.),(47.,162.),(272.,160.)]);
const F14:(&[(f32,f32)],&[(f32,f32)])=(&[(49.,195.),(258.,183.),(270.,170.),(163.,193.)],&[(49.,168.),(58.,168.),(274.,156.),(152.,186.)]);

#[test]
fn hero_portrait_rings_match_hand_labels_including_heroes_on_structures(){
    let an=Analyzer::new();
    for (name,(a,e)) in [("00049_1792966881569.jpg",F49),("00059_1792966931596.jpg",F59),("00014_1792966706577.jpg",F14)]{
        let start=std::time::Instant::now();let m=an.analyze(&frame_map(name));println!("{name}: {:?}",start.elapsed());
        assert_heroes(&m,a,e,name);
    }
    // Blank map, shop and menus: no heroes are invented.
    assert!(an.analyze(&image::RgbImage::new(335,350)).allies.is_empty());
    for name in ["00048_1792966876580.jpg","00167_scoreboard-mid.jpg"]{let m=an.analyze(&frame_map(name));assert!(m.enemies.is_empty(),"{name}: {:?}",m.enemies);}
}
#[test]
fn hero_rings_are_found_at_other_capture_scales(){
    let an=Analyzer::new();let map=frame_map("00059_1792966931596.jpg");
    for size in [(503,525),(670,700)]{
        let m=an.analyze(&image::imageops::resize(&map,size.0,size.1,image::imageops::FilterType::Triangle));
        assert_heroes(&m,F59.0,F59.1,&format!("{size:?}"));
    }
}
#[test]
fn tracks_keep_last_seen_position_velocity_and_forget_after_45_seconds(){
    let mut tr=Tracker::default();
    let at=|x:f32|MinimapState{enemies:vec![Pt{x,y:0.5}],allies:vec![Pt{x:0.2,y:0.8}],..Default::default()};
    tr.observe(at(0.30),0);let m=tr.observe(at(0.33),1000);
    let e=m.heroes.iter().find(|h|!h.ally).unwrap();assert_eq!(e.hits,2);assert!(e.visible());
    let v=e.velocity.unwrap();assert!((v.x-0.03).abs()<0.005,"{v:?}");
    assert!(e.closing_speed(Pt{x:0.6,y:0.5})>0.02 && e.closing_speed(Pt{x:0.1,y:0.5})<0.0);
    // Leaves vision: the track remains at its last position, aging, without velocity.
    let gone=MinimapState{allies:vec![Pt{x:0.2,y:0.8}],..Default::default()};
    let m=tr.observe(gone.clone(),9000);let e=m.heroes.iter().find(|h|!h.ally).unwrap();
    assert_eq!(e.unseen_ms,8000);assert!((e.pos.x-0.33).abs()<1e-4);assert!(e.velocity.is_none());
    // Reappearing nearby continues the same track; far away after long gaps is a new one.
    let id=e.id;let m=tr.observe(at(0.40),12000);assert_eq!(m.heroes.iter().find(|h|!h.ally).unwrap().id,id);
    for t in [20000u64,30000,40000,50000]{tr.observe(gone.clone(),t);}
    assert!(tr.observe(gone,58000).heroes.iter().all(|h|h.ally),"stale enemy track must be forgotten");
}
#[test]
fn green_player_ring_is_confirmed_over_frames_and_not_invented_on_blank_maps() {
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let img=image::open(root.join("samples/frames/00059_1792966931596.jpg")).unwrap();
    let raw=coach_vision::minimap::Analyzer::new().analyze(&coach_vision::minimap::crop_from_frame(&img));
    let mut tracker=coach_vision::minimap::Tracker::default();
    assert!(tracker.observe(raw.clone(),0).you.is_none());
    assert_eq!(tracker.observe(raw,8000).your_lane,Some(coach_core::Lane::Blue));
    assert!(tracker.observe(coach_core::MinimapState::default(),16000).you.is_none());
}
