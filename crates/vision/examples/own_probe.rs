//! Trace your own-slot evidence (backdrop votes, health-gap sums, latched slot) and match resets
//! through a frame range. Usage: own_probe DIR FROM TO
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (from, to): (usize, usize) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(std::path::Path::new(".")).unwrap(), reader);
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let t0: u64 = files.first().and_then(|p| p.file_stem()?.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0);
    for (i, p) in files.iter().enumerate().take(to) {
        let ts: u64 = p.file_stem().and_then(|s| s.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0);
        let img = image::open(p).unwrap();
        let snap = auto.scan(&img, ts - t0);
        if i < from { continue; }
        let (gaps, votes, latched) = auto.scanner().own_evidence();
        let gaps: Vec<String> = gaps.iter().map(|(s, n)| if *n == 0 { "-".into() } else { format!("{:.2}/{n}", s / *n as f32) }).collect();
        println!("{i} {:?} {:?} hero={:?} votes={votes:?} gaps=[{}] latched={latched:?} resets={}", snap.screen, snap.game_clock_secs,
            snap.hero.as_deref().map(|h| h.trim_start_matches("hero_")), gaps.join(" "), auto.scanner().match_resets());
    }
}
