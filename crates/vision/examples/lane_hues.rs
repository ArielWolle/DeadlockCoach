//! Which colour is the left / right side lane in a frame? Usage: lane_hues FRAME...
fn main() {
    let an = coach_vision::minimap::Analyzer::new();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let map = if img.width() == 335 { img.to_rgb8() } else { match coach_vision::localize::MapLocator::default().locate(&img) {
            Some(m) => img.crop_imm(m.bounds.x, m.bounds.y, m.bounds.w, m.bounds.h).to_rgb8(), None => coach_vision::minimap::crop_from_frame(&img) } };
        let (lg, ly, rg, ry) = an.side_lane_hues(&map);
        println!("{:40} left green {lg:5} yellow {ly:5} | right green {rg:5} yellow {ry:5}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy());
    }
}
