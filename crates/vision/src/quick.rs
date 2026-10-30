//! Recognition-only monitoring between expensive full-scene scans.
use coach_core::GameSnapshot;
use coach_perception::Bounds;
use crate::{ocr::Reader,hud};
use image::DynamicImage;
use serde::{Serialize,Deserialize};

#[derive(Clone,Serialize,Deserialize)]
pub struct Context {pub snapshot:GameSnapshot,pub timestamp_ms:u64,pub frame_size:[u32;2],pub game:Bounds,pub hp:Bounds,pub map:Bounds}
pub struct Monitor {pub reader:Reader,pub library:Option<std::sync::Arc<crate::ColorLibrary>>,vitals:crate::vitals::Tracker,map:crate::minimap::Tracker,previous_team:Vec<coach_core::HeroStatus>}
impl Monitor {
    pub fn new(reader:Reader)->Self{Self{reader,library:None,vitals:Default::default(),map:Default::default(),previous_team:vec![]}}
    pub fn reset(&mut self){self.vitals=Default::default();self.map=Default::default();self.previous_team.clear();}
    pub fn scan(&mut self,frame:&DynamicImage,c:&Context,now:u64)->Option<GameSnapshot>{
        if now<c.timestamp_ms||now-c.timestamp_ms>90000||c.frame_size!=[frame.width(),frame.height()]||c.snapshot.dead||c.snapshot.screen.as_deref()!=Some("Gameplay"){self.reset();return None;}
        let valid=|b:Bounds|b.w>0&&b.h>0&&b.x.checked_add(b.w).is_some_and(|x|x<=frame.width())&&b.y.checked_add(b.h).is_some_and(|y|y<=frame.height());
        if ![c.game,c.hp,c.map].into_iter().all(valid){self.reset();return None;}
        let game=if c.game.w>=1600&&c.game.h>=900{frame.crop_imm(c.game.x,c.game.y,c.game.w,c.game.h).thumbnail_exact(1600,900)}
            else{frame.crop_imm(c.game.x,c.game.y,c.game.w,c.game.h).resize_exact(1600,900,image::imageops::FilterType::Triangle)};
        // Every text field of this update in ONE recognition batch.
        let rich=now-c.timestamp_ms<=30000;
        let mut batch=coach_perception::track::LineBatch::default();
        let back_i=batch.add(hud::crop(&game,crate::screen::back_anchor()));
        let respawn_i=batch.add(hud::crop(&game,crate::screen::respawn_text_line()));
        let clocks=batch.add_all(crate::screen::clock_lines().into_iter().map(|r|hud::crop(&game,r)));
        let b=c.hp;let hp_i=batch.add(frame.crop_imm(b.x,b.y,b.w,b.h));
        // Essentials only: team banners refresh in the full scan's round-robin.
        let lines=batch.run(|crops|self.reader.read_lines(crops));
        let (back,respawn)=(lines[back_i].to_ascii_uppercase(),lines[respawn_i].to_ascii_uppercase());
        if back.contains("BACK")||back.contains("RESUME")||respawn.contains("RESPAWN"){self.reset();return None;}
        let expected=c.snapshot.game_clock_secs?+(now-c.timestamp_ms) as u32/1000;
        // A visible clock proves the HUD is live; its value comes from the verified anchor
        // (cheap reads misread minute digits on soft video, but never move the value here).
        // Seconds must still agree (a minute-digit misread differs by whole minutes only);
        // anything else means a stale context, pause or new match, so the monitor abstains.
        let seen=crate::screen::vote_clock(&lines[clocks],Some(expected))?;
        let d=seen.abs_diff(expected);if d>6&&!(d%60<=2||d%60>=58){return None;}
        let clock=expected;
        let raw=&lines[hp_i];
        let max=c.snapshot.max_hp?;if max<100{return None;}
        let bar=crate::vitals::bar_ratio(&game).map(|b|(b/crate::vitals::BAR_FULL).min(1.));
        let value=raw.trim().replace(',',"");
        let digits=(!value.is_empty()&&value.chars().all(|c|c.is_ascii_digit())).then(||value.parse::<u32>().ok()).flatten().filter(|hp|*hp<=max);
        let hp=match digits{
            // The health bar must agree; a truncated tracked box is re-acquired by the full scan.
            Some(hp)=>{if bar.is_some_and(|b|!crate::vitals::bar_agrees(hp as f32/max as f32,b)){return None;}hp}
            // Unreadable digits (the low-health damage vignette): the bar alone is still a measurement.
            None=>(bar?*max as f32).round() as u32,
        };
        let v=self.vitals.accept(hp,max,now);
        let mut snapshot=c.snapshot.clone();snapshot.game_clock_secs=Some(clock);snapshot.hp=Some(hp);snapshot.health_observations=v.observations;snapshot.health_delta_ratio=v.delta;
        if !rich {
            // Keep conservative health/timer monitoring briefly during full-worker recovery,
            // but never reuse old economy, team identity or map evidence for aggressive plans.
            snapshot.minimap=None;snapshot.hero_status.clear();snapshot.own_networth=None;snapshot.souls=None;
            return Some(snapshot);
        }
        let map=frame.crop_imm(c.map.x,c.map.y,c.map.w,c.map.h).to_rgb8();
        snapshot.minimap=Some(self.map.observe(crate::minimap::Analyzer::new().analyze(&map),now));
        // CRITICAL banners, identities and timers come from the latest full scan (seeded context);
        // health bars / dead portraits are pixel reads refreshed every fast sample.
        if snapshot.hero_status.len()==12{
            if self.previous_team.len()==12{for(t,p) in snapshot.hero_status.iter_mut().zip(&self.previous_team){
                t.health_ratio=p.health_ratio;t.health_observations=p.health_observations;}}
            // Respawn timers are the full scan's reads, `age` seconds old: they count down meanwhile
            // (rec16 17:01 / 30:07 said "the first back in about 14 / 28 seconds" for 10 / 25).
            crate::team::age_respawn(&mut snapshot.hero_status,((now-c.timestamp_ms)/1000) as u32);
            let dy=crate::team::top_bar_offset(&game);
            crate::team::apply_bars_at(&crate::team::top_bar_aligned(&game,dy),&mut snapshot.hero_status,Some(now));
        }
        if let Some(library)=&self.library{crate::team::positions(&mut snapshot,&map,library);}else{for slot in &mut snapshot.hero_status{slot.position=None;slot.lane=None;slot.position_observations=0;}}
        self.previous_team=snapshot.hero_status.clone();
        Some(snapshot)
    }
}
