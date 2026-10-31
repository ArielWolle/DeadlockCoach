//! Unstable Rift reads for frames of a recording (full 16:9 frames): panel gate and its three
//! recognised lines, the announcement gate and halves, the minimap site pings.
//! Usage: rift_probe DIR FIRST LAST [STEP]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&a[0]).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jpg")).collect();
    files.sort();
    let (first, last): (usize, usize) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let step: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    for i in (first..=last.min(files.len() - 1)).step_by(step) {
        let frame = image::open(&files[i]).unwrap();
        let canon = frame.resize_exact(1600, 900, image::imageops::FilterType::Triangle);
        let view = coach_vision::hudtext::View { canon: &canon, source: Some((&frame, coach_perception::Bounds { x: 0, y: 0, w: frame.width(), h: frame.height() })) };
        let panel = coach_vision::rift::panel_crops(&view).map(|c| reader.read_lines(&c).unwrap_or_default());
        let parsed = panel.as_ref().map(|l| coach_vision::rift::interpret(l, coach_vision::rift::outcome_pixels(&view).0));
        let ann = coach_vision::rift::announce_crops(&view).map(|c| reader.read_lines(&c).unwrap_or_default());
        let says = ann.as_ref().is_some_and(|l| coach_vision::rift::says_rift_opening(&l[0], &l[1]));
        let map = coach_vision::minimap::crop_from_frame(&canon);
        let pings = coach_vision::rift::site_pings(&map);
        println!("{i} {} dy={:?} panel={panel:?} -> {parsed:?} ann={ann:?} says={says} red={} pings=[{:.0},{:.0}]", files[i].file_name().unwrap().to_string_lossy(), coach_vision::rift::panel_offset(&view), format!("{:?}", coach_vision::rift::outcome_pixels(&view)), pings[0], pings[1]);
    }
}
