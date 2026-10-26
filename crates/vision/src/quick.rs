//! Recognition-only monitoring between expensive full-scene scans.
use coach_core::GameSnapshot;
use coach_perception::Bounds;
use crate::{ocr::Reader,hud,parse};
use image::DynamicImage;
use serde::{Serialize,Deserialize};

#[derive(Clone,Serialize,Deserialize)]
pub struct Context {pub snapshot:GameSnapshot,pub timestamp_ms:u64,pub frame_size:[u32;2],pub game:Bounds,pub hp:Bounds,pub map:Bounds}
pub struct Monitor {pub reader:Reader,pub library:Option<std::sync::Arc<crate::ColorLibrary>>,vitals:crate::vitals::Tracker,map:crate::minimap::Tracker,previous_team:Vec<coach_core::HeroStatus>}
impl Monitor {
    pub fn new(reader:Reader)->Self{Self{reader,library:None,vitals:Default::default(),map:Default::default(),previous_team:vec![]}}
    pub fn reset(&mut self){self.vitals=Default::default();self.map=Default::default();self.previous_team.clear();}
    pub fn scan(&mut self,frame:&DynamicImage,c:&Context,now:u64)->Option<GameSnapshot>{
        if now<c.timestamp_ms||now-c.timestamp_ms>30000||c.frame_size!=[frame.width(),frame.height()]||c.snapshot.dead||c.snapshot.screen.as_deref()!=Some("Gameplay"){self.reset();return None;}
        let valid=|b:Bounds|b.w>0&&b.h>0&&b.x.checked_add(b.w).is_some_and(|x|x<=frame.width())&&b.y.checked_add(b.h).is_some_and(|y|y<=frame.height());
        if ![c.game,c.hp,c.map].into_iter().all(valid){self.reset();return None;}
        let game=frame.crop_imm(c.game.x,c.game.y,c.game.w,c.game.h).resize_exact(1600,900,image::imageops::FilterType::Triangle);
        let back=self.reader.read_line(&hud::crop(&game,crate::screen::back_anchor())).ok()?.to_ascii_uppercase();
        let respawn=self.reader.read_line(&hud::crop(&game,crate::screen::respawn_anchor())).ok()?.to_ascii_uppercase();
        if back.contains("BACK")||back.contains("RESUME")||respawn.contains("RESPAWN"){self.reset();return None;}
        let clock=self.reader.read_line(&game.crop_imm(775,12,50,16)).ok().and_then(|s|parse::parse_clock(&s))?;
        let expected=c.snapshot.game_clock_secs?+(now-c.timestamp_ms) as u32/1000;
        if clock.abs_diff(expected)>6{return None;}
        let b=c.hp;let raw=self.reader.read_line(&frame.crop_imm(b.x,b.y,b.w,b.h)).ok()?;
        let value=raw.trim().replace(',',"");if !value.chars().all(|c|c.is_ascii_digit()){return None;}
        let hp=value.parse::<u32>().ok()?;let max=c.snapshot.max_hp?;if hp>max||max<100{return None;}
        let v=self.vitals.accept(hp,max,now);
        let mut snapshot=c.snapshot.clone();snapshot.game_clock_secs=Some(clock);snapshot.hp=Some(hp);snapshot.health_observations=v.observations;snapshot.health_delta_ratio=v.delta;
        let map=frame.crop_imm(c.map.x,c.map.y,c.map.w,c.map.h).to_rgb8();
        snapshot.minimap=Some(self.map.observe(crate::minimap::Analyzer::new().analyze(&map),now));
        let previous=if self.previous_team.len()==12{&self.previous_team}else{&snapshot.hero_status};
        let mut team=crate::team::health(&self.reader,&game,previous);
        for (i,slot) in team.iter_mut().enumerate(){if let Some(seed)=c.snapshot.hero_status.get(i){slot.hero=seed.hero.clone();slot.networth=seed.networth;slot.networth_observations=seed.networth_observations;}}
        snapshot.hero_status=team;
        if let Some(library)=&self.library{crate::team::positions(&mut snapshot,&map,library);}else{for slot in &mut snapshot.hero_status{slot.position=None;slot.lane=None;slot.position_observations=0;}}
        self.previous_team=snapshot.hero_status.clone();
        Some(snapshot)
    }
}
