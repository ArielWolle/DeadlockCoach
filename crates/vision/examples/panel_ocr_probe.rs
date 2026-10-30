//! Death-panel damage / name reads under several crop treatments. Usage: panel_ocr_probe FRAME...
use coach_vision::death;
use image::{imageops::FilterType, DynamicImage};
fn up(c: &DynamicImage, k: u32) -> DynamicImage { c.resize_exact(c.width() * k, c.height() * k, FilterType::CatmullRom) }
fn inv(c: &DynamicImage) -> DynamicImage {
    let mut g = c.to_luma8();
    for p in g.pixels_mut() { p[0] = 255 - p[0]; }
    DynamicImage::ImageLuma8(g)
}
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for p in std::env::args().skip(1) {
        let src = image::open(&p).unwrap();
        let canon = src.resize_exact(1600, 900, FilterType::Triangle);
        let ctx = coach_perception::Bounds { x: 0, y: 0, w: src.width(), h: src.height() };
        let view = coach_vision::hudtext::View { canon: &canon, source: Some((&src, ctx)) };
        let size = death::panel_size(&view);
        let panel = view.region(death::PANEL_CANONICAL, size, size);
        let rows = death::row_digits(&panel);
        let k = panel.width() as f32 / death::PANEL_SIZE as f32;
        println!("{}", p.rsplit(['\\', '/']).next().unwrap());
        for (i, (b, (lo, hi))) in rows.iter().enumerate() {
            let c = death::digits_crop(&panel, *b);
            let n = death::name_box(i, k); let nc = panel.crop_imm(n.x, n.y, n.w, n.h);
            let crops = vec![c.clone(), up(&c, 2), up(&c, 3), inv(&up(&c, 3)), up(&nc, 2), inv(&up(&nc, 2))];
            let l = reader.read_lines(&crops).unwrap_or_default();
            println!("  row {i} glyphs {lo}-{hi} x={} w={} h={} | {:?}", b.x, b.w, b.h, l);
            let a = death::damage_box(i, k);
            let bl = |v: Vec<coach_vision::minimap::Blob>| v.iter().filter(|b| b.area >= 3).map(|b| format!("{}+{}x{}a{}", b.min_x, b.w(), b.h(), b.area)).collect::<Vec<_>>().join(" ");
            println!("     box x={}..{} y={}..{} white: {}", a.x, a.x + a.w, a.y, a.y + a.h, bl(coach_vision::hudtext::blobs(&panel, a, coach_vision::hudtext::white)));
            println!("     closed: {}", bl(coach_vision::hudtext::blobs_closed(&panel, a, coach_vision::hudtext::white)));
            println!("     loose: {}", bl(coach_vision::hudtext::blobs(&panel, a, |r, g, b| { let (_, s, v) = coach_vision::minimap::hsv(r, g, b); v > 0.6 && s < 0.3 })));
        }
    }
}
