//! Inspect independently detected map, currency and clock boxes on supplied images.
fn main(){
    let locator=coach_vision::localize::MapLocator::default();
    let reader=coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for p in std::env::args().skip(1){let frame=image::open(&p).unwrap();let map=locator.locate(&frame);println!("{p}: {:?}",map);
        let hint=map.map(|m|coach_perception::Bounds{x:(m.bounds.x as f32-m.bounds.w as f32*455./335.).max(0.) as u32,y:(m.bounds.y as f32-m.bounds.h as f32*505./350.).max(0.) as u32,w:(m.bounds.w as f32*160./335.) as u32,h:(m.bounds.h as f32*50./350.) as u32});
        println!("currency={:?}",coach_vision::localize::locate_currency(&frame,&reader));println!("clock={:?}",coach_vision::localize::locate_clock(&frame,&reader,hint));}
}
