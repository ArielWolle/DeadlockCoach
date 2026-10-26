fn main(){
    let reader=coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    let scanner=coach_vision::scan::Scanner::load(std::path::Path::new(".")).unwrap();
    let mut auto=coach_vision::auto::AutoScanner::new(scanner,reader);
    for p in std::env::args().skip(1){let image=image::open(&p).unwrap();auto.reset();
        let start=std::time::Instant::now();auto.scan(&image,0);let s=auto.scan(&image,5000);
        println!("{p}: screen={:?} clock={:?} souls={:?} hp={:?}/{:?} elapsed={:?} boxes={:?}",s.screen,s.game_clock_secs,s.souls,s.hp,s.max_hp,start.elapsed(),auto.regions());
        let mut window=image::RgbaImage::from_pixel(image.width()+400,image.height()+300,image::Rgba([25,25,25,255]));
        image::imageops::overlay(&mut window,&image.to_rgba8(),180,90);auto.reset();
        let window=image::DynamicImage::ImageRgba8(window);auto.scan(&window,0);let s=auto.scan(&window,5000);
        println!("window: screen={:?} clock={:?} souls={:?} hp={:?}/{:?} boxes={:?}",s.screen,s.game_clock_secs,s.souls,s.hp,s.max_hp,auto.regions());
        let scaled=image.resize_exact(image.width()*5/4,image.height()*5/4,image::imageops::FilterType::Triangle);
        let mut window=image::RgbaImage::from_pixel(scaled.width()+500,scaled.height()+350,image::Rgba([25,25,25,255]));image::imageops::overlay(&mut window,&scaled.to_rgba8(),220,120);auto.reset();
        let window=image::DynamicImage::ImageRgba8(window);auto.scan(&window,0);let s=auto.scan(&window,5000);println!("scaled window: screen={:?} clock={:?} souls={:?} boxes={:?}",s.screen,s.game_clock_secs,s.souls,auto.regions());
    }
}
