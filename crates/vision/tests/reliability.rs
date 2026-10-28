//! Data-reliability regressions from the rec5 ground-truth audit: per-match lane layout, the
//! minimap zone label, the urn carried as you, ping / rim rings, hero identity and own slot,
//! item latching and merged trooper waves.
use coach_core::{Lane, MinimapState, Pt, UrnState, ZoneArea};
use coach_vision::minimap::{crop_from_frame, Analyzer, Tracker};
use coach_vision::scan::{latch_item, latched_heroes, own_backdrop_slot, ItemLatch, ItemRead};
use image::DynamicImage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") }

/// The minimap fixtures are the 1920x1080 frame region x 1416-1920, y 600-1080. Paste one back
/// into a black frame so the regular frame-relative crops apply.
fn frame_from(name: &str) -> DynamicImage {
    let part = image::open(root().join("samples/minimap").join(name)).unwrap().to_rgb8();
    let mut f = image::RgbImage::new(1920, 1080);
    let (x, y) = if name.starts_with("topbar") { (0, 0) } else { (1416, 600) };
    image::imageops::overlay(&mut f, &part, x, y);
    DynamicImage::ImageRgb8(f)
}

fn map(name: &str) -> image::RgbImage { crop_from_frame(&frame_from(name)) }

fn zone_of(frame: &DynamicImage) -> Option<String> {
    let reader = coach_vision::ocr::Reader::load(&root().join("data/models")).unwrap();
    let (w, h) = (frame.width() as f32, frame.height() as f32);
    use coach_vision::minimap::{FRAC_H, FRAC_W, FRAC_X, FRAC_Y};
    let strip = coach_vision::zone::label_strip(frame, (FRAC_X * w, FRAC_Y * h, FRAC_W * w, FRAC_H * h));
    let parts = coach_vision::zone::label_parts(&strip);
    coach_vision::zone::zone_from_lines(&reader.read_lines(&parts).unwrap())
}

#[test]
fn lane_layout_is_latched_per_match() {
    // rec5 flipped the per-frame layout on 22 frames; each flip swapped Green / Yellow.
    let mut t = Tracker::default();
    let mirrored = |m: bool| MinimapState { lanes_mirrored: Some(m), lanes_mirrored_frame: Some(m), ..Default::default() };
    for i in 0..7 { t.observe(mirrored(true), i * 1000); }
    assert_eq!(t.layout(), Some(true));
    // A frame analysed under the other layout: its side-lane fields are swapped back.
    let mut flipped = mirrored(false);
    flipped.green.enemy_troopers = 4;
    let mut out = MinimapState::default();
    for i in 0..3 { out = t.observe(flipped.clone(), 7000 + i * 1000); }
    assert_eq!(out.lanes_mirrored, Some(true));
    assert_eq!(out.yellow.enemy_troopers, 4);
    assert_eq!(out.green.enemy_troopers, 0);
    // A short contrary run cannot flip the latch; a new match after a long gap votes again.
    for i in 0..2 { t.observe(mirrored(false), 10_000 + i * 1000); }
    assert_eq!(t.layout(), Some(true));
    for i in 0..6 { t.observe(mirrored(false), 1_000_000 + i * 1000); }
    assert_eq!(t.layout(), Some(false));
    // The analyzer honours a supplied layout: rec5 frame 480 (player on the left, Yellow lane).
    let an = Analyzer::new();
    let st = an.run_layout(&map("rec5-480-york-factory.jpg"), &mut vec![], Some(true));
    assert_eq!(st.lanes_mirrored, Some(true));
    assert_eq!(st.your_lane, Some(Lane::Yellow));
}

