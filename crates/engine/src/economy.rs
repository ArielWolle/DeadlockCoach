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
pub struct WaveReportRule;
impl Rule for WaveReportRule{
    fn evaluate(&self,s:&GameSnapshot)->Vec<Cue>{
        if s.dead||s.screen.as_deref()!=Some("Gameplay"){return vec![];}
        let Some(map)=s.minimap.as_ref().filter(|m|m.observations>=3)else{return vec![]};
        let mut parts=vec![];
        for(lane,name)in [(coach_core::Lane::Green,"Green"),(coach_core::Lane::Blue,"Blue"),(coach_core::Lane::Yellow,"Yellow")]{
            let v=map.lane(lane);if v.ally_troopers+v.enemy_troopers<2{continue;}
            if let Some(front)=v.push {parts.push(format!("{name} {}",if front>=0.7{"pushed out"}else if front<0.35{"coming toward our side"}else{"contested near the middle"}));}
        }
        if parts.is_empty(){return vec![];}
        vec![Cue{key:"wave:overview".into(),text:format!("Wave check: {}. Catch incoming farm before roaming.",parts.join(", ")),priority:Priority::Low,cooldown_secs:75}]
    }
}
