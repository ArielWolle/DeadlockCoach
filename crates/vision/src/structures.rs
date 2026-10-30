//! Lane structures on the minimap: Guardians, Walkers, Base Guardians, Shrines and Patrons.
//!
//! Every structure is a diamond at a fixed map position (canonical 335x350 crop, own base at the
//! bottom). Measured on rec5/rec8/rec12/rec13:
//! - a standing structure is drawn in its team colour (enemy red; yours in the lane colour, your
//!   Shrines and Patron cream-white). Damage drains the fill from the top and leaves a dim outline
//!   (rec12 Walker at 420/460/520 frames: only the bottom chevron lit), so the *outline* is tested,
//!   in bright or dim team colour;
//! - a destroyed structure is not drawn at all: only the dark map panel and the lane line remain
//!   (a destroyed Shrine leaves a black hole).
//!
//! Per frame a site reads Alive (>= 70% of its diamond outline and half of every edge in team
//! colour), Gone (no edge-complete outline, the outline on the dark map panel) or Unclear (a
//! portrait, ping, trooper wave or view cone covers it). [`Tracker`] turns reads into states that
//! only go from alive to destroyed. Validated on rec8/rec10/rec12/rec13 (every structure, every
//! frame): no structure came back after falling; checked transitions matched the frames within
//! 1-3 frames (the latch needs 3 Gone reads).
use coach_core::{Lane, StructureState};
use image::RgbImage;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Guardian, Walker, BaseGuardian, Shrine, Patron }

impl Kind {
    /// The snapshot kind (Base Guardians are Guardians of their lane).
    pub fn label(self) -> &'static str {
        match self { Kind::Guardian | Kind::BaseGuardian => "guardian", Kind::Walker => "walker", Kind::Shrine => "shrine", Kind::Patron => "patron" }
    }
}

/// One structure site. `side`: lane polyline 0 = left, 1 = middle, 2 = right (None for base
/// structures). Position in canonical px; `half`: half-diagonal of the diamond.
#[derive(Debug, Clone, Copy)]
pub struct Site { pub own: bool, pub kind: Kind, pub side: Option<usize>, pub x: f32, pub y: f32, pub half: f32 }

const fn site(own: bool, kind: Kind, side: Option<usize>, x3: f32, y3: f32, half: f32) -> Site {
    Site { own, kind, side, x: x3 / 3.0, y: y3 / 3.0, half }
}
use Kind::*;
/// Measured blob centres (3x canonical px, rec13 frame 40: every structure standing). Per team
/// ordered from the lane front to the base, so a lane's first "guardian" is its Guardian and the
/// second its Base Guardian.
pub const SITES: [Site; 24] = [
    site(false, Guardian, Some(0), 176., 453., 6.),
    site(false, Guardian, Some(1), 488., 469., 6.),
    site(false, Guardian, Some(2), 844., 461., 6.),
    site(false, Walker, Some(0), 236., 311., 12.),
    site(false, Walker, Some(1), 540., 383., 12.),
    site(false, Walker, Some(2), 758., 326., 12.),
    site(false, BaseGuardian, Some(0), 440., 244., 7.),
    site(false, BaseGuardian, Some(1), 540., 293., 7.),
    site(false, BaseGuardian, Some(2), 640., 245., 7.),
    site(false, Shrine, None, 470., 185., 5.4),
    site(false, Shrine, None, 610., 185., 5.4),
    site(false, Patron, None, 539., 137., 8.),
    site(true, Guardian, Some(0), 128., 614., 7.),
    site(true, Guardian, Some(1), 485., 615., 6.),
    site(true, Guardian, Some(2), 792., 626., 7.),
    site(true, Walker, Some(0), 203., 744., 12.),
    site(true, Walker, Some(1), 428., 685., 12.),
    site(true, Walker, Some(2), 722., 756., 12.),
    site(true, BaseGuardian, Some(0), 329., 822., 7.),
    site(true, BaseGuardian, Some(1), 429., 774., 7.),
    site(true, BaseGuardian, Some(2), 529., 822., 7.),
    site(true, Shrine, None, 361., 882., 5.),
    site(true, Shrine, None, 495., 882., 5.),
    site(true, Patron, None, 428., 930., 7.),
];
/// Indices of the two Patrons in [`SITES`].
const PATRONS: [usize; 2] = [11, 23];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read { Alive, Gone, Unclear }

