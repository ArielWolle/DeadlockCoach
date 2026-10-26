//! Detector-first HUD discovery. All coordinates are measured in the supplied frame.
use coach_perception::{Bounds,localization::{TemplateLocator,Located}};
use image::{DynamicImage,GrayImage};
use crate::{ocr::Reader,minimap::{components,hsv}};
use coach_perception::{RegionObservation,RegionOrigin};
pub struct MapLocator {template:TemplateLocator,reference:DynamicImage}
impl Default for MapLocator {
    fn default()->Self {
        let reference=image::load_from_memory(include_bytes!("../../../samples/minimap/no-urn.png")).expect("map reference");
        let mask=GrayImage::from_fn(reference.width(),reference.height(),|x,y|{
            let nx=x as f32/reference.width() as f32;let ny=y as f32/reference.height() as f32;
            let inside=((nx-0.5)/0.48).powi(2)+((ny-0.52)/0.49).powi(2)<1.;
            image::Luma([if inside&&(ny<0.23||ny>0.81||nx<0.14||nx>0.88){255}else{0}])});
        Self {template:TemplateLocator::new(&reference,Some(&mask)),reference}
    }
}
fn landmark(image:&DynamicImage,box_:Bounds,red:bool)->Option<(f32,f32)> {
    let (x0,y0,w,h)=if red {(0.44,0.025,0.22,0.16)}else{(0.33,0.84,0.22,0.13)};
    let mut xsum=0.;let mut ysum=0.;let mut count=0.;
    for y in (box_.y as f32+y0*box_.h as f32) as u32..(box_.y as f32+(y0+h)*box_.h as f32) as u32 {
        for x in (box_.x as f32+x0*box_.w as f32) as u32..(box_.x as f32+(x0+w)*box_.w as f32) as u32 {
            if x>=image.width()||y>=image.height(){continue;}
            let p=image.as_rgb8().map(|i|i.get_pixel(x,y).0).unwrap_or_else(||{use image::GenericImageView;let p=image.get_pixel(x,y);[p[0],p[1],p[2]]});
            let yes=if red {p[0]>160&&p[1]<120&&p[2]<105}else{p.iter().copied().min().unwrap()>175&&p.iter().max().unwrap()-p.iter().min().unwrap()<70};
            if yes{xsum+=x as f32;ysum+=y as f32;count+=1.;}
        }
    }
    if count<(box_.w as f32*box_.h as f32*0.0015).max(3.){None}else{Some((xsum/count,ysum/count))}
}
impl MapLocator {
    pub fn track(&self,frame:&DynamicImage,b:Bounds)->Option<Located>{
        if b.x+b.w>frame.width()||b.y+b.h>frame.height(){return None;}
        if landmark(frame,b,true).is_none()||landmark(frame,b,false).is_none(){return None;}
        let score=self.template.similarity(frame,b);(score>=0.48).then_some(Located{bounds:b,score})
    }
    pub fn locate(&self,frame:&DynamicImage)->Option<Located> {
        let gray=frame.to_luma8();
        let reference_box=Bounds{x:0,y:0,w:self.reference.width(),h:self.reference.height()};
        let rt=landmark(&self.reference,reference_box,true)?;let rb=landmark(&self.reference,reference_box,false)?;
        let white_blobs=|image:&DynamicImage|{
            let rgb=image.to_rgb8();let (w,h)=rgb.dimensions();let raw:Vec<bool>=rgb.pixels().map(|p|p.0.iter().copied().min().unwrap()>180&&p.0.iter().max().unwrap()-p.0.iter().min().unwrap()<65).collect();
            let mut eroded=vec![false;raw.len()];for y in 1..h.saturating_sub(1){for x in 1..w.saturating_sub(1){
                eroded[(y*w+x) as usize]=(-1..=1).all(|dy|(-1..=1).all(|dx|raw[((y as i32+dy) as u32*w+(x as i32+dx) as u32) as usize]));}}
            components(&eroded,w,h).into_iter().filter(|b|b.area>=12&&b.w()>=4&&b.h()>=4&&b.w()<100&&b.h()<100).collect::<Vec<_>>()
        };
        let reference_blob=white_blobs(&self.reference).into_iter().filter(|b|b.cy>self.reference.height() as f32*0.84).max_by_key(|b|b.area);
        let mut proposals=vec![];
        if let Some(reference)=reference_blob {
            for b in white_blobs(frame){let scale=(b.w()+2) as f32/(reference.w()+2) as f32;
                let x=b.cx-reference.cx*scale;let y=b.cy-reference.cy*scale;let w=self.reference.width() as f32*scale;let h=self.reference.height() as f32*scale;
                if w<100.||w>1000.||x<0.||y<0.||x+w>frame.width() as f32||y+h>frame.height() as f32{continue;}
                let bounds=Bounds{x:x as u32,y:y as u32,w:w as u32,h:h as u32};
                if landmark(frame,bounds,true).is_some()&&landmark(frame,bounds,false).is_some(){proposals.push(Located{bounds,score:self.template.similarity_gray(&gray,bounds)});}
            }
        }
        proposals.sort_by(|a,b|b.score.total_cmp(&a.score));
        if proposals.first().is_none_or(|p|p.score<0.5){proposals.extend(self.template.candidates(frame,100.,900.,0.48));}
        for candidate in proposals {
            let Some(top)=landmark(frame,candidate.bounds,true) else{continue};let Some(bottom)=landmark(frame,candidate.bounds,false)else{continue};
            let scale=(bottom.1-top.1)/(rb.1-rt.1);
            if !scale.is_finite()||scale<0.25||scale>4. {continue;}
            let x=bottom.0-rb.0*scale;let y=top.1-rt.1*scale;let w=self.reference.width() as f32*scale;let h=self.reference.height() as f32*scale;
            if x<0.||y<0.||x+w>frame.width() as f32||y+h>frame.height() as f32{continue;}
            // Independent landmarks must agree in both axes, not merely form a dark patch.
            if ((top.0-bottom.0)-(rt.0-rb.0)*scale).abs()>w*0.05{continue;}
            if candidate.score<0.48{continue;}
            let x=x.round() as u32;let y=y.round() as u32;
            return Some(Located{bounds:Bounds{x,y,w:(w.round() as u32).min(frame.width()-x),h:(h.round() as u32).min(frame.height()-y)},score:candidate.score});
        }None
    }
}

