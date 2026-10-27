//! Match review: remembers the last minutes of play to explain each death on the death screen,
//! summarise play patterns every five minutes (time in lanes / jungle / dead, team fights joined
//! or missed, unspent souls, waves left deep), and warn live when a fight is lost or you walk out
//! alone after respawning.
//!
//! Evidence rules: only screenshot readings are used; every stated reason was observed in the
//! window before the death (an unread value is never a reason). Built from three reviewed
//! recordings: 4 of 5 rec4 deaths were 6-10 s bursts, three followed teammates dying or walking
//! out alone with enemies unseen, every one with 1,376-3,019 souls unspent.
use crate::{laning::Source, positioning::TeamPicture, Cue, Priority, Rule};
use coach_core::{GameSnapshot, Pt};
use serde::Deserialize;
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

#[derive(Clone, Deserialize)]
pub struct Thresholds { pub death_window_secs: u32, pub alone_distance: f32, pub unseen_enemies: usize, pub souls_at_death: u32,
    pub pattern_window_secs: u32, pub fight_radius: f32, pub missed_fight_distance: f32, pub hoard_souls: u32, pub respawn_window_secs: u32 }
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub lesson: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub cards: Vec<Card> }

/// Where you were on one sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area { Lane, Jungle, Base, Dead, Unknown }

/// One observation, reduced to what the review needs.
#[derive(Debug, Clone)]
struct Sample {
    clock: u32, area: Area, hp: Option<f32>, you: Option<Pt>, nearest_ally: Option<f32>,
    /// Alive enemies with no top-bar health bar (not seen by your team).
    unseen: Option<usize>, allies_dead: Option<usize>, souls: Option<u32>,
    /// A cluster of >= 2 allies and >= 2 enemies on the minimap: its distance from you.
    fight_at: Option<f32>, deep_wave: bool,
}

#[derive(Debug, Clone)]
struct Death { clock: u32, cause: Option<String> }

#[derive(Default)]
struct State {
    samples: VecDeque<Sample>, deaths: Vec<Death>, dead: bool, reviewed: bool,
    respawned_at: Option<u32>, last_clock: Option<u32>,
}

/// The review coach. One instance per engine; it keeps its own history.
pub struct ReviewCoach { catalog: Arc<Catalog>, state: RefCell<State> }
impl Default for ReviewCoach {
    fn default() -> Self { Self { catalog: Arc::new(serde_json::from_str(include_str!("../../../data/coaching/review.json")).expect("shipped review policy")), state: RefCell::new(State::default()) } }
}

fn dist(a: Pt, b: Pt) -> f32 { (a.x - b.x).hypot(a.y - b.y) }
fn plural(n: usize, one: &str, many: &str) -> String { if n == 1 { format!("1 {one}") } else { format!("{n} {many}") } }
fn minutes(secs: u32) -> String { if secs < 90 { format!("{secs} seconds") } else { format!("{:.1} minutes", secs as f32 / 60.0) } }
/// Coarse family of a death-panel cause, for "same killer again".
fn cause_family(c: &str) -> String {
    let l = c.to_ascii_lowercase();
    if l.contains("melee") || l.contains("bashdown") { "melee hits".into() }
    else if l.contains("trooper") { "troopers".into() }
    else if l.contains("guardian") || l.contains("walker") || l.contains("patron") || l.contains("shrine") { "structures".into() }
    else { c.split_whitespace().take(3).collect::<Vec<_>>().join(" ") }
}

