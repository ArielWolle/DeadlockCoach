fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = &a[0];
    let files = &a[1..];
    let cell = 480u32;
    let cols = 3u32.min(files.len() as u32);
    let rows = (files.len() as u32 + cols - 1) / cols;
    let mut sheet = image::RgbImage::new(cell * cols, cell * rows);
    for (i, f) in files.iter().enumerate() {
        let img = image::open(f).unwrap();
        let (w, h) = (img.width() as f32 / 1600.0, img.height() as f32 / 900.0);
        let c = img.crop_imm((1255.0 * w) as u32, (525.0 * h) as u32, (335.0 * w) as u32, (350.0 * h) as u32)
            .resize_exact(cell, cell, image::imageops::FilterType::Lanczos3).to_rgb8();
        image::imageops::overlay(&mut sheet, &c, ((i as u32 % cols) * cell) as i64, ((i as u32 / cols) * cell) as i64);
    }
    sheet.save(out).unwrap();
}
