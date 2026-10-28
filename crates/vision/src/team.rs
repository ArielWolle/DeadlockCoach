//! Health-status and earned-souls observations for each top-bar slot.
//! The portrait UI exposes CRITICAL, not trustworthy numeric HP for every hero.
//! Readers expose their crops so callers can fold them into one batched recognition call.
use coach_core::{HeroStatus,GameSnapshot,Pt};
use crate::{hud,ocr::Reader,ColorLibrary};
use image::DynamicImage;

/// Vertical offset of the whole top bar in a 1600x900 frame. On some recorded frames (rec5
/// 298-326: death screen and the spectating run after it) the top bar sits 6-7 px lower; every
/// fixed top-bar crop then misses (labels read "55", "2e"). Measured from the top edge of the
/// ally team-total box (orange or blue, row 37 normally). 0 when the box is not found.
pub fn top_bar_offset(frame:&DynamicImage)->u32{
    if frame.width()!=1600||frame.height()<100{return 0;}
    let band=frame.crop_imm(712,20,48,40).to_rgb8();
    let orange=|y:u32|(0..48).filter(|x|{let p=band.get_pixel(*x,y);let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);(h>21.&&h<47.||h>200.&&h<235.)&&s>0.55&&v>0.5}).count();
    match (0..40).find(|y|orange(*y)>=30){Some(y)=>(y+20).saturating_sub(37).min(12),None=>0}
}
/// The frame with its top bar moved back to the calibrated rows (only the top 140 rows are
/// copied; callers read nothing below the top bar from it).
pub fn top_bar_aligned(frame:&DynamicImage,dy:u32)->std::borrow::Cow<'_,DynamicImage>{
    if dy==0{return std::borrow::Cow::Borrowed(frame);}
    let mut out=DynamicImage::new(frame.width(),frame.height(),frame.color());
    image::imageops::overlay(&mut out,&frame.crop_imm(0,dy,frame.width(),140),0,0);
    std::borrow::Cow::Owned(out)
}
fn centers()->impl Iterator<Item=f32>{hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS)}
fn status_region(cx:f32)->crate::Region{crate::Region{x:(cx-32.)/1600.,y:63./900.,w:64./1600.,h:18./900.}}

