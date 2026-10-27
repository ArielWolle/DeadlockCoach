use coach_vision::{auto::AutoScanner,scan::Scanner,ocr::Reader};
use image::{DynamicImage,RgbaImage,Rgba,imageops::FilterType};
fn scanner()->AutoScanner{let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");AutoScanner::new(Scanner::load(&root).unwrap(),Reader::load(&root.join("data/models")).unwrap())}
fn reference()->DynamicImage{image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/frames/00059_1792966931596.jpg")).unwrap()}

#[test]
fn full_window_discovery_does_not_require_video_dimensions_or_offsets(){
    let source=reference();let scaled=source.resize_exact(2000,1125,FilterType::Triangle);
    let mut window=RgbaImage::from_pixel(2500,1500,Rgba([25,25,25,255]));image::imageops::overlay(&mut window,&scaled.to_rgba8(),220,120);
    // A still frame at the live one-second cadence; the clock is wall-time extrapolated, so
    // a still image sampled 5 s later would correctly read as 318 rather than 313.
    let window=DynamicImage::ImageRgba8(window);let mut auto=scanner();auto.scan(&window,0);let s=auto.scan(&window,1000);
    assert!(matches!(s.game_clock_secs,Some(313|314)),"{:?}",s.game_clock_secs);assert_eq!(s.souls,Some(1732));
    let regions=auto.regions();let map=regions.iter().find(|r|r.id=="minimap").unwrap();
    assert!(map.bounds.x.abs_diff(1789)<20,"{:?}",map);assert!(map.bounds.y.abs_diff(776)<20,"{:?}",map);
    let money=regions.iter().find(|r|r.id=="souls").unwrap();assert!(money.bounds.x>280&&money.bounds.x<400);
    assert_eq!(money.origin,coach_perception::RegionOrigin::Detected);
}
#[test]
fn crowded_minimap_is_located_and_team_health_reaches_the_snapshot(){
    // Heroes on the map edges lowered the template score and a small patch near the ability bar
    // used to win: the frame produced no snapshot at all.
    let frame=image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/frames/00401_health-mixed.jpg")).unwrap();
    let mut auto=scanner();auto.scan(&frame,0);let s=auto.scan(&frame,1000);
    assert_eq!(s.screen.as_deref(),Some("Gameplay"));assert!(matches!(s.game_clock_secs,Some(293|294)),"{:?}",s.game_clock_secs);
    let map=auto.regions().into_iter().find(|r|r.id=="minimap").unwrap();assert!(map.bounds.x.abs_diff(1255)<8&&map.bounds.w.abs_diff(335)<10,"{map:?}");
    let t=&s.hero_status;assert_eq!(t.len(),12);
    let hp=|i:usize|t[i].health_ratio.unwrap_or(-1.0);
    assert!(hp(0)<0.3&&hp(4)>0.9&&(hp(3)-0.66).abs()<0.1,"{:?}",t.iter().map(|h|h.health_ratio).collect::<Vec<_>>());
    assert_eq!(t[7].bar_visible,Some(false));assert_eq!(t[10].bar_visible,Some(false));assert!(t[7].dead!=Some(true));
    assert_eq!(t[11].dead,Some(true));assert_eq!(t[11].respawn_secs,Some(2));
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
fn letterboxed_native_pixels_are_analyzed_without_resampling_blur(){
    // Native 1600x900 game pixels with black bars: the context must be cropped pixel-exact.
    let mut framed=RgbaImage::from_pixel(1600,1000,Rgba([0,0,0,255]));image::imageops::overlay(&mut framed,&reference().to_rgba8(),0,50);
    let framed=DynamicImage::ImageRgba8(framed);let mut auto=scanner();auto.scan(&framed,0);let s=auto.scan(&framed,1000);
    assert!(matches!(s.game_clock_secs,Some(313|314)),"{:?}",s.game_clock_secs);assert_eq!(s.souls,Some(1732));
}
#[test]
fn soft_video_minute_misreads_never_move_the_verified_clock(){
    // A sharp frame anchors the clock (detector-verified). Then a softened video frame whose
    // cheap line reads say 6:13 (measured) must not move the value between verifications.
    let sharp=reference();
    let soft=sharp.resize_exact(1280,720,FilterType::Triangle).resize_exact(1600,900,FilterType::Triangle);
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let mut scanner=Scanner::load(&root).unwrap();
    assert_eq!(scanner.scan_image(&sharp,None,0).game_clock_secs,Some(313));
    for t in [500u64,1000,1500]{
        let v=scanner.scan_image(&soft,None,t).game_clock_secs;
        assert!(matches!(v,Some(313|314)),"t={t}: {v:?}");
    }
}
#[test]
fn moving_the_map_does_not_require_moving_currency_or_the_clock(){
    let source=reference();let map=source.crop_imm(1255,525,335,350).to_rgba8();let mut changed=source.to_rgba8();
    for y in 525..875{for x in 1255..1590{changed.put_pixel(x,y,Rgba([0,0,0,255]));}}
    image::imageops::overlay(&mut changed,&map,930,300);let changed=DynamicImage::ImageRgba8(changed);let mut auto=scanner();auto.scan(&changed,0);let s=auto.scan(&changed,1000);
    assert!(matches!(s.game_clock_secs,Some(313|314)),"{:?}",s.game_clock_secs);assert_eq!(s.souls,Some(1732));
    let r=auto.regions().into_iter().find(|r|r.id=="minimap").unwrap();assert!(r.bounds.x.abs_diff(930)<10&&r.bounds.y.abs_diff(300)<10,"{r:?}");
}
