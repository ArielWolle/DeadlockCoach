//! Source-grounded lane policy. A recommendation is a single prioritized plan, with the
//! observations and missing information exposed separately from the spoken cooldown.
use crate::{Cue, Priority, Rule};
use coach_core::{GameSnapshot, Lane, UrnState};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source { pub path:String, pub section:String, pub basis:String, pub excerpt:String }
#[derive(Clone, Deserialize)]
pub struct Card { pub id:String, pub speech:String, pub rationale:String, pub cooldown_secs:u32, pub sources:Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Thresholds { pub low_health:f32, pub healthy:f32, pub damage_drop:f32, pub danger_front:f32, pub crashed_front:f32,
    pub camp_min_clock:u32, pub laning_end:u32, pub nearby_radius:f32 }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version:u32, pub thresholds:Thresholds, pub cards:Vec<Card> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Action { LastHit, CatchWave, HoldWave, BackOff, PushWave, Poke, Jungle, Gank, Shop, PrepareObjective, NeedEvidence }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub action:Action, pub text:String, pub rationale:String, pub lane:Option<Lane>, pub lane_source:String,
    pub evidence:Vec<String>, pub missing:Vec<String>, pub sources:Vec<Source>, pub conditional:bool,
    pub cue_key:String, pub priority:Priority, pub cooldown_secs:u32, pub speak:bool,
}

#[derive(Clone)]
pub struct LaneCoach { catalog:Arc<Catalog>, position:crate::positioning::PositionCoach, jungle:crate::jungle::JungleCoach }
impl Default for LaneCoach {
    fn default() -> Self { Self { catalog:Arc::new(serde_json::from_str(include_str!("../../../data/coaching/laning.json")).expect("shipped lane policy")), position:Default::default(), jungle:Default::default() } }
}
pub fn lane_name(l:Lane) -> &'static str { match l { Lane::Green=>"Green",Lane::Blue=>"Blue",Lane::Yellow=>"Yellow" } }
pub fn focus(s:&GameSnapshot) -> Option<Lane> { s.coaching_context.focus_lane.or_else(|| s.minimap.as_ref().and_then(|m| m.your_lane)) }

