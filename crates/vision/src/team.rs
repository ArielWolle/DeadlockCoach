//! Health-status and earned-souls observations for each top-bar slot.
//! The portrait UI exposes CRITICAL, not trustworthy numeric HP for every hero.
//! Readers expose their crops so callers can fold them into one batched recognition call.
use coach_core::{HeroStatus,GameSnapshot,Pt};
use crate::{hud,ocr::Reader,ColorLibrary};
use image::DynamicImage;

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
        status.critical=reading;status.health_ratio=None;status
    }).collect()
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
/// Earned-souls crops for a subset of slots (round-robin refresh).
pub fn networth_crops_for(frame:&DynamicImage,slots:&[usize])->Vec<DynamicImage>{
    let centers:Vec<f32>=centers().collect();slots.iter().map(|i|frame.crop_imm((centers[*i]-18.) as u32,82,36,14)).collect()
}
/// Update only `slots` from their batched lines.
pub fn apply_networth_slots(lines:&[String],slots:&[usize],clock:u32,status:&mut [HeroStatus]){
    for (j,s) in slots.iter().enumerate(){
        let Some(slot)=status.get_mut(*s) else{continue};
        let mut one=[slot.clone()];apply_networth(std::slice::from_ref(lines.get(j).unwrap_or(&String::new())),clock,&mut one);
        *slot=one[0].clone();
    }
}
pub fn health(reader:&Reader,frame:&DynamicImage,previous:&[HeroStatus])->Vec<HeroStatus>{
    apply_status(frame,&reader.read_lines(&status_crops(frame)).unwrap_or_default(),previous)
}

/// Twelve earned-souls label crops for batched recognition.
pub fn networth_crops(frame:&DynamicImage)->Vec<DynamicImage>{centers().map(|cx|frame.crop_imm((cx-18.) as u32,82,36,14)).collect()}
/// Apply batched net-worth lines (same order as [`networth_crops`]) with temporal plausibility.
pub fn apply_networth(lines:&[String],clock:u32,slots:&mut [HeroStatus]){
    let previous:Vec<_>=slots.iter().map(|s|(s.networth,s.networth_observations)).collect();
    for (row,s) in slots.iter_mut().enumerate(){
        s.networth=None;s.networth_observations=0;
        let text=lines.get(row).map(|t|t.trim_matches(|c:char|!c.is_ascii_digit()&&c!='k'&&c!='K')).unwrap_or("");
        if let Some(value)=hud::parse_networth(text).filter(|n|*n>=600&&*n<=600+(clock/60+1)*2500){
            s.networth=Some(value);
            s.networth_observations=if previous.get(row).and_then(|p|p.0).is_some_and(|n|value>=n&&value-n<=5000){previous[row].1.saturating_add(1)}else{1};
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
        slot.lane=position.and_then(|p|crate::minimap::Analyzer::new().project((p.x*335.,p.y*350.))).filter(|(_,_,d)|*d<22.).map(|(i,_,_)|[coach_core::Lane::Green,coach_core::Lane::Blue,coach_core::Lane::Yellow][i]);
    }
    snapshot.own_networth=snapshot.hero.as_ref().and_then(|hero|snapshot.hero_status.iter().find(|s|s.ally&&s.hero.as_ref()==Some(hero))).and_then(|s|s.networth);
}