/// Twelve status-banner line crops for batched recognition.
pub fn status_crops(frame:&DynamicImage)->Vec<DynamicImage>{centers().map(|cx|hud::crop(frame,status_region(cx))).collect()}
/// Apply batched status lines (same order as [`status_crops`]).
pub fn apply_status(frame:&DynamicImage,lines:&[String],previous:&[HeroStatus])->Vec<HeroStatus>{
    centers().enumerate().map(|(i,cx)|{
        let mut status=previous.get(i).cloned().unwrap_or_default();status.ally=i<6;status.slot=i%6;
        let rgb=hud::crop(frame,status_region(cx)).to_rgb8();
        let red=rgb.pixels().filter(|p|p[0]>160&&p[0] as f32>p[1] as f32*1.6&&p[0] as f32>p[2] as f32*1.5).count();
        let critical=lines.get(i).is_some_and(|s|s.to_ascii_uppercase().contains("CRITICAL"));
        let reading=if critical{Some(true)}else if red as f32/((rgb.width()*rgb.height()) as f32)<0.1{Some(false)}else{None};
        status.observations=if status.critical==reading&&reading.is_some(){status.observations.saturating_add(1)}else{1};
        status.critical=reading;status
    }).collect()
}
/// Pixel reading of one top-bar slot (no OCR).
#[derive(Debug,Clone,Copy,PartialEq,Default)]
pub struct BarReading{
    /// A bar-shaped fill/empty pattern was found.
    pub visible:bool,
    /// Filled fraction of the bar (bottom-up), when visible.
    pub ratio:Option<f32>,
    /// The whole bar column is empty-coloured: either ~0 HP or no bar over a dark scene.
    pub empty:bool,
    /// Greyed (desaturated) portrait: dead, waiting to respawn.
    pub dead:bool,
    /// Red respawn-timer pixels over the portrait ([`timer_pixels`]).
    pub timer:u32,
}
/// Measured on 1600x900 frames: bars span rows 24..=62. Ally bars keep their right edge at
/// cx-20 (left edge cx-28 -> cx-22); enemy bars mirror (cx+25..34 at the top, cx+22..24 at the bottom).
const BAR_TOP:u32=24;const BAR_BOTTOM:u32=62;const BAR_ROWS:u32=BAR_BOTTOM-BAR_TOP+1;
fn bar_core_x(ally:bool,cx:f32,y:u32)->f32{let t=((y-BAR_TOP) as f32/38.).clamp(0.,1.);if ally{cx-21.}else{cx+26.-3.*t}}
#[derive(Clone,Copy,PartialEq)]enum Cell{Fill,Empty,Other}
/// Fill: white (allies) or bright red (enemies). Empty: dark, including the dark red tint the
/// CRITICAL glow puts over an empty bar.
fn cell(ally:bool,p:&image::Rgb<u8>)->Cell{
    let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);
    // Recorded 1920x1080 captures show a vertical gradient: the top of an enemy fill is dim red
    // (v 0.33-0.6, s ~0.55). The CRITICAL glow over an empty bar is darker and more saturated.
    // Fill hue is 0-10 / 345-360 with saturation ~0.55-0.75; orange portrait art (hue ~24,
    // saturation ~1) beside a missing bar must not count.
    let red=(h<15.||h>335.)&&s>0.4&&s<0.88&&(v>0.6||v>0.3&&s<0.7);
    if ally&&v>0.72&&s<0.3 || !ally&&red{Cell::Fill}else if v<0.48{Cell::Empty}else{Cell::Other}
}
/// Bright red-orange respawn-timer digit pixels in the portrait centre. Pale or grey-furred
/// living portraits are desaturated too; only a dead portrait carries the timer.
pub fn timer_pixels(frame:&image::RgbImage,cx:f32)->u32{
    let mut n=0;
    for y in 26u32..64{for x in (cx as u32).saturating_sub(16)..(cx as u32+16){
        if x>=frame.width()||y>=frame.height(){continue;}
        let p=frame.get_pixel(x,y);let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);
        if (h<25.||h>335.)&&s>0.6&&v>0.65{n+=1;}
    }}
    n
}
/// Red pixels in a ring (radius 22-28) around the portrait: the CRITICAL glow of a living hero
/// (123-390 px on rec5 / fixture frames) also makes many "timer" pixels; dead portraits: 0-1.
pub fn glow_pixels(frame:&image::RgbImage,cx:f32)->u32{
    let mut n=0;
    for y in 17u32..frame.height().min(74){for x in (cx as u32).saturating_sub(28)..(cx as u32+29).min(frame.width()){
        let d=((x as f32-cx).powi(2)+(y as f32-45.).powi(2)).sqrt();
        if !(22. ..=28.).contains(&d){continue;}
        let p=frame.get_pixel(x,y);let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);
        if (h<25.||h>335.)&&s>0.6&&v>0.65{n+=1;}
    }}
    n
}
/// Timer pixels that mark a dead slot on their own (see [`read_bar`]).
pub const TIMER_DEAD:u32=55;
fn portrait_dead(frame:&image::RgbImage,cx:f32)->bool{
    // Living portraits sit on a saturated team backdrop (orange / blue) with coloured art; a dead
    // hero's portrait is greyed. The red respawn digits are excluded.
    let (mut n,mut coloured)=(0u32,0u32);
    for dy in -18i32..=18{for dx in -18i32..=18{
        if dx*dx+dy*dy>18*18{continue;}
        let (x,y)=((cx as i32+dx) as u32,(45+dy) as u32);if x>=frame.width()||y>=frame.height(){continue;}
        let p=frame.get_pixel(x,y);let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);
        if (h<22.||h>338.)&&s>0.5{continue;}
        n+=1;if s>0.38&&v>0.3{coloured+=1;}
    }}
    n>0&&(coloured as f32/n as f32)<0.12
}
/// Read one slot's health bar and dead state from pixels.
pub fn read_bar(frame:&image::RgbImage,slot:usize)->BarReading{
    let ally=slot<6;let cx=centers().nth(slot).unwrap();
    if frame.width()<1600||frame.height()<70{return BarReading::default();}
    let dead=portrait_dead(frame,cx);
    let mut best:(u32,BarReading)=(0,BarReading{dead,..Default::default()});
    for shift in -2i32..=2{
        let rows:Vec<Cell>=(BAR_TOP..=BAR_BOTTOM).map(|y|{
            let x=bar_core_x(ally,cx,y).round() as i32+shift;
            let cells:Vec<Cell>=(x-1..=x+1).map(|xx|cell(ally,frame.get_pixel(xx as u32,y))).collect();
            if cells.iter().filter(|c|**c==Cell::Fill).count()>=2{Cell::Fill}else if cells.iter().filter(|c|**c==Cell::Empty).count()>=2{Cell::Empty}else{Cell::Other}
        }).collect();
        // Fill run from the bottom, tolerating up to two anti-aliased edge rows at the bottom and
        // one stray row inside the run.
        let (mut fill,mut gap,mut edge)=(0u32,0u32,0u32);
        for c in rows.iter().rev(){match c{
            Cell::Fill=>{fill+=1+gap+edge;gap=0;edge=0;}
            _ if fill==0&&edge<2=>edge+=1,
            _ if gap==0&&fill>0=>gap=1,
            _=>break}}
        let above=&rows[..(BAR_ROWS-fill) as usize];
        let empty=above.iter().filter(|c|**c==Cell::Empty).count() as u32;
        // A (near-)full bar leaves only edge rows above the fill: nothing to check there.
        let empty_frac=if above.len()<4{1.0}else{empty as f32/above.len() as f32};
        let visible=fill>=2&&empty_frac>=0.7;
        let all_empty=fill==0&&empty_frac>=0.92;
        let score=if visible{fill+empty}else{empty/2};
        if score>best.0||best.0==0&&all_empty{best=(score,BarReading{visible,ratio:visible.then(||fill as f32/BAR_ROWS as f32),empty:all_empty,dead,timer:0});}
    }
    // A dead hero's bar is gone and the portrait is greyed; a pale living portrait keeps its bar.
    // ...and only a dead portrait carries the red respawn timer (dead: 78-216 px on recorded and
    // fixture frames; pale living portraits without a bar: 0).
    // The red digits alone decide when they are plentiful: dark portraits (Haze, Rat King) never
    // read as greyed, and a dark greyed portrait can look like an empty bar column. Over all 7,200
    // rec5 slot-frames, living slots without a bar peaked at 44 timer pixels (red death-screen
    // tint) and dead slots started at 65 (a single "7").
    let timer=timer_pixels(frame,cx);
    let glow=glow_pixels(frame,cx)>=50;
    best.1.dead=!best.1.visible&&(timer>=TIMER_DEAD&&!glow||dead&&!best.1.empty&&timer>=40);
    if best.1.dead{best.1.empty=false;best.1.ratio=None;}
    best.1.timer=timer;
    best.1
}
/// Update all twelve slots from their health bars (pixel-only, every gameplay frame).
/// Returns the slots that look dead, for respawn-timer recognition.
pub fn apply_bars(frame:&DynamicImage,status:&mut Vec<HeroStatus>)->Vec<usize>{
    if status.len()!=12{*status=twelve(status);}
    if frame.width()!=1600||frame.height()<70{return vec![];}
    let band=frame.crop_imm(0,0,1600,70).to_rgb8();
    let mut dead=vec![];
    for (i,s) in status.iter_mut().enumerate(){
        let r=read_bar(&band,i);
        // An all-empty bar is ~0 HP only when the CRITICAL banner confirms it; otherwise it can
        // be a missing bar over a dark scene.
        let near_zero=r.empty&&s.critical==Some(true);
        let ratio=if r.visible{r.ratio}else if near_zero{Some(0.02)}else{None};
        let visible=if r.visible||near_zero{Some(true)}else if r.empty{None}else{Some(false)};
        // Evidence before a state change: seen <-> not seen <-> dead switches only when two
        // consecutive frames agree (a single blurred frame flipped full bars to "unseen").
        let state=(visible,r.dead);
        let known=s.dead.is_some()||s.bar_visible.is_some();
        // Unmistakable evidence commits at once: a dense red timer (>= 100 px; living slots peak
        // at 44) starts a death, a drawn bar ends one. rec5 lagged one frame on every death and
        // respawn (a respawned hero still reported dead without a timer).
        let decisive=r.dead&&r.timer>=100||s.dead==Some(true)&&r.visible;
        if known&&!decisive&&(s.bar_visible,s.dead==Some(true))!=state&&s.pending_state!=Some(state){
            s.pending_state=Some(state);
            // A slot that now shows a respawn timer has no live health, even before the change is
            // committed (rec5 reported a dead hero at 31% from the previous frame).
            if r.dead{s.health_ratio=None;s.health_observations=0;}
            if s.dead==Some(true)||r.dead{dead.push(i);}
            continue;
        }
        s.pending_state=None;
        s.health_observations=match (ratio,s.health_ratio){(Some(a),Some(b)) if (a-b).abs()<=0.15=>s.health_observations.saturating_add(1),(Some(_),_)=>1,_=>0};
        s.health_ratio=ratio;
        s.bar_visible=visible;
        s.dead=Some(r.dead);
        if r.dead{dead.push(i);}else{s.respawn_secs=None;s.respawn_anchor=None;s.respawn_candidate=None;s.respawn_observations=0;}
    }
    dead
}
/// Respawn-timer crops (red digits over the greyed portrait) for dead slots.
/// The red digits are isolated as black-on-white and tightly cropped: the line recognizer
/// misreads them over the busy greyed portrait.
pub fn respawn_crops(frame:&DynamicImage,slots:&[usize])->Vec<DynamicImage>{
    let centers:Vec<f32>=centers().collect();
    slots.iter().map(|i|{
        let src=frame.crop_imm((centers[*i]-22.) as u32,22,44,44).to_rgb8();
        let red=|p:&image::Rgb<u8>|{let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);(h<25.||h>335.)&&s>0.5&&v>0.5};
        let (mut x0,mut y0,mut x1,mut y1)=(u32::MAX,u32::MAX,0,0);
        for (x,y,p) in src.enumerate_pixels(){if red(p){x0=x0.min(x);y0=y0.min(y);x1=x1.max(x);y1=y1.max(y);}}
        let ink=image::GrayImage::from_fn(src.width(),src.height(),|x,y|image::Luma([if red(src.get_pixel(x,y)){0}else{255}]));
        if x0>x1{return DynamicImage::ImageLuma8(ink).into_rgb8().into();}
        let (m,w,h)=(3u32,src.width(),src.height());
        let (cx0,cy0)=(x0.saturating_sub(m),y0.saturating_sub(m));let (cw,ch)=((x1+m+1).min(w)-cx0,(y1+m+1).min(h)-cy0);
        DynamicImage::ImageLuma8(image::imageops::crop_imm(&ink,cx0,cy0,cw,ch).to_image()).into_rgb8().into()
    }).collect()
}
/// Widths of the red respawn-timer digits of one slot, left to right: runs of columns with at
/// least 4 red pixels within cx-17..cx+17, at least 5 px wide. Measured on rec5: a "1" is 8 px
/// wide, every other digit 10-14 px.
pub fn timer_digit_widths(frame:&DynamicImage,cx:f32)->Vec<u32>{
    if frame.width()<(cx as u32+18)||frame.height()<70{return vec![];}
    let src=frame.crop_imm((cx-17.) as u32,22,34,44).to_rgb8();
    let red=|p:&image::Rgb<u8>|{let (h,s,v)=crate::minimap::hsv(p[0],p[1],p[2]);(h<25.||h>335.)&&s>0.5&&v>0.5};
    let mut runs=vec![];let mut run=0;
    for x in 0..=src.width(){
        let lit=x<src.width()&&(0..src.height()).filter(|y|red(src.get_pixel(x,*y))).count()>=4;
        if lit{run+=1;}else{if run>=5{runs.push(run);}run=0;}
    }
    runs
}
/// Check a respawn-timer read against its digit glyphs: the recognizer drops the narrow "1"
/// (rec5 read 14 as "4" on every frame of a death). One digit over two glyphs is completed with
/// the "1" when the missing glyph is narrow; any other count mismatch is rejected.
pub fn check_timer_read(text:&str,widths:&[u32])->Option<String>{
    let digits:String=text.chars().filter(char::is_ascii_digit).collect();
    if widths.is_empty()||digits.is_empty(){return Some(digits);}
    if digits.len()==widths.len(){return Some(digits);}
    match (digits.len(),widths){
        (1,[l,r]) if *l<=9&&*r>=10=>Some(format!("1{digits}")),
        (1,[l,r]) if *r<=9&&*l>=10=>Some(format!("{digits}1")),
        _=>None,
    }
}
/// [`check_timer_read`] for batched respawn lines of `slots`.
pub fn checked_respawn_lines(frame:&DynamicImage,lines:&[String],slots:&[usize])->Vec<String>{
    let centers:Vec<f32>=centers().collect();
    slots.iter().enumerate().map(|(j,i)|check_timer_read(lines.get(j).map(String::as_str).unwrap_or(""),&timer_digit_widths(frame,centers[*i])).unwrap_or_default()).collect()
}
/// Respawn timers count down 1 s per second. A read is reported once it agrees with an earlier
/// read of the same death (or with the countdown from the last confirmed read); a read that breaks
/// the countdown (rec5: 14 -> "4", 53 -> "63") is dropped and the confirmed countdown is reported
/// instead, while the slot still shows its timer. `now_ms` is the frame time.
pub fn apply_respawn(lines:&[String],slots:&[usize],now_ms:u64,status:&mut [HeroStatus]){
    for (j,i) in slots.iter().enumerate(){
        let digits:String=lines.get(j).map(|t|t.chars().filter(char::is_ascii_digit).collect()).unwrap_or_default();
        let Some(s)=status.get_mut(*i) else{continue};
        let read=digits.parse::<u32>().ok().filter(|v|(1..=180).contains(v));
        let elapsed=|t:u64|(now_ms.saturating_sub(t)/1000) as i64;
        let expect=|a:Option<(u32,u64)>|a.map(|(v,t)|v as i64-elapsed(t));
        // Timers read once per frame; frames 1-3 s apart round either way.
        let agrees=|v:u32,e:i64|(v as i64-e).abs()<=2;
        match (read,expect(s.respawn_anchor)) {
            (Some(v),Some(e)) if agrees(v,e)=>{s.respawn_anchor=Some((v,now_ms));s.respawn_observations=s.respawn_observations.saturating_add(1);}
            (Some(v),e)=>{
                // Two consecutive reads that count down together (re)start the countdown.
                if expect(s.respawn_candidate).is_some_and(|c|agrees(v,c)){s.respawn_anchor=Some((v,now_ms));s.respawn_observations=2;s.respawn_candidate=None;}
                else{s.respawn_candidate=Some((v,now_ms));if e.is_none_or(|e|e<=0){s.respawn_anchor=None;s.respawn_observations=0;}}
            }
            (None,_)=>{}
        }
        s.respawn_secs=expect(s.respawn_anchor).filter(|e|*e>=1).map(|e|e as u32);
    }
}
fn twelve(previous:&[HeroStatus])->Vec<HeroStatus>{
    if previous.len()==12{return previous.to_vec();}
    (0..12).map(|i|HeroStatus{ally:i<6,slot:i%6,..Default::default()}).collect()
}
/// Status-banner crops for a subset of slots (round-robin refresh).
pub fn status_crops_for(frame:&DynamicImage,slots:&[usize])->Vec<DynamicImage>{
    let centers:Vec<f32>=centers().collect();slots.iter().map(|i|hud::crop(frame,status_region(centers[*i]))).collect()
}
/// Update only `slots` from their batched lines; other slots keep their previous observation.
pub fn apply_status_slots(frame:&DynamicImage,lines:&[String],slots:&[usize],previous:&[HeroStatus])->Vec<HeroStatus>{
    let fresh=apply_status(frame,&(0..12).map(|i|slots.iter().position(|s|*s==i).and_then(|j|lines.get(j).cloned()).unwrap_or_default()).collect::<Vec<_>>(),previous);
    let mut out=twelve(previous);for s in slots{if let Some(v)=fresh.get(*s){out[*s]=v.clone();}}out
}
/// The earned-souls label box under a portrait spans cx-12..cx+15 (rows 82..95) with the digits
/// at cx-7..cx+6 and a faint small "k" after them. A crop of the digits plus the "k"
/// (cx-9..cx+9) read 33/36 labels right on rec5; the old 36 px crop with the dark surround read
/// 4/36 ("113", "1k", "2").
fn networth_crop(frame:&DynamicImage,cx:f32)->DynamicImage{frame.crop_imm((cx-9.) as u32,82,18,14)}
/// Earned-souls crops for a subset of slots (round-robin refresh).
pub fn networth_crops_for(frame:&DynamicImage,slots:&[usize])->Vec<DynamicImage>{
    let centers:Vec<f32>=centers().collect();slots.iter().map(|i|networth_crop(frame,centers[*i])).collect()
}
/// Update only `slots` from their batched lines.
pub fn apply_networth_slots(lines:&[String],slots:&[usize],clock:u32,status:&mut [HeroStatus]){
    for (j,s) in slots.iter().enumerate(){
        let Some(slot)=status.get_mut(*s) else{continue};
        let mut one=[slot.clone()];apply_networth(std::slice::from_ref(lines.get(j).unwrap_or(&String::new())),clock,&mut one);
        *slot=one[0].clone();
    }
}
/// Plausible earned souls of one hero at `clock` seconds: rec4/rec5 labels ran 12-21 souls/s
/// (13k at 17:54, 57k at 44:00); the band is generous on both sides.
pub fn hero_worth_range(clock:u32)->(u32,u32){((clock*4).max(400),clock*35+5000)}
/// Readings of a per-hero label such as "13k", "9.9k", "511" (= "51k" with the small "k" read as
/// a digit) or "950", in order of preference, restricted to the plausible band at `clock`.
pub fn hero_worth_candidates(text:&str,clock:u32)->Vec<u32>{
    let t=text.trim_start_matches(|c:char|!c.is_ascii_digit());
    let digits:String=t.chars().take_while(char::is_ascii_digit).collect();
    let rest=&t[digits.len()..];
    let d=|i:usize|digits[i..i+1].parse::<u32>().unwrap_or(0);
    let mut c=vec![];
    match digits.len(){
        1 if rest.starts_with('.')=>{if let Some(f)=rest[1..].chars().next().and_then(|c|c.to_digit(10)){c.push(d(0)*1000+f*100);}}
        1=>c.push(d(0)*1000),
        2=>{c.push(d(0)*10_000+d(1)*1000);c.push(d(0)*1000+d(1)*100);}
        3=>{c.push(d(0)*10_000+d(1)*1000);c.push(digits.parse().unwrap_or(0));c.push(d(0)*1000+d(1)*100);}
        _=>{}
    }
    let (lo,hi)=hero_worth_range(clock);
    c.retain(|v|*v>=lo&&*v<=hi);
    c
}
pub fn health(reader:&Reader,frame:&DynamicImage,previous:&[HeroStatus])->Vec<HeroStatus>{
    apply_status(frame,&reader.read_lines(&status_crops(frame)).unwrap_or_default(),previous)
}

