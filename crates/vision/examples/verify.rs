use coach_vision::{hud, scan::Scanner};
fn main() {
    let mut s = Scanner::load(std::path::Path::new(".")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("0")).collect();
    files.sort();
    let mut last = Default::default();
    for p in &files { last = s.scan(&std::fs::read(p).unwrap()); }
    println!("ally {:?}\nenemy {:?}\nnw {:?}/{:?}", last.ally_heroes, last.enemy_heroes, last.ally_networth, last.enemy_networth);
    for (i, o) in s.hero_scores().iter().enumerate() { println!("slot {i}: {:?}", o); }
    // sheet: portrait on top, matched template below
    let img = image::open(files.last().unwrap()).unwrap();
    let names: Vec<String> = last.ally_heroes.iter().chain(last.enemy_heroes.iter()).cloned().collect();
    let mut sheet = image::RgbImage::new(104*12, 208);
    for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
        let c = hud::crop(&img, hud::portrait(*cx)).resize_exact(104, 104, image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut sheet, &c, (i as i64)*104, 0);
        if let Some(n) = names.get(i) { if let Ok(t) = image::open(format!("data/icons/h_card/{n}.png")) {
            let t = t.crop_imm(0,0,t.width(),t.width().min(t.height())).resize_exact(104,104,image::imageops::FilterType::Triangle).to_rgb8();
            image::imageops::overlay(&mut sheet, &t, (i as i64)*104, 104); } }
    }
    sheet.save("frames/verify.png").unwrap();
}
