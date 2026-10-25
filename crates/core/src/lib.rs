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
pub struct GameSnapshot {
    pub game_clock_secs: Option<u32>,
    pub souls: Option<u32>,
    pub kills: Option<u32>,
    pub deaths: Option<u32>,
    pub assists: Option<u32>,
    pub hero: Option<String>,
    pub items: Vec<String>,
    pub enemy_items: Vec<String>,
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
