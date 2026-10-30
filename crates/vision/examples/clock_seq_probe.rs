//! Run the automatic scanner over the first frames of a recorded directory and print the
//! reported clock, screen and death / revive flags per frame.
//! Usage: clock_seq_probe DIR [FROM] [COUNT]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader);
    let from: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let count: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let ts = |p: &std::path::Path| -> u64 { p.file_stem().and_then(|s| s.to_str()?.split('_').nth(1)?.parse().ok()).unwrap_or(0) };
    let t0 = files.first().map(|p| ts(p)).unwrap_or(0);
    for p in files.iter().skip(from).take(count) {
        let snap = auto.scan(&image::open(p).unwrap(), ts(p) - t0);
        let c = snap.game_clock_secs.map(|c| format!("{}:{:02}", c / 60, c % 60)).unwrap_or("--".into());
        println!("{} {} {:?} dead={} revived={}", p.file_name().unwrap().to_string_lossy(), c, snap.screen, snap.dead, snap.revived);
    }
}
