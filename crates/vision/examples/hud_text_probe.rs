//! Diagnose pixel-located HUD text on recorded frames (canonical 1600x900 resize).
//! Usage: hud_text_probe MODE DIR FROM TO   MODE = souls | respawn | panel | status | detect:x,y,w,h
use coach_vision::{death, hudtext, status};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&a[1]).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let (from, to): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    for (i, p) in files.iter().enumerate().filter(|(i, _)| (from..=to).contains(i)) {
        let img = image::open(p).unwrap();
        let img = if img.width() == 1600 && img.height() == 900 { img } else { if img.width() >= 1600 { image::DynamicImage::ImageRgba8(img.thumbnail_exact(1600, 900).to_rgba8()) } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle) } };
        let mode = a[0].as_str();
        if mode == "souls" {
            let src = image::open(p).unwrap();
            let view = hudtext::View { canon: &img, source: Some((&src, coach_perception::Bounds { x: 0, y: 0, w: src.width(), h: src.height() })) };
            let img = hudtext::souls_area(&view);
            let s = hudtext::locate_souls(&img);
            if std::env::var("DBG").is_ok() {
                let all = coach_perception::Bounds { x: 0, y: 0, w: img.width(), h: img.height() };
                for b in hudtext::blobs_closed(&img, all, hudtext::mint).into_iter().filter(|b| b.h() >= 12) { eprintln!("  $cand x{}-{} y{}-{} area{}", b.min_x, b.max_x, b.min_y, b.max_y, b.area); }
                for b in hudtext::blobs(&img, all, hudtext::souls_text).into_iter().filter(|b| b.h() >= 10) { eprintln!("  dig x{}-{} y{}-{} area{}", b.min_x, b.max_x, b.min_y, b.max_y, b.area); }
            }
            let mut crops = vec![]; let mut what = vec![];
            if let Some(s) = &s {
                crops.extend(hudtext::souls_crops(&img, s)); what.push("souls"); what.push("souls");
                if let Some((b, _)) = hudtext::level_glyphs(&img, s) { crops.push(death::digits_crop(&img, b)); what.push("level"); }
                if let Some(b) = hudtext::unsecured_line(&img, s) { crops.push(img.crop_imm(b.x, b.y, b.w, b.h)); what.push("unsecured"); }
            }
            let lines = reader.read_lines(&crops).unwrap_or_default();
            println!("{i} {:?} lvl={:?} {:?}", s.map(|s| (s.dollar, s.digits, s.count)), s.and_then(|s| hudtext::level_glyphs(&img, &s)).map(|x| x.1), what.iter().zip(lines.iter()).collect::<Vec<_>>());
        } else if mode == "respawn" {
            for (name, area) in [("death", death::RESPAWN_BOX), ("shop", death::SHOP_RESPAWN_BOX)] {
                let g = death::digit_glyphs(&img, area);
                let mut crops = vec![coach_vision::hud::crop(&img, coach_vision::screen::respawn_number())];
                if let Some((b, _)) = g { crops.push(death::digits_crop(&img, b)); }
                if name == "shop" { crops.push(img.crop_imm(death::SHOP_RESPAWN_LABEL.x, death::SHOP_RESPAWN_LABEL.y, death::SHOP_RESPAWN_LABEL.w, death::SHOP_RESPAWN_LABEL.h)); }
                println!("{i} {name} glyphs={:?} {:?}", g, reader.read_lines(&crops).unwrap_or_default());
            }
        } else if mode == "panel" {
            let src = image::open(p).unwrap();
            let view = hudtext::View { canon: &img, source: Some((&src, coach_perception::Bounds { x: 0, y: 0, w: src.width(), h: src.height() })) };
            let view = if src.width() >= 1800 { view } else { hudtext::View::canonical(&img) };
            let size = death::panel_size(&view);
            let img = view.region(death::PANEL_CANONICAL, size, size);
            let (rows, crops) = death::panel_crops(&img);
            let lines = reader.read_lines(&crops).unwrap_or_default();
            let kb: Vec<u32> = (0..rows.len()).map(|k| death::killing_blow_px(&img, k)).collect();
            println!("{i} rows={:?} kb={kb:?} {:?}", rows.iter().map(|r| r.1).collect::<Vec<(usize, usize)>>(), lines);
            println!("   -> {:?}", death::parse_panel(&img, &rows, &lines));
        } else if mode == "status" {
            let src = image::open(p).unwrap();
            let view = hudtext::View { canon: &img, source: Some((&src, coach_perception::Bounds { x: 0, y: 0, w: src.width(), h: src.height() })) };
            let (kinds, crops): (Vec<status::Kind>, Vec<image::DynamicImage>) = status::crops(&view).into_iter().unzip();
            if crops.is_empty() { continue; }
            let lines = reader.read_lines(&crops).unwrap_or_default();
            println!("{i} {:?} {:?} -> {:?}", kinds, lines, status::interpret(&kinds, &lines));
        } else if let Some(r) = mode.strip_prefix("detect:") {
            let n: Vec<u32> = r.split(',').map(|v| v.parse().unwrap()).collect();
            let t = reader.read(&img.crop_imm(n[0], n[1], n[2], n[3])).unwrap_or_default();
            println!("{i} {:?}", t.replace('\n', " | "));
        }
    }
}
