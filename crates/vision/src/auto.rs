//! Automatic, locator-first adapter. Full windows are inputs; no user viewport is required.
//! A registered game-area estimate is only a compatibility search hint. Published located
//! boxes come from the individual template/text detectors, never from that estimate.
use coach_core::GameSnapshot;
use coach_perception::{Bounds,RegionObservation,RegionOrigin};
use crate::{localize::{MapLocator,locate_clock,locate_currency,track_currency,detected_map},ocr::Reader,scan::Scanner};
use image::DynamicImage;

pub struct AutoScanner {legacy:Scanner,reader:Reader,map:MapLocator,clock:crate::parse::Tracker,regions:Vec<RegionObservation>,previous_map:Option<Bounds>,previous_currency:Option<Bounds>,map_tracker:crate::minimap::Tracker}
impl AutoScanner {
    pub fn new(legacy:Scanner,reader:Reader)->Self{Self{legacy,reader,map:MapLocator::default(),clock:Default::default(),regions:vec![],previous_map:None,previous_currency:None,map_tracker:Default::default()}}
    pub fn reset(&mut self){self.legacy.reset();self.clock=Default::default();self.regions.clear();self.previous_map=None;self.previous_currency=None;self.map_tracker=Default::default();}
    pub fn regions(&self)->Vec<RegionObservation>{self.regions.clone()}
    pub fn scan(&mut self,frame:&DynamicImage,now:u64)->GameSnapshot{
        self.regions.clear();
        let map=self.previous_map.and_then(|b|self.map.track(frame,b)).or_else(||self.map.locate(frame));self.previous_map=map.map(|m|m.bounds);
        if let Some(m)=map{self.regions.push(detected_map(m));}
        let currency=self.previous_currency.and_then(|b|track_currency(frame,&self.reader,b)).or_else(||locate_currency(frame,&self.reader));self.previous_currency=currency.as_ref().map(|(_,r)|r.bounds);
        if let Some((_,r))=&currency{self.regions.push(r.clone());}
        let hint=map.map(|m|Bounds{x:(m.bounds.x as f32-m.bounds.w as f32*455./335.).max(0.) as u32,
            y:(m.bounds.y as f32-m.bounds.h as f32*505./350.).max(0.) as u32,
            w:(m.bounds.w as f32*160./335.).max(60.) as u32,h:(m.bounds.h as f32*50./350.).max(24.) as u32});
        let clock=locate_clock(frame,&self.reader,hint);
        if let Some((_,r))=&clock{self.regions.push(r.clone());}
        let mut independent=GameSnapshot{screen:Some("Unknown".into()),..Default::default()};
        independent.game_clock_secs=self.clock.accept_clock(clock.as_ref().map(|(n,_)|*n),now);
        independent.souls=currency.as_ref().and_then(|(n,_)|self.clock.accept_souls(Some(*n)));
        if let Some(m)=map {let crop=frame.crop_imm(m.bounds.x,m.bounds.y,m.bounds.w,m.bounds.h).to_rgb8();independent.minimap=Some(self.map_tracker.observe(crate::minimap::Analyzer::new().analyze(&crop),now));}
        let (Some(map),Some((time,clock_box)),Some((souls,currency_box)))=(map,clock,currency)else{return independent;};
        let width=map.bounds.w as f32*1600./335.;let height=map.bounds.h as f32*900./350.;
        let x=map.bounds.x as f32-width*1255./1600.;let y=map.bounds.y as f32-height*525./900.;
        if x < -width*0.04 || y < -height*0.04 || x+width>frame.width() as f32+width*0.04 || y+height>frame.height() as f32+height*0.04 {
            return independent;
        }
        let left=x.max(0.) as u32;let top=y.max(0.) as u32;
        let w=(width as u32).min(frame.width()-left);let h=(height as u32).min(frame.height()-top);
        if w<320||h<180{return independent;}
        let within=|b:Bounds|b.x>=left.saturating_sub(10)&&b.y>=top.saturating_sub(10)&&b.x+b.w<=left+w+10&&b.y+b.h<=top+h+10;
        if !within(clock_box.bounds)||!within(currency_box.bounds){return independent;}
        let mut canonical=frame.crop_imm(left,top,w,h).resize_exact(1600,900,image::imageops::FilterType::Triangle).to_rgba8();
        // If the clock has moved within a custom HUD, preserve its detected glyphs at the
        // compatibility reader's clock anchor; its exposed box remains the original one.
        let relative_x=(clock_box.bounds.x as f32-left as f32)*1600./w as f32;
        let relative_y=(clock_box.bounds.y as f32-top as f32)*900./h as f32;
        if !(740.0..840.0).contains(&relative_x)||!(5.0..35.0).contains(&relative_y){
            let b=clock_box.bounds;let glyph=frame.crop_imm(b.x,b.y,b.w,b.h).resize_exact((b.w as f32*1600./w as f32).round().max(1.) as u32,(b.h as f32*900./h as f32).round().max(1.) as u32,image::imageops::FilterType::Triangle).to_rgba8();
            for yy in 8..41{for xx in 750..850{canonical.put_pixel(xx,yy,image::Rgba([0,0,0,255]));}}
            image::imageops::overlay(&mut canonical,&glyph,800-glyph.width() as i64/2,20-glyph.height() as i64/2);
        }
        let map_image=frame.crop_imm(map.bounds.x,map.bounds.y,map.bounds.w,map.bounds.h).to_rgb8();
        let mut snapshot=self.legacy.scan_located(&DynamicImage::ImageRgba8(canonical),Some(&map_image),now,time,souls);
        if snapshot.screen.as_deref()==Some("Unknown"){return independent;}
        snapshot.game_clock_secs=independent.game_clock_secs;
        if !snapshot.dead{snapshot.souls=independent.souls;}else{if let Some(r)=self.regions.iter_mut().find(|r|r.id=="souls"){r.value_path=None;r.label.push_str(" · ownership unconfirmed while spectating");}}
        for mut r in self.legacy.regions(){r.bounds=Bounds{x:left+(r.bounds.x as f32/1600.*w as f32).round() as u32,
            y:top+(r.bounds.y as f32/900.*h as f32).round() as u32,w:(r.bounds.w as f32/1600.*w as f32).round().max(1.) as u32,h:(r.bounds.h as f32/900.*h as f32).round().max(1.) as u32};self.regions.push(r);}
        // Compatibility-only boxes are explicitly not exposed as located detections.
        for (id,label,r) in [("health_search","Health search · compatibility",crate::Region{x:350./1600.,y:575./900.,w:145./1600.,h:100./900.}),
            ("max_health_search","Max HP search · compatibility",crate::Region{x:145./1600.,y:675./900.,w:110./1600.,h:45./900.})] {
            self.regions.push(RegionObservation{id:id.into(),label:label.into(),bounds:Bounds{x:left+(r.x*w as f32) as u32,y:top+(r.y*h as f32) as u32,w:(r.w*w as f32) as u32,h:(r.h*h as f32) as u32},origin:RegionOrigin::Configured,score:None,value_path:Some(if id=="health_search"{"/hp"}else{"/max_hp"}.into())});
        }
        snapshot
    }
}
