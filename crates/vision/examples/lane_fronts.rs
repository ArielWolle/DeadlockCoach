//! Lane fronts and trooper dots per frame. Usage: lane_fronts FRAME...
fn main() {
    let an = coach_vision::minimap::Analyzer::new();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let map = if img.width() == 335 { img.to_rgb8() } else { match coach_vision::localize::MapLocator::default().locate(&img) {
            Some(m) => img.crop_imm(m.bounds.x, m.bounds.y, m.bounds.w, m.bounds.h).to_rgb8(), None => coach_vision::minimap::crop_from_frame(&img) } };
        let st = an.analyze(&map);
        let fmt = |l: &coach_core::LaneState| format!("front {} dots {}/{}", l.push.map(|p| format!("{p:.2}")).unwrap_or("-".into()), l.ally_troopers, l.enemy_troopers);
        println!("{:36} mirrored={:?} | G {} | B {} | Y {}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy(), st.lanes_mirrored, fmt(&st.green), fmt(&st.blue), fmt(&st.yellow));
    }
}
