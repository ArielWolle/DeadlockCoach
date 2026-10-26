use coach_vision::minimap::*;
fn main() {
    for f in std::env::args().skip(1) {
        let img = crop_from_frame(&image::open(&f).unwrap());
        let m = masks(&img);
        println!("{f}");
        for b in components(&m.cyan, m.w, m.h) {
            if b.area > 10 {
                println!("cyan {:.1},{:.1} {}x{} area={} fill={:.2}", b.cx, b.cy, b.w(), b.h(), b.area, b.fill());
            }
        }
    }
}