/// Color-assisted text proposals are searched across the entire frame, not a screen corner.
fn text_proposals(frame:&DynamicImage,mint:bool)->Vec<Bounds>{
    let rgb=frame.to_rgb8();let (w,h)=rgb.dimensions();let mask:Vec<bool>=rgb.pixels().map(|p|{
        let (h,s,v)=hsv(p[0],p[1],p[2]);if mint {(125.0..=180.0).contains(&h)&&s>0.2&&v>0.65}else{p.0.iter().copied().min().unwrap()>170&&s<0.25}
    }).collect();
    let mut glyphs:Vec<_>=components(&mask,w,h).into_iter().filter(|b|b.h()>=5&&b.h()<=80&&b.w()<=120&&b.area>=5&&b.area<=4000&&b.w() as f32/(b.h() as f32)<2.0).collect();
    glyphs.sort_by(|a,b|a.cy.total_cmp(&b.cy));let mut groups:Vec<(Bounds,u32)>=vec![];
    for g in glyphs.into_iter().take(2000){
        if let Some((b,n))=groups.iter_mut().find(|(b,_)|((b.y as f32+b.h as f32/2.)-g.cy).abs()<b.h.max(g.h()) as f32*0.45
            && g.min_x<=b.x+b.w+g.h()*2&&g.max_x+g.h()*2>=b.x&&b.w<500){
            let right=(b.x+b.w).max(g.max_x+1);let bottom=(b.y+b.h).max(g.max_y+1);b.x=b.x.min(g.min_x);b.y=b.y.min(g.min_y);b.w=right-b.x;b.h=bottom-b.y;*n+=1;
        }else{groups.push((Bounds{x:g.min_x,y:g.min_y,w:g.w(),h:g.h()},1));}
    }
    groups.into_iter().filter(|(b,n)|*n>=2&&b.w>b.h*2&&b.w<500).map(|(b,_)|{
        let pad=b.h/2+2;let x=b.x.saturating_sub(pad);let y=b.y.saturating_sub(pad);let right=(b.x+b.w+if mint {b.h*7}else{pad}).min(w);let bottom=(b.y+b.h+pad).min(h);
        Bounds{x,y,w:right-x,h:bottom-y}
    }).collect()
}
pub fn locate_currency(frame:&DynamicImage,reader:&Reader)->Option<(u32,RegionObservation)>{
    locate_currency_limited(frame,reader,12)
}
pub fn locate_currency_limited(frame:&DynamicImage,reader:&Reader,limit:usize)->Option<(u32,RegionObservation)>{
    let mut proposals=text_proposals(frame,true);proposals.sort_by(|a,b|(b.w*b.h).cmp(&(a.w*a.h)));
    for b in proposals.into_iter().take(limit){
        let Ok(spans)=reader.read_spans(&frame.crop_imm(b.x,b.y,b.w,b.h),3)else{continue};
        for mut span in spans {
            let Some(value)=currency_value(&span.label)else{continue};
            span.id="souls".into();span.value_path=Some("/souls".into());span.bounds.x+=b.x;span.bounds.y+=b.y;return Some((value,span));
        }
    }None
}
fn currency_value(text:&str)->Option<u32>{
    let upper=text.to_ascii_uppercase().replace('0',"O");let pos=upper.find("SOULS").or_else(||upper.find("SDULS"))?;
    if upper.get(pos+5..).and_then(|s|s.chars().next()).is_some_and(|c|c.is_alphabetic()){return None;}
    let prefix=text.get(..pos)?.trim_end();if !prefix.chars().last()?.is_ascii_digit(){return None;}
    let digits:String=prefix.chars().rev().take_while(|c|c.is_ascii_digit()||*c==','||*c=='.').collect::<String>().chars().rev().collect();
    if digits.contains([',','.'])&&digits.split([',','.']).skip(1).any(|part|part.len()!=3){return None;}
    crate::parse::parse_souls(&digits).filter(|v|*v<=1_000_000)
}

