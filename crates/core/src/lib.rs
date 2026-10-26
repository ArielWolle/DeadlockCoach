//! Shared types for DeadlockCoach (Agent <-> Brain).
use serde::{Deserialize, Serialize};

/// Which half of the system this process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Gaming PC: captures screen, plays audio.
    Agent,
    /// Analysis PC: vision, state, coaching, main GUI.
    Brain,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        match s.to_ascii_lowercase().as_str() {
            "agent" => Some(Role::Agent),
            "brain" => Some(Role::Brain),
            _ => None,
        }
    }
}

/// Metadata the Agent sends with every captured frame so the Brain can
/// auto-detect resolution / scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMeta {
    pub width: u32,
    pub height: u32,
    pub dpi_scale: f32,
    pub timestamp_ms: u64,
}

/// Structured result of reading one screenshot. Fields are optional because
/// any HUD element may fail to parse on a given frame.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSnapshot {
    pub game_clock_secs: Option<u32>,
    pub souls: Option<u32>,
    pub kills: Option<u32>,
    pub deaths: Option<u32>,
    pub assists: Option<u32>,
    pub hero: Option<String>,
    pub items: Vec<String>,
    pub enemy_items: Vec<String>,
    /// Hero class names (e.g. "hero_inferno") from the top bar; empty until confident.
    pub ally_heroes: Vec<String>,
    pub enemy_heroes: Vec<String>,
    /// Team net worth totals from the top bar.
    pub ally_networth: Option<u32>,
    pub enemy_networth: Option<u32>,
    pub hp: Option<u32>,
    pub max_hp: Option<u32>,
    /// Consecutive plausible personal HP/max-HP reads. Not a probability.
    pub health_observations: u32,
    pub health_delta_ratio: Option<f32>,
    /// Explicit user context; kept separate from inferred minimap identity.
    pub coaching_context: CoachingContext,
    /// True while the player is dead/spectating (personal HUD values are not the player's own).
    pub dead: bool,
    pub respawn_secs: Option<u32>,
    /// Top source in the "Incoming damage (last death)" panel, e.g. "Temple Guardian".
    pub death_cause: Option<String>,
    /// Match id from the bottom-right corner; changes between matches.
    pub match_id: Option<u64>,
    /// Filled only on frames showing the scoreboard (Tab): 12 rows, own team first.
    pub players: Vec<PlayerRow>,
    /// Which screen the frame showed ("Gameplay", "Shop", ...), for debugging.
    pub screen: Option<String>,
    /// Read from the minimap; None if it could not be found on this frame.
    pub minimap: Option<MinimapState>,
}

/// Lanes by colour: Yellow = right, Blue = centre, Green = left (as seen on the minimap,
/// own base at the bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lane { Green, Blue, Yellow }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CoachingContext { pub focus_lane: Option<Lane> }

/// Positions are normalised minimap coordinates, (0,0) top-left, (1,1) bottom-right.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Pt { pub x: f32, pub y: f32 }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaneState {
    /// Own-team troopers seen in this lane and how far up the lane they are
    /// (0 = own base, 1 = enemy base), averaged.
    pub ally_troopers: u32,
    pub ally_progress: Option<f32>,
    pub enemy_troopers: u32,
    pub enemy_progress: Option<f32>,
    /// Fraction of the lane line drawn in your colour (rest is red). 0 = the enemy has pushed
    /// to your base, 1 = you have pushed to theirs. This is the lane "front".
    pub push: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrnState {
    /// Vase marker detected, but possession has not been established.
    Visible,
    /// Lying on the ground (spawned, nobody carries it).
    Ground,
    /// Carried by this team's hero.
    CarriedByAlly,
    CarriedByEnemy,
    CarriedByYou,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MinimapState {
    /// Number of recent supporting observations. Rules require at least three.
    pub observations: u32,
    pub green: LaneState,
    pub blue: LaneState,
    pub yellow: LaneState,
    /// Positive detection of the gold icon. False means not confirmed, NOT "boss dead".
    pub midboss_up: bool,
    pub urn: Option<UrnState>,
    pub urn_pos: Option<Pt>,
    /// Enemy heroes currently visible on the minimap (red ring), with positions.
    pub enemies: Vec<Pt>,
    /// Allied hero rings; the own candidate is excluded when identified.
    pub allies: Vec<Pt>,
    /// Estimated number of visible enemy heroes (clusters are split by size).
    pub enemy_count: u32,
    pub you: Option<Pt>,
    /// Lane you are standing in, if you are near one.
    pub your_lane: Option<Lane>,
    pub your_progress: Option<f32>,
}

impl MinimapState {
    pub fn lane(&self, l: Lane) -> &LaneState {
        match l { Lane::Green => &self.green, Lane::Blue => &self.blue, Lane::Yellow => &self.yellow }
    }
}

/// One scoreboard column.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerRow {
    pub ally: bool,
    /// Hero class name (e.g. "hero_inferno") if recognised.
    pub hero: Option<String>,
    pub kills: Option<u32>,
    pub deaths: Option<u32>,
    pub assists: Option<u32>,
    /// Item class names (e.g. "upgrade_clip_size"). Empty for the local player.
    pub items: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_parses() {
        assert_eq!(Role::parse("Agent"), Some(Role::Agent));
        assert_eq!(Role::parse("brain"), Some(Role::Brain));
        assert_eq!(Role::parse("x"), None);
    }
}