/// Twelve earned-souls label crops for batched recognition.
pub fn networth_crops(frame:&DynamicImage)->Vec<DynamicImage>{centers().map(|cx|networth_crop(frame,cx)).collect()}
/// Apply batched net-worth lines (same order as [`networth_crops`]) with temporal plausibility.
/// Earned souls only grow (a little loss on death at most) and by at most ~40/s: a value outside
/// that band replaces the last one only when the next read repeats it (resync after a gap or a
/// wrong first read). A failed read keeps the last value for 45 s of game time, then it expires.
pub fn apply_networth(lines:&[String],clock:u32,slots:&mut [HeroStatus]){
    for (row,s) in slots.iter_mut().enumerate(){
        let text=lines.get(row).map(String::as_str).unwrap_or("");
        let prev=s.networth.zip(s.networth_clock).filter(|(_,at)|clock>=*at&&clock-*at<=45);
        if prev.is_none(){s.networth=None;s.networth_observations=0;s.networth_clock=None;}
        let cands=hero_worth_candidates(text,clock);
        let value=match prev{Some((p,_))=>cands.iter().copied().min_by_key(|v|v.abs_diff(p)),None=>cands.first().copied()};
        let Some(value)=value else{continue};
        let in_band=prev.is_some_and(|(p,at)|value+p/10>=p&&value<=p+2000+(clock-at)*40);
        let repeated=s.networth_pending.is_some_and(|(v,_)|v==value);
        if prev.is_none()&&repeated||in_band{
            s.networth_observations=if in_band{s.networth_observations.saturating_add(1)}else{2};
            s.networth=Some(value);s.networth_clock=Some(clock);s.networth_pending=None;
        }else if prev.is_some()&&repeated&&s.networth_pending.is_some_and(|(_,n)|n>=1){
            // Out of band twice in a row: the old baseline was wrong or stale.
            s.networth=Some(value);s.networth_clock=Some(clock);s.networth_observations=2;s.networth_pending=None;
        }else{
            s.networth_pending=Some((value,if repeated{s.networth_pending.map(|p|p.1).unwrap_or(0).saturating_add(1)}else{1}));
        }
    }
}
pub fn networth(reader:&Reader,frame:&DynamicImage,clock:u32,slots:&mut [HeroStatus]){
    apply_networth(&reader.read_lines(&networth_crops(frame)).unwrap_or_default(),clock,slots);
}

