//! Raw top-bar line reads per frame: team totals, the twelve per-hero net-worth labels, bar
//! readings and respawn-timer digits for every slot. Usage: topbar_probe FRAME...
fn main() {
    let reader = coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let img = image::DynamicImage::ImageRgba8(if img.width() >= 1600 { img.thumbnail_exact(1600, 900).to_rgba8() } else { img.resize_exact(1600, 900, image::imageops::FilterType::Triangle).to_rgba8() });
        let dy = coach_vision::team::top_bar_offset(&img);
        let img = coach_vision::team::top_bar_aligned(&img, dy).into_owned();
        println!("top bar offset {dy}");
        let totals: Vec<_> = [(728, 40, 44, 20), (720, 40, 50, 20), (722, 42, 48, 16), (832, 40, 44, 20), (832, 40, 48, 20), (834, 42, 44, 16)]
            .iter().map(|(x, y, w, h)| img.crop_imm(*x, *y, *w, *h)).collect();
        let mut t = reader.read_lines(&totals).unwrap();
        let inv: Vec<_> = totals.iter().map(|c| { let mut g = c.to_luma8(); image::imageops::invert(&mut g); image::DynamicImage::ImageLuma8(g) }).collect();
        t.extend(reader.read_lines(&inv).unwrap());
        let w = reader.read_lines(&coach_vision::team::networth_crops(&img)).unwrap();
        let cs: Vec<f32> = coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS).collect();
        let up = |c: image::DynamicImage, k: u32| c.resize_exact(c.width() * k, c.height() * k, image::imageops::FilterType::CatmullRom);
        let va: Vec<_> = cs.iter().map(|cx| up(img.crop_imm((*cx - 9.) as u32, 82, 16, 14), 3)).collect();
        let vb: Vec<_> = cs.iter().map(|cx| img.crop_imm((*cx - 9.) as u32, 82, 18, 14)).collect();
        let vc: Vec<_> = cs.iter().map(|cx| up(img.crop_imm((*cx - 9.) as u32, 82, 18, 14), 3)).collect();
        let a = reader.read_lines(&va).unwrap();
        let b = reader.read_lines(&vb).unwrap();
        let c = reader.read_lines(&vc).unwrap();
        println!("  up3 {a:?}\n  tight {b:?}\n  digits3 {c:?}");
        let all: Vec<usize> = (0..12).collect();
        let r = reader.read_lines(&coach_vision::team::respawn_crops(&img, &all)).unwrap();
        let rgb = img.to_rgb8();
        println!("{} totals={t:?}", std::path::Path::new(&f).file_name().unwrap().to_string_lossy());
        for i in 0..12 {
            let b = coach_vision::team::read_bar(&rgb, i);
            let cx = coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS).nth(i).unwrap();
            println!("  {}{} worth={:?} timer={:?} px={} bar={:?}", if i < 6 { "A" } else { "E" }, i % 6, w[i], r[i], coach_vision::team::timer_pixels(&rgb, cx), b);
        }
    }
}
