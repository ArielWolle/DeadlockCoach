use coach_vision::minimap::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let files = &a[1..];
    let k = 3u32;
    let cols = files.len().min(2) as u32;
    let rows = (files.len() as u32 + cols - 1) / cols;
    let mut sheet = image::RgbImage::new(335 * k * cols, 350 * k * rows);
    let an = Analyzer::new();
    for (i, f) in files.iter().enumerate() {
        let img = image::open(f).unwrap();
        let c = crop_from_frame(&img);
        let s = c.width() as f32 / CW;
        let mut canvas = image::imageops::resize(&c, c.width() * k, c.height() * k, image::imageops::FilterType::Lanczos3);
        let mut mk = vec![];
        let st = an.run(&c, &mut mk);
        for (x, y, kind) in mk {
            let col = match kind { 'a' => [255u8, 255, 255], 'e' => [255, 0, 0], 'E' => [255, 0, 255], 'M' => [255, 255, 0], _ => [0, 255, 255] };
            let r = if kind == 'a' || kind == 'e' { 5 } else { 12 };
            let (cx, cy) = ((x * s * k as f32) as i32, (y * s * k as f32) as i32);
            for d in -r..=r { for (dx, dy) in [(d, -r), (d, r), (-r, d), (r, d)] {
                let (xx, yy) = (cx + dx, cy + dy);
                if xx >= 0 && yy >= 0 && (xx as u32) < canvas.width() && (yy as u32) < canvas.height() { canvas.put_pixel(xx as u32, yy as u32, image::Rgb(col)); }
            }}
        }
        println!("{} push G{:?} B{:?} Y{:?} en={} mb={} urn={:?}", i, st.green.push, st.blue.push, st.yellow.push, st.enemy_count, st.midboss_up, st.urn);
        image::imageops::overlay(&mut sheet, &canvas, ((i as u32 % cols) * 335 * k) as i64, ((i as u32 / cols) * 350 * k) as i64);
    }
    sheet.save(&a[0]).unwrap();
}
