use coach_vision::minimap::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let img = image::open(&a[1]).unwrap();
    let c = crop_from_frame(&img);
    let k = 3u32;
    let mut canvas = image::imageops::resize(&c, c.width() * k, c.height() * k, image::imageops::FilterType::Lanczos3);
    let s = c.width() as f32 / CW;
    let cols = [[0u8, 255, 0], [0, 200, 255], [255, 255, 0]];
    for (li, lane) in lanes().iter().enumerate() {
        for w in lane.windows(2) {
            let (x0, y0, x1, y1) = (w[0].0, w[0].1, w[1].0, w[1].1);
            let n = ((x1 - x0).hypot(y1 - y0) * 2.0) as i32 + 1;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                let (x, y) = ((x0 + (x1 - x0) * t) * s * k as f32, (y0 + (y1 - y0) * t) * s * k as f32);
                if x >= 0.0 && y >= 0.0 && (x as u32) < canvas.width() && (y as u32) < canvas.height() { canvas.put_pixel(x as u32, y as u32, image::Rgb(cols[li])); }
            }
        }
    }
    for (x, y) in structures().iter() {
        let (px, py) = ((x * s * k as f32) as i32, (y * s * k as f32) as i32);
        for d in -4..=4 { for (dx, dy) in [(d, 0), (0, d)] {
            let (xx, yy) = (px + dx, py + dy);
            if xx >= 0 && yy >= 0 && (xx as u32) < canvas.width() && (yy as u32) < canvas.height() { canvas.put_pixel(xx as u32, yy as u32, image::Rgb([255, 0, 255])); }
        }}
    }
    canvas.save(&a[0]).unwrap();
}
