//! Ability-bar read of single frames (each scanned twice by a fresh scanner, 2.5 s apart).
//! Usage: abilities_probe DIR INDEX... (INDEX = position in the sorted *.jpg list).
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::Path::new(".");
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    for i in a[1..].iter().filter_map(|s| s.parse::<usize>().ok()) {
        let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
        let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader);
        let img = image::open(&files[i]).unwrap();
        auto.scan(&img, 0);
        let s = auto.scan(&img, 2500);
        let ab: Vec<String> = s.abilities.iter().map(|a| format!("{}:{}", a.slot, match (a.ready, a.cooldown_secs) { (Some(true), _) => "R".into(), (_, Some(c)) => c.to_string(), _ => "X".into() })).collect();
        println!("{i} {} {}", s.screen.unwrap_or_default(), ab.join(" "));
        // RAW=1: the recognised text of each located digit crop on the frame's 16:9 canonical image.
        if std::env::var("RAW").is_ok() {
            // The automatic scanner's context of a larger 16:9 frame is a box-filter thumbnail.
            let canon = image::DynamicImage::ImageRgba8(img.thumbnail_exact(1600, 900).to_rgba8());
            for k in 0..4 { let (b, dk, g) = coach_vision::abilities::slot_shares(&canon, k); println!("   slot {} shares {b:.2} {dk:.2} {g:.2}", k + 1); }
            let rd = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
            for k in 0..4 {
                let (cx, cy) = coach_vision::abilities::CENTRES[k]; let r = coach_vision::abilities::RADIUS;
                let area = coach_perception::Bounds { x: (cx - r) as u32, y: (cy - r) as u32, w: (2.0 * r) as u32, h: (2.0 * r) as u32 };
                let bl: Vec<String> = coach_vision::hudtext::blobs(&canon, area, coach_vision::hudtext::white).iter().filter(|b| b.area >= 6)
                    .map(|b| format!("({},{} {}x{} a{})", b.min_x, b.min_y, b.w(), b.h(), b.area)).collect();
                println!("   slot {} white blobs {}", k + 1, bl.join(" "));
            }
            for k in 0..4 { if let Some(d) = coach_vision::abilities::locate_digits(&canon, k) {
                let c = coach_vision::abilities::digits_crop(&canon, &d);
                println!("   slot {} {:?} -> {:?}", k + 1, d, rd.read_lines(&[c]).ok());
            }}
        }
    }
}