// Pixel classes (bit flags).
const RED: u8 = 1;
const LANE: u8 = 2;
const CREAM: u8 = 4;
const DARK: u8 = 8;

fn class(r: u8, g: u8, b: u8) -> u8 {
    let (h, s, v) = crate::minimap::hsv(r, g, b);
    let mut c = 0;
    // Bright or dim (damaged outline) red.
    if (h < 22.0 || h > 340.0) && s > 0.5 && v > 0.22 { c |= RED; }
    // Green / blue / yellow lane colours.
    if ((85.0..150.0).contains(&h) || (185.0..220.0).contains(&h) || (38.0..62.0).contains(&h)) && s > 0.5 && v > 0.3 { c |= LANE; }
    if s < 0.25 && v > 0.55 { c |= CREAM; }
    if v < 0.30 { c |= DARK; }
    c
}

/// Outline sample points per site.
const N: usize = 32;
/// Search offsets (canonical px, each axis).
const OFF: i32 = 3;
/// Outline points of a site at an offset (canonical px), 8 per diamond edge.
fn points(st: &Site, dx: f32, dy: f32) -> [(f32, f32); N] {
    let corners = [(1.0f32, 0.0f32), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)];
    let r = st.half * 0.8;
    std::array::from_fn(|k| {
        let t = k as f32 / N as f32 * 4.0; let e = t as usize; let f = t - e as f32;
        let (a, b) = (corners[e], corners[e + 1]);
        (st.x + dx + (a.0 + (b.0 - a.0) * f) * r, st.y + dy + (a.1 + (b.1 - a.1) * f) * r)
    })
}

/// One outline test: team-coloured share of all points, the smallest share of the four edges,
/// and the dark share.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Outline { pub share: f32, pub edge_min: f32, pub dark: f32 }

fn outline(cls: &[u8], w: u32, h: u32, s: f32, st: &Site, dx: f32, dy: f32) -> Outline {
    let want = if !st.own { RED } else if matches!(st.kind, Shrine | Patron) { CREAM } else { LANE };
    // Shrines are thin hollow outlines: a 1% scale change of the located minimap box (rec10
    // 404x422 -> 401x418) moved yours off the 0.8 radius and read them destroyed. No lane line
    // runs through a Shrine or Patron, so their samples look wider (+-1.5 px instead of +-0.8).
    let rr = (s * if matches!(st.kind, Shrine | Patron) { 1.5 } else { 0.8 }).round().max(1.0) as i32;
    let (mut edges, mut dark) = ([0usize; 4], 0usize);
    for (k, p) in points(st, dx, dy).iter().enumerate() {
        let px = (p.0 * s).round() as i32;
        let py = (p.1 * s).round() as i32;
        let mut hit = false;
        'n: for yy in py - rr..=py + rr { for xx in px - rr..=px + rr {
            if xx < 0 || yy < 0 || xx >= w as i32 || yy >= h as i32 { continue; }
            if cls[(yy as u32 * w + xx as u32) as usize] & want != 0 { hit = true; break 'n; }
        }}
        if hit { edges[k * 4 / N] += 1; }
        if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h && cls[(py as u32 * w + px as u32) as usize] & DARK != 0 { dark += 1; }
    }
    let per = (N / 4) as f32;
    Outline { share: edges.iter().sum::<usize>() as f32 / N as f32, edge_min: edges.iter().map(|e| *e as f32 / per).fold(1.0, f32::min), dark: dark as f32 / N as f32 }
}

/// Alive: this share of the outline in team colour, and at least `ALIVE_EDGE` of every edge.
/// The lane line runs through every lane structure in the lane's (or red) colour and stays when
/// it falls; a line (also slid along by the offset search) fills at most one edge and crosses
/// the others at 1-3 points of 8 (rec13 enemy left Base Guardian after it fell: 0.44-0.83 of
/// the whole outline, but never all four edges).
pub const ALIVE_SHARE: f32 = 0.7;
pub const ALIVE_EDGE: f32 = 0.5;
/// Gone: no offset gives an outline with more than `GONE_EDGE` of every edge, and the nominal
/// outline lies on the dark map panel.
pub const GONE_EDGE: f32 = 0.25;
pub const GONE_DARK: f32 = 0.55;

