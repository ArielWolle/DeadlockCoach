//! Regressions from a recorded match (1920x1080 JPEG captures) where live readings went wrong.
use coach_perception::Bounds;
use coach_vision::{localize, minimap::Analyzer, team::read_bar, vitals};
use image::DynamicImage;
use std::path::Path;

fn frame(name: &str) -> DynamicImage { image::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/frames").join(name)).unwrap() }
fn canonical(img: &DynamicImage) -> DynamicImage { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) }
fn map(img: &DynamicImage) -> image::RgbImage {
    let m = localize::MapLocator::default().locate(img).expect("minimap").bounds;
    img.crop_imm(m.x, m.y, m.w, m.h).to_rgb8()
}

#[test]
fn health_bar_exposes_a_truncated_hp_number() {
    // The live tracked box read "79"/808 for 20 s while the HUD showed 794/808.
    let full = vitals::bar_ratio(&canonical(&frame("00500_rec-hp-truncated.jpg"))).unwrap() / vitals::BAR_FULL;
    assert!(full > 0.95, "{full}");
    // A real fight frame: 355/847 = 0.42.
    let low = vitals::bar_ratio(&canonical(&frame("00504_rec-low-hp-fight.jpg"))).unwrap() / vitals::BAR_FULL;
    assert!((low - 0.42).abs() < 0.05, "{low}");
}

#[test]
fn dollar_sign_read_as_a_digit_is_removed_by_counting_glyphs() {
    // "$717 SOULS" was recognized as 3717 from a tracked box that starts right of the `$`.
    let img = frame("00503_rec-souls-dollar.jpg");
    assert_eq!(localize::correct_souls(&img, Bounds { x: 103, y: 896, w: 108, h: 25 }, 3717, "3717 SOULS"), 717);
    // Genuine four-digit values (merged "23" glyphs in 1,232) are kept.
    let f = frame("00401_health-mixed.jpg");
    assert_eq!(localize::correct_souls(&f, Bounds { x: 74, y: 739, w: 115, h: 34 }, 1232, "$ 1,232 SOULS"), 1232);
}

#[test]
fn mirrored_lane_layout_and_player_marker_next_to_a_tower() {
    // Yellow lane on the left, Green on the right in this match; you stand by the Green walker.
    let st = Analyzer::new().analyze(&map(&frame("00501_rec-mirrored-lanes.jpg")));
    assert_eq!(st.lanes_mirrored, Some(true));
    let you = st.you.expect("player marker");
    assert!((you.x * 335.0 - 264.0).abs() < 6.0 && (you.y * 350.0 - 198.0).abs() < 6.0, "{you:?}");
    assert_eq!(st.your_lane, Some(coach_core::Lane::Green));
    // Minute two: no lane is pushed into either base.
    for l in [&st.green, &st.blue, &st.yellow] { let p = l.push.unwrap(); assert!((0.25..0.75).contains(&p), "{p}"); }
    // The original calibration matches keep the other layout.
    let other = Analyzer::new().analyze(&image::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/minimap/urn-bridge.png")).unwrap().to_rgb8());
    assert_eq!(other.lanes_mirrored, Some(false));
}

#[test]
fn dead_needs_a_respawn_timer_and_full_bars_are_seen() {
    let dead = canonical(&frame("00500_rec-hp-truncated.jpg")).to_rgb8();
    assert!(read_bar(&dead, 6).dead, "enemy 0 shows a 4 s respawn timer");
    // A pale living hero without a bar is unseen, not dead; full bars beside it are visible.
    let pale = canonical(&frame("00502_rec-pale-enemy-unseen.jpg")).to_rgb8();
    let e0 = read_bar(&pale, 6); assert!(!e0.dead && !e0.visible, "{e0:?}");
    for slot in [0, 1, 3, 5] { assert!(read_bar(&pale, slot).visible, "ally {slot}"); }
}
