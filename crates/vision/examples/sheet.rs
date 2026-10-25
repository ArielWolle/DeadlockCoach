fn main() {
    let mut files: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("0") && p.extension().map(|x| x=="jpg").unwrap_or(false)).collect();
    files.sort();
    let (cw, ch, cols) = (400u32, 225u32, 4u32);
    for (s, chunk) in files.chunks(16).enumerate() {
        let mut sheet = image::RgbImage::new(cw*cols, ch*4);
        for (i, p) in chunk.iter().enumerate() {
            let im = image::open(p).unwrap().resize_exact(cw, ch, image::imageops::FilterType::Triangle).to_rgb8();
            image::imageops::overlay(&mut sheet, &im, ((i as u32 % cols)*cw) as i64, ((i as u32 / cols)*ch) as i64);
        }
        sheet.save(format!("frames/sheet{s}.png")).unwrap();
        println!("sheet{s}: {} .. {}", chunk[0].file_name().unwrap().to_string_lossy(), chunk.last().unwrap().file_name().unwrap().to_string_lossy());
    }
}
