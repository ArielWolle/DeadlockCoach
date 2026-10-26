fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = &a[0]; let from: usize = a[1].parse().unwrap(); let to: usize = a[2].parse().unwrap();
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    files.sort();
    let (cw, ch, cols) = (266u32, 150u32, 6u32);
    let sel: Vec<_> = files[from..=to.min(files.len()-1)].to_vec();
    let rows = (sel.len() as u32 + cols - 1) / cols;
    let mut sheet = image::RgbImage::new(cw*cols, ch*rows);
    for (i, p) in sel.iter().enumerate() {
        let im = image::open(p).unwrap().resize_exact(cw, ch, image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut sheet, &im, ((i as u32 % cols)*cw) as i64, ((i as u32 / cols)*ch) as i64);
    }
    sheet.save(&a[3]).unwrap();
}
