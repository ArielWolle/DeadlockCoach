//! Minimap diagnostics per frame (canonical px): urn / you, troopers, enemy rings, own-marker
//! rings, on-lane white / red components, large red components (pings) and cyan (urn) blobs.
//! Usage: mm_probe FRAME...
use coach_vision::minimap::*;
fn main() {
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let c = crop_from_frame(&img);
        let s = c.width() as f32 / CW;
        let m = masks(&c);
        let st = Analyzer::new().analyze(&c);
        println!("{f}: urn={:?} at {:?} you={:?}", st.urn, st.urn_pos.map(|p| (p.x * CW, p.y * CH)), st.you.map(|p| (p.x * CW, p.y * CH)));
        println!("  troopers G {}/{} B {}/{} Y {}/{} mirrored {:?}", st.green.ally_troopers, st.green.enemy_troopers, st.blue.ally_troopers, st.blue.enemy_troopers, st.yellow.ally_troopers, st.yellow.enemy_troopers, st.lanes_mirrored);
        println!("  enemies {:?}", st.enemies.iter().map(|p| ((p.x * CW) as i32, (p.y * CH) as i32)).collect::<Vec<_>>());
        let an = Analyzer::new();
        for (mask, t) in [(&m.white, 'w'), (&m.red, 'r')] {
            let mut v = vec![];
            for b in components(mask, m.w, m.h) {
                let p = (b.cx / s, b.cy / s);
                let a = b.area as f32 / (s * s);
                if a < 2.0 || a > 200.0 { continue; }
                if let Some((k, _, d)) = an.project(p) { if d <= 6.0 { v.push(format!("{k}:{a:.0}@({:.0},{:.0}){:.0}x{:.0}", p.0, p.1, b.w() as f32 / s, b.h() as f32 / s)); } }
            }
            println!("  {t} on-lane comps: {}", v.join(" "));
        }
        let (_, enemy) = ring_masks(&c);
        for b in components(&enemy, c.width(), c.height()) {
            if (b.w() as f32) < 20.0 * s && (b.h() as f32) < 20.0 * s { continue; }
            println!("  red comp {:.0}x{:.0} at ({:.0},{:.0}) fill {:.2}", b.w() as f32 / s, b.h() as f32 / s, b.cx / s, b.cy / s, b.fill());
        }
        let own_mask: Vec<bool> = c.pixels().map(|p| { let (hh, ss, vv) = hsv(p[0], p[1], p[2]); (128.0..148.0).contains(&hh) && ss > 0.45 && vv > 0.45 }).collect();
        for cov in [0.6f32, 0.45] {
            let r: Vec<_> = rings_scaled(&own_mask, c.width(), c.height(), s, cov, 0.58).into_iter().map(|r| (r.x as i32, r.y as i32, (r.coverage * 100.0) as i32)).collect();
            println!("  own rings cov>={cov}: {r:?}");
        }
        for b in components(&m.cyan, m.w, m.h) {
            if (b.area as f32) < 8.0 * s * s { continue; }
            println!("  cyan area {:.0} at ({:.0},{:.0}) {:.0}x{:.0}", b.area as f32 / (s * s), b.cx / s, b.cy / s, b.w() as f32 / s, b.h() as f32 / s);
        }
    }
}
