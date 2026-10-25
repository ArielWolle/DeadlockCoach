fn main() {
    let f = coach_capture::capture_frame(1600, 80).expect("capture");
    std::fs::write("snap.jpg", &f.jpeg).unwrap();
    println!("{} {}x{} scale={} bytes={}", f.source, f.meta.width, f.meta.height, f.meta.dpi_scale, f.jpeg.len());
}
