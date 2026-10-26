use coach_vision::{auto::AutoScanner,scan::Scanner,ocr::Reader};
use image::{DynamicImage,RgbaImage,Rgba,imageops::FilterType};
fn scanner()->AutoScanner{let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");AutoScanner::new(Scanner::load(&root).unwrap(),Reader::load(&root.join("data/models")).unwrap())}
fn reference()->DynamicImage{image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/frames/00059_1792966931596.jpg")).unwrap()}

#[test]
fn full_window_discovery_does_not_require_video_dimensions_or_offsets(){
    let source=reference();let scaled=source.resize_exact(2000,1125,FilterType::Triangle);
    let mut window=RgbaImage::from_pixel(2500,1500,Rgba([25,25,25,255]));image::imageops::overlay(&mut window,&scaled.to_rgba8(),220,120);
    let window=DynamicImage::ImageRgba8(window);let mut auto=scanner();auto.scan(&window,0);let s=auto.scan(&window,5000);
    assert_eq!(s.game_clock_secs,Some(313));assert_eq!(s.souls,Some(1732));
    let regions=auto.regions();let map=regions.iter().find(|r|r.id=="minimap").unwrap();
    assert!(map.bounds.x.abs_diff(1789)<20,"{:?}",map);assert!(map.bounds.y.abs_diff(776)<20,"{:?}",map);
    let money=regions.iter().find(|r|r.id=="souls").unwrap();assert!(money.bounds.x>280&&money.bounds.x<400);
    assert_eq!(money.origin,coach_perception::RegionOrigin::Detected);
}
#[test]
fn relocating_currency_independently_does_not_change_its_reader(){
    let source=reference();let glyph=source.crop_imm(60,728,170,55).to_rgba8();let mut changed=source.to_rgba8();
    for y in 720..790{for x in 60..240{changed.put_pixel(x,y,Rgba([0,0,0,255]));}}
    image::imageops::overlay(&mut changed,&glyph,550,650);let changed=DynamicImage::ImageRgba8(changed);let mut auto=scanner();auto.scan(&changed,0);let s=auto.scan(&changed,5000);
    assert_eq!(s.souls,Some(1732));let region=auto.regions().into_iter().find(|r|r.id=="souls").unwrap();assert!(region.bounds.x>=550,"{region:?}");
}
#[test]
fn a_blank_window_is_unknown_and_never_requests_manual_calibration(){
    let mut auto=scanner();let frame=DynamicImage::ImageRgba8(RgbaImage::from_pixel(640,480,Rgba([0,0,0,255])));
    let s=auto.scan(&frame,0);assert_eq!(s.screen.as_deref(),Some("Unknown"));assert!(s.game_clock_secs.is_none());assert!(auto.regions().is_empty());
}
#[test]
fn moving_the_map_does_not_require_moving_currency_or_the_clock(){
    let source=reference();let map=source.crop_imm(1255,525,335,350).to_rgba8();let mut changed=source.to_rgba8();
    for y in 525..875{for x in 1255..1590{changed.put_pixel(x,y,Rgba([0,0,0,255]));}}
    image::imageops::overlay(&mut changed,&map,930,300);let changed=DynamicImage::ImageRgba8(changed);let mut auto=scanner();auto.scan(&changed,0);let s=auto.scan(&changed,5000);
    assert_eq!(s.game_clock_secs,Some(313));assert_eq!(s.souls,Some(1732));
    let r=auto.regions().into_iter().find(|r|r.id=="minimap").unwrap();assert!(r.bounds.x.abs_diff(930)<10&&r.bounds.y.abs_diff(300)<10,"{r:?}");
}