impl LaneCoach {
    fn plan(&self,id:&str,action:Action,s:&GameSnapshot,priority:Priority,evidence:Vec<String>,missing:Vec<String>,conditional:bool) -> Decision {
        let c=self.catalog.cards.iter().find(|c| c.id==id).expect("policy card"); let lane=focus(s);
        let lane_source=if s.coaching_context.focus_lane.is_some() {"manual focus"} else if lane.is_some() {"minimap estimate"} else {"not identified"};
        let name=lane.map(|l| format!("{} lane",lane_name(l))).unwrap_or("The lane".into());
        let cue_key=if id=="catch" {format!("map:wave:{:?}",lane.unwrap())} else if id=="jungle" {"map:farm-window".into()} else {format!("lane:{id}")};
        Decision {action,text:c.speech.replace("{lane}",&name),rationale:c.rationale.clone(),lane,lane_source:lane_source.into(),
            evidence,missing,sources:c.sources.clone(),conditional,cue_key,priority,cooldown_secs:c.cooldown_secs,speak:true}
    }
    /// The lane plan, reconciled with positional danger. Gank/outnumbered warnings are spoken by
    /// the positioning coach; the plan then holds silently instead of contradicting it, and
    /// aggressive plans are muted while enemies near you are missing.
    pub fn decide(&self,s:&GameSnapshot) -> Option<Decision> {
        let mut d=self.decide_lane(s)?;
        // A specific invade / stay-on-your-side call replaces the generic camp suggestion.
        if matches!(d.action,Action::Jungle|Action::HoldWave)&&self.jungle.invade(s).is_some(){d.speak=false;}
        let Some(danger)=self.position.assess(s) else {return Some(d)};
        let aggressive=matches!(d.action,Action::Poke|Action::PushWave|Action::Jungle|Action::Gank|Action::PrepareObjective);
        if matches!(danger.id.as_str(),"gank"|"outnumbered") && d.action!=Action::BackOff {
            let mut hold=self.plan("hold",Action::HoldWave,s,Priority::High,d.evidence.clone(),d.missing.clone(),true);
            hold.text=danger.text.clone();hold.evidence.extend(danger.evidence);hold.speak=false;d=hold;
        } else if matches!(danger.id.as_str(),"missing"|"unseen"|"allies_down") && aggressive {
            d.evidence.extend(danger.evidence);d.speak=false;
        }
        Some(d)
    }
    fn decide_lane(&self,s:&GameSnapshot) -> Option<Decision> {
        let clock=s.game_clock_secs?;
        if s.dead || s.screen.as_deref().is_some_and(|v| v!="Gameplay") {return None;}
        let cfg=&self.catalog.thresholds;
        let health=s.hp.zip(s.max_hp).filter(|(hp,max)| *max>=100 && hp<=max).map(|(hp,max)| hp as f32/max as f32);
        let mut evidence=vec![]; let mut missing=vec![];
        if let Some(h)=health {evidence.push(format!("Personal health OCR: {:.0}% ({} reads)",h*100.0,s.health_observations));}
        else {missing.push("Personal health could not be read".into());}
        // Low health needs two consistent reads; a sudden drop needs a real burst (a recorded match
        // showed a stuck misread and small trades triggering this far too often).
        if health.is_some_and(|h| h<cfg.low_health) && s.health_observations>=2 || s.health_delta_ratio.is_some_and(|d| d < -cfg.damage_drop) {
            if let Some(delta)=s.health_delta_ratio {evidence.push(format!("Health changed by {:.0} percentage points since the previous read",delta*100.0));}
            return Some(self.plan("back_off",Action::BackOff,s,Priority::High,evidence,missing,s.health_observations<2));
        }
        if s.minimap.as_ref().is_some_and(|m| m.urn==Some(UrnState::CarriedByYou)) {return None;}
        let lane=focus(s);
        let Some(map)=s.minimap.as_ref().filter(|m| m.observations>=3) else {
            missing.push("Need three recent minimap observations".into());
            let mut d=self.plan("last_hit",Action::LastHit,s,Priority::Low,evidence,missing,true);
            d.speak=clock<180; if !d.speak {d.action=Action::NeedEvidence;d.text="Waiting for stable lane evidence. Select a lane focus or inspect the minimap detector results.".into();} return Some(d);
        };
        let Some(lane)=lane else {
            missing.push("Current lane is not identified; set a lane focus to enable personal wave advice".into());
            let mut d=self.plan("last_hit",Action::LastHit,s,Priority::Low,evidence,missing,true);d.speak=clock<180;
            if !d.speak {d.action=Action::NeedEvidence;d.text="Choose a lane focus, or wait for a confirmed player marker.".into();} return Some(d);
        };
        let state=map.lane(lane);
        evidence.push(format!("{} lane: {} allied / {} enemy trooper-dot candidates",lane_name(lane),state.ally_troopers,state.enemy_troopers));
        if let Some(p)=state.push {evidence.push(format!("Stable lane-front estimate: {:.0}% up lane",p*100.0));}
        else {missing.push("Lane front is not stable".into());}
        let nearby=map.you.map(|me| {
            let near=|p:&coach_core::Pt| (p.x-me.x).hypot(p.y-me.y)<=cfg.nearby_radius;
            // Prefer confirmed, currently visible tracks; single-frame rings are a fallback.
            if map.heroes.is_empty() {(map.allies.iter().filter(|p| near(p)).count() as u32+1,map.enemies.iter().filter(|p| near(p)).count() as u32)}
            else {let n=|ally:bool| map.heroes.iter().filter(|h|h.ally==ally&&h.confirmed()&&h.visible()&&near(&h.pos)).count() as u32;(n(true)+1,n(false))}
        });
        if let Some((a,e))=nearby {evidence.push(format!("Nearby visible hero rings: {a} allied including you / {e} enemy"));}
        else {missing.push("Player position is unconfirmed; nearby hero counts are unknown".into());}
        if nearby.is_some_and(|(a,e)| e>a) {
            return Some(self.plan("hold",Action::HoldWave,s,Priority::High,evidence,missing,true));
        }
        if state.push.is_some_and(|p| p<cfg.danger_front) && state.enemy_troopers>=2 && state.enemy_progress.is_some_and(|p| p<0.4) {
            return Some(self.plan("catch",Action::CatchWave,s,Priority::High,evidence,missing,false));
        }
        let healthy=health.is_some_and(|h| h>=cfg.healthy) && s.health_observations>=2;
        let in_lane=map.your_lane==Some(lane) && map.you.is_some();
        if in_lane && nearby.is_some_and(|(_,e)|e>=1) && map.your_progress.zip(state.ally_progress).is_some_and(|(you,wave)|you>0.55 && you>wave+0.12) {
            return Some(self.plan("overextended",Action::BackOff,s,Priority::High,evidence,missing,true));
        }
        let crashed=state.push.is_some_and(|p| p>=cfg.crashed_front) && state.ally_troopers>=2 && state.ally_progress.is_some_and(|p| p>=0.65);
        let endangered=[Lane::Green,Lane::Blue,Lane::Yellow].iter().any(|l| {
            let v=map.lane(*l);v.push.is_some_and(|p| p<0.3)&&v.enemy_troopers>=2&&v.enemy_progress.is_some_and(|p|p<0.38)
        });
        let objective_soon=clock>=570 && (600u32.saturating_sub(clock)<=30 && clock<600 || clock>=600 && (300-clock%300)<=30);
        let grouped_boss=clock>=600 && map.midboss_up && map.allies.iter().filter(|p|(p.x-0.484).hypot(p.y-0.5)<0.14).count()>=3;
        if crashed && healthy && in_lane && (objective_soon || grouped_boss) {
            evidence.push(if grouped_boss {"Allied markers are clustered near the lit Midboss icon".into()} else {"A scheduled Soul Urn spawn is within 30 seconds".into()});
            missing.push("Team intent and actual objective contest state are not detected".into());
            return Some(self.plan("rotate",Action::PrepareObjective,s,Priority::Normal,evidence,missing,true));
        }
        if crashed && healthy && in_lane && s.souls.is_some_and(|v| v>=1500) {
            return Some(self.plan("shop",Action::Shop,s,Priority::Normal,evidence,missing,true));
        }
        if crashed&&healthy&&in_lane&&!endangered&&!objective_soon {
            let target=s.hero_status.iter().find(|p|!p.ally&&p.critical==Some(true)&&p.observations>=2&&p.position_observations>=2
                &&p.lane.is_some_and(|l|l!=lane)&&p.position.is_some_and(|target|map.you.is_some_and(|me|(target.x-me.x).hypot(target.y-me.y)<0.45))
                &&s.hero_status.iter().any(|ally|ally.ally&&ally.critical==Some(false)&&ally.observations>=2&&ally.position_observations>=2
                    &&ally.position.zip(p.position).is_some_and(|(a,b)|(a.x-b.x).hypot(a.y-b.y)<0.2)));
            if let Some(target)=target {
                evidence.push(format!("{} has a confirmed critical-health label and a matched minimap position",target.hero.as_deref().unwrap_or("Enemy")));
                missing.push("Exact enemy/ally HP, cooldowns, hidden enemies and the rotation path are not fully observed".into());
                let mut d=self.plan("gank",Action::Gank,s,Priority::Normal,evidence,missing,true);
                d.text=d.text.replace("{target}",target.hero.as_deref().unwrap_or("the critical enemy")).replace("{target_lane}",lane_name(target.lane.unwrap()));return Some(d);
            }
        }
        if crashed && healthy && in_lane && nearby.is_some_and(|(_,e)|e==0) && !endangered && clock>=cfg.camp_min_clock && !objective_soon {
            missing.push("No vulnerable enemy has a confirmed matching health label, position and nearby support for a specific gank".into());
            missing.push("Camp availability, enemy cooldowns and unseen enemies are unknown".into());
            return Some(self.plan("jungle",Action::Jungle,s,Priority::Normal,evidence,missing,true));
        }
        if crashed&&healthy&&in_lane {return Some(self.plan("wave_good",Action::HoldWave,s,Priority::Low,evidence,missing,true));}
        let behind_wave=map.your_progress.zip(state.ally_progress).is_some_and(|(you,wave)| you<=wave+0.06 && you<=0.65);
        if healthy && in_lane && behind_wave && nearby.is_some_and(|(_,e)| e>=1)
            && state.ally_troopers>=2 && state.ally_troopers>state.enemy_troopers
            && state.push.is_some_and(|p| (0.45..0.7).contains(&p)) && !s.health_delta_ratio.is_some_and(|d|d < -0.05) {
            missing.push("Enemy cooldowns and escape resources are not detected; avoid committing beyond cover".into());
            return Some(self.plan("poke",Action::Poke,s,Priority::Normal,evidence,missing,true));
        }
        if healthy && in_lane && state.enemy_troopers>0 && state.ally_troopers>=2 && !crashed
            && state.push.is_some_and(|p| (0.35..0.7).contains(&p)) && behind_wave {
            return Some(self.plan(if objective_soon {"objective"} else {"push"},if objective_soon {Action::PrepareObjective} else {Action::PushWave},s,Priority::Normal,evidence,missing,true));
        }
        if !healthy {missing.push("Confirmed healthy readings are required before recommending a trade or jungle detour".into());}
        let mut d=self.plan("last_hit",Action::LastHit,s,Priority::Low,evidence,missing,true);
        d.speak=clock<=cfg.laning_end; Some(d)
    }
}
impl Rule for LaneCoach {
    fn evaluate(&self,s:&GameSnapshot) -> Vec<Cue> {
        self.decide(s).filter(|d|d.speak).map(|d|vec![Cue {key:d.cue_key,text:d.text,priority:d.priority,cooldown_secs:d.cooldown_secs}]).unwrap_or_default()
    }
}
