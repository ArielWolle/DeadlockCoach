//! Screen class per frame through the full automatic scanner, with the clock and the scene's
//! internal clock state. Usage: screen_trace IMG_DIR FIRST LAST
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, first, last) = (&a[1], a[2].parse::<usize>().unwrap(), a[3].parse::<usize>().unwrap());
    let root = std::path::Path::new(".");
    let mut s = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap());
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let t0: u64 = files[0].file_stem().unwrap().to_string_lossy().split('_').nth(1).unwrap().parse().unwrap();
    for (i, f) in files.iter().enumerate().take(last + 1) {
        let ms: u64 = f.file_stem().unwrap().to_string_lossy().split('_').nth(1).unwrap().parse().unwrap();
        let img = image::open(f).unwrap();
        let snap = s.scan(&img, ms - t0);
        if i >= first { let r = s.regions(); let b = |id: &str| r.iter().find(|x| x.id == id).map(|x| (x.bounds.x, x.bounds.y, x.bounds.w, x.bounds.h)); println!("{i} {:?} clock={:?} map={:?} ctx={:?}", snap.screen, snap.game_clock_secs, b("minimap"), b("game_context")); }
    }
}
