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
/// Visible hero portrait rings as inspector overlays, in source-frame pixels.
fn hero_regions(map:Bounds,m:&coach_core::MinimapState)->Vec<RegionObservation>{
    let r=(11.0*map.w as f32/335.0).max(2.0);
    m.heroes.iter().filter(|h|h.visible()).map(|h|{
        let (cx,cy)=(map.x as f32+h.pos.x*map.w as f32,map.y as f32+h.pos.y*map.h as f32);
        RegionObservation{id:format!("hero_{}_{}",if h.ally{"ally"}else{"enemy"},h.id),
            label:format!("{} hero ring (seen {}x{})",if h.ally{"Ally"}else{"Enemy"},h.hits,if h.confirmed(){""}else{", unconfirmed"}),
            bounds:Bounds{x:(cx-r).max(0.0) as u32,y:(cy-r).max(0.0) as u32,w:(2.0*r) as u32,h:(2.0*r) as u32},
            origin:RegionOrigin::Detected,score:None,value_path:Some("/minimap/heroes".into())}
    }).collect()
}
fn observation(id:&str,label:&str,b:Bounds,path:&str)->RegionObservation{
    RegionObservation{id:id.into(),label:label.into(),bounds:b,origin:RegionOrigin::Detected,score:None,value_path:Some(path.into())}
}

pub struct AutoScanner {legacy:Scanner,reader:Reader,map:MapLocator,clock:crate::parse::Tracker,regions:Vec<RegionObservation>,previous_map:Option<Bounds>,
    clock_box:Tracked,currency_box:Tracked,map_tracker:crate::minimap::Tracker,
    /// Per-phase timing of the most recent scan (includes the nested scanner).
    pub laps:coach_perception::track::Laps,had_context:bool,
    /// Last landmark-verified minimap box and the frame size it was found in.
    verified_map:Option<(Bounds,(u32,u32))>,
    /// Last game-context rectangle and the frame size it was formed in. Frames without a located
    /// minimap (shop, menus, effects over the map) are classified in it.
    last_context:Option<(Bounds,(u32,u32))>,
    /// When the tracked minimap box is next re-checked by a fresh search (ms).
    relocate_at:u64,
    /// A differently sized minimap find and how many searches in a row found it (see `scan`).
    map_contender:Option<(Bounds,u32)>,
    /// Match resets of the scene scanner already applied to this adapter's per-match state.
    resets_seen:u32}