#[cfg(test)]mod tests{use super::*;#[test]fn currency_labels_are_not_arbitrary_item_or_percentage_text(){assert_eq!(currency_value("$ 1,732 sDuLs"),Some(1732));assert!(currency_value("15% SOULS").is_none());assert!(currency_value("100 SOULSHREDDER").is_none());assert!(currency_value("$15.50 SOULS").is_none());}}
pub fn track_currency(frame:&DynamicImage,reader:&Reader,b:Bounds)->Option<(u32,RegionObservation)>{
    if b.x+b.w>frame.width()||b.y+b.h>frame.height(){return None;}
    let x=b.x.saturating_sub(b.h/2);let y=b.y.saturating_sub(b.h/2);
    let w=(b.w+b.h*3).min(frame.width()-x);let h=(b.h*2).min(frame.height()-y);
    for mut r in reader.read_spans(&frame.crop_imm(x,y,w,h),3).ok()?{
        let Some(value)=currency_value(&r.label)else{continue};r.id="souls".into();r.value_path=Some("/souls".into());r.bounds.x+=x;r.bounds.y+=y;return Some((value,r));
    }None
}
pub fn locate_clock(frame:&DynamicImage,reader:&Reader,hint:Option<Bounds>)->Option<(u32,RegionObservation)>{
    locate_clock_limited(frame,reader,hint,16)
}
pub fn locate_clock_limited(frame:&DynamicImage,reader:&Reader,hint:Option<Bounds>,limit:usize)->Option<(u32,RegionObservation)>{
    let mut proposals=text_proposals(frame,false);
    proposals.retain(|b|b.w as f32/(b.h as f32)>1.5&&b.w<350);
    if let Some(hint)=hint {proposals.sort_by(|a,b|{
        let distance=|b:&Bounds|(b.x as f32+b.w as f32/2.-hint.x as f32).hypot(b.y as f32-hint.y as f32);
        distance(a).total_cmp(&distance(b))
    });}
    if let Some(hint)=hint {
        let x=hint.x.saturating_sub(hint.w/2);let y=hint.y.saturating_sub(hint.h/3);
        proposals.insert(0,Bounds{x,y,w:hint.w.min(frame.width()-x),h:hint.h.min(frame.height()-y)});
    }
    for b in proposals.into_iter().take(limit){
        let Ok(spans)=reader.read_spans(&frame.crop_imm(b.x,b.y,b.w,b.h),3)else{continue};
        for mut span in spans {
            let text=span.label.trim();if text.matches(':').count()!=1{continue;}
            let Some(mut value)=crate::parse::parse_clock(text)else{continue};if value>14400{continue;}
            // Normalize recognition context from the located glyph scale, not screen size.
            // Tight text boxes alone can remove anti-aliased strokes or distort small digits.
            let glyph=span.bounds;let char_width=glyph.w as f32/text.chars().count().max(1) as f32;
            let cw=(char_width*15.36).round().max(glyph.w as f32) as u32;let ch=(char_width*4.96).round().max(glyph.h as f32) as u32;
            let cx=(b.x+glyph.x+glyph.w/2).saturating_sub(cw/2);let cy=(b.y+glyph.y).saturating_sub((char_width*0.96).round() as u32);
            let context=if cx+cw<=frame.width()&&cy+ch<=frame.height(){reader.read(&frame.crop_imm(cx,cy,cw,ch)).ok().and_then(|s|crate::parse::parse_clock(&s)).filter(|n|*n<=14400)}else{None};
            // Re-read actual detected glyphs; a wide search region must not decide a digit
            // merely because padding/layout changed. Agreement is exposed without fake scores.
            let mut votes=std::collections::BTreeMap::from([(value,1u32)]);
            for padding in [0,2,4] {
                let r=span.bounds;let x=r.x.saturating_sub(padding);let y=r.y.saturating_sub(padding);
                let w=(r.w+padding*2).min(b.w-x);let h=(r.h+padding*2).min(b.h-y);
                if let Ok(t)=reader.read_line(&frame.crop_imm(b.x+x,b.y+y,w,h)) {if let Some(t)=crate::parse::parse_clock(&t){*votes.entry(t).or_default()+=1;}}
            }
            if let Some(context)=context{value=context;}else if let Some((winner,count))=votes.into_iter().max_by_key(|(_,count)|*count){if count>=2{value=winner;}else{continue;}}
            span.label=format!("{}:{:02}",value/60,value%60);
            span.id="clock".into();span.value_path=Some("/game_clock_secs".into());span.bounds.x+=b.x;span.bounds.y+=b.y;return Some((value,span));
        }
    }None
}

pub fn detected_map(b:Located)->RegionObservation{RegionObservation{id:"minimap".into(),label:"Minimap · landmark verified".into(),bounds:b.bounds,origin:RegionOrigin::Detected,score:Some(b.score),value_path:Some("/minimap".into())}}
