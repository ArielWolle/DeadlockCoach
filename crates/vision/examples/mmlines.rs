use coach_vision::{minimap::*, scan::Scanner};
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let s = Scanner::load(std::path::Path::new(".")).unwrap();
    let (w, h) = (335u32, 350u32);
    let mut acc = vec![[0u16; 4]; (w * h) as usize]; // red, green, blue, yellow
    let mut n = 0u32;
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    files.sort();
    for p in files {
        let bytes = std::fs::read(&p).unwrap();
        if !s.classify(&bytes).has_hud() { continue; }
        let img = image::load_from_memory(&bytes).unwrap();
        let c = crop_from_frame(&img);
        if c.width() != w || c.height() != h { continue; }
        for (x, y, px) in c.enumerate_pixels() {
            let (hh, ss, vv) = hsv(px[0], px[1], px[2]);
            let i = (y * w + x) as usize;
            if ss > 0.45 && vv > 0.45 {
                if hh < 22.0 || hh > 345.0 { acc[i][0] += 1; }
                else if (38.0..62.0).contains(&hh) { acc[i][3] += 1; }
                else if (85.0..150.0).contains(&hh) { acc[i][1] += 1; }
                else if (195.0..215.0).contains(&hh) { acc[i][2] += 1; }
            }
        }
        n += 1;
    }
    println!("frames {n}");
    let k = 3u32;
    let cols = [[255u8, 70, 50], [60, 255, 60], [60, 160, 255], [255, 230, 40]];
    let mut out = image::RgbImage::new(w * k, h * k);
    for y in 0..h { for x in 0..w {
        let a = acc[(y * w + x) as usize];
        let (mut bi, mut bv) = (0usize, 0u16);
        for c in 0..4 { if a[c] > bv { bv = a[c]; bi = c; } }
        let col = if bv as f32 > 0.03 * n as f32 { let f = (0.4 + 0.6 * (bv as f32 / n as f32).min(1.0) * 3.0).min(1.0); [(cols[bi][0] as f32 * f) as u8, (cols[bi][1] as f32 * f) as u8, (cols[bi][2] as f32 * f) as u8] } else { [0, 0, 0] };
        for dy in 0..k { for dx in 0..k { out.put_pixel(x * k + dx, y * k + dy, image::Rgb(col)); } }
    }}
    for g in (0..w).step_by(20) { let c = if g % 100 == 0 { [140u8, 140, 140] } else { [55, 55, 55] }; for y in 0..h * k { out.put_pixel(g * k, y, image::Rgb(c)); } }
    for g in (0..h).step_by(20) { let c = if g % 100 == 0 { [140u8, 140, 140] } else { [55, 55, 55] }; for x in 0..w * k { out.put_pixel(x, g * k, image::Rgb(c)); } }
    out.save("frames/mmlines.png").unwrap();
}
