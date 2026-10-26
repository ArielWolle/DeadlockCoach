//! Export small map-only fixtures from the calibration recording (no desktop/name panels).
//! cargo run --release -p coach-vision --example mmfixtures -- <recording_dir>
use coach_vision::minimap::crop_from_frame;
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("recording directory"));
    let out = std::path::Path::new("samples/minimap");
    std::fs::create_dir_all(out).unwrap();
    for (index, name) in [(110, "pre-urn"), (224, "urn-bridge"), (238, "urn-pin"),
                           (241, "urn-moving"), (244, "no-urn")] {
        let prefix = format!("{index:05}_");
        let p = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok())
            .find(|e| e.path().extension().is_some_and(|e| e == "jpg")
                && e.file_name().to_string_lossy().starts_with(&prefix)).unwrap().path();
        let img = image::open(p).unwrap();
        crop_from_frame(&img).save(out.join(format!("{name}.png"))).unwrap();
    }
}
