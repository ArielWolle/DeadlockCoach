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
    /// Earned net worth from the identified own top-bar slot, never unspent balance.
    pub own_networth:Option<u32>,
    pub hero_status:Vec<HeroStatus>,
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
    /// Age of `souls` in ms: 0 when read on this frame, otherwise how long ago the reported
    /// value was confirmed (held for at most 10 s). None when `souls` is None.
    #[serde(default)]
    pub souls_age_ms: Option<u64>,
    /// Age of `respawn_secs` in ms: 0 when read on this frame, otherwise the countdown is
    /// extrapolated from a read this long ago (at most 10 s).
    #[serde(default)]
    pub respawn_age_ms: Option<u64>,
    /// Own hero level from the badge beside the souls counter.
    #[serde(default)]
    pub level: Option<u32>,
    /// Unsecured souls ("N HAUNTED SOULS" under the souls counter), when the line is read.
    /// None means not read, not zero.
    #[serde(default)]
    pub unsecured_souls: Option<u32>,
    /// Rows of the "Incoming damage (last death)" panel, largest first, while dead.
    #[serde(default)]
    pub death_damage: Vec<DamageRow>,
    /// Source of the row tagged "Killing Blow", when the tag was seen.
    #[serde(default)]
    pub death_killing_blow: Option<String>,
    /// "Total Damage: N" from the same panel.
    #[serde(default)]
    pub death_total_damage: Option<u32>,
    /// The damage panel on this death screen: "open" (a complete read confirmed by a second
    /// read: death_damage / death_killing_blow / death_total_damage / death_cause are reported),
    /// "reading" (rows on screen, not confirmed yet), "unreadable" (rows on screen but no
    /// reliable read: disregarded, nothing is reported), "closed" (collapsed to the portrait tab
    /// at the right edge). None = not dead or nothing seen of the panel (e.g. the shop covers it).
    /// Unless "open", no damage-panel field is reported.
    #[serde(default)]
    pub death_panel: Option<String>,
    /// Status effects on you this frame: "stunned", "silenced", "disarmed", "out_of_stamina",
    /// "knocked_off_zipline", "afflicted", "frozen", "slowed" (a movement-speed debuff badge),
    /// "carrying_urn" (the "DELIVER THE URN" prompt), plus debuff badge names (e.g. "Aura of
    /// Suffering"). Empty means none read on this frame (not proof of none).
    #[serde(default)]
    pub status_effects: Vec<String>,
    /// Boss health banner at the top centre ("HIDDEN KING" / "ARCHMOTHER": a Patron is under
    /// attack). None = no banner read on this frame.
    #[serde(default)]
    pub objective_banner: Option<ObjectiveBanner>,
    /// "win" / "loss" when the end-of-match Victory / Defeat banner was read.
    #[serde(default)]
    pub match_result: Option<String>,
    /// Lane structures seen on the minimap (Guardian / Walker / Shrine / Patron). Empty = not read;
    /// a structure is listed only when its state was observed.
    #[serde(default)]
    pub structures: Vec<StructureState>,
    /// Who took the most recent Midboss, when evidence supports it ("ally" / "enemy").
    #[serde(default)]
    pub midboss_taken_by: Option<String>,
    /// Your own ability bar (slots 1-4, 4 = ultimate): ready / cooldown seconds when read.
    #[serde(default)]
    pub abilities: Vec<AbilityState>,
    /// A Rejuvenator revive is seen on this frame: the greyed "Respawn in: 1" death screen right
    /// after a live frame (`dead` stays true there), or, when its countdown was not read, the
    /// live frame that returns within seconds of such a screen (rec8 39:17). Not a real death:
    /// the team spent a Rejuvenator charge.
    #[serde(default)]
    pub revived: bool,
}

/// One lane structure's observed state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StructureState {
    /// "Green" / "Blue" / "Yellow" (lane colour as on your minimap); None for base structures.
    pub lane: Option<Lane>,
    /// "guardian", "walker", "shrine" or "patron".
    pub kind: String,
    /// Your team's structure.
    pub own: bool,
    /// Standing (true) or destroyed (false).
    pub alive: bool,
}

/// One ability slot on your HUD.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AbilityState {
    /// 1-4 (4 = ultimate).
    pub slot: u8,
    /// Ready to cast (no cooldown number shown); None when unread.
    pub ready: Option<bool>,
    /// Cooldown seconds remaining when a number is shown.
    pub cooldown_secs: Option<u32>,
}

/// A top-centre objective health banner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectiveBanner {
    /// Canonical name: "HIDDEN KING" or "ARCHMOTHER".
    pub name: String,
    /// Whether it is your team's objective (Hidden King is yours on a mirrored lane layout,
    /// Archmother otherwise); None while the layout is unknown.
    pub own: Option<bool>,
    /// Remaining health of the bar, 0..1 (both phases together); None when unreadable.
    pub health_ratio: Option<f32>,
}

/// One source row of the incoming-damage panel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DamageRow {
    /// Ability / item / world source, normalised to its dictionary name when matched.
    pub source: String,
    /// Hit count ("x18"), when shown.
    pub hits: Option<u32>,
    pub damage: u32,
}

/// Lanes by colour: Yellow = right, Blue = centre, Green = left (as seen on the minimap,
/// own base at the bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lane { Green, Blue, Yellow }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CoachingContext { pub focus_lane: Option<Lane> }

