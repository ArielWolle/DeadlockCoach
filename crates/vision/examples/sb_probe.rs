//! Scoreboard (Tab) per-column reads: hero-name line text, matched hero and K/D/A.
//! Usage: sb_probe FRAME...
fn main() {
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    let heroes: Vec<(String, String)> = serde_json::from_str::<serde_json::Value>(std::fs::read_to_string(root.join("data/heroes.json")).unwrap().trim_start_matches('\u{feff}')).unwrap()
        .as_array().unwrap().iter().filter_map(|h| Some((h["class_name"].as_str()?.to_string(), h["name"].as_str()?.to_string()))).collect();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let img = image::DynamicImage::ImageRgba8(if img.width() >= 1600 { img.thumbnail_exact(1600, 900).to_rgba8() } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle).to_rgba8() });
        println!("{} cue={}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy(), coach_vision::scoreboard::panel_cue(&img));
        let h = reader.read_lines(&coach_vision::scoreboard::hero_line_crops(&img)).unwrap();
        let k = coach_vision::scoreboard::read_kda(&img);
        for i in 0..12 { println!("  {i:2} hero={:?} -> {:?} kda={:?}", h[i], coach_vision::scoreboard::match_hero(&h[i], &heroes), k[i]); }
    }
}
