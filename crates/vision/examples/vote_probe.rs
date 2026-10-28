//! Trace hero identity votes of one top-bar slot through a frame range.
//! Usage: vote_probe DIR FROM TO SLOT
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (from, to, slot): (usize, usize, usize) = (a[1].parse().unwrap(), a[2].parse().unwrap(), a[3].parse().unwrap());
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(std::path::Path::new(".")).unwrap(), reader);
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let t0: u64 = files.first().and_then(|p| p.file_stem()?.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0);
    for (i, p) in files.iter().enumerate().take(to).skip(from) {
        let ts: u64 = p.file_stem().and_then(|s| s.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0);
        let snap = auto.scan(&image::open(p).unwrap(), ts - t0);
        let sc = auto.scanner();
        let ctx = auto.regions().into_iter().find(|r| r.id == "game_context").map(|r| (r.bounds.x, r.bounds.y, r.bounds.w, r.bounds.h));
        let mapb = auto.regions().into_iter().find(|r| r.id == "minimap").map(|r| (r.bounds.x, r.bounds.y, r.bounds.w, r.bounds.h));
        if i % 10 == 0 { println!("   ctx {ctx:?} map {mapb:?}"); }
        let mut v: Vec<_> = sc.hero_votes()[slot].iter().map(|(k, v)| (k.trim_start_matches("hero_").to_string(), *v)).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        let hero = snap.hero_status.get(slot).and_then(|s| s.hero.clone());
        let all: Vec<String> = (0..12).map(|s| { let mut v: Vec<_> = sc.hero_votes()[s].iter().collect(); v.sort_by(|a, b| b.1.total_cmp(a.1)); v.first().map(|(k, x)| format!("{}={:.0}", k.trim_start_matches("hero_"), x)).unwrap_or_default() }).collect();
        if i % 10 == 0 || (i > 515 && i < 525) { println!("{i} {:?} {:?} {:?} | {}", snap.screen, hero, v.iter().take(3).map(|(k, x)| format!("{k}={x:.1}")).collect::<Vec<_>>(), all.join(" ")); }
    }
}
