//! Calibrated own-health reading. Never use a dead/spectated player's HUD as personal HP.
use crate::{hud, ocr::Reader, Region};
use image::DynamicImage;

#[derive(Default)]
pub struct Tracker { previous: Option<(u64,u32,u32,u32)> }
pub struct Reading { pub hp:u32, pub max:u32, pub observations:u32, pub delta:Option<f32>, pub regions:Vec<coach_perception::RegionObservation> }

fn numbers(text: &str) -> Vec<u32> {
    text.split(|c:char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect()
}

impl Tracker {
    pub fn read(&mut self, reader: &Reader, frame: &DynamicImage, now:u64) -> Option<Reading> {
        let raw=(|| {
        let current = hud::crop(frame,Region { x:350.0/1600.0,y:575.0/900.0,w:145.0/1600.0,h:100.0/900.0 });
        let mut words = reader.read_word_regions(&current).ok()?;
        // Current HP is the dominant large number. Smaller max-HP/damage labels must not win.
        words.retain(|r|r.label.chars().all(|c| c.is_ascii_digit() || c==',') && r.label.len() <= 6);
        words.sort_by_key(|r|std::cmp::Reverse(r.bounds.w*r.bounds.h));
        let mut hp_box=words.first()?.clone();let hp=hp_box.label.replace(',',"").parse::<u32>().ok()?;
        let maxima=reader.read_spans(&hud::crop(frame,Region{x:145.0/1600.0,y:675.0/900.0,w:110.0/1600.0,h:45.0/900.0}),3).ok()?;
        let mut values=vec![];for r in maxima{for n in numbers(&r.label).into_iter().filter(|n|(100..=20000).contains(n)){values.push((n,r.clone()));}}
        if values.len()!=1{return None;}let(max,mut max_box)=values.pop()?;
        hp_box.id="hp".into();hp_box.value_path=Some("/hp".into());hp_box.bounds.x+=(350.0/1600.0*frame.width() as f32) as u32;hp_box.bounds.y+=(575.0/900.0*frame.height() as f32) as u32;
        max_box.id="max_hp".into();max_box.value_path=Some("/max_hp".into());max_box.bounds.x+=(145.0/1600.0*frame.width() as f32) as u32;max_box.bounds.y+=(675.0/900.0*frame.height() as f32) as u32;
        (hp<=max).then_some((hp,max,vec![hp_box,max_box]))
        })();
        match raw {Some((hp,max,regions))=>{let mut r=self.accept(hp,max,now);r.regions=regions;Some(r)},None=>{self.previous=None;None}}
    }
    pub fn accept(&mut self,hp:u32,max:u32,now:u64) -> Reading {
        let previous = self.previous.filter(|(t,_,m,_)| now >= *t && now-*t <= 20000 && max.abs_diff(*m) <= max/10);
        let observations = previous.map(|p| p.3.saturating_add(1)).unwrap_or(1);
        let delta = previous.map(|(_,old,old_max,_)| hp as f32/max as f32-old as f32/old_max as f32);
        self.previous=Some((now,hp,max,observations));
        Reading { hp,max,observations,delta,regions:vec![] }
    }
}
