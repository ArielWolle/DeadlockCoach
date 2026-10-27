//! Top-bar health bars, "not seen recently" (no bar) and dead portraits on real screenshots.
use coach_vision::team::read_bar;
use std::path::Path;

fn band(name: &str) -> image::RgbImage {
    image::open(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/frames").join(name)).unwrap().to_rgb8()
}
/// Expected per slot (allies 0-5, enemies 6-11): Some(hp) visible bar, None no bar, or dead.
#[derive(Clone, Copy, Debug)]
enum Want { Hp(f32), NoBar, Dead, NearZero }
use Want::*;

fn check(name: &str, want: [Want; 12]) {
    let img = band(name);
    for (i, w) in want.iter().enumerate() {
        let r = read_bar(&img, i);
        let ok = match w {
            Hp(v) => r.visible && r.ratio.is_some_and(|x| (x - v).abs() <= 0.1),
            NoBar => !r.visible && !r.dead && !r.empty,
            Dead => r.dead && !r.visible,
            // CRITICAL hero at ~0: either a tiny visible fill or an all-empty bar.
            NearZero => r.visible && r.ratio.is_some_and(|x| x <= 0.1) || r.empty,
        };
        assert!(ok, "{name} slot {i}: want {w:?}, got {r:?}");
    }
}

#[test]
fn every_bar_full_at_match_start() {
    check("00400_health-full.jpg", [Hp(1.0); 12]);
}

#[test]
fn damaged_critical_missing_and_dead_heroes_mid_game() {
    // 4:53: ally 0 CRITICAL, allies damaged; enemy 6 CRITICAL, 7 and 10 not seen recently,
    // 8 and 9 damaged, 11 dead with a respawn timer.
    check("00401_health-mixed.jpg", [Hp(0.15), Hp(0.69), Hp(0.95), Hp(0.64), Hp(1.0), Hp(1.0),
        NearZero, NoBar, Hp(0.54), Hp(0.85), NoBar, Dead]);
}

#[test]
fn older_fixtures_keep_consistent_readings() {
    check("00059_1792966931596.jpg", [Hp(1.0), Hp(0.69), Hp(0.82), Hp(0.74), Hp(1.0), Hp(1.0),
        NearZero, Hp(0.85), NoBar, NearZero, Hp(1.0), Hp(1.0)]);
}
