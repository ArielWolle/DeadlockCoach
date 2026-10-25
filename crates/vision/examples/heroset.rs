use coach_vision::{hud, ColorLibrary};
use std::collections::{HashMap, HashSet};
fn main() {
    let mut files: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("0")).collect();
    files.sort();
    let imgs: Vec<_> = files.iter().skip(2).step_by(2).map(|p| image::open(p).unwrap()).collect();
    let sets = [("topbar", true), ("h_small", true), ("h_card", true), ("h_mm", true), ("h_crit", true), ("h_gloat", true)];
    let last = imgs.last().unwrap();
    let mut sheet = image::RgbImage::new(104*12, 104*(sets.len() as u32 + 1));
    for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
        let c = hud::crop(last, hud::portrait(*cx)).resize_exact(104,104,image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut sheet, &c, (i as i64)*104, 0);
    }
    for (row, (set, top)) in sets.iter().enumerate() {
        let lib = ColorLibrary::load(std::path::Path::new(&format!("data/icons/{set}")), *top).unwrap();
        let mut sums: Vec<HashMap<String, f32>> = vec![HashMap::new(); 12];
        for im in &imgs { for (i, cx) in hud::ALLY_CENTERS.iter().chain(hud::ENEMY_CENTERS.iter()).enumerate() {
            for (n, s) in lib.scores(&hud::crop(im, hud::portrait(*cx))) { *sums[i].entry(n).or_default() += s / imgs.len() as f32; } } }
        let mut pairs: Vec<(f32, usize, String)> = vec![];
        for (i, m) in sums.iter().enumerate() { for (n, s) in m { pairs.push((*s, i, n.clone())); } }
        pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let mut out: Vec<Option<(String, f32)>> = vec![None; 12]; let mut used = HashSet::new();
        for (s, i, n) in pairs { if out[i].is_none() && used.insert(n.clone()) { out[i] = Some((n, s)); } }
        let names: Vec<String> = out.iter().map(|o| o.as_ref().map(|x| x.0.replace("hero_", "")).unwrap_or_default()).collect();
        println!("{set}: {}", names.join(","));
        for (i, o) in out.iter().enumerate() { if let Some((n, _)) = o {
            if let Ok(t) = image::open(format!("data/icons/{set}/{n}.png")) {
                let t = t.crop_imm(0,0,t.width(),t.width().min(t.height())).resize_exact(104,104,image::imageops::FilterType::Triangle).to_rgb8();
                image::imageops::overlay(&mut sheet, &t, (i as i64)*104, ((row+1) as i64)*104); } } }
    }
    sheet.save("frames/heroset.png").unwrap();
}
