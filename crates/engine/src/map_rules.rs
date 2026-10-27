//! Map-aware coaching. Lane colour/front estimates and trooper counts are heuristics,
//! not omniscient state: missing heroes, camps and icons are never assumed safe/dead.
use crate::{Cue, Priority, Rule};
use coach_core::{GameSnapshot, Lane, UrnState};

pub struct MapRule;

fn name(l: Lane) -> &'static str {
    match l { Lane::Green => "Green", Lane::Blue => "Blue", Lane::Yellow => "Yellow" }
}

fn cue(key: String, text: String, priority: Priority, cooldown_secs: u32) -> Cue {
    Cue { key, text, priority, cooldown_secs }
}

impl Rule for MapRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let (Some(map), Some(clock)) = (&snap.minimap, snap.game_clock_secs) else { return vec![] };
        if snap.dead || map.observations < 3 { return vec![]; }
        let mut out = vec![];
        let poor_health=snap.hp.zip(snap.max_hp).is_some_and(|(hp,max)|max>0 && hp as f32/(max as f32)<0.65);
        let side = map.urn_pos.map(|p| if p.x < 0.45 { "Green side" }
            else if p.x > 0.65 { "Yellow side" } else { "the middle of the map" }).unwrap_or("the map");
        if clock >= 600 {
            match map.urn {
                Some(UrnState::CarriedByEnemy) => out.push(cue("map:urn-enemy".into(),
                    format!("The urn appears to be moving with an enemy on {side}. Group up before contesting the delivery."),
                    Priority::High, 45)),
                Some(UrnState::CarriedByYou) => out.push(cue("map:urn-you".into(),
                    "You are carrying the urn. Head for delivery and avoid isolated fights.".into(), Priority::High, 60)),
                Some(UrnState::CarriedByAlly) => out.push(cue("map:urn-ally".into(),
                    format!("The urn appears to be moving with an ally on {side}. Help escort them if you are nearby."), Priority::Normal, 90)),
                Some(UrnState::Ground) => out.push(cue("map:urn-ground".into(),
                    format!("The urn marker is at the {side} bridge. Check enemy positions before picking it up."), Priority::Low, 120)),
                _ => {}
            }
        }
        // Don't send the courier off to farm or catch a wave mid-delivery.
        if map.urn == Some(UrnState::CarriedByYou) { return out; }
        let lanes = [Lane::Green, Lane::Blue, Lane::Yellow];
        let mut urgent = false;
        for l in lanes {
            if crate::laning::focus(snap).is_some_and(|focus|focus!=l) {continue;}
            let state = map.lane(l);
            // Both colour-front and actual enemy-dot evidence are needed. No visible dots
            // doesn't mean an empty lane; occluded/unseen waves remain unknown.
            if state.push.is_some_and(|p| p < 0.30)
                && state.enemy_troopers >= 2
                && state.enemy_progress.is_some_and(|p| p < 0.38) {
                urgent = true;
                out.push(cue(format!("map:wave:{l:?}"),
                    format!("Enemy troopers are deep in {} lane. Catch that wave before taking jungle camps.", name(l)),
                    Priority::High, 60));
            }
        }
        let pushed = lanes.iter().filter(|l| map.lane(**l).push.is_some_and(|p| p > 0.65)).count();
        let no_bad_lane = lanes.iter().all(|l| map.lane(*l).push.is_some_and(|p| p >= 0.35));
        if !urgent && !poor_health && pushed >= 2 && no_bad_lane && clock >= 300 && crate::laning::focus(snap).is_none() {
            out.push(cue("map:farm-window".into(),
                "Two lanes look pushed. There may be time for nearby camps before the next wave. Watch missing enemies.".into(),
                Priority::Low, 150));
        }
        // The Midboss is up from 0:00, so "it is lit" is news only once, and again when it returns
        // after a kill (the tracker confirms a 20 s absence, then a sighting).
        if map.midboss_up && map.midboss_back_ms.is_some_and(|ms| ms <= 30_000) && !urgent {
            out.push(cue("map:midboss-back".into(),
                "The Midboss is back on the map. Push your lanes and gather teammates before considering it.".into(),
                Priority::Normal, 120));
        } else if map.midboss_up && map.midboss_back_ms.is_none() && clock >= 600 && !urgent && !poor_health && no_bad_lane {
            out.push(cue("map:midboss".into(),
                "The Midboss icon is lit. Push your lanes and gather teammates before considering it.".into(),
                Priority::Low, 36_000));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_core::{MinimapState, Pt};

    fn snap() -> GameSnapshot {
        GameSnapshot { game_clock_secs: Some(1000), minimap: Some(MinimapState {
            observations: 3, ..Default::default()
        }), ..Default::default() }
    }

    #[test]
    fn endangered_wave_blocks_jungle_advice() {
        let mut s = snap();
        let m = s.minimap.as_mut().unwrap();
        m.green.push = Some(0.2); m.green.enemy_troopers = 3; m.green.enemy_progress = Some(0.22);
        m.blue.push = Some(0.8); m.yellow.push = Some(0.8); m.midboss_up = true;
        let cues = MapRule.evaluate(&s);
        assert!(cues.iter().any(|c| c.key == "map:wave:Green"));
        assert!(!cues.iter().any(|c| c.key == "map:farm-window" || c.key == "map:midboss"));
    }

    #[test]
    fn pushed_lanes_create_conditional_farming_window() {
        let mut s = snap(); let m = s.minimap.as_mut().unwrap();
        m.green.push = Some(0.7); m.blue.push = Some(0.72); m.yellow.push = Some(0.5);
        assert!(MapRule.evaluate(&s).iter().any(|c| c.key == "map:farm-window"));
        s.minimap.as_mut().unwrap().yellow.push = None;
        assert!(!MapRule.evaluate(&s).iter().any(|c| c.key == "map:farm-window"));
    }

    #[test]
    fn enemy_urn_overrides_farming_priority_and_unknown_possession_is_silent() {
        let mut s = snap(); let m = s.minimap.as_mut().unwrap();
        m.urn = Some(UrnState::CarriedByEnemy); m.urn_pos = Some(Pt { x: 0.8, y: 0.4 });
        let c = MapRule.evaluate(&s); assert_eq!(c[0].priority, Priority::High);
        assert!(c[0].text.contains("Yellow side"));
        s.minimap.as_mut().unwrap().urn = Some(UrnState::Visible);
        assert!(MapRule.evaluate(&s).is_empty());
        s.dead = true; s.minimap.as_mut().unwrap().urn = Some(UrnState::CarriedByEnemy);
        assert!(MapRule.evaluate(&s).is_empty());
    }

    #[test]
    fn courier_gets_delivery_advice_not_farming_advice() {
        let mut s = snap(); let m = s.minimap.as_mut().unwrap();
        m.urn = Some(UrnState::CarriedByYou);
        m.green.push = Some(0.2); m.green.enemy_troopers = 3; m.green.enemy_progress = Some(0.2);
        let c = MapRule.evaluate(&s);
        assert_eq!(c.len(), 1); assert_eq!(c[0].key, "map:urn-you");
    }
}
