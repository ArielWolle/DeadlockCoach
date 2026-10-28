//! Run the automatic scanner over frames FROM..=TO of a directory and, for each, print the
//! registered game context and the K/D/A read on that exact canonical crop.
//! Usage: context_kda DIR FROM TO
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader);
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let (from, to): (usize, usize) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let t0: u64 = files[from].file_stem().unwrap().to_str().unwrap().split('_').nth(1).unwrap().parse().unwrap();
    for i in from..=to {
        let ts: u64 = files[i].file_stem().unwrap().to_str().unwrap().split('_').nth(1).unwrap().parse().unwrap();
        let img = image::open(&files[i]).unwrap();
        let snap = auto.scan(&img, ts - t0);
        let ctx = auto.regions().into_iter().find(|r| r.id == "game_context").map(|r| r.bounds);
        let kda = ctx.map(|b| { let c = img.crop_imm(b.x, b.y, b.w, b.h); let c = if b.w >= 1600 { c.thumbnail_exact(1600, 900) } else { c.resize_exact(1600, 900, image::imageops::FilterType::Triangle) }; coach_vision::scoreboard::read_kda(&image::DynamicImage::ImageRgba8(c.to_rgba8())) });
        println!("{i} {:?} ctx={ctx:?} players={:?}", snap.screen, snap.players.iter().map(|p| (p.kills, p.deaths, p.assists)).collect::<Vec<_>>());
        if let Some(k) = kda { println!("   ctx kda {k:?}"); }
    }
}
