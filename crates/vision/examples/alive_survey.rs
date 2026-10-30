//! Per-slot top-bar dead-state features for every frame of a directory (pixel-only): the
//! respawn-timer pixels, CRITICAL glow ring, greyed-portrait test and the bar reading.
//! Usage: alive_survey DIR > out.tsv
//! Columns: committed dead slots (stateful [`apply_bars`]), index, file, then per slot
//! "timer:glow:visible:empty:dead:digits:digit_rows".
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let cs: Vec<f32> = coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS).collect();
    let mut status = vec![];
    for (i, p) in files.iter().enumerate() {
        let img = image::open(p).unwrap();
        let img = if img.width() >= 1600 { img.thumbnail_exact(1600, 900) } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) };
        let dy = coach_vision::team::top_bar_offset(&img);
        let top = coach_vision::team::top_bar_aligned(&img, dy).into_owned();
        coach_vision::team::apply_bars(&top, &mut status);
        let dead: Vec<String> = status.iter().enumerate().filter(|(_, s)| s.dead == Some(true)).map(|(k, _)| k.to_string()).collect();
        let img = top.to_rgb8();
        print!("{}\t", dead.join(","));
        let row: Vec<String> = (0..12).map(|s| {
            let b = coach_vision::team::read_bar(&img, s);
            let (d, rows) = coach_vision::team::digit_glyph(&img, cs[s]);
            format!("{}:{}:{}:{}:{}:{}:{}", coach_vision::team::timer_pixels(&img, cs[s]), coach_vision::team::glow_pixels(&img, cs[s]), b.visible as u8, b.empty as u8, b.dead as u8, d, rows)
        }).collect();
        println!("{i}\t{}\t{}", p.file_name().unwrap().to_string_lossy(), row.join("\t"));
    }
}