/// Top-bar health evidence for all twelve slots. "Not critical" does not mean full HP.
#[derive(Debug,Clone,Default,Serialize,Deserialize)]
#[serde(default)]
pub struct HeroStatus {
    pub ally:bool,pub slot:usize,pub hero:Option<String>,pub critical:Option<bool>,
    pub health_ratio:Option<f32>,pub networth:Option<u32>,pub networth_observations:u32,pub observations:u32,
    pub position:Option<Pt>,pub position_observations:u32,pub lane:Option<Lane>,
    /// Whether the top-bar health bar is drawn. Enemy bars are only shown while your team has
    /// recently seen that hero: `Some(false)` means "not seen in a while" (missing), not dead.
    pub bar_visible:Option<bool>,
    /// Consecutive frames with a consistent health-bar reading (`health_ratio`).
    pub health_observations:u32,
    /// Greyed portrait with a respawn timer.
    pub dead:Option<bool>,
    pub respawn_secs:Option<u32>,
    /// A different (bar visible, dead) state seen once; committed when the next frame agrees.
    #[serde(skip)]
    pub pending_state:Option<(Option<bool>,bool)>,
    /// Consecutive respawn-timer reads that agree with a 1 s/s countdown (`respawn_secs`).
    pub respawn_observations:u32,
    /// Last countdown-confirmed respawn read (seconds, frame ms) and an unconfirmed candidate.
    #[serde(skip)]
    pub respawn_anchor:Option<(u32,u64)>,
    #[serde(skip)]
    pub respawn_candidate:Option<(u32,u64)>,
    /// Game clock (s) when `networth` was last read, and a disagreeing value seen in a row.
    #[serde(skip)]
    pub networth_clock:Option<u32>,
    #[serde(skip)]
    pub networth_pending:Option<(u32,u8)>,
}

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
    /// Hero portraits tracked across frames (both teams). Unlike `allies` / `enemies` (this
    /// frame only), tracks remember where a hero was last seen, so "missing" and "approaching"
    /// can be judged. A track that is not visible is NOT known to be dead or safe.
    pub heroes: Vec<HeroTrack>,
    /// Lane layout seen on the map: the side lanes swap between teams (`true` = Yellow on the
    /// left, Green on the right). None when no lane colour was visible.
    pub lanes_mirrored: Option<bool>,
    /// Midboss events from the tracker: milliseconds since its icon disappeared for good (likely
    /// killed), and since it reappeared (respawned). None = no such event.
    pub midboss_down_ms: Option<u64>,
    pub midboss_back_ms: Option<u64>,
    /// Zone label printed under the minimap ("YORK : FACTORY"), normalised. Held briefly
    /// between reads; None when not read recently.
    #[serde(default)]
    pub zone: Option<String>,
    /// Lane named by the zone label (York = Yellow, Broadway = Blue, Greenwich = Green).
    #[serde(default)]
    pub zone_lane: Option<Lane>,
    /// Coarse area of the zone label (base / mid / lane / jungle).
    #[serde(default)]
    pub zone_area: Option<ZoneArea>,
    /// This frame's own lane-colour evidence for the layout (`lanes_mirrored` is the per-match
    /// latched value). Internal to the vision tracker.
    #[serde(skip)]
    pub lanes_mirrored_frame: Option<bool>,
    /// Urn-shaped icons too small or occluded for `urn_pos` (a carried vase drawn with your
    /// view cone). Internal to the vision tracker.
    #[serde(skip)]
    pub urn_marks: Vec<Pt>,
}

/// Coarse area from the minimap zone label (plus your marker for lane vs jungle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoneArea {
    /// A lane's zone (and near its line when your marker is seen).
    Lane,
    /// A lane's zone with your marker seen away from every lane line.
    Jungle,
    Mid,
    OwnBase,
    EnemyBase,
    /// A base whose owner is not known (lane layout not established).
    Base,
}

/// One hero portrait followed across minimap observations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HeroTrack {
    /// Stable within a tracking session only; not a hero identity.
    pub id: u32,
    pub ally: bool,
    /// Last observed position (map fractions, own base at the bottom).
    pub pos: Pt,
    /// Milliseconds since last seen; 0 means visible in this observation.
    pub unseen_ms: u64,
    /// Number of observations matched to this track (>= 2 is confirmed).
    pub hits: u32,
    /// Smoothed velocity in map fractions per second, when seen moving between sightings.
    pub velocity: Option<Pt>,
    /// Nearest lane when the hero is on one.
    pub lane: Option<Lane>,
}

impl HeroTrack {
    pub fn visible(&self) -> bool { self.unseen_ms == 0 }
    pub fn confirmed(&self) -> bool { self.hits >= 2 }
    pub fn distance(&self, p: Pt) -> f32 { (self.pos.x - p.x).hypot(self.pos.y - p.y) }
    /// Speed toward `p` in map fractions per second (positive = closing in).
    pub fn closing_speed(&self, p: Pt) -> f32 {
        let (Some(v), d) = (self.velocity, self.distance(p)) else { return 0.0 };
        if d < 1e-4 { return 0.0; }
        (v.x * (p.x - self.pos.x) + v.y * (p.y - self.pos.y)) / d
    }
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
