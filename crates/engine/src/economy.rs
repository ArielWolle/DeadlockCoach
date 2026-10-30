//! Relative earned-souls pace; unspent balance is never used as earned net worth.
use coach_core::GameSnapshot;
use crate::{Cue,Rule,Priority};

/// The item tier normally bought at `clock` (shop costs 800 / 1,600 / 3,200 / 6,400): spend
/// reminders start only once you can afford it. Holding less is saving for the next item, not
/// hoarding: a recorded match nagged about 1,633 souls at 32:18 and while saving for a 6,400
/// item. Coach heuristic, see docs/strategy/economy-and-buy-order.md section 4.
pub fn spend_target(clock: u32) -> u32 { if clock < 600 { 1600 } else if clock < 1200 { 3200 } else { 6400 } }
/// Souls worth naming at a death (the death screen is a shop window): the tier below the target.
pub fn death_spend(clock: u32) -> u32 { (spend_target(clock) / 2).max(1200) }
/// Farm pace against the observed lobby. A band (behind / on track / ahead) is spoken once it
/// held on 3 readings over at least 15 s (rec12 said "ahead" at 3:32 and "on track" at 3:40), when
/// it changes; only "behind" is repeated, after 5 minutes (rec6 said "Good farming" 16 times).
#[derive(Default)]
pub struct PaceRule{last:std::cell::Cell<Option<(&'static str,u32)>>,seen:std::cell::RefCell<Vec<(&'static str,u32)>>,
    /// Highest plausible own net worth read: earned souls never fall, so a reading 10% under it
    /// is a misread (rec11 read 7,100 from 14:42 after 11,000 at 14:39 and said "behind").
    peak:std::cell::Cell<u32>,prev_own:std::cell::Cell<u32>}
/// Readings and seconds a band must hold before it is spoken.
pub const PACE_STABLE_READS:usize=3;
pub const PACE_STABLE_SECS:u32=15;
/// Smallest souls gap to the lobby median for "behind" / "ahead" (net worth reads in hundreds;
/// at 4:00 a 20% band is ~500 souls, inside the reading noise).
pub const PACE_MIN_GAP:u32=1000;
impl Rule for PaceRule{
    fn evaluate(&self,s:&GameSnapshot)->Vec<Cue>{
        let clock=s.game_clock_secs.unwrap_or(0);
        let mut seen=self.seen.borrow_mut();
        if seen.last().is_some_and(|x|clock+120<x.1){seen.clear();self.last.set(None);self.peak.set(0);self.prev_own.set(0);}
        let Some((band,cue))=self.assess(s) else{return vec![]};
        let own=s.own_networth.unwrap_or(0);
        // The peak counts readings seen twice in a row (within 5%): one high misread is not a peak.
        let prev=self.prev_own.replace(own);
        if prev.abs_diff(own)*20<=own{self.peak.set(self.peak.get().max(own.min(prev)));}
        if own*10<self.peak.get()*9{return vec![];}
        if seen.last().is_some_and(|x|x.0!=band){seen.clear();}
        seen.push((band,clock));
        if seen.len()<PACE_STABLE_READS||clock<seen[0].1+PACE_STABLE_SECS{return vec![];}
        if self.last.get().is_some_and(|(b,t)|b==band&&(band!="behind"||clock>=t&&clock<t+300)){return vec![];}
        self.last.set(Some((band,clock)));
        vec![cue]
    }
}
impl PaceRule{
    /// The band of one reading (no stability check), when the data is reliable: your own top-bar
    /// net worth read twice and plausible next to the lobby, at least six lobby readings.
    pub fn assess(&self,s:&GameSnapshot)->Option<(&'static str,Cue)>{
        let clock=s.game_clock_secs.filter(|t|*t>=180)?;
        if s.dead||s.screen.as_deref()!=Some("Gameplay"){return None;}
        let own=s.own_networth?;
        let hero=s.hero.as_ref()?;
        if !s.hero_status.iter().any(|p|p.ally&&p.hero.as_ref()==Some(hero)&&p.networth==Some(own)&&p.networth_observations>=2){return None;}
        let median=|v:&mut Vec<u32>|{v.sort_unstable();v.get(v.len()/2).copied()};
        let mut all:Vec<u32>=s.hero_status.iter().filter(|p|p.networth_observations>=2).filter_map(|p|p.networth).collect();
        let rough=median(&mut all.clone())?.max(1);
        // A digit misread (rec12 read 8,000 for a 2,400 slot at 2:55-3:13) is not a lobby value.
        let ok=|v:u32|v*5>=rough*2&&v*2<=rough*5;
        if !ok(own){return None;}
        all.retain(|v|ok(*v));
        if all.len()<6{return None;}
        let lobby=median(&mut all)?.max(1);
        let ratio=own as f32/lobby as f32;
        // Under 0.8 but within the minimum gap is reading noise, not a band: no call.
        let band=if ratio<0.8 { if lobby-own.min(lobby)>=PACE_MIN_GAP {"behind"} else {return None} }
            else if ratio>=1.2&&own.saturating_sub(lobby)>=PACE_MIN_GAP {"ahead"} else {"on-track"};
        // "Behind" never contradicts a team that is ahead: then it needs you behind the enemy
        // heroes too (rec12 4:00 said "behind" with the team even to ahead); else no call.
        let team_ahead=s.ally_networth.zip(s.enemy_networth).is_some_and(|(a,e)|e>0&&a*100>=e*105&&a<=e*2);
        if band=="behind"&&team_ahead{
            let mut enemy:Vec<u32>=s.hero_status.iter().filter(|p|!p.ally&&p.networth_observations>=2).filter_map(|p|p.networth).filter(|v|ok(*v)).collect();
            if median(&mut enemy).is_none_or(|e|own*100>=e*85){return None;}
        }
        let text=match band {
            "behind"=>format!("You are behind the lobby on farm at {} minutes. Safe wave first, then a nearby camp.",clock/60),
            "ahead"=>"Good farm: you are ahead of the lobby. Turn it into objectives with us.".into(),
            _=>"Good farm, on pace with the lobby. Keep catching waves between rotations.".into(),
        };
        Some((band,Cue::new(format!("pace:{band}"),text,Priority::Low,120)))
    }
}
/// Lane-by-lane wave summary. A recorded match repeated "contested near the middle" 12 times:
/// a report is spoken only when some lane needs action (pushed out or coming toward you), and
/// the same report is not repeated for 5 minutes; a changed report waits at least 90 s.
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
                let state=if front>=0.7{"is pushed out"}else if front<0.35{"is coming to us"}else{"is even"};
                actionable|=front>=0.7||front<0.35;parts.push(format!("{name} {state}"));
            }
        }
        if parts.is_empty()||!actionable{return vec![];}
        let summary=parts.join(", ");
        let mut last=self.last.borrow_mut();
        if let Some((prev,at))=last.as_ref(){
            let since=clock.saturating_sub(*at);
            // rec8 still spoke 11-17 overviews in 38 minutes at 45 s: a changed report waits 90 s.
            if clock>=*at&&(since<90||*prev==summary&&since<300){return vec![];}
        }
        *last=Some((summary.clone(),clock));
        vec![Cue::new("wave:overview",format!("Waves: {summary}. Catch the farm before you roam."),Priority::Low,45)]
    }
}