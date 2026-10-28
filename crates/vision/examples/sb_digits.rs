//! Scoreboard K/D/A digit glyphs: per-number reads against known truth and template generation.
//! Usage: sb_digits FRAME TRUTH [FRAME TRUTH...], TRUTH = 12 comma-separated K/D/A ("4/5/6,6/9/7,...")
//! or "-" for read-only. ASCII=1 dumps the glyphs.
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut samples: Vec<(u32, Vec<f32>)> = vec![];
    let (mut right, mut wrong, mut none) = (0, 0, 0);
    for pair in a.chunks(2) {
        let mut img = image::open(&pair[0]).unwrap();
        if let Ok(c) = std::env::var("CTX") { let v: Vec<u32> = c.split(',').map(|s| s.parse().unwrap()).collect(); img = img.crop_imm(v[0], v[1], v[2], v[3]); }
        let img = image::DynamicImage::ImageRgba8(if img.width() >= 1600 { img.thumbnail_exact(1600, 900).to_rgba8() } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle).to_rgba8() });
        let truth: Vec<u32> = pair.get(1).filter(|t| *t != "-").map(|t| t.split([',', '/']).map(|v| v.parse().unwrap()).collect()).unwrap_or_default();
        let glyphs = coach_vision::scoreboard::kda_glyphs(&img);
        let mut line = String::new();
        for (i, g) in glyphs.iter().enumerate() {
            let want = truth.get(i).copied();
            let read = coach_vision::scoreboard::classify_number(g);
            match (want, read) { (Some(w), Some(r)) if w == r => right += 1, (Some(_), Some(_)) => wrong += 1, (Some(_), None) => none += 1, _ => {} }
            line.push_str(&format!("{}{}", read.map(|v| v.to_string()).unwrap_or("?".into()), if i % 3 == 2 { if want.is_some() && read != want { "!  " } else { "   " } } else { "/" }));
            if want.is_some() && read != want { line.push_str(&format!("(want {want:?} scores {:?}) ", g.iter().map(|x| coach_vision::scoreboard::classify_glyph(x)).collect::<Vec<_>>())); }
            if std::env::var("ASCII").ok().and_then(|v| v.parse::<usize>().ok()).is_some_and(|n| n == i) { for gl in g { for y in 0..coach_vision::scoreboard::GLYPH_H { println!("    {}", (0..coach_vision::scoreboard::GLYPH_W).map(|x| { let v = gl[(y * coach_vision::scoreboard::GLYPH_W + x) as usize]; if v > 0.6 { '#' } else if v > 0.3 { '+' } else { '.' } }).collect::<String>()); } println!(); } }
            if let Some(w) = want { let d: Vec<u32> = w.to_string().chars().map(|c| c.to_digit(10).unwrap()).collect(); if d.len() == g.len() { for (dv, gl) in d.iter().zip(g) { samples.push((*dv, gl.clone())); } } else { println!("  segment mismatch slot {} want {w} glyphs {}", i / 3, g.len()); } }
        }
        println!("{}: {line}", std::path::Path::new(&pair[0]).file_name().unwrap().to_string_lossy());
    }
    println!("right {right} wrong {wrong} unread {none}");
    if std::env::var_os("GEN").is_some() {
        for dgt in 0..10 {
            let s: Vec<_> = samples.iter().filter(|(d, _)| *d == dgt).collect();
            if s.is_empty() { println!("    \"\", // {dgt}: no samples"); continue; }
            let n = s.len() as f32;
            let mean: String = (0..(coach_vision::scoreboard::GLYPH_W * coach_vision::scoreboard::GLYPH_H) as usize).map(|k| { let v = s.iter().map(|(_, g)| g[k]).sum::<f32>() / n; char::from_digit((v * 9.0).round().clamp(0., 9.) as u32, 10).unwrap() }).collect();
            println!("    \"{mean}\", // {dgt}: {} samples", s.len());
        }
    }
}
