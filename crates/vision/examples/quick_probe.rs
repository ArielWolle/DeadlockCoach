//! Measure recognition-only health and top-bar reads before wiring the fast path.
fn main(){
    let reader=coach_vision::ocr::Reader::load(std::path::Path::new("data/models")).unwrap();
    for path in std::env::args().skip(1){let frame=image::open(&path).unwrap();
        if let Some((value,region))=coach_vision::localize::locate_clock(&frame,&reader,None){let b=region.bounds;println!("located clock {value} {b:?}: {:?}",reader.read_line(&frame.crop_imm(b.x,b.y,b.w,b.h)));}
        for(x,y,w,h)in [(685,674,120,25),(680,670,150,30)]{println!("respawn {x},{y},{w},{h}: {:?}",reader.read_line(&frame.crop_imm(x,y,w,h)));}
        for(x,y,w,h)in [(775,12,50,16),(782,15,36,12),(780,16,40,10)]{println!("clock {x},{y},{w},{h}: {:?}",reader.read_line(&frame.crop_imm(x,y,w,h)));}
        let mut tracker=coach_vision::vitals::Tracker::default();
        if let Some(v)=tracker.read(&reader,&frame,0){let b=v.regions[0].bounds;
            let start=std::time::Instant::now();println!("{path}: hp={:?} in {:?}",reader.read_line(&frame.crop_imm(b.x,b.y,b.w,b.h)),start.elapsed());}
        let start=std::time::Instant::now();
        for cx in coach_vision::hud::ALLY_CENTERS.into_iter().chain(coach_vision::hud::ENEMY_CENTERS){
            println!("{cx}: worth={:?}, status={:?}",reader.read_line(&frame.crop_imm((cx-18.) as u32,82,36,14)),
                reader.read_line(&frame.crop_imm((cx-32.) as u32,63,64,18)));
        }println!("top bar {:?}",start.elapsed());
        let start=std::time::Instant::now();println!("top spans {:?}",reader.read_spans(&frame.crop_imm(275,62,1045,43),3));println!("spans {:?}",start.elapsed());
    }
}