/// Read every site on a minimap crop (any resolution of the 335x350 canonical crop). The
/// minimap's own position jitters by a few px between captures (rec8/rec12 vs rec13: up to
/// 3 px), so each site takes its best outline within +-3 px. None when the crop does not show
/// the map: both Patrons stand until the match ends, so a frame where either is not read
/// Alive (menus, black frames, effects over the map) reads nothing.
pub fn read(img: &RgbImage) -> Option<[Read; 24]> {
    let sh = shares(img)?;
    let out: [Read; 24] = std::array::from_fn(|i| { let (alive, edge, dark) = sh[i];
        if alive { Read::Alive }
        else if edge <= GONE_EDGE && dark >= GONE_DARK { Read::Gone }
        else { Read::Unclear } });
    PATRONS.iter().all(|i| out[*i] == Read::Alive).then_some(out)
}

/// Per site: whether an offset passes the Alive test, the best smallest-edge share over the
/// offsets, and the dark share of the nominal outline. Diagnostics and [`read`].
pub fn shares(img: &RgbImage) -> Option<[(bool, f32, f32); 24]> {
    let (w, h) = img.dimensions();
    if w < 100 || h < 100 { return None; }
    let s = w as f32 / crate::minimap::CW;
    let cls: Vec<u8> = img.pixels().map(|p| class(p[0], p[1], p[2])).collect();
    Some(std::array::from_fn(|i| {
        let st = &SITES[i];
        let (mut alive, mut edge) = (false, 0f32);
        for dy in -OFF..=OFF { for dx in -OFF..=OFF {
            let o = outline(&cls, w, h, s, st, dx as f32, dy as f32);
            alive |= o.share >= ALIVE_SHARE && o.edge_min >= ALIVE_EDGE;
            edge = edge.max(o.edge_min);
        }}
        (alive, edge, outline(&cls, w, h, s, st, 0.0, 0.0).dark)
    }))
}

/// Reads kept per site (decisive reads only).
const WINDOW_MS: u64 = 60_000;
/// Gone reads among the last [`LAST_READS`] decisive reads (within the window, the latest one
/// Gone) that destroy a structure. A standing structure never reads Gone on a map frame; a fallen
/// one reads Alive now and then under a trooper wave.
pub const GONE_READS: usize = 3;
pub const LAST_READS: usize = 5;
/// Alive reads in a row that undo a destroyed latch (a wrong latch; the game never rebuilds).
/// (Heroes and trooper waves over a fallen Guardian's spot read Alive for up to ~30 s.)
pub const REVIVE_READS: u32 = 15;
/// An Alive state is reported this long after the last Alive read.
pub const ALIVE_HOLD_MS: u64 = 15_000;

#[derive(Default, Clone)]
struct SiteTrack { recent: VecDeque<(u64, bool)>, destroyed: bool, alive_run: u32, last_alive: Option<u64> }

/// Per-match structure states: Alive -> destroyed only. Red trooper waves walking over an enemy
/// Guardian's spot read Alive now and then after it fell (rec13 eGM), so a structure is destroyed
/// by repeated Gone reads and a later stray Alive read does not bring it back.
#[derive(Default, Clone)]
pub struct Tracker { sites: Vec<SiteTrack>, last_ms: Option<u64> }