/// Canonical 1600x900 image of a context rectangle (pixel-exact when within 2% of 1:1; a ~0.1%
/// resample blurred the small HUD digits enough to read 5:13 as 6:13 on a letterboxed shot).
fn canonical_of(frame:&DynamicImage,left:u32,top:u32,w:u32,h:u32)->(image::RgbaImage,Bounds){
    let near=(w as f32/1600.-1.).abs()<=0.02&&(h as f32/900.-1.).abs()<=0.02;
    let (cx,cy)=(left+w/2,top+h/2);
    let exact=near.then(||(cx.checked_sub(800),cy.checked_sub(450))).and_then(|(x,y)|x.zip(y)).filter(|(x,y)|x+1600<=frame.width()&&y+900<=frame.height());
    let (left,top,w,h)=exact.map(|(x,y)|(x,y,1600,900)).unwrap_or((left,top,w,h));
    let img=match exact{
        Some((x,y))=>frame.crop_imm(x,y,1600,900).to_rgba8(),
        None=>{let area=frame.crop_imm(left,top,w,h);if w>=1600&&h>=900{area.thumbnail_exact(1600,900).to_rgba8()}else{area.resize_exact(1600,900,image::imageops::FilterType::Triangle).to_rgba8()}}
    };
    (img,Bounds{x:left,y:top,w,h})
}
/// Template similarity needed to keep the verified map box without landmarks (normal frames
/// measured 0.52-0.61, late-game base fights 0.26-0.37, the end screen 0.0).
const MAP_HOLD_SCORE:f32=0.2;
/// How often a tracked minimap box is re-checked by a fresh search, and by how much a fresh box
/// must out-score it to replace it.
const MAP_RECHECK_MS:u64=5_000;
const MAP_SWITCH_MARGIN:f32=0.02;
/// Consecutive searches that must find a differently sized minimap before it replaces the box
/// in use.
const MAP_CONTENDER_FINDS:u32=3;
/// Minimap boxes of the same size (within 5%).
fn same_size(a:Bounds,b:Bounds)->bool{(a.w as f32/b.w.max(1) as f32-1.).abs()<=0.05&&(a.h as f32/b.h.max(1) as f32-1.).abs()<=0.05}
/// The same minimap box re-located with landmark jitter (within 3% / 3 px).
fn jitter(a:Bounds,b:Bounds)->bool{let t=(b.w as f32*0.03).max(3.);(a.x as f32-b.x as f32).abs()<=t&&(a.y as f32-b.y as f32).abs()<=t&&(a.w as f32-b.w as f32).abs()<=t&&(a.h as f32-b.h as f32).abs()<=t}
/// Count a contender find: consecutive finds of the same box, restarting on another box.
fn contend(c:&mut Option<(Bounds,u32)>,f:Bounds)->u32{
    let n=match *c{Some((b,n)) if jitter(f,b)=>n+1,_=>1};
    *c=Some((f,n));n
}
impl AutoScanner {
    pub fn new(legacy:Scanner,reader:Reader)->Self{Self{legacy,reader,map:MapLocator::default(),clock:Default::default(),regions:vec![],previous_map:None,
        clock_box:Tracked::default(),currency_box:Tracked::default(),map_tracker:Default::default(),laps:Default::default(),had_context:false,verified_map:None,relocate_at:0,last_context:None,map_contender:None,resets_seen:0}}
    pub fn reset(&mut self){self.legacy.reset();self.clock=Default::default();self.regions.clear();self.previous_map=None;self.clock_box.reset();self.currency_box.reset();self.map_tracker=Default::default();self.had_context=false;self.verified_map=None;self.last_context=None;self.map_contender=None;}
    /// A frame without a located minimap is scanned in the last context rectangle of this frame
    /// size (the HUD does not move within a match), or the whole frame when it is 16:9. The
    /// scene then decides: Shop / dead with the shop open, post-game, menus, loading, or
    /// gameplay when the clock and the own health bar are visible. None = Unknown.
    fn scan_off_map(&mut self,frame:&DynamicImage,now:u64,souls:Option<u32>)->Option<GameSnapshot>{
        let dims=(frame.width(),frame.height());
        let rect=self.last_context.filter(|(_,d)|*d==dims).map(|(b,_)|b)
            .or_else(||((dims.0 as f32/dims.1 as f32)/(16./9.)-1.).abs().le(&0.02).then_some(Bounds{x:0,y:0,w:dims.0,h:dims.1}))?;
        if rect.w<320||rect.h<180{return None;}
        let (canonical,ctx)=canonical_of(frame,rect.x,rect.y,rect.w,rect.h);
        let snapshot=self.legacy.scan_context(&DynamicImage::ImageRgba8(canonical),None,now,souls,Some((frame,ctx)),true);
        (snapshot.screen.as_deref()!=Some("Unknown")).then_some(snapshot)
    }
    pub fn regions(&self)->Vec<RegionObservation>{self.regions.clone()}
    /// The scene scanner (diagnostics, e.g. identity votes).
    pub fn scanner(&self)->&Scanner{&self.legacy}
    pub fn scan(&mut self,frame:&DynamicImage,now:u64)->GameSnapshot{
        // The scene scanner detected a new match (see scan::match_boundary): the independent clock
        // and lane layout belong to the previous one. The located boxes stay (same window).
        if self.legacy.match_resets()!=self.resets_seen{self.resets_seen=self.legacy.match_resets();self.clock=Default::default();self.map_tracker=Default::default();}
        self.regions.clear();
        let mut laps=coach_perception::track::Laps::default();
        let dims=(frame.width(),frame.height());
        let verified=self.verified_map.filter(|(_,d)|*d==dims).map(|(b,_)|b);
        let tracked=self.previous_map.and_then(|b|self.map.track(frame,b));
        // A tracked or held box is re-checked against a fresh search every few seconds and replaced
        // when the fresh one matches the minimap clearly better. rec6 latched the slightly smaller
        // spectator minimap of a death screen (396x414 for 404x422); tracking kept it, the scene
        // context sat 2% off and every later frame of the match read as Unknown.
        let due=now>=self.relocate_at||now+60_000<self.relocate_at;
        if due{self.relocate_at=now+MAP_RECHECK_MS;}
        let better=|f:&coach_perception::localization::Located,score:f32|f.score>score+MAP_SWITCH_MARGIN;
        // A fresh find of another size than the box it would replace is a contender: it replaces
        // that box only after MAP_CONTENDER_FINDS searches in a row found it. rec5 8-32 re-checked
        // a tracked minimap against a 262-px "minimap" over your hero that scored higher, and
        // rec8 48:20 (base under siege, every lane red) adopted a 313-px one on a weak hold;
        // both lost the minimap for the rest of the stretch (rec8: until death, 48:20-49:42).
        let rechecked=match (tracked,due) {
            (Some(t),true) => self.map.locate(frame).filter(|f|better(f,t.score))
                .filter(|f|same_size(f.bounds,t.bounds)||contend(&mut self.map_contender,f.bounds)>=MAP_CONTENDER_FINDS),
            _ => None,
        };
        let tracked=rechecked.or(tracked);
        let map=tracked.or_else(||{
            let found=self.map.locate(frame);
            match (found,verified) {
                // A find within a few pixels of the verified box is the same minimap re-located with
                // landmark jitter: keep the verified box (rec2 late game re-located 398x416 for the
                // verified 403x421, which moved the derived scene context by 17 px; every top-bar
                // crop slid and ally slot 2 read Bookworm instead of Bebop for the rest of the
                // match), unless the find matches clearly better (a wrong box latched).
                (Some(f),Some(v)) if jitter(f.bounds,v) => { self.map_contender=None; match self.map.hold(frame,v,0.0) {
                    Some(h) if !better(&f,h.score) => Some(coach_perception::localization::Located{bounds:v,score:h.score}),
                    _ => Some(f),
                }},
                // The minimap does not move within a match: a differently sized box is a false find
                // while the verified one still matches (rec4 frame 500 found a 340-px box); it
                // replaces the verified box only as a confirmed contender.
                (Some(f),Some(v)) if !same_size(f.bounds,v) => {
                    let confirmed=contend(&mut self.map_contender,f.bounds)>=MAP_CONTENDER_FINDS;
                    match self.map.hold(frame,v,MAP_HOLD_SCORE) {
                        Some(h) if !(confirmed&&better(&f,h.score)) => Some(h),
                        _ => confirmed.then_some(f),
                    }
                }
                (Some(f),_) => { self.map_contender=None; Some(f) },
                (None,Some(v)) => self.map.hold(frame,v,MAP_HOLD_SCORE),
                (None,None) => None,
            }
        });
        if let Some(m)=tracked.or_else(||map.filter(|m|verified!=Some(m.bounds))) {self.verified_map=Some((m.bounds,dims));}
        self.previous_map=map.map(|m|m.bounds);
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
        let mut currency=currency_line.and_then(|r|lines[r].iter().find_map(|l|currency_value(l).map(|v|(v,l.clone())))).zip(self.currency_box.bounds)
            .map(|((v,l),b)|(crate::localize::correct_souls(frame,b,v,&l),b))
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
            if let Some(m)=map{let crop=frame.crop_imm(m.bounds.x,m.bounds.y,m.bounds.w,m.bounds.h).to_rgb8();independent.minimap=Some(self.map_tracker.observe(crate::minimap::Analyzer::new().run_layout(&crop,&mut vec![],self.map_tracker.layout()),now));
                if let Some(mm)=&independent.minimap{self.regions.extend(hero_regions(m.bounds,mm));}}
            laps.lap("independent minimap");self.laps=laps;self.had_context=false;
            return independent;
        }}}
        // The game context needs a landmark-verified map and a located currency label. The clock
        // is then read inside the canonical 1600x900 context (measured more reliable than at an
        // arbitrary window scale); the auto-tracked clock is the fallback outside a context.
        // A landmark-verified map alone fixes the context scale and position. The souls label only
        // validates it when read: on light backgrounds its small SOULS text is often unreadable,
        // which used to drop the whole frame (57 of 120 frames in a recorded match).
        let souls=currency.as_ref().map(|(v,_)|*v);
        // No usable context from the map on this frame (not located, or a mis-scaled find that
        // puts the context off the frame, rec5 base fight 564-568): the last good context decides.
        macro_rules! off_map { () => {{
            if let Some(s)=self.scan_off_map(frame,now,souls){
                laps.absorb("off-map scene: ",&self.legacy.laps);self.laps=laps;self.had_context=false;
                return s;
            }
            independent!()
        }}}
        let Some(map)=map else{ off_map!() };
        let currency_box=currency.as_ref().map(|(_,r)|r.bounds);
        let clock_box=clock.as_ref().map(|(_,r)|r.bounds);
        let width=map.bounds.w as f32*1600./335.;let height=map.bounds.h as f32*900./350.;
        let x=map.bounds.x as f32-width*1255./1600.;let y=map.bounds.y as f32-height*525./900.;
        if x < -width*0.04 || y < -height*0.04 || x+width>frame.width() as f32+width*0.04 || y+height>frame.height() as f32+height*0.04 {
            off_map!();
        }
        let left=x.max(0.) as u32;let top=y.max(0.) as u32;
        let w=(width as u32).min(frame.width()-left);let h=(height as u32).min(frame.height()-top);
        if w<320||h<180{off_map!();}
        let within=|b:Bounds|b.x>=left.saturating_sub(10)&&b.y>=top.saturating_sub(10)&&b.x+b.w<=left+w+10&&b.y+b.h<=top+h+10;
        if currency_box.is_some_and(|b|!within(b))||clock_box.is_some_and(|b|!within(b)){off_map!();}
        // Box-filter downsampling is ~2x faster than a triangle filter for the common case of a
        // larger-than-1600 window; smaller sources still upsample smoothly. Region mapping uses
        // the pixels actually analyzed.
        let (mut canonical,ctx)=canonical_of(frame,left,top,w,h);
        let (left,top,w,h)=(ctx.x,ctx.y,ctx.w,ctx.h);
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
        let mut snapshot=self.legacy.scan_context(&DynamicImage::ImageRgba8(canonical),Some(&map_image),now,souls,Some((frame,ctx)),false);
        laps.absorb("scene: ",&self.legacy.laps);
        if snapshot.screen.as_deref()==Some("Unknown"){independent!();}
        if matches!(snapshot.screen.as_deref(),Some("Gameplay"|"Scoreboard"|"Dead")){self.last_context=Some((ctx,dims));}
        self.regions.push(RegionObservation{id:"game_context".into(),label:"Registered game context · compatibility".into(),bounds:Bounds{x:left,y:top,w,h},origin:RegionOrigin::Configured,score:None,value_path:None});
        if snapshot.game_clock_secs.is_none(){snapshot.game_clock_secs=independent.game_clock_secs;}
        // Souls come from the scene's pixel-located `$` row (the located label is one candidate).
        for mut r in self.legacy.regions(){r.bounds=Bounds{x:left+(r.bounds.x as f32/1600.*w as f32).round() as u32,
            y:top+(r.bounds.y as f32/900.*h as f32).round() as u32,w:(r.bounds.w as f32/1600.*w as f32).round().max(1.) as u32,h:(r.bounds.h as f32/900.*h as f32).round().max(1.) as u32};self.regions.push(r);}
        // Compatibility-only boxes are explicitly not exposed as located detections.
        for (id,label,r) in [("health_search","Health search · compatibility",crate::Region{x:350./1600.,y:575./900.,w:145./1600.,h:100./900.}),
            ("max_health_search","Max HP search · compatibility",crate::Region{x:145./1600.,y:675./900.,w:110./1600.,h:45./900.})] {
            self.regions.push(RegionObservation{id:id.into(),label:label.into(),bounds:Bounds{x:left+(r.x*w as f32) as u32,y:top+(r.y*h as f32) as u32,w:(r.w*w as f32) as u32,h:(r.h*h as f32) as u32},origin:RegionOrigin::Configured,score:None,value_path:Some(if id=="health_search"{"/hp"}else{"/max_hp"}.into())});
        }
        if let Some(m)=&snapshot.minimap{self.regions.extend(hero_regions(map.bounds,m));}
        self.laps=laps;self.had_context=true;
        snapshot
    }
}
