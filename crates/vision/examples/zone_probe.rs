//! Read the minimap zone label of frames (canonical 1600x900 resample, as the scanner does).
//! Usage: zone_probe [--save DIR] FRAME...
fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    let save = if a.first().map(|s| s == "--save").unwrap_or(false) { a.remove(0); Some(a.remove(0)) } else { None };
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for f in &a {
        let img = image::open(f).unwrap();
        let canon = img.resize_exact(1600, 900, image::imageops::FilterType::Triangle).to_rgb8();
        let strip = coach_vision::zone::label_strip(&canon, (1255.0, 525.0, 335.0, 350.0));
        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy().to_string();
        let mut crops = vec![strip.clone()];
        crops.extend(coach_vision::zone::label_parts(&strip));
        let lines = reader.read_lines(&crops).unwrap();
        let zone = coach_vision::zone::zone_from_lines(&lines[1..]);
        println!("{name}\twhole={:?}\tparts={:?}\tzone={:?}", lines[0], &lines[1..], zone);
        if let Some(d) = &save { strip.save(format!("{d}/zone_{name}.png")).unwrap(); }
    }
}
