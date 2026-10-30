//! Per-site structure timeline of a recording through the automatic scanner.
//! Usage: structures_probe DIR [STEP]. One row per site of `structures::SITES`, one column per
//! STEP frames: A = standing, x = destroyed, . = not listed; then the frame/clock it fell and
//! whether it ever reappeared after falling (must not).
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    // BENCH=<jpg>: time the per-frame pixel reads (structures, ability bar) on one frame.
    if let Ok(p) = std::env::var("BENCH") {
        let img = image::open(p).unwrap();
        let map = coach_vision::minimap::crop_from_frame(&img);
        let canon = image::DynamicImage::ImageRgba8(img.thumbnail_exact(1600, 900).to_rgba8());
        let t = std::time::Instant::now();
        for _ in 0..20 { std::hint::black_box(coach_vision::structures::read(&map)); }
        let s = t.elapsed().as_secs_f32() * 50.0;
        let t = std::time::Instant::now();
        for _ in 0..20 { std::hint::black_box(coach_vision::abilities::slots(&canon)); }
        println!("structures::read {s:.1} ms, abilities::slots {:.1} ms", t.elapsed().as_secs_f32() * 50.0);
        return;
    }
    let step: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader);
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let t0: u64 = files.first().and_then(|p| p.file_stem()?.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0);
    let n = coach_vision::structures::SITES.len();
    let mut rows = vec![String::new(); n];
    let mut fell: Vec<Option<(usize, Option<u32>)>> = vec![None; n];
    let mut back = vec![false; n];
    // SITE=<index>: print that site's outline shares per frame (on the located minimap crop).
    let debug: Option<usize> = std::env::var("SITE").ok().and_then(|s| s.parse().ok());
    for (fi, p) in files.iter().enumerate() {
        let ts: u64 = p.file_stem().and_then(|s| s.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(t0);
        let img = image::open(p).unwrap();
        let snap = auto.scan(&img, ts - t0);
        if let (Some(k), Some(m)) = (debug, auto.regions().into_iter().find(|r| r.id == "minimap")) {
            let crop = img.crop_imm(m.bounds.x, m.bounds.y, m.bounds.w, m.bounds.h).to_rgb8();
            if let Some(sh) = coach_vision::structures::shares(&crop) { eprintln!("{fi} {:?} {}x{}", sh[k], m.bounds.w, m.bounds.h); }
        }
        let st = auto.scanner().structure_sites(ts - t0);
        for i in 0..n {
            let c = st.iter().find(|(k, _)| *k == i).map(|(_, s)| if s.alive { 'A' } else { 'x' }).unwrap_or('.');
            if c == 'x' && fell[i].is_none() { fell[i] = Some((fi, snap.game_clock_secs)); }
            if c == 'A' && fell[i].is_some() { back[i] = true; }
            if fi % step == 0 { rows[i].push(c); }
        }
    }
    for (i, s) in coach_vision::structures::SITES.iter().enumerate() {
        let name = format!("{}{:?}{}", if s.own { "own " } else { "enemy " }, s.kind, s.side.map(|k| ["L", "M", "R"][k]).unwrap_or(""));
        let f = fell[i].map(|(f, c)| format!("fell@{f} {}", c.map(|c| format!("{}:{:02}", c / 60, c % 60)).unwrap_or("-".into()))).unwrap_or_default();
        println!("{name:22} {} {f}{}", rows[i], if back[i] { " REAPPEARED" } else { "" });
    }
}
