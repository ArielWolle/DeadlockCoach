//! Minimap locator diagnostics for a verified box: hold score, track and fresh locate.
//! Usage: map_hold_probe DIR FIRST LAST X Y W H
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let n: Vec<u32> = a[1..].iter().map(|s| s.parse().unwrap()).collect();
    let b = coach_perception::Bounds { x: n[2], y: n[3], w: n[4], h: n[5] };
    let map = coach_vision::localize::MapLocator::default();
    for i in n[0] as usize..=(n[1] as usize).min(files.len() - 1) {
        let f = image::open(&files[i]).unwrap();
        println!("{i} hold={:?} track={:?} locate={:?}", map.hold(&f, b, 0.0).map(|l| l.score), map.track(&f, b).map(|l| l.score), map.locate(&f).map(|l| (l.bounds, l.score)));
    }
}
