// Usage: mmdbg <out.png> <frame.jpg>...   draws colour-mask blobs over the enlarged minimap.
use coach_vision::minimap::*;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = &a[0];
    let files = &a[1..];
    let k = 3u32;
    let cols = files.len().min(2) as u32;
    let rows = (files.len() as u32 + cols - 1) / cols;
    let mut sheet = image::RgbImage::new(335 * k * cols, 350 * k * rows);
    for (i, f) in files.iter().enumerate() {
        let img = image::open(f).unwrap();
        let c = crop_from_frame(&img);
        let big = image::imageops::resize(&c, c.width() * k, c.height() * k, image::imageops::FilterType::Nearest);
        let mut canvas = big.clone();
        let m = masks(&c);
        let sets: [(&Vec<bool>, [u8; 3]); 5] = [(&m.white, [255, 255, 255]), (&m.red, [255, 0, 0]), (&m.gold, [255, 255, 0]), (&m.cyan, [0, 255, 255]), (&m.green, [0, 255, 0])];
        for (mask, col) in sets {
            for b in components(mask, m.w, m.h) {
                if b.area < 3 { continue; }
                let (x0, y0, x1, y1) = (b.min_x * k, b.min_y * k, (b.max_x + 1) * k, (b.max_y + 1) * k);
                for x in x0..x1.min(canvas.width()) { for y in [y0, y1.saturating_sub(1)] { if y < canvas.height() { canvas.put_pixel(x, y, image::Rgb(col)); } } }
                for y in y0..y1.min(canvas.height()) { for x in [x0, x1.saturating_sub(1)] { if x < canvas.width() { canvas.put_pixel(x, y, image::Rgb(col)); } } }
            }
        }
        image::imageops::overlay(&mut sheet, &canvas, ((i as u32 % cols) * 335 * k) as i64, ((i as u32 / cols) * 350 * k) as i64);
    }
    sheet.save(out).unwrap();
}
