//! Full-frame, scale-independent template proposals for HUD locator plug-ins.
//! No game, screen aspect or viewport offset is assumed here.
use image::{DynamicImage,GrayImage,imageops::FilterType};
use crate::Bounds;

pub struct TemplateLocator {values:Vec<f32>,mask:Vec<bool>,aspect:f32}
#[derive(Debug,Clone,Copy)]
pub struct Located {pub bounds:Bounds,pub score:f32}
impl TemplateLocator {
    /// Score one candidate box. Only that box is converted to grayscale (not the whole frame),
    /// which makes per-frame tracking verification cheap.
    pub fn similarity(&self,frame:&DynamicImage,bounds:Bounds)->f32{
        if bounds.w==0||bounds.h==0||bounds.x.checked_add(bounds.w).is_none_or(|x|x>frame.width())
            ||bounds.y.checked_add(bounds.h).is_none_or(|y|y>frame.height()){return 0.;}
        let crop=frame.crop_imm(bounds.x,bounds.y,bounds.w,bounds.h).to_luma8();
        self.score(&crop,0.,0.,bounds.w as f32)
    }
    pub fn similarity_gray(&self,frame:&GrayImage,bounds:Bounds)->f32{
        if bounds.w==0||bounds.h==0||bounds.x.checked_add(bounds.w).is_none_or(|x|x>frame.width())
            ||bounds.y.checked_add(bounds.h).is_none_or(|y|y>frame.height()){return 0.;}
        self.score(frame,bounds.x as f32,bounds.y as f32,bounds.w as f32)
    }
    pub fn new(image:&DynamicImage,mask:Option<&GrayImage>) -> Self {
        let gray=image::imageops::resize(&image.to_luma8(),24,24,FilterType::Triangle);
        let mask=mask.map(|m|image::imageops::resize(m,24,24,FilterType::Triangle));
        Self {values:gray.pixels().map(|p|p[0] as f32).collect(),mask:(0..576).map(|i|mask.as_ref().is_none_or(|m|m.get_pixel(i%24,i/24)[0]>=128)).collect(),aspect:image.width() as f32/image.height() as f32}
    }
    fn score(&self,image:&GrayImage,x:f32,y:f32,w:f32)->f32 {
        let h=w/self.aspect;let(mut a,mut b,mut aa,mut bb,mut ab,mut n)=(0.,0.,0.,0.,0.,0.);
        for i in 0..576 {if !self.mask[i]{continue;}let px=(x+((i%24) as f32+0.5)*w/24.).min((image.width()-1) as f32) as u32;
            let py=(y+((i/24) as f32+0.5)*h/24.).min((image.height()-1) as f32) as u32;
            let v=image.get_pixel(px,py)[0] as f32;let t=self.values[i];a+=v;b+=t;aa+=v*v;bb+=t*t;ab+=v*t;n+=1.;}
        let denominator=((aa-a*a/n).max(0.)*(bb-b*b/n).max(0.)).sqrt();
        if n<12. || denominator<1. {0.}else{(ab-a*b/n)/denominator}
    }
    pub fn locate(&self,frame:&DynamicImage,min_width:f32,max_width:f32,min_score:f32)->Option<Located> {
        self.candidates(frame,min_width,max_width,min_score).into_iter().next()
    }
    pub fn candidates(&self,frame:&DynamicImage,min_width:f32,max_width:f32,min_score:f32)->Vec<Located> {
        if frame.width()==0||frame.height()==0||!min_width.is_finite()||!max_width.is_finite()||min_width<1.||max_width<min_width{return vec![];}
        let factor=(480./frame.width() as f32).min(480./frame.height() as f32).min(1.);
        let gray=image::imageops::resize(&frame.to_luma8(),(frame.width() as f32*factor).round().max(1.) as u32,(frame.height() as f32*factor).round().max(1.) as u32,FilterType::Triangle);
        let range=gray.pixels().fold((255u8,0u8),|(lo,hi),p|(lo.min(p[0]),hi.max(p[0])));
        if range.1.saturating_sub(range.0)<8{return vec![];}
        let mut widths=vec![];let mut width=min_width;while width<=max_width {widths.push(width*factor);width*=1.10;}
        let mut peaks:Vec<(f32,f32,f32,f32)>=vec![];
        for w in widths {let h=w/self.aspect;if w>=gray.width() as f32 || h>=gray.height() as f32{continue;}
            // Spread a fixed proposal budget across scales instead of scanning every pixel.
            let placements=(gray.width() as f32-w+1.)*(gray.height() as f32-h+1.);
            let step=(placements/3000.).sqrt().ceil().max(4.) as usize;
            for y in (0..=(gray.height() as f32-h) as u32).step_by(step) {for x in (0..=(gray.width() as f32-w) as u32).step_by(step) {
                let score=self.score(&gray,x as f32,y as f32,w);
                if peaks.len()<32 || score>peaks.last().unwrap().0 {
                    peaks.push((score,x as f32,y as f32,w));peaks.sort_by(|a,b|b.0.total_cmp(&a.0));
                    let mut kept:Vec<(f32,f32,f32,f32)>=vec![];for p in &peaks {if !kept.iter().any(|q|(p.1-q.1).hypot(p.2-q.2)<8. && (p.3-q.3).abs()<8.){kept.push(*p);}}kept.truncate(32);peaks=kept;
                }
            }}
        }
        let full=frame.to_luma8();let mut results=vec![];
        for (_,x,y,w) in peaks.into_iter().filter(|p|p.0>=min_score-0.25).take(6) {
            let mut best=None;
            let (cx,cy,cw)=(x/factor,y/factor,w/factor);let radius=(5./factor).ceil() as i32;
            let stride=(1./factor).round().max(1.) as usize;
            for size in -8..=8 {let width=cw*(1.+size as f32*0.01);let height=width/self.aspect;
                for dy in (-radius..=radius).step_by(stride) {for dx in (-radius..=radius).step_by(stride) {
                    let x=cx+dx as f32;let y=cy+dy as f32;
                    if x<0. || y<0. || x+width>frame.width() as f32 || y+height>frame.height() as f32{continue;}
                    let score=self.score(&full,x,y,width);
                    if best.as_ref().is_none_or(|b:&Located|score>b.score){
                        let x=x.round() as u32;let y=y.round() as u32;
                        let bounds=Bounds{x,y,w:(width.round() as u32).min(frame.width()-x),h:(height.round() as u32).min(frame.height()-y)};
                        if bounds.w>0&&bounds.h>0{best=Some(Located{score,bounds});}
                    }
                }}
            }
            if let Some(best)=best.filter(|b|b.score>=min_score){results.push(best);}
        }
        results.sort_by(|a,b|b.score.total_cmp(&a.score));results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_extremely_narrow_sources_abstain_without_panicking() {
        let template=DynamicImage::new_rgb8(24,24);
        let locator=TemplateLocator::new(&template,None);
        for (w,h) in [(0,0),(1,2304),(4096,1)] {
            assert!(locator.candidates(&DynamicImage::new_rgb8(w,h),100.,900.,0.7).is_empty());
        }
    }
    #[test]
    fn invalid_tracking_boxes_do_not_score_clamped_border_pixels() {
        let template=DynamicImage::new_rgb8(24,24);
        let locator=TemplateLocator::new(&template,None);
        for bounds in [Bounds{x:0,y:0,w:0,h:1},Bounds{x:20,y:0,w:8,h:8},Bounds{x:u32::MAX,y:0,w:8,h:8}] {
            assert_eq!(locator.similarity(&template,bounds),0.);
        }
    }
}