impl ReviewCoach {
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("review card") }
    fn fill(&self, id: &str, vars: &[(&str, String)]) -> (String, String) {
        let c = self.card(id); let mut t = c.speech.clone();
        for (k, v) in vars { t = t.replace(&format!("{{{k}}}"), v); }
        (t, c.lesson.clone())
    }

    fn sample(&self, s: &GameSnapshot, clock: u32) -> Sample {
        let t = &self.catalog.thresholds;
        let team = TeamPicture::from(s);
        let map = s.minimap.as_ref();
        let you = map.and_then(|m| m.you);
        let area = if s.dead || s.screen.as_deref() == Some("Dead") { Area::Dead }
            else if s.screen.as_deref() != Some("Gameplay") { Area::Unknown }
            else { match (you, map) {
                (Some(p), _) if p.y > 0.86 => Area::Base,
                (Some(_), Some(m)) if m.your_lane.is_some() => Area::Lane,
                (Some(_), _) => Area::Jungle,
                _ => Area::Unknown,
            } };
        let live: Vec<&coach_core::HeroTrack> = map.map(|m| m.heroes.iter().filter(|h| h.confirmed() && h.unseen_ms <= 3000).collect()).unwrap_or_default();
        let nearest_ally = you.and_then(|me| live.iter().filter(|h| h.ally).map(|h| h.distance(me)).min_by(f32::total_cmp));
        let fight_at = you.and_then(|me| live.iter().filter(|h| h.ally).filter(|a| {
            live.iter().filter(|o| o.ally && o.distance(a.pos) <= t.fight_radius).count() >= 2
                && live.iter().filter(|e| !e.ally && e.distance(a.pos) <= t.fight_radius).count() >= 2
        }).map(|a| a.distance(me)).min_by(f32::total_cmp));
        let deep_wave = map.is_some_and(|m| [&m.green, &m.blue, &m.yellow].iter().any(|l| l.push.is_some_and(|p| p < 0.3) && l.enemy_troopers >= 2));
        Sample { clock, area, hp: s.hp.zip(s.max_hp).filter(|(h, m)| *m >= 100 && h <= m).map(|(h, m)| h as f32 / m as f32), you, nearest_ally,
            unseen: team.as_ref().map(|p| p.enemies_unseen), allies_dead: team.as_ref().map(|p| p.allies_dead.len()), souls: s.souls, fight_at, deep_wave }
    }

    /// Why the last death happened, from the samples before it. Returns (spoken text, reason ids).
    fn death_review(&self, st: &State, death: &Death) -> (String, Vec<String>) {
        let t = &self.catalog.thresholds;
        // The window ends at the last alive reading: the death screen can be read 15 s after the
        // killing blow (rec4 death 1: last alive 24:31, death screen 24:46).
        let end = st.samples.iter().rev().find(|x| x.area != Area::Dead && x.area != Area::Unknown && x.clock <= death.clock && x.clock + 30 >= death.clock)
            .map(|x| x.clock).unwrap_or(death.clock);
        let before: Vec<&Sample> = st.samples.iter().filter(|x| x.area != Area::Dead && x.clock + t.death_window_secs >= end && x.clock <= end).collect();
        let alive_known: Vec<&&Sample> = before.iter().filter(|x| x.hp.is_some()).collect();
        let mut reasons: Vec<(u8, String, String, String)> = vec![]; // (rank, id, text, lesson)
        let mk = |rank: u8, id: &str, vars: &[(&str, String)]| { let (a, b) = self.fill(id, vars); (rank, id.to_string(), a, b) };
        // Burst: the last healthy (>= 60%) reading and how long before the death it was.
        if let Some(h) = alive_known.iter().rev().find(|x| x.hp.is_some_and(|h| h >= 0.6)) {
            // If even the last alive reading was healthy, the fall was between readings: count to
            // the death screen instead (an upper bound, never an invented shorter time).
            let secs = (if h.clock == end { death.clock } else { end }).saturating_sub(h.clock).max(2);
            // 15 s: the death screen is seen a few seconds after the killing blow.
            if secs <= 15 { reasons.push(mk(1, "burst", &[("from", format!("{:.0}", h.hp.unwrap() * 100.0)), ("secs", secs.to_string())])); }
        }
        // Teammates dying in the window before you.
        let dead_first = before.iter().filter_map(|x| x.allies_dead).next();
        let dead_last = before.iter().rev().filter_map(|x| x.allies_dead).next();
        if let (Some(a), Some(b)) = (dead_first, dead_last) { if b > a { reasons.push(mk(2, "fight_lost", &[("count", plural(b - a, "teammate", "teammates"))])); } }
        // Walked out alone right after a respawn.
        // Alone = teammates were visible on the minimap and all far (unseen is not "far").
        let recent: Vec<&&Sample> = before.iter().rev().filter(|x| x.nearest_ally.is_some()).take(4).collect();
        let alone = !recent.is_empty() && recent.iter().all(|x| x.nearest_ally.is_some_and(|d| d > t.alone_distance));
        let after_respawn = st.respawned_at.is_some_and(|r| end.saturating_sub(r) <= t.respawn_window_secs);
        if alone && after_respawn { reasons.push(mk(2, "respawn_alone", &[("secs", end.saturating_sub(st.respawned_at.unwrap()).to_string())])); }
        else if alone { reasons.push(mk(4, "alone", &[])); }
        // In the enemy half with enemies unseen.
        if let Some(x) = before.iter().rev().find(|x| x.you.is_some_and(|p| p.y < 0.48) && x.unseen.is_some_and(|u| u >= t.unseen_enemies)) {
            reasons.push(mk(3, "forward_missing", &[("count", plural(x.unseen.unwrap(), "enemy", "enemies"))]));
        }
        // Same cause family as earlier deaths.
        if let Some(fam) = death.cause.as_deref().map(cause_family) {
            let n = st.deaths.iter().filter(|d| d.cause.as_deref().map(cause_family).as_deref() == Some(fam.as_str())).count();
            if n >= 2 { reasons.push(mk(3, "repeat_cause", &[("count", n.to_string()), ("cause", fam)])); }
        }
        if let Some(souls) = before.iter().rev().filter_map(|x| x.souls).next().filter(|v| *v >= t.souls_at_death) {
            reasons.push(mk(5, "souls", &[("souls", souls.to_string())]));
        }
        // Non-hero killers (Midboss, Walker, structures, troopers) have their own specific advice.
        if let Some((tag, advice)) = death.cause.as_deref().map(crate::death_cause_advice).filter(|(tag, _)| *tag != "heroes") {
            reasons.push((0, format!("cause_{tag}"), advice.to_string(), String::new()));
        }
        if reasons.is_empty() { reasons.push(mk(9, "no_reason", &[])); }
        reasons.sort_by_key(|r| r.0);
        let top: Vec<_> = reasons.iter().take(3).collect();
        let why = top.iter().map(|r| r.2.as_str()).collect::<Vec<_>>().join(" ");
        // Lessons: the two most important, without repeats.
        let mut lessons: Vec<&str> = vec![];
        for r in &top { if !r.3.is_empty() && !lessons.contains(&r.3.as_str()) && lessons.len() < 2 { lessons.push(&r.3); } }
        let cause = death.cause.as_deref().map(|c| format!("Damage panel: {c}. ")).unwrap_or_default();
        let next = if lessons.is_empty() { String::new() } else { format!("Next time: {}", lessons.join(" ")) };
        (format!("Why you died: {why} {cause}{next}").trim_end().to_string(), top.iter().map(|r| r.1.clone()).collect())
    }

    /// The last `pattern_window_secs` of play summarised in one or two observations.
    fn pattern(&self, st: &State, now: u32) -> Option<String> {
        let t = &self.catalog.thresholds;
        let w: Vec<&Sample> = st.samples.iter().filter(|x| x.clock + t.pattern_window_secs >= now).collect();
        let span = w.last()?.clock.saturating_sub(w.first()?.clock);
        if span < t.pattern_window_secs * 4 / 5 { return None; }
        // Time per area: each sample stands for the gap to the next one (capped at 10 s).
        let mut time = [0u32; 5]; let mut hoard = 0u32; let mut deep_away = 0u32;
        let mut fights_missed = 0u32; let mut fights_joined = 0u32; let mut in_fight: Option<bool> = None;
        for (a, b) in w.iter().zip(w.iter().skip(1)) {
            let dt = b.clock.saturating_sub(a.clock).min(10);
            time[a.area as usize] += dt;
            if a.area != Area::Dead && a.souls.is_some_and(|v| v >= t.hoard_souls) { hoard += dt; }
            if a.deep_wave && matches!(a.area, Area::Jungle | Area::Base) { deep_away += dt; }
            // A fight episode starts when a cluster appears; it counts once, joined if you were close.
            match (a.fight_at, in_fight) {
                (Some(d), None) => { let far = d > t.missed_fight_distance && a.area != Area::Dead; in_fight = Some(far); if far { fights_missed += 1 } else { fights_joined += 1 } }
                (None, Some(_)) => in_fight = None,
                _ => {}
            }
        }
        let deaths = st.deaths.iter().filter(|d| d.clock + t.pattern_window_secs >= now).count();
        let known = (time[Area::Lane as usize] + time[Area::Jungle as usize] + time[Area::Base as usize]).max(1);
        let pct = |a: Area| time[a as usize] * 100 / known;
        let min = (span + 30) / 60;
        let mut out: Vec<(String, String)> = vec![];
        if deaths >= 2 || time[Area::Dead as usize] >= 60 {
            out.push(self.fill("pattern_deaths", &[("min", min.to_string()), ("deaths", plural(deaths, "time", "times")), ("dead", minutes(time[Area::Dead as usize]))]));
        }
        if fights_missed >= 2 { out.push(self.fill("pattern_missed", &[("fights", plural(fights_missed as usize, "time", "times"))])); }
        if pct(Area::Jungle) >= 50 && known >= 120 { out.push(self.fill("pattern_jungle", &[("jungle", pct(Area::Jungle).to_string()), ("min", min.to_string())])); }
        if deep_away >= 30 { out.push(self.fill("pattern_waves", &[("secs", deep_away.to_string())])); }
        if hoard >= 60 { out.push(self.fill("pattern_hoard", &[("hoard", t.hoard_souls.to_string()), ("secs", hoard.to_string())])); }
        if out.is_empty() {
            if deaths > 0 || known < 120 { return None; }
            out.push(self.fill("pattern_good", &[("min", min.to_string()), ("lane", pct(Area::Lane).to_string()), ("jungle", pct(Area::Jungle).to_string()), ("joined", fights_joined.to_string())]));
        }
        let picked: Vec<_> = out.into_iter().take(2).collect();
        Some(picked.iter().map(|(a, b)| format!("{a} {b}")).collect::<Vec<_>>().join(" "))
    }
}

