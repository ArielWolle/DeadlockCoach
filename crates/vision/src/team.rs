//! Health-status and earned-souls observations for each top-bar slot.
//! The portrait UI exposes CRITICAL, not trustworthy numeric HP for every hero.
use coach_core::{HeroStatus,GameSnapshot,Pt};
use crate::{hud,ocr::Reader,ColorLibrary};
use image::DynamicImage;

pub fn health(reader:&Reader,frame:&DynamicImage,previous:&[HeroStatus])->Vec<HeroStatus>{
    hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).enumerate().map(|(i,cx)|{
        let mut status=previous.get(i).cloned().unwrap_or_default();status.ally=i<6;status.slot=i%6;
        let image=hud::crop(frame,crate::Region{x:(cx-32.)/1600.,y:63./900.,w:64./1600.,h:18./900.});
        let rgb=image.to_rgb8();let red=rgb.pixels().filter(|p|p[0]>160&&p[0] as f32>p[1] as f32*1.6&&p[0] as f32>p[2] as f32*1.5).count();
        let critical=reader.read_line(&image).ok().is_some_and(|s|s.to_ascii_uppercase().contains("CRITICAL"));
        let reading=if critical{Some(true)}else if red as f32/((rgb.width()*rgb.height()) as f32)<0.1{Some(false)}else{None};
        status.observations=if status.critical==reading&&reading.is_some(){status.observations.saturating_add(1)}else{1};
        status.critical=reading;status.health_ratio=None;status
    }).collect()
}

pub fn networth(reader:&Reader,frame:&DynamicImage,clock:u32,slots:&mut [HeroStatus]){
    // Stack the actual tiny labels vertically so one detector pass separates the lines.
    let mut bank=image::RgbImage::new(72,36*12);
    for(i,cx)in hud::ALLY_CENTERS.into_iter().chain(hud::ENEMY_CENTERS).enumerate(){
        let cell=hud::crop(frame,hud::networth_label(cx)).resize_exact(48,18,image::imageops::FilterType::Triangle).to_rgb8();
        image::imageops::overlay(&mut bank,&cell,12,(i*36+9) as i64);
    }
    let previous:Vec<_>=slots.iter().map(|s|(s.networth,s.networth_observations)).collect();
    for s in slots.iter_mut(){s.networth=None;s.networth_observations=0;}
    if let Ok(lines)=reader.read_spans(&DynamicImage::ImageRgb8(bank),3){for line in lines{
        let row=((line.bounds.y+line.bounds.h/2)/36) as usize;
        let text=line.label.trim_matches(|c:char|!c.is_ascii_digit()&&c!='k'&&c!='K');
        if let Some(value)=hud::parse_networth(text).filter(|n|*n>=600&&*n<=600+(clock/60+1)*2500){
            if let Some(slot)=slots.get_mut(row){slot.networth=Some(value);slot.networth_observations=if previous[row].0.is_some_and(|n|value>=n&&value-n<=5000){previous[row].1.saturating_add(1)}else{1};}
        }
    }}
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