impl Tracker {
    pub fn observe(&mut self, reads: Option<&[Read; 24]>, now_ms: u64) {
        if self.sites.len() != SITES.len() || self.last_ms.is_some_and(|t| now_ms < t) { self.sites = vec![SiteTrack::default(); SITES.len()]; }
        self.last_ms = Some(now_ms);
        let Some(reads) = reads else { return };
        for (t, r) in self.sites.iter_mut().zip(reads.iter()) {
            let alive = match r { Read::Alive => true, Read::Gone => false, Read::Unclear => continue };
            t.recent.retain(|(at, _)| now_ms.saturating_sub(*at) <= WINDOW_MS);
            t.recent.push_back((now_ms, alive));
            if alive { t.last_alive = Some(now_ms); t.alive_run += 1; } else { t.alive_run = 0; }
            if t.destroyed {
                if t.alive_run >= REVIVE_READS { t.destroyed = false; }
            } else {
                let gone = t.recent.iter().rev().take(LAST_READS).filter(|(_, a)| !a).count();
                if !alive && gone >= GONE_READS { t.destroyed = true; }
            }
        }
    }

    /// Observed states. `mirrored`: the lane layout (side lanes are Green / Yellow by it); side
    /// lane structures are omitted while it is unknown. A standing structure is listed when its
    /// last two decisive reads were Alive and recent; a destroyed one always.
    pub fn states(&self, now_ms: u64, mirrored: Option<bool>) -> Vec<StructureState> {
        self.site_states(now_ms, mirrored).into_iter().map(|(_, s)| s).collect()
    }
    /// [`Tracker::states`] with each entry's index in [`SITES`] (diagnostics: a lane has two
    /// Guardians, its own and the Base Guardian, both reported as "guardian").
    pub fn site_states(&self, now_ms: u64, mirrored: Option<bool>) -> Vec<(usize, StructureState)> {
        let mut out = vec![];
        for (i, (st, t)) in SITES.iter().zip(self.sites.iter()).enumerate() {
            let lane: Option<Lane> = match st.side { Some(1) => Some(Lane::Blue), Some(k) => match mirrored { Some(m) => Some(crate::minimap::lane_of(k, m)), None => continue }, None => None };
            let alive = if t.destroyed { false } else {
                let last2 = t.recent.iter().rev().take(2).filter(|(_, a)| *a).count() == 2;
                if !(last2 && t.last_alive.is_some_and(|a| now_ms.saturating_sub(a) <= ALIVE_HOLD_MS)) { continue; }
                true
            };
            out.push((i, StructureState { lane, kind: st.kind.label().into(), own: st.own, alive }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reads(f: impl Fn(usize) -> Read) -> [Read; 24] { std::array::from_fn(f) }
    #[test]
    fn destroyed_after_repeated_gone_reads_and_not_revived_by_a_stray_alive() {
        let mut t = Tracker::default();
        let walker = 15; // own left Walker
        for k in 0..3 { t.observe(Some(&reads(|_| Read::Alive)), k * 4000); }
        assert!(t.states(8000, Some(false)).iter().any(|s| s.kind == "walker" && s.own && s.lane == Some(Lane::Green) && s.alive));
        for k in 3..6 { t.observe(Some(&reads(|i| if i == walker { Read::Gone } else { Read::Alive })), k * 4000); }
        let gone = |t: &Tracker, now| t.states(now, Some(false)).into_iter().filter(|s| s.kind == "walker" && s.own && s.lane == Some(Lane::Green)).collect::<Vec<_>>();
        assert_eq!(gone(&t, 20_000).len(), 1);
        assert!(!gone(&t, 20_000)[0].alive);
        t.observe(Some(&reads(|_| Read::Alive)), 24_000);
        assert!(!gone(&t, 24_000)[0].alive, "one Alive read does not rebuild a structure");
        // Mirrored layout: the left lane is Yellow.
        assert!(t.states(24_000, Some(true)).iter().any(|s| s.kind == "walker" && s.own && s.lane == Some(Lane::Yellow) && !s.alive));
        // Unknown layout: side lanes omitted, middle and base structures listed.
        let st = t.states(24_000, None);
        assert!(st.iter().all(|s| s.lane.is_none() || s.lane == Some(Lane::Blue)));
        assert!(st.iter().any(|s| s.kind == "patron" && s.own && s.alive));
    }
    #[test]
    fn unclear_reads_list_nothing() {
        let mut t = Tracker::default();
        t.observe(Some(&reads(|_| Read::Unclear)), 0);
        t.observe(None, 4000);
        assert!(t.states(4000, Some(false)).is_empty());
    }
}