impl Rule for ReviewCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        let Some(clock) = s.game_clock_secs else { return vec![] };
        let t = self.catalog.thresholds.clone();
        let mut st = self.state.borrow_mut();
        // A new match (clock back by more than two minutes) starts a fresh review.
        if st.last_clock.is_some_and(|c| clock + 120 < c) { *st = State::default(); }
        st.last_clock = Some(clock);
        let sample = self.sample(s, clock);
        let dead = sample.area == Area::Dead;
        let mut cues = vec![];
        if dead && !st.dead {
            st.dead = true; st.reviewed = false;
            st.deaths.push(Death { clock, cause: None });
        }
        if dead {
            if let Some(c) = s.death_cause.clone() { if let Some(d) = st.deaths.last_mut() { d.cause.get_or_insert(c); } }
            // Offered on every death-screen frame under a per-death key (the engine speaks a key
            // once): another High cue may take this frame's slot, and a rule cannot know whether
            // it was spoken. Ready once the cause is read, or on a later dead frame.
            let ready = st.deaths.last().is_some_and(|d| d.cause.is_some() || clock > d.clock + 2);
            if ready {
                let death = st.deaths.last().unwrap().clone();
                let (text, _) = self.death_review(&st, &death);
                st.reviewed = true;
                cues.push(Cue { key: format!("death:review:{}", death.clock), text, priority: Priority::High, cooldown_secs: 600 });
            }
            // Waiting to respawn is the natural moment for the bigger picture.
            if st.reviewed { if let Some(text) = self.pattern(&st, clock) {
                cues.push(Cue { key: format!("review:pattern:{}", clock / t.pattern_window_secs), text, priority: Priority::Normal, cooldown_secs: 600 });
            }}
        } else if s.screen.as_deref() == Some("Gameplay") {
            if st.dead { st.dead = false; st.respawned_at = Some(clock); }
            let me = sample.you;
            // Live: two teammates fell in the last 10 s while an enemy is on you.
            let earlier = st.samples.iter().filter(|x| x.clock + 10 >= clock).filter_map(|x| x.allies_dead).next();
            let enemy_close = me.is_some_and(|me| s.minimap.as_ref().is_some_and(|m| m.heroes.iter().any(|h| !h.ally && h.confirmed() && h.unseen_ms <= 3000 && dist(h.pos, me) <= 0.2)));
            if let (Some(a), Some(b)) = (earlier, sample.allies_dead) { if b >= a + 2 && enemy_close {
                let (text, _) = self.fill("live_fight_lost", &[("count", plural(b - a, "teammate", "teammates"))]);
                cues.push(Cue { key: "review:fight-lost".into(), text, priority: Priority::High, cooldown_secs: 30 });
            }}
            // Live: leaving base alone right after a respawn with enemies unseen.
            let fresh = st.respawned_at.is_some_and(|r| clock.saturating_sub(r) <= t.respawn_window_secs);
            if fresh && me.is_some_and(|p| p.y < 0.75) && sample.nearest_ally.is_none_or(|d| d > t.missed_fight_distance) && sample.unseen.is_some_and(|u| u >= t.unseen_enemies) {
                let (text, _) = self.fill("live_respawn_alone", &[("count", plural(sample.unseen.unwrap(), "enemy", "enemies"))]);
                cues.push(Cue { key: "review:respawn-alone".into(), text, priority: Priority::Normal, cooldown_secs: 60 });
            }
            // Calm moments (healthy, no enemy close) every five minutes: the play pattern.
            // One summary per five-minute block of game time (the key carries the block).
            let calm = sample.hp.is_some_and(|h| h >= 0.6) && !enemy_close;
            if calm { if let Some(text) = self.pattern(&st, clock) {
                cues.push(Cue { key: format!("review:pattern:{}", clock / t.pattern_window_secs), text, priority: Priority::Low, cooldown_secs: 600 });
            }}
        }
        st.samples.push_back(sample);
        let keep = t.pattern_window_secs + 60;
        while st.samples.front().is_some_and(|x| x.clock + keep < clock) { st.samples.pop_front(); }
        cues
    }
}
