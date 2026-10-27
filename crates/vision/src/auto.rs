//! Automatic, locator-first adapter. Full windows are inputs; no user viewport is required.
//! A registered game-area estimate is only a compatibility search hint. Published located
//! boxes come from the individual template/text detectors, never from that estimate.
use coach_core::GameSnapshot;
use coach_perception::{Bounds,RegionObservation,RegionOrigin};
use crate::{localize::{MapLocator,locate_clock_limited,locate_currency_limited,currency_value,detected_map},ocr::Reader,scan::Scanner};
use coach_perception::track::{LineBatch,Tracked};
use image::DynamicImage;

/// Clamp a widened box to the frame.
fn widen(frame:&DynamicImage,b:Bounds,left:f32,right:f32,vertical:f32)->DynamicImage{
    let (l,r,v)=((b.w as f32*left) as u32,(b.w as f32*right) as u32,(b.h as f32*vertical) as u32);
    let x=b.x.saturating_sub(l);let y=b.y.saturating_sub(v);
    frame.crop_imm(x,y,(b.w+l+r).min(frame.width()-x).max(1),(b.h+2*v).min(frame.height()-y).max(1))
}
/// The four calibrated clock read boxes (see `screen::clock_lines`), expressed relative to
/// the located glyph centre in units of glyph height, so they scale with any window size.
fn clock_crops(frame:&DynamicImage,b:Bounds)->Vec<DynamicImage>{
    let (cx,cy,h)=(b.x as f32+b.w as f32/2.,b.y as f32+b.h as f32/2.,b.h.max(4) as f32);
    [(-2.45,-0.8,5.0,1.6),(-2.95,-0.4,6.0,2.0),(-3.45,-0.6,7.0,2.2),(-2.95,-0.8,6.0,1.8)].into_iter().map(|(dx,dy,w,hh):(f32,f32,f32,f32)|{
        let x=(cx+dx*h).max(0.) as u32;let y=(cy+dy*h).max(0.) as u32;
        frame.crop_imm(x.min(frame.width()-1),y.min(frame.height()-1),((w*h) as u32).min(frame.width()-x.min(frame.width()-1)).max(1),((hh*h) as u32).min(frame.height()-y.min(frame.height()-1)).max(1))
    }).collect()
}
fn observation(id:&str,label:&str,b:Bounds,path:&str)->RegionObservation{
    RegionObservation{id:id.into(),label:label.into(),bounds:b,origin:RegionOrigin::Detected,score:None,value_path:Some(path.into())}
}

pub struct AutoScanner {legacy:Scanner,reader:Reader,map:MapLocator,clock:crate::parse::Tracker,regions:Vec<RegionObservation>,previous_map:Option<Bounds>,
    clock_box:Tracked,currency_box:Tracked,map_tracker:crate::minimap::Tracker,
    /// Per-phase timing of the most recent scan (includes the nested scanner).
    pub laps:coach_perception::track::Laps,had_context:bool}