fn portrait_at(frame:&image::RgbImage,p:Pt,library:&ColorLibrary)->Option<String>{
    let radius=(frame.width() as f32*9./335.).max(5.) as u32;
    let x=(p.x*frame.width() as f32) as u32;let y=(p.y*frame.height() as f32) as u32;
    if x<radius||y<radius||x+radius>=frame.width()||y+radius>=frame.height(){return None;}
    let cell=image::imageops::crop_imm(frame,x-radius,y-radius,radius*2,radius*2).to_image();
    let (hero,score,margin)=library.identify(&DynamicImage::ImageRgb8(cell))?;
    (score>=0.65&&margin>=0.08).then_some(hero)
}
pub fn positions(snapshot:&mut GameSnapshot,map:&image::RgbImage,library:&ColorLibrary){
    let Some(state)=&snapshot.minimap else{return};
    if let Some(me)=state.you{snapshot.hero=portrait_at(map,me,library);}
    for slot in &mut snapshot.hero_status{
        let points=if slot.ally{&state.allies}else{&state.enemies};
        let matched:Vec<_>=points.iter().filter(|p|slot.hero.is_some()&&portrait_at(map,**p,library)==slot.hero).copied().collect();
        let position=if matched.len()==1{Some(matched[0])}else{None};
        slot.position_observations=if position.zip(slot.position).is_some_and(|(a,b)|(a.x-b.x).hypot(a.y-b.y)<0.12){slot.position_observations.saturating_add(1)}else if position.is_some(){1}else{0};
        slot.position=position;
        slot.lane=position.and_then(|p|crate::minimap::Analyzer::new().project((p.x*335.,p.y*350.))).filter(|(_,_,d)|*d<22.).map(|(i,_,_)|crate::minimap::lane_of(i,state.lanes_mirrored.unwrap_or(false)));
    }
    snapshot.own_networth=snapshot.hero.as_ref().and_then(|hero|snapshot.hero_status.iter().find(|s|s.ally&&s.hero.as_ref()==Some(hero))).and_then(|s|s.networth);
}
