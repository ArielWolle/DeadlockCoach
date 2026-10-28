//! Status-effect gate + recognised lines for frames of a recording (full 16:9 frames).
//! Usage: status_probe DIR FIRST LAST [STEP]
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
        let crops = coach_vision::status::crops(&view);
        let (kinds, imgs): (Vec<_>, Vec<_>) = crops.into_iter().unzip();
        let lines = if imgs.is_empty() { vec![] } else { reader.read_lines(&imgs).unwrap_or_default() };
        let urn = coach_vision::status::urn_prompt(&view).map(|c| reader.read_lines(&[c]).unwrap_or_default());
        let banner = coach_vision::status::objective_banner(&view).map(|(c, h)| (reader.read_lines(&[c]).unwrap_or_default(), h));
        let mut vt = coach_vision::vitals::Tracker::default();
        let det = vt.read(&reader, &canon, 0).map(|r| (r.hp, r.max));
        print!("bar={:?} det={det:?} ", coach_vision::vitals::bar_ratio(&canon));
        println!("{i} kinds={kinds:?} lines={lines:?} -> {:?} urn={urn:?} banner={banner:?}", coach_vision::status::interpret(&kinds, &lines));
    }
}