#[test]
fn reads_the_zone_label_and_maps_it_to_a_lane() {
    let york = zone_of(&frame_from("rec5-480-york-factory.jpg"));
    assert_eq!(york.as_deref(), Some("YORK : FACTORY"));
    assert_eq!(coach_vision::zone::lane_of_prefix(york.as_deref().unwrap()), Some(Lane::Yellow));
    let base = zone_of(&frame_from("rec5-560-hidden-king.jpg"));
    assert_eq!(base.as_deref(), Some("HIDDEN KING : BASE"));
    // rec5 is the mirrored layout and its own base is the Hidden King's.
    assert_eq!(coach_vision::zone::area_of("HIDDEN KING : BASE", Some(true)), Some(ZoneArea::OwnBase));
    assert_eq!(coach_vision::zone::area_of("CHINATOWN", None), Some(ZoneArea::Jungle));
    assert_eq!(coach_vision::zone::area_of("CANAL : YORK", None), Some(ZoneArea::Jungle));
    // In the tracker the label settles your lane and a base label clears it.
    let mut t = Tracker::default();
    let at = |p: Pt, zone: &str, lane: Option<Lane>| MinimapState { you: Some(p), your_lane: lane, zone: Some(zone.into()), lanes_mirrored: Some(true), ..Default::default() };
    let left = Pt { x: 0.25, y: 0.73 };
    t.observe(at(left, "YORK : FACTORY", Some(Lane::Green)), 0);
    let st = t.observe(at(left, "YORK : FACTORY", Some(Lane::Green)), 1000);
    assert_eq!(st.zone_lane, Some(Lane::Yellow));
    assert_eq!(st.your_lane, Some(Lane::Yellow), "the label names the lane under the marker");
    let home = Pt { x: 0.42, y: 0.89 };
    let st = t.observe(at(home, "HIDDEN KING : BASE", Some(Lane::Blue)), 2000);
    let st2 = t.observe(at(home, "HIDDEN KING : BASE", Some(Lane::Blue)), 3000);
    assert!(st.zone_area == Some(ZoneArea::OwnBase) || st2.zone_area == Some(ZoneArea::OwnBase));
    assert_eq!(st2.your_lane, None, "in your base you are not laning");
}

#[test]
fn urn_carried_with_your_cone_is_you() {
    // rec5 1:00-1:25 of the carry: the marker is replaced by a small vase under your cone.
    let raw = Analyzer::new().analyze(&map("rec5-150-urn-carry.jpg"));
    assert!(raw.you.is_none());
    let mark = *raw.urn_marks.first().expect("small carried vase");
    let mut t = Tracker::default();
    let me = Pt { x: mark.x - 0.03, y: mark.y };
    t.observe(MinimapState { you: Some(me), ..Default::default() }, 0);
    t.observe(MinimapState { you: Some(me), ..Default::default() }, 1000);
    let st = t.observe(raw, 2000);
    let you = st.you.expect("the vase is you");
    assert!((you.x - mark.x).abs() < 1e-4 && (you.y - mark.y).abs() < 1e-4);
    assert_eq!(st.urn, Some(UrnState::CarriedByYou));
}

#[test]
fn ping_circles_and_the_rim_are_not_enemies() {
    let an = Analyzer::new();
    // Frame 122: a large red ping circle produced six enemy rings on its band.
    let ping = an.analyze(&map("rec5-122-ping.jpg"));
    assert!(ping.enemies.len() <= 3, "{:?}", ping.enemies);
    assert!(ping.enemies.iter().all(|p| (p.x * 335.0 - 205.0).hypot(p.y * 350.0 - 208.0) > 30.0), "ring on the ping band: {:?}", ping.enemies);
    // Frame 560: lit scenery past the rim read as an enemy (the real ones are at your base).
    let rim = an.analyze(&map("rec5-560-hidden-king.jpg"));
    assert!(rim.enemies.iter().all(|p| coach_vision::minimap::on_map((p.x * 335.0, p.y * 350.0)) && p.y > 0.6), "{:?}", rim.enemies);
    assert!(!rim.enemies.is_empty());
}

