//! Measure scoreboard column-panel colour statistics (a pixel-only scoreboard cue).
use coach_vision::hud;
fn main() {
    for path in std::env::args().skip(1) {
        let frame = image::open(&path).unwrap().resize_exact(1600, 900, image::imageops::FilterType::Triangle).to_rgb8();
        print!("{path:<45}");
        for (i, cx) in hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).enumerate() {
            let (mut s, mut s2, mut n) = ([0f32; 3], 0f32, 0f32);
            for y in 232..242 { for x in (cx as u32 - 30)..(cx as u32 + 30) {
                let p = frame.get_pixel(x, y); for c in 0..3 { s[c] += p[c] as f32; }
                let l = p[0] as f32 * 0.3 + p[1] as f32 * 0.59 + p[2] as f32 * 0.11; s2 += l * l; n += 1.;
            }}
            let m = [s[0] / n, s[1] / n, s[2] / n];
            let lum = m[0] * 0.3 + m[1] * 0.59 + m[2] * 0.11; let sd = (s2 / n - lum * lum).max(0.).sqrt();
            print!(" {}{:>4.0}/{:>3.0}", if i == 6 { "|" } else { "" }, m[2] - m[0], sd);
        }
        println!();
    }
}
