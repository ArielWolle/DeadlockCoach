use coach_vision::scan::Scanner;
fn hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let mx = r.max(g).max(b); let mn = r.min(g).min(b); let d = mx - mn;
    let h = if d == 0.0 { 0.0 } else if mx == r { 60.0 * (((g - b) / d) % 6.0) } else if mx == g { 60.0 * ((b - r) / d + 2.0) } else { 60.0 * ((r - g) / d + 4.0) };
    (if h < 0.0 { h + 360.0 } else { h }, if mx == 0.0 { 0.0 } else { d / mx }, mx)
}
fn class(r: u8, g: u8, b: u8) -> usize {
    let (h, s, v) = hsv(r, g, b);
    if s > 0.6 && v > 0.55 && (h < 20.0 || h > 345.0) { 1 }       // red
    else if s > 0.6 && v > 0.7 && (38.0..62.0).contains(&h) { 2 }  // yellow
    else if s > 0.5 && v > 0.5 && (85.0..150.0).contains(&h) { 3 } // green
    else if s > 0.6 && v > 0.65 && (194.0..218.0).contains(&h) { 4 } // blue
    else if s > 0.6 && v > 0.7 && (165.0..193.0).contains(&h) { 5 } // cyan
    else { 0 }
}
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let s = Scanner::load(std::path::Path::new(".")).unwrap();
    let (w, h) = (335u32, 350u32);
    let mut acc = vec![vec![0u32; (w * h) as usize]; 6];
    let mut n = 0u32;
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    files.sort();
    for p in files {
        let bytes = std::fs::read(&p).unwrap();
        if !s.classify(&bytes).has_hud() { continue; }
        let img = image::load_from_memory(&bytes).unwrap();
        let c = img.crop_imm(1255, 525, w, h).to_rgb8();
        for (x, y, px) in c.enumerate_pixels() { acc[class(px[0], px[1], px[2])][(y * w + x) as usize] += 1; }
        n += 1;
    }
    println!("frames {n}");
    let mut out = image::RgbImage::new(w * 2, h * 2);
    let cols = [[0u8,0,0], [255,60,40], [255,220,40], [60,230,60], [60,150,255], [0,255,255]];
    for y in 0..h { for x in 0..w {
        let i = (y * w + x) as usize;
        let mut best = (0u32, 0usize);
        for c in 1..6 { if acc[c][i] > best.0 { best = (acc[c][i], c); } }
        let f = (best.0 as f32 / n as f32).min(1.0);
        let col = if best.0 as f32 > 0.04 * n as f32 { let c = cols[best.1]; [ (c[0] as f32 * (0.35 + 0.65 * f)) as u8, (c[1] as f32 * (0.35 + 0.65 * f)) as u8, (c[2] as f32 * (0.35 + 0.65 * f)) as u8 ] } else { [10, 10, 10] };
        for dy in 0..2 { for dx in 0..2 { out.put_pixel(x * 2 + dx, y * 2 + dy, image::Rgb(col)); } }
    }}
    // grid every 50 px (crop coords)
    for g in (0..w).step_by(50) { for y in 0..h * 2 { out.put_pixel(g * 2, y, image::Rgb([90, 90, 90])); } }
    for g in (0..h).step_by(50) { for x in 0..w * 2 { out.put_pixel(x, g * 2, image::Rgb([90, 90, 90])); } }
    out.save("frames/mmacc.png").unwrap();
}