impl AutoScanner {
    pub fn new(legacy:Scanner,reader:Reader)->Self{Self{legacy,reader,map:MapLocator::default(),clock:Default::default(),regions:vec![],previous_map:None,
        clock_box:Tracked::default(),currency_box:Tracked::default(),map_tracker:Default::default(),laps:Default::default(),had_context:false}}
    pub fn reset(&mut self){self.legacy.reset();self.clock=Default::default();self.regions.clear();self.previous_map=None;self.clock_box.reset();self.currency_box.reset();self.map_tracker=Default::default();self.had_context=false;}
    pub fn regions(&self)->Vec<RegionObservation>{self.regions.clone()}
    pub fn scan(&mut self,frame:&DynamicImage,now:u64)->GameSnapshot{
        self.regions.clear();
        let mut laps=coach_perception::track::Laps::default();
        let map=self.previous_map.and_then(|b|self.map.track(frame,b)).or_else(||self.map.locate(frame));self.previous_map=map.map(|m|m.bounds);
        if let Some(m)=map{self.regions.push(detected_map(m));}
        laps.lap("map track/locate");
        // Steady state: located clock/currency boxes are read in one recognition batch.
        // Overlapping clock crops tolerate minute-width changes (9:59 -> 10:00) by voting.
        let mut batch=LineBatch::default();
        // Inside a game context the clock is read on the canonical image; the window-scale
        // clock is only a fallback when no context formed on the previous frame.
        let clock_lines=(!self.had_context).then_some(()).and(self.clock_box.bounds).filter(|b|b.x+b.w<=frame.width()&&b.y+b.h<=frame.height())
            .map(|b|batch.add_all(clock_crops(frame,b)));
        let read_clock=clock_lines.is_some();
        // Tight box first (measured most reliable at any scale); slight right padding as a
        // backup. Both require the "SOULS" label, so a widened number fails cleanly -> re-acquire.
        let currency_line=self.currency_box.bounds.filter(|b|b.x+b.w<=frame.width()&&b.y+b.h<=frame.height())
            .map(|b|batch.add_all([widen(frame,b,0.,0.,0.),widen(frame,b,0.05,0.2,0.1)]));
        let lines=batch.run(|c|self.reader.read_lines(c));
        laps.lap("auto line batch");
        let expected=self.clock.expected_clock(now);
        let mut clock=clock_lines.and_then(|r|crate::screen::vote_clock(&lines[r],expected)).zip(self.clock_box.bounds)
            .map(|(v,b)|(v,observation("clock",&format!("{}:{:02} · tracked",v/60,v%60),b,"/game_clock_secs")));
        let mut currency=currency_line.and_then(|r|lines[r].iter().find_map(|l|currency_value(l))).zip(self.currency_box.bounds)
            .map(|(v,b)|(v,observation("souls","Souls · tracked",b,"/souls")));
        if read_clock{match &clock{Some((_,r))=>self.clock_box.hit(r.bounds),None=>{self.clock_box.miss();}}}
        match &currency{Some((_,r))=>self.currency_box.hit(r.bounds),None=>{self.currency_box.miss();}}
        // Acquisition (text detection) only for lost elements, each throttled to once per 2 s.
        let budget=if map.is_some(){6}else{2};
        if clock.is_none()&&!self.had_context&&self.clock_box.may_acquire(now,2000){
            let hint=map.map(|m|Bounds{x:(m.bounds.x as f32-m.bounds.w as f32*455./335.).max(0.) as u32,
                y:(m.bounds.y as f32-m.bounds.h as f32*505./350.).max(0.) as u32,
                w:(m.bounds.w as f32*160./335.).max(60.) as u32,h:(m.bounds.h as f32*50./350.).max(24.) as u32});
            clock=locate_clock_limited(frame,&self.reader,hint,budget);
            if let Some((_,r))=&clock{self.clock_box.hit(r.bounds);}
        }
        if currency.is_none()&&self.currency_box.may_acquire(now,2000){
            currency=locate_currency_limited(frame,&self.reader,budget);
            if let Some((_,r))=&currency{self.currency_box.hit(r.bounds);}
        }
        laps.lap("auto acquisition");
        if let Some((_,r))=&currency{self.regions.push(r.clone());}
        if let Some((_,r))=&clock{self.regions.push(r.clone());}
        let mut independent=GameSnapshot{screen:Some("Unknown".into()),..Default::default()};
        independent.game_clock_secs=self.clock.accept_clock(clock.as_ref().map(|(n,_)|*n),now);
        independent.souls=currency.as_ref().and_then(|(n,_)|self.clock.accept_souls(Some(*n)));
        // The independent minimap analysis is only needed when no game context forms (below);
        // inside a context the compatibility scanner analyzes the same crop once.
        macro_rules! independent { () => {{
            if let Some(m)=map{let crop=frame.crop_imm(m.bounds.x,m.bounds.y,m.bounds.w,m.bounds.h).to_rgb8();independent.minimap=Some(self.map_tracker.observe(crate::minimap::Analyzer::new().analyze(&crop),now));}
            laps.lap("independent minimap");self.laps=laps;self.had_context=false;
            return independent;
        }}}
        // The game context needs a landmark-verified map and a located currency label. The clock
        // is then read inside the canonical 1600x900 context (measured more reliable than at an
        // arbitrary window scale); the auto-tracked clock is the fallback outside a context.
        let (Some(map),Some((souls,currency_box)))=(map,currency)else{independent!()};
        let clock_box=clock.as_ref().map(|(_,r)|r.bounds);
        let width=map.bounds.w as f32*1600./335.;let height=map.bounds.h as f32*900./350.;
        let x=map.bounds.x as f32-width*1255./1600.;let y=map.bounds.y as f32-height*525./900.;
        if x < -width*0.04 || y < -height*0.04 || x+width>frame.width() as f32+width*0.04 || y+height>frame.height() as f32+height*0.04 {
            independent!();
        }
        let left=x.max(0.) as u32;let top=y.max(0.) as u32;
        let w=(width as u32).min(frame.width()-left);let h=(height as u32).min(frame.height()-top);
        if w<320||h<180{independent!();}
        let within=|b:Bounds|b.x>=left.saturating_sub(10)&&b.y>=top.saturating_sub(10)&&b.x+b.w<=left+w+10&&b.y+b.h<=top+h+10;
        if !within(currency_box.bounds)||clock_box.is_some_and(|b|!within(b)){independent!();}
        // Box-filter downsampling is ~2x faster than a triangle filter for the common case of a
        // larger-than-1600 window; smaller sources still upsample smoothly.
        // A near-1:1 context (within 2%) is cropped pixel-exact: resampling by ~0.1% blurs the
        // small HUD digits enough to read 5:13 as 6:13 (measured on a letterboxed screenshot).
        let near=(w as f32/1600.-1.).abs()<=0.02&&(h as f32/900.-1.).abs()<=0.02;
        let (cx,cy)=(left+w/2,top+h/2);
        let exact=near.then(||(cx.checked_sub(800),cy.checked_sub(450))).and_then(|(x,y)|x.zip(y)).filter(|(x,y)|x+1600<=frame.width()&&y+900<=frame.height());
        // Keep region mapping consistent with the pixels actually analyzed.
        let (left,top,w,h)=exact.map(|(x,y)|(x,y,1600,900)).unwrap_or((left,top,w,h));
        let mut canonical=match exact{
            Some((x,y))=>frame.crop_imm(x,y,1600,900).to_rgba8(),
            None=>{let area=frame.crop_imm(left,top,w,h);if w>=1600&&h>=900{area.thumbnail_exact(1600,900).to_rgba8()}else{area.resize_exact(1600,900,image::imageops::FilterType::Triangle).to_rgba8()}}
        };
        // If the clock has moved within a custom HUD, preserve its detected glyphs at the
        // compatibility reader's clock anchor; its exposed box remains the original one.
        if let Some(b)=clock_box {
            let relative_x=(b.x as f32-left as f32)*1600./w as f32;
            let relative_y=(b.y as f32-top as f32)*900./h as f32;
            if !(740.0..840.0).contains(&relative_x)||!(5.0..35.0).contains(&relative_y){
                let glyph=frame.crop_imm(b.x,b.y,b.w,b.h).resize_exact((b.w as f32*1600./w as f32).round().max(1.) as u32,(b.h as f32*900./h as f32).round().max(1.) as u32,image::imageops::FilterType::Triangle).to_rgba8();
                for yy in 8..41{for xx in 750..850{canonical.put_pixel(xx,yy,image::Rgba([0,0,0,255]));}}
                image::imageops::overlay(&mut canonical,&glyph,800-glyph.width() as i64/2,20-glyph.height() as i64/2);
            }
        }
        laps.lap("canonical resample");
        let map_image=frame.crop_imm(map.bounds.x,map.bounds.y,map.bounds.w,map.bounds.h).to_rgb8();
        // The window-scale glyph read is independent of the context crop's pixel offset, so it
        // lets the scene's clock verifier skip the expensive glyph search when they agree.
        self.legacy.clock_hint=clock.as_ref().map(|(n,_)|*n);
        let mut snapshot=self.legacy.scan_located(&DynamicImage::ImageRgba8(canonical),Some(&map_image),now,None,Some(souls));
        laps.absorb("scene: ",&self.legacy.laps);
        if snapshot.screen.as_deref()==Some("Unknown"){independent!();}
        self.regions.push(RegionObservation{id:"game_context".into(),label:"Registered game context · compatibility".into(),bounds:Bounds{x:left,y:top,w,h},origin:RegionOrigin::Configured,score:None,value_path:None});
        if snapshot.game_clock_secs.is_none(){snapshot.game_clock_secs=independent.game_clock_secs;}
        if !snapshot.dead{snapshot.souls=independent.souls;}else{if let Some(r)=self.regions.iter_mut().find(|r|r.id=="souls"){r.value_path=None;r.label.push_str(" · ownership unconfirmed while spectating");}}
        for mut r in self.legacy.regions(){r.bounds=Bounds{x:left+(r.bounds.x as f32/1600.*w as f32).round() as u32,
            y:top+(r.bounds.y as f32/900.*h as f32).round() as u32,w:(r.bounds.w as f32/1600.*w as f32).round().max(1.) as u32,h:(r.bounds.h as f32/900.*h as f32).round().max(1.) as u32};self.regions.push(r);}
        // Compatibility-only boxes are explicitly not exposed as located detections.
        for (id,label,r) in [("health_search","Health search · compatibility",crate::Region{x:350./1600.,y:575./900.,w:145./1600.,h:100./900.}),
            ("max_health_search","Max HP search · compatibility",crate::Region{x:145./1600.,y:675./900.,w:110./1600.,h:45./900.})] {
            self.regions.push(RegionObservation{id:id.into(),label:label.into(),bounds:Bounds{x:left+(r.x*w as f32) as u32,y:top+(r.y*h as f32) as u32,w:(r.w*w as f32) as u32,h:(r.h*h as f32) as u32},origin:RegionOrigin::Configured,score:None,value_path:Some(if id=="health_search"{"/hp"}else{"/max_hp"}.into())});
        }
        self.laps=laps;self.had_context=true;
        snapshot
    }
}
