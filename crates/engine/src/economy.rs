//! Relative earned-souls pace; unspent balance is never used as earned net worth.
use coach_core::GameSnapshot;
use crate::{Cue,Rule,Priority};
pub struct PaceRule;
impl Rule for PaceRule{
    fn evaluate(&self,s:&GameSnapshot)->Vec<Cue>{
        let Some(clock)=s.game_clock_secs.filter(|t|*t>=180)else{return vec![]};
        if s.dead||s.screen.as_deref()!=Some("Gameplay"){return vec![];}
        let Some(own)=s.own_networth else{return vec![]};
        let Some(hero)=s.hero.as_ref()else{return vec![]};
        if !s.hero_status.iter().any(|p|p.ally&&p.hero.as_ref()==Some(hero)&&p.networth==Some(own)&&p.networth_observations>=2){return vec![];}
        let mut worth:Vec<_>=s.hero_status.iter().filter(|p|p.networth_observations>=2).filter_map(|p|p.networth).collect();
        if worth.len()<6{return vec![];}worth.sort_unstable();let median=worth[worth.len()/2].max(1);let ratio=own as f32/median as f32;
        let (band,text)=if ratio<0.8 {("behind",format!("Your earned souls are behind the observed lobby at {} minutes. Prioritize a safe wave, then a nearby camp instead of a speculative gank.",clock/60))}
            else if ratio>=1.2 {("ahead","Good farm pace. Your earned souls are ahead of the observed lobby; protect that lead and convert wave priority into objectives.".into())}
            else {("on-track","Good farming. Your earned souls are keeping pace with the observed lobby. Keep catching waves between rotations.".into())};
        vec![Cue{key:format!("pace:{band}"),text,priority:Priority::Low,cooldown_secs:120}]
    }
}
/// Lane-by-lane wave summary. A recorded match repeated "contested near the middle" 12 times:
/// a report is spoken only when some lane needs action (pushed out or coming toward you), and
/// the same report is not repeated for 5 minutes; a changed report waits at least 45 s.
#[derive(Default)]
pub struct WaveReportRule{last:std::cell::RefCell<Option<(String,u32)>>}
impl Rule for WaveReportRule{
    fn evaluate(&self,s:&GameSnapshot)->Vec<Cue>{
        if s.dead||s.screen.as_deref()!=Some("Gameplay"){return vec![];}
        let Some(clock)=s.game_clock_secs else{return vec![]};
        let Some(map)=s.minimap.as_ref().filter(|m|m.observations>=3)else{return vec![]};
        let mut parts=vec![];let mut actionable=false;
        for(lane,name)in [(coach_core::Lane::Green,"Green"),(coach_core::Lane::Blue,"Blue"),(coach_core::Lane::Yellow,"Yellow")]{
            let v=map.lane(lane);if v.ally_troopers+v.enemy_troopers<2{continue;}
            if let Some(front)=v.push {
                let state=if front>=0.7{"pushed out"}else if front<0.35{"coming toward our side"}else{"contested near the middle"};
                actionable|=front>=0.7||front<0.35;parts.push(format!("{name} {state}"));
            }
        }
        if parts.is_empty()||!actionable{return vec![];}
        let summary=parts.join(", ");
        let mut last=self.last.borrow_mut();
        if let Some((prev,at))=last.as_ref(){
            let since=clock.saturating_sub(*at);
            if clock>=*at&&(since<45||*prev==summary&&since<300){return vec![];}
        }
        *last=Some((summary.clone(),clock));
        vec![Cue{key:"wave:overview".into(),text:format!("Wave check: {summary}. Catch incoming farm before roaming."),priority:Priority::Low,cooldown_secs:45}]
    }
}