#[test]
fn merged_trooper_wave_is_counted() {
    // Frame 250: ~8 red troopers in file on the right (Green, mirrored) lane read as 0-2.
    let st = Analyzer::new().run_layout(&map("rec5-250-wave.jpg"), &mut vec![], Some(true));
    assert!(st.green.enemy_troopers >= 5, "{:?}", st.green);
}

#[test]
fn haze_silhouette_portrait_is_haze() {
    // rec5 ally slot 3 (Amber) and rec4 ally slot 0 (Sapphire) are Haze; the card art (lit
    // face) lost to Vyper / Infernus. Real top-bar captures are stored as hero_haze@*.png.
    let lib = |d: &str| coach_vision::ColorLibrary::load(&root().join("data/icons").join(d), true).unwrap();
    let (card, crit) = (lib("h_card"), lib("h_crit"));
    for (name, slot) in [("topbar-rec5-210.jpg", 3usize), ("topbar-rec4-134.jpg", 0)] {
        let img = frame_from(name);
        let c = coach_vision::hud::crop(&img, coach_vision::hud::portrait(coach_vision::hud::ALLY_CENTERS[slot]));
        let mut best: HashMap<String, f32> = HashMap::new();
        for (n, s) in card.scores(&c).into_iter().chain(crit.scores(&c)) {
            let e = best.entry(coach_vision::scan::hero_key(&n).to_string()).or_insert(f32::MIN); *e = e.max(s);
        }
        let top = best.iter().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        assert_eq!(top.0, "hero_haze", "{name}: {best:?}");
    }
}

#[test]
fn own_slot_from_the_portrait_backdrop() {
    // Your portrait has a lit square backdrop; the slot differs per match (rec2 slot 5).
    assert_eq!(own_backdrop_slot(&frame_from("topbar-rec5-210.jpg")), Some(0));
    assert_eq!(own_backdrop_slot(&frame_from("topbar-rec4-134.jpg")), Some(4));
}

#[test]
fn hero_identity_latches_per_slot() {
    let mut votes = vec![HashMap::new(); 12];
    votes[3].insert("hero_haze".to_string(), 8.0);
    votes[3].insert("hero_viper".to_string(), 3.0);
    let mut fresh = vec![None; 12];
    fresh[3] = Some("hero_viper".to_string());
    let out = latched_heroes(&votes, &fresh);
    assert_eq!(out[3].as_deref(), Some("hero_haze"));
    // Without enough votes this frame's assignment is used.
    assert_eq!(latched_heroes(&vec![HashMap::new(); 12], &fresh)[3].as_deref(), Some("hero_viper"));
}

#[test]
fn items_latch_through_cooldown_dimming_and_single_misreads() {
    let mut l = ItemLatch::default();
    let item = |n: &str| ItemRead::Item(n.to_string());
    latch_item(&mut l, item("upgrade_express_shot"));
    assert_eq!(l.item, None, "one read is not enough");
    latch_item(&mut l, item("upgrade_express_shot"));
    assert_eq!(l.item.as_deref(), Some("upgrade_express_shot"));
    // Cooldown: dimmed / unmatched / briefly flat for 1-3 scans.
    for r in [ItemRead::Unclear, ItemRead::Empty, ItemRead::Empty, ItemRead::Unclear, ItemRead::Empty] { latch_item(&mut l, r); }
    assert_eq!(l.item.as_deref(), Some("upgrade_express_shot"));
    // A single (or double) confident misread does not swap it.
    latch_item(&mut l, item("upgrade_mercurial_magnum"));
    latch_item(&mut l, item("upgrade_mercurial_magnum"));
    assert_eq!(l.item.as_deref(), Some("upgrade_express_shot"));
    latch_item(&mut l, item("upgrade_mercurial_magnum"));
    assert_eq!(l.item.as_deref(), Some("upgrade_mercurial_magnum"), "a sustained new item replaces it");
    for _ in 0..4 { latch_item(&mut l, ItemRead::Empty); }
    assert_eq!(l.item, None, "sold");
}
