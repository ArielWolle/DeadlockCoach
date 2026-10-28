//! Respawn-timer pixel counts per top-bar slot for every frame of a directory (pixel-only).
//! Usage: timer_survey DIR > out.tsv   (columns: index, then per slot "px:visible:dead")
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let cs: Vec<f32> = coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS).collect();
    for (i, p) in files.iter().enumerate() {
        let img = image::open(p).unwrap();
        let img = if img.width() >= 1600 { img.thumbnail_exact(1600, 900) } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) }.to_rgb8();
        let row: Vec<String> = (0..12).map(|s| { let b = coach_vision::team::read_bar(&img, s); format!("{}:{}:{}", coach_vision::team::timer_pixels(&img, cs[s]), b.visible as u8, b.dead as u8) }).collect();
        println!("{i}\t{}", row.join("\t"));
    }
}
