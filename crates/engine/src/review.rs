//! Match review: remembers the last minutes of play to explain each death on the death screen,
//! summarise play patterns every five minutes (time in lanes / jungle / dead, team fights joined
//! or missed, unspent souls, waves left deep, repeat killers, crowd control before deaths), and
//! warn live when a fight is lost or you walk out alone after respawning.
//!
//! Evidence rules: only screenshot readings are used; every stated reason was observed in the
//! window before the death (an unread value is never a reason). The damage panel is attributed
//! to enemy heroes through data/abilities.json (an ability shared by several heroes, a gun, melee
//! or an item nobody is known to own stays unattributed), and a panel that lists less than its
//! total is reported as "at least". Built from reviewed recordings: rec4 (6-10 s bursts, walking
//! out alone, 1,376-3,019 souls unspent) and rec5 (Victor's aura and Drifter's mark, stunned or
//! knocked off the zipline before every death).
use crate::{laning::Source, positioning::TeamPicture, Cue, Priority, Rule};
use coach_core::{DamageRow, GameSnapshot, Pt};
use serde::Deserialize;
use std::{cell::RefCell, collections::{HashMap, VecDeque}, sync::{Arc, OnceLock}};

#[derive(Clone, Deserialize)]
pub struct Thresholds { pub death_window_secs: u32, pub alone_distance: f32, pub unseen_enemies: usize, pub souls_at_death: u32,
    pub pattern_window_secs: u32, pub fight_radius: f32, pub missed_fight_distance: f32, pub hoard_souls: u32, pub respawn_window_secs: u32 }
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub lesson: String, pub sources: Vec<Source>,
    /// Other phrasings of a live call, said in rotation.
    #[serde(default)] pub variants: Vec<String> }
/// An ability with its own death reason and lesson (matched by panel source or debuff badge).
#[derive(Clone, Deserialize)]
pub struct Threat { pub source: String, pub hero: String, pub close: bool, pub counter: String,
    /// The death-review cause ("You died because {because}") and the next-life change (ranged heroes: `next_ranged`).
    #[serde(default)] pub because: String, #[serde(default)] pub next: String, #[serde(default)] pub next_ranged: String,
    pub sources: Vec<Source> }
/// Counter items for one need ("spirit", "debuff", "cc", "bullet"), in order of preference.
#[derive(Clone, Deserialize)]
pub struct Counter { pub need: String, pub items: Vec<String>, pub keyword: String, pub why: String, pub sources: Vec<Source> }
#[derive(Clone, Default, Deserialize)]
pub struct Roles { #[serde(default)] pub ranged: Vec<String>,
    /// Items whose effect is a melee attack (class names): never suggested to a ranged hero.
    #[serde(default)] pub melee_items: Vec<String>,
    /// Frontliners (tanks and brawlers who stand in front): "Bebop went down first".
    #[serde(default)] pub frontline: Vec<String> }
#[derive(Clone, Deserialize)]
pub struct Catalog { pub schema_version: u32, pub thresholds: Thresholds, pub cards: Vec<Card>,
    #[serde(default)] pub threats: Vec<Threat>, #[serde(default)] pub counters: Vec<Counter>, #[serde(default)] pub roles: Roles }

/// Shipped game data: abilities (name -> kind, heroes), hero names, items.
#[derive(Deserialize)]
struct AbilityRow { name: String, kind: String, #[serde(default)] heroes: Vec<String> }
#[derive(Deserialize)]
struct HeroRow { class_name: String, name: String }
#[derive(Deserialize, Clone)]
pub struct ItemRow { pub class_name: String, pub name: String, #[serde(default)] pub cost: u32, #[serde(default)] pub desc: String, #[serde(default)] pub effect: String }
pub struct GameData { abilities: HashMap<String, AbilityRow>, heroes: HashMap<String, String>, pub items: Vec<ItemRow> }
pub fn game_data() -> &'static GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let strip = |s: &'static str| s.trim_start_matches('\u{feff}');
        let abilities: Vec<AbilityRow> = serde_json::from_str(strip(include_str!("../../../data/abilities.json"))).expect("abilities.json");
        let heroes: Vec<HeroRow> = serde_json::from_str(strip(include_str!("../../../data/heroes.json"))).expect("heroes.json");
        let items: Vec<ItemRow> = serde_json::from_str(strip(include_str!("../../../data/items.json"))).expect("items.json");
        GameData { abilities: abilities.into_iter().map(|a| (a.name.to_lowercase(), a)).collect(),
            heroes: heroes.into_iter().map(|h| (h.class_name, h.name)).collect(), items }
    })
}
impl GameData {
    pub fn hero_name(&self, class: &str) -> String {
        self.heroes.get(class).cloned().unwrap_or_else(|| class.trim_start_matches("hero_").replace('_', " "))
    }
    pub fn item(&self, name: &str) -> Option<&ItemRow> { self.items.iter().find(|i| i.name.eq_ignore_ascii_case(name)) }
    /// The one enemy hero an ability belongs to (None for items, shared abilities, or heroes not
    /// on the enemy team; an empty team list accepts any single owner).
    pub fn ability_hero(&self, ability: &str, enemies: &[String]) -> Option<String> {
        let a = self.abilities.get(&ability.to_lowercase()).filter(|a| a.kind == "ability")?;
        let c: Vec<&String> = a.heroes.iter().filter(|h| enemies.is_empty() || enemies.contains(h)).collect();
        match c.as_slice() { [h] if a.heroes.len() <= 3 => Some((*h).clone()), _ => None }
    }
}

/// Where you were on one sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area { Lane, Jungle, Base, Dead, Unknown }

/// One observation, reduced to what the review needs.
#[derive(Debug, Clone)]
struct Sample {
    clock: u32, area: Area, hp: Option<f32>, you: Option<Pt>, nearest_ally: Option<f32>,
    /// Alive enemies with no top-bar health bar (not seen by your team).
    unseen: Option<usize>, allies_dead: Option<usize>, enemies_dead: Option<usize>, souls: Option<u32>,
    /// A cluster of >= 2 allies and >= 2 enemies on the minimap: its distance from you.
    fight_at: Option<f32>, deep_wave: bool,
    /// Status effects and debuff badges on you (normalised: lower case, no apostrophes).
    effects: Vec<String>,
    /// A live gameplay frame (alive even when your position was not read), and the respawn
    /// timer on a death screen.
    alive: bool, respawn: Option<u32>,
    /// Allied heroes shown dead / alive on the top bar (class names), and an enemy structure banner.
    allies_dead_heroes: Vec<String>, allies_alive_heroes: Vec<String>, banner: Option<String>,
    /// Enemy top-bar slots shown dead (None = top bar not read).
    enemies_dead_slots: Option<Vec<usize>>, allies_dead_slots: Option<Vec<usize>>,
    /// Your escape ability cast fresh on this frame in their half: (ability, enemies unseen or back soon).
    cast: Option<(String, usize)>,
}

/// What one death came down to, kept for repeat killers and pattern summaries.
#[derive(Debug, Clone, Default)]
struct Summary { main: Option<String>, cc: Vec<String>, close: Vec<String> }

#[derive(Debug, Clone, Default)]
struct Death { clock: u32, cause: Option<String>, cause_at: Option<u32>, panel: Vec<DamageRow>, kb: Option<String>, total: Option<u32>,
    /// Souls on the first death-screen frame (before any shopping while dead).
    souls_dead: Option<u32>, summary: Option<Summary>,
    /// Structure sources (Walker, Shrine, Guardian, Patron) seen on any panel read of this death.
    structures_seen: Vec<String>,
    /// Consecutive death-screen frames that showed the damage panel collapsed (vision
    /// `death_panel` "closed") and the clock of the first; the last panel state seen.
    closed_frames: u32, closed_since: Option<u32>, panel_state: Option<String> }

#[derive(Default)]
struct State {
    samples: VecDeque<Sample>, deaths: Vec<Death>, dead: bool, reviewed: bool,
    respawned_at: Option<u32>, last_clock: Option<u32>,
    /// Respawn seconds last read in the current death, and the clock of the last dead sample.
    last_respawn: Option<u32>, last_dead_clock: Option<u32>,
    /// The last spoken pattern text (the same summary is never said twice in a row).
    last_pattern: Option<(String, String)>,
    /// The summary offered under the previous key.
    prev_pattern: Option<String>,
    /// Your hero, the enemy team, enemy item owners (item name lower -> hero classes), your items.
    me: Option<String>, enemies: Vec<String>, owners: HashMap<String, Vec<String>>, own_items: Vec<String>,
    /// Calls the engine spoke that a death can ignore: (clock, "hold" | "urn" | "outnumbered" | "exit").
    calls: Vec<(u32, &'static str)>,
    /// Live fight-lost calls spoken (phrasing rotation).
    fight_lost_said: usize,
    /// Escape ability: last clock seen ready, and whether a cooldown was read since.
    esc_ready_at: Option<u32>, esc_seen: bool,
    /// The death (its first death-screen clock) during which the "open the damage panel" tip was
    /// spoken: once per match.
    panel_tip_at: Option<u32>,
}
/// Consecutive death-screen frames, over at least [`PANEL_CLOSED_SECS`], with the panel collapsed
/// before the open-the-panel tip (the panel is still collapsed on the first frame of a death while
/// it opens: rec8 49:42).
pub const PANEL_CLOSED_FRAMES: u32 = 2;
pub const PANEL_CLOSED_SECS: u32 = 3;

/// The review coach. One instance per engine; it keeps its own history.
pub struct ReviewCoach { catalog: Arc<Catalog>, state: RefCell<State> }
impl Default for ReviewCoach {
    fn default() -> Self { Self { catalog: Arc::new(serde_json::from_str(include_str!("../../../data/coaching/review.json")).expect("shipped review policy")), state: RefCell::new(State::default()) } }
}

fn dist(a: Pt, b: Pt) -> f32 { (a.x - b.x).hypot(a.y - b.y) }
/// A spoken death review is at most this many words (docs/coach-review-13.md: death screen ≤ 30).
pub const REVIEW_WORDS: usize = 30;
fn words(s: &str) -> usize { s.split_whitespace().count() }
fn plural(n: usize, one: &str, many: &str) -> String { if n == 1 { format!("1 {one}") } else { format!("{n} {many}") } }
fn minutes(secs: u32) -> String { if secs < 90 { format!("{secs} seconds") } else { format!("{:.1} minutes", secs as f32 / 60.0) } }
fn num(v: u32) -> String { if v >= 1000 { format!("{},{:03}", v / 1000, v % 1000) } else { v.to_string() } }
fn norm(s: &str) -> String { s.to_lowercase().replace(['\'', '\u{2019}'], "").trim().to_string() }
/// "a", "a and b", "a, b and c".
fn and_list(v: &[String]) -> String {
    match v.len() { 0 => String::new(), 1 => v[0].clone(), n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]) }
}
/// The respawn moment of one death from its timer reads (frame clock + timer each): the largest
/// group of reads within 2 s of each other (the latest on a tie), its median. Misreads drop a
/// digit (rec4 read 9 / 5 / 7 for 49 / 45 / 27), so neither the first nor the last read is trusted.
/// A read refuted by a later death screen (implied respawn before `last_dead`) is dropped: rec4
/// 30:52 read 8, 5, 4, 2, 0 for 68-60 and was still dead at 31:06.
pub(crate) fn respawn_estimate(ends: &[u32], last_dead: u32) -> Option<u32> {
    let ends: Vec<u32> = ends.iter().copied().filter(|&e| e + 1 >= last_dead).collect();
    let near = |a: u32, b: u32| a.abs_diff(b) <= 2;
    let best = ends.iter().copied().max_by_key(|&e| (ends.iter().filter(|&&f| near(f, e)).count(), e))?;
    let mut group: Vec<u32> = ends.iter().copied().filter(|&f| near(f, best)).collect();
    group.sort_unstable();
    Some(group[group.len() / 2])
}

/// Deaths and seconds dead in a window (`eff` = the area of each sample, menus inside a death
/// already marked dead). The same definition as the player profile (docs/player-profile.md "Time
/// dead"): a death is a run of death screens; it lasts from its first death screen to the respawn,
/// i.e. the respawn-timer estimate, never before the last death screen nor after the first alive
/// gameplay frame; without a timer read, the first alive frame (at most 10 s after the last death
/// screen). A death still running at the end of the window counts up to the estimate (the rest of
/// the timer is part of it). Recording gaps (> 60 s between frames) are not dead time. From 10:00
/// on, a run that ends in gameplay within 10 s without a longer timer is the grey "Respawn in: 1"
/// screen, not a death (the scoreboard count does not change: rec4 26:32, rec8 39:17, rec11 25:17).
fn death_runs(w: &[&Sample], eff: &[Area]) -> (usize, u32) {
    let (mut deaths, mut secs, mut i) = (0usize, 0u32, 0usize);
    while i < w.len() {
        if eff[i] != Area::Dead { i += 1; continue; }
        let (start, mut j, mut ends, mut gaps) = (w[i].clock, i, vec![], 0u32);
        while j < w.len() && eff[j] == Area::Dead {
            if let Some(r) = w[j].respawn.filter(|r| *r <= 120) { ends.push(w[j].clock + r); }
            if j > i { let d = w[j].clock.saturating_sub(w[j - 1].clock); if d > 60 { gaps += d; } }
            j += 1;
        }
        // The last death screen (menus and unreadable frames inside the run prove nothing).
        let last = (i..j).rev().find(|&k| w[k].area == Area::Dead).map_or(start, |k| w[k].clock);
        let alive_at = w[j..].iter().find(|x| x.alive).map(|x| x.clock);
        let est = respawn_estimate(&ends, last);
        let downed = i > 0 && start >= 600 && alive_at.is_some_and(|a| a < start + 10) && !est.is_some_and(|e| e >= start + 10);
        if !downed {
            let end = match (est, alive_at) {
                (Some(e), Some(a)) => e.min(a),
                (Some(e), None) => e,
                (None, Some(a)) => a.min(last + 10),
                (None, None) => last,
            }.max(last);
            deaths += 1;
            secs += end.saturating_sub(start).saturating_sub(gaps);
        }
        i = j;
    }
    (deaths, secs)
}

/// Coarse family of a death-panel cause, for "same killer again".
fn cause_family(c: &str) -> String {
    let l = c.to_ascii_lowercase();
    if l.contains("melee") || l.contains("bashdown") { "melee hits".into() }
    else if l.contains("trooper") { "troopers".into() }
    else if l.contains("guardian") || l.contains("walker") || l.contains("patron") || l.contains("shrine") { "structures".into() }
    else { c.split_whitespace().take(3).collect::<Vec<_>>().join(" ") }
}

/// Where one damage-panel source came from.
#[derive(Debug, Clone, PartialEq)]
enum Src { Hero(String), Gun, Generic, Item, Troopers, Structure, Neutral }

/// The damage panel attributed to heroes and the world.
#[derive(Debug, Default)]
struct Attribution {
    /// (hero class, damage, sources), largest first.
    heroes: Vec<(String, u32, Vec<String>)>,
    guns: usize, troopers: u32, structures: Vec<(String, u32)>, neutral: Vec<(String, u32)>,
    listed: u32, zipline: bool, top: Option<(String, u32)>,
}
impl Attribution {
    /// Fewest distinct enemy heroes: named ones, or one per separate gun row if more.
    fn min_heroes(&self) -> usize { self.heroes.len().max(self.guns) }
}

impl ReviewCoach {
    fn card(&self, id: &str) -> &Card { self.catalog.cards.iter().find(|c| c.id == id).expect("review card") }
    fn fill(&self, id: &str, vars: &[(&str, String)]) -> (String, String) {
        let c = self.card(id); let (mut t, mut l) = (c.speech.clone(), c.lesson.clone());
        for (k, v) in vars { t = t.replace(&format!("{{{k}}}"), v); l = l.replace(&format!("{{{k}}}"), v); }
        (t, l)
    }
    fn threat(&self, source: &str) -> Option<&Threat> { let n = norm(source); self.catalog.threats.iter().find(|t| norm(&t.source) == n) }
    fn ranged(&self, st: &State) -> bool { st.me.as_ref().is_some_and(|m| self.catalog.roles.ranged.contains(m)) }

    /// Which hero (or world source) one panel source belongs to.
    fn classify(&self, source: &str, st: &State) -> Src {
        let l = source.to_lowercase();
        let data = game_data();
        let Some(a) = data.abilities.get(&l) else {
            return if l.contains("trooper") { Src::Troopers } else if ["guardian", "walker", "shrine", "patron"].iter().any(|k| l.contains(k)) { Src::Structure }
                else if l.contains("gun") { Src::Gun } else { Src::Generic };
        };
        let heroes = |hs: &[String]| -> Src {
            // Shared abilities (melee, zipline, dash: listed for many heroes) and abilities of
            // heroes not in the enemy team stay unattributed; two candidates in the dictionary
            // (Pulse Grenade) resolve when only one of them is an enemy.
            let c: Vec<&String> = hs.iter().filter(|h| st.enemies.is_empty() || st.enemies.contains(h)).collect();
            if c.len() == 1 && hs.len() <= 3 { Src::Hero(c[0].clone()) } else { Src::Generic }
        };
        match a.kind.as_str() {
            "world" => if l.contains("trooper") { Src::Troopers } else if ["guardian", "walker", "shrine", "patron", "boss"].iter().any(|k| l.contains(k)) { Src::Structure } else { Src::Neutral },
            "weapon" if l == "gun damage" => Src::Gun,
            "item" => match st.owners.get(&l) { Some(o) if o.len() == 1 => Src::Hero(o[0].clone()), _ => Src::Item },
            _ if l.contains("zipline") || l == "melee" => Src::Generic,
            _ => heroes(&a.heroes),
        }
    }

    fn attribute(&self, d: &Death, st: &State) -> Option<Attribution> {
        if d.panel.is_empty() { return None; }
        let mut a = Attribution::default();
        for r in &d.panel {
            a.listed += r.damage;
            if r.source.to_lowercase().contains("zipline") { a.zipline = true; }
            if a.top.as_ref().is_none_or(|t| r.damage > t.1) { a.top = Some((r.source.clone(), r.damage)); }
            match self.classify(&r.source, st) {
                Src::Hero(h) => match a.heroes.iter_mut().find(|x| x.0 == h) {
                    Some(x) => { x.1 += r.damage; if !x.2.contains(&r.source) { x.2.push(r.source.clone()); } }
                    None => a.heroes.push((h, r.damage, vec![r.source.clone()])),
                },
                Src::Gun => a.guns += 1,
                Src::Troopers => a.troopers += r.damage,
                Src::Structure => a.structures.push((r.source.clone(), r.damage)),
                Src::Neutral => a.neutral.push((r.source.clone(), r.damage)),
                Src::Generic | Src::Item => {}
            }
        }
        a.heroes.sort_by(|x, y| y.1.cmp(&x.1));
        Some(a)
    }

    /// A counter item for `need`: the first listed item that exists, matches the keyword and
    /// is not owned. Returns (lesson text).
    fn counter(&self, need: &str, hero: &str, threat: &str, souls: Option<u32>, st: &State) -> Option<String> {
        let c = self.catalog.counters.iter().find(|c| c.need == need)?;
        let data = game_data();
        let item = c.items.iter().filter_map(|n| data.item(n))
            .filter(|i| format!("{} {}", i.desc, i.effect).to_lowercase().contains(&c.keyword.to_lowercase()))
            .find(|i| !st.own_items.contains(&i.class_name))?;
        let afford = souls.filter(|s| *s >= item.cost).map(|s| format!(", you had {} souls", num(s))).unwrap_or_default();
        let why = c.why.replace("{hero}", hero).replace("{threat}", threat);
        Some(self.fill("counter_item", &[("item", item.name.clone()), ("cost", num(item.cost)), ("afford", afford), ("why", why)]).1)
    }

    fn sample(&self, s: &GameSnapshot, clock: u32) -> Sample {
        let t = &self.catalog.thresholds;
        let team = TeamPicture::from(s);
        let map = s.minimap.as_ref();
        let you = map.and_then(|m| m.you);
        // Dead stays dead while the shop is open (snapshot.dead with screen "Shop").
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
        let effects = if area == Area::Dead { vec![] } else { s.status_effects.iter().map(|e| norm(e)).collect() };
        Sample { clock, area, hp: s.hp.zip(s.max_hp).filter(|(h, m)| *m >= 100 && h <= m).map(|(h, m)| h as f32 / m as f32), you, nearest_ally,
            unseen: team.as_ref().map(|p| p.enemies_unseen), allies_dead: team.as_ref().map(|p| p.allies_dead.len()),
            enemies_dead: team.as_ref().map(|p| p.enemies_dead.len()), souls: s.souls, fight_at, deep_wave, effects,
            alive: area != Area::Dead && s.screen.as_deref() == Some("Gameplay"), respawn: if area == Area::Dead { s.respawn_secs } else { None },
            allies_dead_heroes: s.hero_status.iter().filter(|h| h.ally && h.dead == Some(true)).filter_map(|h| h.hero.clone()).collect(),
            enemies_dead_slots: if s.hero_status.len() == 12 { Some(s.hero_status.iter().filter(|h| !h.ally && h.dead == Some(true)).map(|h| h.slot).collect()) } else { None },
            allies_dead_slots: if s.hero_status.len() == 12 { Some(s.hero_status.iter().filter(|h| h.ally && h.dead == Some(true)).map(|h| h.slot).collect()) } else { None },
            allies_alive_heroes: s.hero_status.iter().filter(|h| h.ally && h.dead == Some(false)).filter_map(|h| h.hero.clone()).filter(|h| s.hero.as_ref() != Some(h)).collect(),
            banner: s.objective_banner.as_ref().filter(|b| b.own != Some(true) && !b.name.trim().is_empty()).map(|b| {
                let n = b.name.trim().to_ascii_uppercase();
                if n.contains("HIDDEN") || n.contains("ARCHMOTHER") || n.contains("PATRON") { "Patron".to_string() } else { b.name.trim().to_string() }
            }), cast: None }
    }

    /// Why the last death happened, from the samples before it and the damage panel: one coach
    /// sentence of at most [`REVIEW_WORDS`] words, "You died because <cause>. Next life: <one
    /// change>. <Buy X now | Spend N souls now>." The single most important cause is picked
    /// (rec13: a 90-word list of facts missed "5 enemies dead, you at 15% hitting their Walker").
    /// Returns (spoken text, the cause id, summary).
    fn death_review(&self, st: &State, death: &Death, souls_now: Option<u32>) -> (String, Vec<String>, Summary) {
        let t = &self.catalog.thresholds;
        let data = game_data();
        let you = st.me.as_deref().map(|m| data.hero_name(m)).unwrap_or_default();
        let ranged = self.ranged(st);
        // The window ends at the last alive reading: the death screen can be read 15 s after the
        // killing blow (rec4 death 1: last alive 24:31, death screen 24:46).
        // A gameplay frame with health read but no minimap marker still counts (rec5 death 1: the
        // last frame, stunned at 18%, had no marker).
        let end = st.samples.iter().rev().find(|x| x.area != Area::Dead && (x.area != Area::Unknown || x.hp.is_some()) && x.clock <= death.clock && x.clock + 30 >= death.clock)
            .map(|x| x.clock).unwrap_or(death.clock);
        let before: Vec<&Sample> = st.samples.iter().filter(|x| x.area != Area::Dead && x.clock + t.death_window_secs >= end && x.clock <= end).collect();
        let alive_known: Vec<&&Sample> = before.iter().filter(|x| x.hp.is_some()).collect();
        let no_frames = before.iter().all(|x| x.area == Area::Unknown && x.hp.is_none());
        // (rank, id, because, next): lower rank = the more important cause.
        let mut cands: Vec<(u8, String, String, String)> = vec![];
        let mut summary = Summary::default();
        let add = |cands: &mut Vec<(u8, String, String, String)>, rank: u8, id: &str, vars: &[(&str, String)]| {
            let (a, b) = self.fill(id, vars);
            if !a.is_empty() { cands.push((rank, id.to_string(), a, b)); }
        };
        // Your front line by name: the allied frontliner standing last (else "your front line").
        let front_alive = before.last().map(|x| x.allies_alive_heroes.clone()).unwrap_or_default();
        let front = front_alive.iter().find(|h| self.catalog.roles.frontline.contains(h)).map(|h| data.hero_name(h)).unwrap_or_else(|| "your front line".into());

        // --- The damage panel: who did it.
        let attr = self.attribute(death, st);
        let mut threats_seen: Vec<&Threat> = vec![];
        let mut killer: Option<String> = None;
        if let Some(a) = &attr {
            for r in &death.panel { if let Some(th) = self.threat(&r.source) { if !threats_seen.iter().any(|x| x.source == th.source) { threats_seen.push(th); } } }
            // A named hero leads only with a real share of the listed damage (rec3: an unnamed gun
            // did 461 and Viscous 20).
            if let Some((h, _, srcs)) = a.heroes.first().filter(|x| x.1 * 5 >= a.listed) {
                summary.main = Some(h.clone());
                killer = Some(data.hero_name(h));
                add(&mut cands, 12, "killer", &[("hero", data.hero_name(h)), ("abilities", and_list(&srcs.iter().take(2).cloned().collect::<Vec<_>>())), ("front", front.clone())]);
            } else if let Some((src, dmg)) = &a.top {
                let src = if src.eq_ignore_ascii_case("gun damage") { "An enemy gun".to_string() } else { src.clone() };
                add(&mut cands, 14, "panel_top", &[("source", src), ("dmg", num(*dmg))]);
            }
            if a.troopers > 0 && (a.troopers * 10 >= a.listed || a.troopers >= 300) { add(&mut cands, 10, "troopers", &[("dmg", num(a.troopers))]); }
            if let Some((src, _)) = a.structures.iter().max_by_key(|x| x.1) { add(&mut cands, 10, "structure", &[("source", src.clone())]); }
        }
        // The top hero's biggest ability, named with its owner ("Kelvin's Arctic Beam"): the death
        // review says what hit you, not only who (rec8: "They burst you", "X and Y focused you").
        let top_ability: Option<(String, String)> = attr.as_ref().and_then(|a| a.heroes.first().filter(|x| x.1 * 5 >= a.listed))
            .and_then(|(h, _, srcs)| srcs.first().map(|s| (data.hero_name(h), s.clone())));
        let owned = |hero: &str, ability: &str| if hero.ends_with('s') { format!("{hero}' {ability}") } else { format!("{hero}'s {ability}") };
        let names: Vec<String> = attr.as_ref().map(|a| a.heroes.iter().take(2).map(|x| data.hero_name(&x.0)).collect()).unwrap_or_default();
        let mut names_ab = names.clone();
        if let (Some(first), Some((h, ab))) = (names_ab.first_mut(), top_ability.as_ref()) { if first == h { *first = owned(h, ab); } }
        let many = attr.as_ref().is_some_and(|a| a.min_heroes() >= 2);
        let who = if names.len() >= 2 { and_list(&names) } else if many { format!("{} enemies", attr.as_ref().map(|a| a.min_heroes()).unwrap_or(2)) } else { String::new() };
        let who_ab = if names_ab.len() >= 2 { and_list(&names_ab) } else { who.clone() };
        if many { add(&mut cands, 11, "involved", &[("names", who_ab.clone()), ("front", front.clone())]); }
        // Without panel rows, a cause naming a threat ability is that threat (rec5 death 3).
        if attr.is_none() { if let Some(th) = death.cause.as_deref().and_then(|c| self.threat(c)) { threats_seen.push(th); } }
        // Debuff badges seen on you before the death are threats too (Stalker's Mark 4-6 s
        // before the hit, rec5 death 4).
        for x in &before { for e in &x.effects { if let Some(th) = self.threat(e) { if !threats_seen.iter().any(|y| y.source == th.source) { threats_seen.push(th); } } } }
        for th in &threats_seen {
            if th.close { summary.close.push(th.source.clone()); }
            if th.because.is_empty() { continue; }
            let hero = data.hero_name(&th.hero);
            let next = if ranged && !th.next_ranged.is_empty() { &th.next_ranged } else { &th.next };
            cands.push((3, format!("threat:{}", th.source), th.because.replace("{hero}", &hero), next.replace("{hero}", &hero).replace("{you}", &you)));
        }

        // --- Crowd control in the window (status effects) and the zipline knock-off (panel row).
        let stuns = before.iter().enumerate().filter(|(i, x)| x.effects.iter().any(|e| e == "stunned")
            && (*i == 0 || !before[i - 1].effects.iter().any(|e| e == "stunned"))).count();
        let has = |e: &str| before.iter().any(|x| x.effects.iter().any(|y| y == e));
        let zipline = has("knocked_off_zipline") || attr.as_ref().is_some_and(|a| a.zipline);
        if zipline { add(&mut cands, 1, "zipline", &[("who", killer.clone().unwrap_or_else(|| "A hit".into())), ("front", front.clone())]); }
        if stuns > 0 {
            let times = match stuns { 1 => String::new(), 2 => " twice".into(), n => format!(" {n} times") };
            let finish = killer.clone().map(|k| format!("{k} finished you")).unwrap_or_else(|| "you couldn't get out".into());
            add(&mut cands, 5, "stunned", &[("times", times), ("finish", finish)]);
        }
        let mut cc: Vec<&str> = vec![];
        if zipline { cc.push("cc_zipline"); }
        if stuns > 0 { cc.push("cc_stunned"); }
        for (e, id) in [("silenced", "cc_silenced"), ("disarmed", "cc_disarmed"), ("out_of_stamina", "cc_out_of_stamina")] { if has(e) { cc.push(id); } }
        summary.cc = cc.iter().map(|c| c.trim_start_matches("cc_").to_string()).collect();

        // --- Repeat killer by hero (the counter item is the buy line), else by cause family.
        let souls_at = before.iter().rev().filter_map(|x| x.souls).next().or(if no_frames { death.souls_dead } else { None });
        let mut buy: Option<String> = None;
        // A repeat killer is folded into the one death review (rec8 47:38 said "Pocket's Affliction
        // topped 2 of your deaths" as a second death-screen message): (hero, ability, deaths).
        let mut repeat: Option<(String, String, usize)> = None;
        if let Some(main) = &summary.main {
            let n = 1 + st.deaths.iter().filter(|d| d.clock != death.clock).filter(|d| d.summary.as_ref().and_then(|s| s.main.as_ref()) == Some(main)).count();
            if n >= 2 {
                let hero = data.hero_name(main);
                let th = threats_seen.iter().find(|t| &t.hero == main && !t.counter.is_empty());
                let need = th.map(|t| t.counter.as_str()).unwrap_or("spirit");
                add(&mut cands, 9, "repeat_killer", &[("count", n.to_string()), ("hero", hero.clone())]);
                let ability = th.map(|t| t.source.clone()).or_else(|| attr.as_ref().and_then(|a| a.heroes.iter().find(|x| &x.0 == main)).and_then(|x| x.2.first().cloned()));
                if let Some(ab) = ability { repeat = Some((hero.clone(), ab, n)); }
                if let Some((item, cost)) = self.counter_item(need, st) {
                    if souls_now.or(souls_at).is_none_or(|s| s >= cost) { buy = Some(self.fill("buy_item", &[("item", item)]).0); }
                }
            }
        }
        if death.panel.is_empty() && summary.main.is_none() {
            if let Some(fam) = death.cause.as_deref().map(cause_family) {
                let n = st.deaths.iter().filter(|d| d.cause.as_deref().map(cause_family).as_deref() == Some(fam.as_str())).count();
                if n >= 2 { add(&mut cands, 13, "repeat_cause", &[("count", n.to_string()), ("cause", fam)]); }
            }
        }

        // --- What the frames before the death showed.
        if !no_frames {
            // A call the player did not follow in the last 60 s is the decision that killed him
            // (rec8 27:33 "hold the urn" -> dead 27:57; 41:46 "back to base" -> dead 42:05: the
            // review said "Kelvin and Yamato focused you").
            let last = before.iter().rev().find(|x| x.you.is_some() || x.area != Area::Unknown);
            let out_of_base = last.is_some_and(|x| x.area != Area::Base && x.you.is_none_or(|p| p.y < 0.86));
            let allies_dead = before.iter().rev().find_map(|x| x.allies_dead);
            let in_half = last.and_then(|x| x.you).is_some_and(|p| p.y < 0.5);
            let dead_count = || match allies_dead { Some(2) => "two", Some(3) => "three", Some(4) => "four", Some(5) => "five", _ => "several" }.to_string();
            // The most important ignored call, the newest among equals (hold / urn before exit).
            let ignored = st.calls.iter().rev().filter(|c| c.0 <= end && c.0 + 60 >= end).filter_map(|c| match c.1 {
                "hold" if out_of_base && allies_dead.is_some_and(|n| n >= 2) => Some(("ignored_hold", 0u8)),
                "urn" if before.iter().rev().take(4).any(|x| x.effects.iter().any(|e| e == "carrying_urn")) => Some(("ignored_urn", 0)),
                "outnumbered" if c.0 + 30 >= end => Some(("ignored_outnumbered", 0)),
                "exit" if in_half => Some(("ignored_exit", 1)),
                _ => None,
            }).min_by_key(|c| c.1);
            // Walked out of base (in base within the last 45 s) while three or more were dead.
            let walked_out = st.samples.iter().filter(|x| x.clock <= end && x.clock + 45 >= end).skip_while(|x| x.area != Area::Base)
                .find(|x| x.area != Area::Base && x.area != Area::Unknown).and_then(|x| x.allies_dead).is_some_and(|n| n >= 3);
            // Your escape ability spent on the way in, toward enemies off the map (rec8 16:54, 20:27).
            if let Some((ability, n)) = before.iter().rev().find_map(|x| x.cast.clone()).filter(|c| c.1 >= 2) {
                add(&mut cands, 0, "escape_in", &[("ability", ability), ("count", match n { 2 => "two".into(), 3 => "three".into(), 4 => "four".into(), 5 => "five".into(), n => n.to_string() })]);
            }
            if let Some((id, rank)) = ignored { add(&mut cands, rank, id, &[("count", dead_count())]); }
            // Walked out of base while three or more teammates were dead, without a call heard.
            else if out_of_base && walked_out && allies_dead.is_some_and(|n| n >= 3) { add(&mut cands, 1, "walked_out_dead", &[("count", dead_count())]); }
            // Won the fight, then hit a structure nearly dead: three or more enemies dead, you under
            // 30%, and structure damage on the panel or a structure banner up (rec13 29:25: five
            // enemies dead, 15% health, their Walker).
            let won = before.iter().filter_map(|x| x.enemies_dead).max().is_some_and(|n| n >= 3);
            let low = alive_known.iter().filter_map(|x| x.hp).fold(1.0f32, f32::min);
            let structure = attr.as_ref().and_then(|a| a.structures.iter().max_by_key(|x| x.1).map(|x| x.0.clone()))
                .or_else(|| death.structures_seen.first().cloned()).or_else(|| before.iter().rev().find_map(|x| x.banner.clone()));
            if let (true, true, Some(s)) = (won, low < 0.3, structure) {
                add(&mut cands, 0, "won_fight_structure", &[("structure", s), ("pct", format!("{:.0}", (low * 100.0).max(1.0)))]);
            }
            // Burst: from the healthiest reading in the window (the earliest one at that level) to
            // the end; rec5 said "69% to dead in 2 s" for a 100% -> 0 fall over ~10 s.
            let peak = alive_known.iter().map(|x| x.hp.unwrap()).fold(0.0f32, f32::max);
            if let Some(h) = alive_known.iter().find(|x| x.hp.is_some_and(|h| h >= 0.3 && h >= peak - 0.03)) {
                let secs = (if h.clock == end { death.clock } else { end }).saturating_sub(h.clock).max(2);
                if secs <= 15 {
                    // Mid-sentence: "You died because Kelvin's Arctic Beam burst you" (never "because They").
                    let by = top_ability.as_ref().filter(|_| !many).map(|(h, ab)| owned(h, ab)).or_else(|| killer.clone()).or_else(|| (!who_ab.is_empty()).then(|| who_ab.clone())).unwrap_or_else(|| "they".into());
                    let id = if ranged { "burst_ranged" } else { "burst" };
                    add(&mut cands, 11, id, &[("by", by), ("from", format!("{:.0}", h.hp.unwrap() * 100.0)), ("secs", secs.to_string()), ("you", you.clone()), ("front", front.clone())]);
                }
            }
            // Teammates dying in the window before you, by name; a frontliner first is its own cause.
            let first_dead = before.iter().find(|x| x.allies_dead.is_some()).map(|x| x.allies_dead_heroes.clone()).unwrap_or_default();
            let fell: Vec<String> = before.iter().rev().find(|x| x.allies_dead.is_some()).map(|x| x.allies_dead_heroes.iter().filter(|h| !first_dead.contains(h)).cloned().collect()).unwrap_or_default();
            let (dead_first, dead_last) = (before.iter().filter_map(|x| x.allies_dead).next(), before.iter().rev().filter_map(|x| x.allies_dead).next());
            if let Some(f) = fell.iter().find(|h| self.catalog.roles.frontline.contains(h)) {
                add(&mut cands, 2, "frontline_down", &[("front", data.hero_name(f))]);
            } else if let (Some(a), Some(b)) = (dead_first, dead_last) {
                if b > a {
                    let named: Vec<String> = fell.iter().take(2).map(|h| data.hero_name(h)).collect();
                    let count = if named.len() == b - a { and_list(&named) } else { plural(b - a, "teammate", "teammates") };
                    add(&mut cands, 7, "fight_lost", &[("count", count)]);
                }
            }
            // Alone = teammates were visible on the minimap and all far (unseen is not "far").
            let recent: Vec<&&Sample> = before.iter().rev().filter(|x| x.nearest_ally.is_some()).take(4).collect();
            let alone = !recent.is_empty() && recent.iter().all(|x| x.nearest_ally.is_some_and(|d| d > t.alone_distance));
            // "After respawning" only from a confirmed respawn (the engine's LifeTracker).
            let after_respawn = st.respawned_at.is_some_and(|r| end >= r && end - r <= t.respawn_window_secs);
            if alone && many { add(&mut cands, 4, "solo_vs", &[("names", who.clone())]); }
            if alone && after_respawn { add(&mut cands, 6, "respawn_alone", &[("secs", end.saturating_sub(st.respawned_at.unwrap()).to_string())]); }
            else if alone { add(&mut cands, 13, "alone", &[]); }
            // Went back in weak: every health reading of the last 30 s under 60% (rec5: 44-49% for
            // 30 s after shopping, then dead).
            let minute: Vec<f32> = st.samples.iter().filter(|x| x.clock + 30 >= end && x.clock <= end).filter_map(|x| x.hp).collect();
            if minute.len() >= 3 && minute.iter().all(|h| *h < 0.6) {
                let top = minute.iter().copied().fold(0.0f32, f32::max);
                let against = killer.clone().map(|k| format!(" against {k}")).unwrap_or_default();
                add(&mut cands, 8, "weak_entry", &[("pct", format!("{}", ((top * 100.0 / 5.0).ceil() * 5.0) as u32)), ("against", against)]);
            }
            // In the enemy half with enemies unseen.
            if let Some(x) = before.iter().rev().find(|x| x.you.is_some_and(|p| p.y < 0.48) && x.unseen.is_some_and(|u| u >= t.unseen_enemies)) {
                add(&mut cands, 9, "forward_missing", &[("count", plural(x.unseen.unwrap(), "enemy", "enemies"))]);
            }
        }
        // Non-hero killers (Midboss, Walker, structures, troopers) when the panel rows were not read.
        if attr.is_none() {
            if let Some((because, next)) = death.cause.as_deref().and_then(crate::death_cause_parts) { cands.push((10, "cause".into(), because.into(), next.into())); }
            else if let Some(c) = death.cause.as_deref() { add(&mut cands, 14, "panel_top", &[("source", c.to_string()), ("dmg", "most".into())]); }
        }
        let souls = souls_now.or(souls_at).filter(|v| *v >= t.souls_at_death.max(crate::economy::death_spend(death.clock)));
        if buy.is_none() { if let Some(v) = souls { buy = Some(self.fill("spend", &[("souls", num(v))]).0); } }
        // The counter to the ability that hit you replaces the generic "fight behind ..." line.
        if let Some(next) = top_ability.as_ref().and_then(|(h, ab)| self.ability_counter(h, ab, &you, ranged)) {
            for c in cands.iter_mut().filter(|c| matches!(c.1.as_str(), "killer" | "involved" | "burst" | "burst_ranged")) { c.3 = next.clone(); }
        }
        cands.sort_by_key(|c| c.0);
        let text = match cands.first() {
            Some((_, _, because, next)) => {
                let lead = if no_frames { self.card("no_frames").speech.clone() } else { String::new() };
                format!("{lead}{}", self.fill("review", &[("because", because.clone()), ("next", next.clone())]).0)
            }
            None => self.card("no_reason").speech.clone(),
        };
        let first_id = cands.first().map(|c| c.1.clone()).unwrap_or_default();
        let text = match repeat.filter(|_| first_id != "repeat_killer") {
            Some((h, ab, n)) => {
                let owner = if h.ends_with('s') { format!("{h}'") } else { format!("{h}'s") };
                let note = self.fill("repeat_note", &[("owner", owner), ("ability", ab), ("count", n.to_string())]).0;
                if words(&text) + words(&note) <= REVIEW_WORDS { format!("{text} {note}") } else { text }
            }
            None => text,
        };
        // The buy line is not repeated when the lesson already names the item.
        let named = |b: &str| b.strip_prefix("Buy ").and_then(|x| x.strip_suffix(" now.")).is_some_and(|item| text.contains(item));
        let text = match &buy { Some(b) if words(&text) + words(b) <= REVIEW_WORDS && !named(b) => format!("{text} {b}"), _ => text };
        (text, cands.first().map(|c| vec![c.1.clone()]).unwrap_or_default(), summary)
    }

    /// What to do next time against one enemy ability: the review threat's lesson, else the live
    /// threat call's counter-play for your role (data/coaching/plans.json).
    fn ability_counter(&self, hero: &str, ability: &str, you: &str, ranged: bool) -> Option<String> {
        if let Some(th) = self.threat(ability).filter(|t| !t.next.is_empty()) {
            let next = if ranged && !th.next_ranged.is_empty() { &th.next_ranged } else { &th.next };
            return Some(next.replace("{hero}", hero).replace("{you}", you));
        }
        let role = if ranged { "long_range" } else { "default" };
        let cat = crate::plans::catalog();
        let t = cat.threats.iter().find(|t| t.ability.eq_ignore_ascii_case(ability))?;
        t.counter.get(role).or_else(|| t.counter.get("default")).map(|c| format!("against {hero}'s {ability}, {c}"))
    }

    /// The counter item for `need` not owned yet: (name, cost).
    fn counter_item(&self, need: &str, st: &State) -> Option<(String, u32)> {
        let c = self.catalog.counters.iter().find(|c| c.need == need)?;
        let data = game_data();
        c.items.iter().filter_map(|n| data.item(n))
            .filter(|i| format!("{} {}", i.desc, i.effect).to_lowercase().contains(&c.keyword.to_lowercase()))
            .find(|i| !st.own_items.contains(&i.class_name)).map(|i| (i.name.clone(), i.cost))
    }

    /// The last `pattern_window_secs` of play summarised in a few observations.
    fn pattern(&self, st: &State, now: u32) -> Option<(String, bool, Option<String>)> {
        let t = &self.catalog.thresholds;
        let data = game_data();
        let w: Vec<&Sample> = st.samples.iter().filter(|x| x.clock + t.pattern_window_secs >= now).collect();
        let span = w.last()?.clock.saturating_sub(w.first()?.clock);
        if span < t.pattern_window_secs * 4 / 5 { return None; }
        // Where you were per sample. A menu frame (shop, scoreboard) between death screens is
        // still dead: rec8 said "dead for 29 seconds" of an 83 s respawn spent partly in the shop.
        let known = |x: &Sample| x.area != Area::Unknown || x.alive;
        let eff: Vec<Area> = (0..w.len()).map(|i| {
            if known(w[i]) { return w[i].area; }
            let prev = w[..i].iter().rev().find(|x| known(x)).map(|x| x.area);
            let next = w[i + 1..].iter().find(|x| known(x)).map(|x| x.area);
            if prev == Some(Area::Dead) && next.is_none_or(|n| n == Area::Dead) { Area::Dead } else { Area::Unknown }
        }).collect();
        // Time per area (shares only; time dead is death_runs()): each sample stands for the gap
        // to the next one (capped at 10 s, 30 s within a death).
        let mut time = [0u32; 5]; let mut hoard = 0u32; let mut deep_away = 0u32; let mut peak = 0u32;
        // The souls tier that applied while hoarding (the lowest one counted, so "held over N"
        // and "up to M" agree: rec8 said "over 6,400 ... up to 5,130").
        let mut hoard_tier = u32::MAX;
        // Enemy-dead windows: (start, length, most dead, ever in their half).
        let mut windows: Vec<(u32, u32, usize, bool, bool)> = vec![]; let mut open: Option<(u32, u32, usize, bool, bool)> = None;
        let mut fights_missed = 0u32; let mut fights_joined = 0u32; let mut in_fight: Option<bool> = None;
        // Clearly in their half (a mid-lane front sits at y 0.45-0.5) with a teammate beside you: a
        // window used together, not a solo dive (the praise coach uses the same line).
        let grouped = |a: &Sample| a.you.is_some_and(|p| p.y < 0.38) && a.nearest_ally.is_some_and(|d| d <= 0.15);
        for (i, (a, b)) in w.iter().zip(w.iter().skip(1)).enumerate() {
            let area = eff[i];
            let cap = if area == Area::Dead && eff[i + 1] == Area::Dead { 30 } else { 10 };
            let dt = b.clock.saturating_sub(a.clock).min(cap);
            time[area as usize] += dt;
            let tier = t.hoard_souls.max(crate::economy::spend_target(a.clock));
            if area != Area::Dead && a.souls.is_some_and(|v| v >= tier) { hoard += dt; peak = peak.max(a.souls.unwrap()); hoard_tier = hoard_tier.min(tier); }
            // Defending in base is not being away from a deep wave; only farming camps is.
            if a.deep_wave && a.area == Area::Jungle { deep_away += dt; }
            let alive = matches!(a.area, Area::Lane | Area::Jungle | Area::Base);
            match (a.enemies_dead.filter(|n| *n >= 2), alive, open.as_mut()) {
                (Some(n), true, Some(w)) => { w.1 += dt; w.2 = w.2.max(n); w.3 |= a.you.is_some_and(|p| p.y < 0.5); w.4 |= grouped(a); }
                (Some(n), true, None) => open = Some((a.clock, dt, n, a.you.is_some_and(|p| p.y < 0.5), grouped(a))),
                _ => if let Some(w) = open.take() { windows.push(w); },
            }
            // A fight episode starts when a cluster appears; it counts once, joined if you were close.
            match (a.fight_at, in_fight) {
                (Some(d), None) => { let far = d > t.missed_fight_distance && a.area != Area::Dead; in_fight = Some(far); if far { fights_missed += 1 } else { fights_joined += 1 } }
                (None, Some(_)) => in_fight = None,
                _ => {}
            }
        }
        if let Some(w) = open.take() { windows.push(w); }
        let used = windows.iter().filter(|w| w.1 >= 10 && w.4).max_by_key(|w| (w.2, w.1)).copied();
        let unused = windows.into_iter().filter(|w| w.1 >= 20 && !w.3).max_by_key(|w| w.1);
        let (deaths, dead_secs) = death_runs(&w, &eff);
        // Purchases: souls falling by an item's worth and staying down on the next reading (one
        // misread is not a purchase: rec12 3:52 read 7 souls between 978 and 1,007).
        let souls: Vec<u32> = w.iter().filter_map(|x| x.souls).collect();
        let purchases = souls.windows(3).filter(|v| v[0] >= v[1] + 800 && v[0] >= v[2] + 800).count();
        let known = (time[Area::Lane as usize] + time[Area::Jungle as usize] + time[Area::Base as usize]).max(1);
        let pct = |a: Area| time[a as usize] * 100 / known;
        let min = (span + 30) / 60;
        // Reviewed deaths in the window first: repeat killers, crowd control, close range.
        let mut out: Vec<(String, String)> = vec![];
        // Card id of each item (its skill: data/coaching/skills.json).
        let mut ids: Vec<&str> = vec![];
        let recent: Vec<&Summary> = st.deaths.iter().filter(|d| d.clock + t.pattern_window_secs >= now).filter_map(|d| d.summary.as_ref()).collect();
        let mut killers: Vec<(&String, usize)> = vec![];
        for s in &recent { if let Some(m) = &s.main { match killers.iter_mut().find(|k| k.0 == m) { Some(k) => k.1 += 1, None => killers.push((m, 1)) } } }
        if let Some((hero, n)) = killers.into_iter().filter(|k| k.1 >= 2).max_by_key(|k| k.1) {
            let name = data.hero_name(hero);
            let (text, generic) = self.fill("pattern_killer", &[("hero", name.clone()), ("count", n.to_string()), ("min", min.to_string())]);
            let need = self.catalog.threats.iter().find(|th| &th.hero == hero && !th.counter.is_empty());
            let lesson = self.counter(need.map(|t| t.counter.as_str()).unwrap_or("spirit"), &name, need.map(|t| t.source.as_str()).unwrap_or("these"), None, st).unwrap_or(generic);
            ids.push("pattern_killer"); out.push((text, lesson));
        }
        let with_cc: Vec<&&Summary> = recent.iter().filter(|s| !s.cc.is_empty()).collect();
        if recent.len() >= 2 && with_cc.len() >= 2 {
            let mut effects: Vec<String> = vec![];
            for s in &with_cc { for e in &s.cc { let f = self.card(&format!("cc_{e}")).speech.replace("{times}", ""); if !effects.contains(&f) { effects.push(f); } } }
            let which = match (with_cc.len(), recent.len()) { (2, 2) => "both deaths".to_string(), (a, b) if a == b => format!("all {b} deaths"), (a, b) => format!("{a} of your {b} deaths") };
            ids.push("pattern_cc"); out.push(self.fill("pattern_cc", &[("which", which), ("effects", and_list(&effects))]));
        }
        if self.ranged(st) && recent.len() >= 2 && recent.iter().all(|s| !s.close.is_empty()) {
            let mut srcs: Vec<String> = vec![];
            for s in &recent { for c in &s.close { if !srcs.contains(c) { srcs.push(c.clone()); } } }
            let you = data.hero_name(st.me.as_deref().unwrap_or_default());
            let count = if recent.len() == 2 { "Both".to_string() } else { format!("All {}", recent.len()) };
            ids.push("pattern_close"); out.push(self.fill("pattern_close", &[("count", count), ("sources", and_list(&srcs)), ("you", you)]));
        }
        let about_deaths = !out.is_empty();
        if deaths >= 2 || dead_secs >= 60 {
            ids.push("pattern_deaths"); out.push(self.fill("pattern_deaths", &[("min", min.to_string()), ("deaths", plural(deaths, "time", "times")), ("dead", minutes(dead_secs))]));
        }
        // A summary never repeats an item the previous summary already said (rec6 said "held over
        // 6,400 souls" in two summaries in a row; rec5 repeated the same 38:21 window).
        // The previous summary: the last one offered for an earlier 5-minute block, else the one before.
        let block = format!(":{}", now / t.pattern_window_secs);
        let previous = st.last_pattern.as_ref().filter(|(k, _)| !k.ends_with(&block)).map(|(_, text)| text).or(st.prev_pattern.as_ref());
        let said_before = |needle: &str| previous.is_some_and(|text| text.contains(needle));
        let unused = unused.filter(|(start, ..)| !said_before(&format!("At {}:{:02},", start / 60, start % 60)));
        // What went well, named as specifically as the frames allow (said with the criticism, or
        // alone when there is none): rec12 asked to point out what made the match go well.
        let mut good: Vec<String> = vec![];
        if deaths == 0 && known >= 120 { good.push("no deaths".into()); }
        if let Some((start, _, n, _, _)) = used { good.push(format!("at {}:{:02}, with {} dead, you pushed into their half", start / 60, start % 60, plural(n, "enemy", "enemies"))); }
        if fights_joined > 0 { good.push(format!("you were with your team for {}", plural(fights_joined as usize, "fight", "fights"))); }
        if purchases > 0 && hoard == 0 { good.push(format!("you spent your souls on time ({})", plural(purchases, "purchase", "purchases"))); }
        let hoard = if said_before("souls for") { 0 } else { hoard };
        if let Some((start, secs, n, _, _)) = unused {
            ids.push("pattern_window"); out.push(self.fill("pattern_window", &[("time", format!("{}:{:02}", start / 60, start % 60)), ("count", plural(n, "enemy", "enemies")), ("secs", secs.to_string())]));
        }
        if fights_missed >= 2 { ids.push("pattern_missed"); out.push(self.fill("pattern_missed", &[("fights", plural(fights_missed as usize, "time", "times"))])); }
        if pct(Area::Jungle) >= 50 && known >= 120 { ids.push("pattern_jungle"); out.push(self.fill("pattern_jungle", &[("jungle", pct(Area::Jungle).to_string()), ("min", min.to_string())])); }
        if deep_away >= 30 { ids.push("pattern_waves"); out.push(self.fill("pattern_waves", &[("secs", deep_away.to_string())])); }
        if hoard >= 60 { ids.push("pattern_hoard"); out.push(self.fill("pattern_hoard", &[("hoard", num(hoard_tier)), ("secs", hoard.to_string()), ("peak", num(peak))])); }
        if out.is_empty() {
            if deaths > 0 || known < 120 { return None; }
            // Lane / jungle split as context after the specific items.
            good.push(format!("a {}/{} split between lanes and jungle", pct(Area::Lane), pct(Area::Jungle)));
            ids.push("pattern_good"); out.push(self.fill("pattern_good", &[("min", min.to_string()), ("good", and_list(&good))]));
            good.clear();
        }
        // Two observations and one good thing at most: a summary is a call, not a report.
        let mut picked: Vec<_> = out.into_iter().take(2).collect();
        good.truncate(1);
        // A balanced summary: the criticism, then what went well.
        if !good.is_empty() { picked.push(self.fill("pattern_balance", &[("good", and_list(&good))])); }
        let text = picked.iter().map(|(a, b)| format!("{a} {b}").trim_end().to_string()).collect::<Vec<_>>().join(" ");
        // The summary coaches the skill of its first item.
        let skill = ids.first().and_then(|id| crate::skills::for_card(&format!("review.{id}")));
        Some((text, about_deaths, skill))
    }
}

impl ReviewCoach {
    /// Your escape ability cast fresh on this frame (ready shortly before, then a long cooldown
    /// read) in their half: (ability, enemies unseen or back within 12 s). rec8 16:54 / 20:27:
    /// Grey Talon flew Rain of Arrows toward enemies off the map and died 6 and 13 s later.
    fn escape_cast(st: &mut State, s: &GameSnapshot, clock: u32, x: &Sample) -> Option<(String, usize)> {
        let (slot, ability) = st.me.as_deref().and_then(crate::active::escape_of)?;
        let a = s.abilities.iter().find(|a| a.slot == slot)?;
        match (a.ready, a.cooldown_secs) {
            (Some(true), _) => { st.esc_ready_at = Some(clock); st.esc_seen = false; None }
            (Some(false), Some(cd)) if !st.esc_seen => {
                st.esc_seen = true;
                let fresh = cd >= 6 && st.esc_ready_at.is_some_and(|r| clock <= r + 15);
                let deep = x.you.is_some_and(|p| p.y < 0.42);
                let p = TeamPicture::from(s);
                let back = p.as_ref().map_or(0, |p| p.enemies_dead.iter().filter(|r| r.is_some_and(|r| r <= 12)).count());
                // Not while a window is open (two or more of them dead for longer).
                let window = p.as_ref().is_some_and(|p| p.enemies_dead.iter().filter(|r| r.is_none_or(|r| r > 12)).count() >= 2);
                (fresh && deep && !window && clock >= 900).then(|| (ability, x.unseen.unwrap_or(0) + back))
            }
            _ => None,
        }
    }
    /// The same summary is never offered under a new key (a new block saying the same thing);
    /// under its own key it is re-offered until the engine speaks it (another cue may have taken
    /// the frame's slot).
    fn offer_pattern(st: &mut State, key: String, text: String, priority: Priority, skill: Option<String>) -> Option<Cue> {
        if st.last_pattern.as_ref().is_some_and(|(k, t)| *t == text && *k != key) { return None; }
        // The previous block's summary (not this one, which may be re-offered) gates repeats.
        if let Some((k, t)) = st.last_pattern.take() { if k != key { st.prev_pattern = Some(t); } }
        st.last_pattern = Some((key.clone(), text.clone()));
        Some(Cue { key, text, priority, cooldown_secs: 600, skill })
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
        // Who is who: your hero, the enemy team, and which enemy owns which item (scoreboard).
        if let Some(h) = s.hero.clone() { st.me = Some(h); }
        let enemies: Vec<String> = if !s.enemy_heroes.is_empty() { s.enemy_heroes.clone() }
            else { s.hero_status.iter().filter(|h| !h.ally).filter_map(|h| h.hero.clone()).collect() };
        if enemies.len() >= 3 { st.enemies = enemies; }
        if s.players.iter().any(|p| !p.ally && !p.items.is_empty()) {
            let data = game_data();
            let mut owners: HashMap<String, Vec<String>> = HashMap::new();
            for p in s.players.iter().filter(|p| !p.ally) {
                let Some(h) = &p.hero else { continue };
                for c in &p.items { if let Some(i) = data.items.iter().find(|i| &i.class_name == c) { owners.entry(i.name.to_lowercase()).or_default().push(h.clone()); } }
            }
            st.owners = owners;
        }
        if !s.items.is_empty() && !s.dead { st.own_items = s.items.clone(); }
        let mut sample = self.sample(s, clock);
        if sample.area == Area::Dead { st.esc_ready_at = None; } else { sample.cast = Self::escape_cast(&mut st, s, clock, &sample); }
        let dead = sample.area == Area::Dead;
        let mut cues = vec![];
        // A new death: entering the death screen, or, while it shows, more than 60 s without any
        // frame (rec5 merged a death across a 7-minute recording gap). The respawn timer is not
        // used: its digits misread too often (9 read as 45 split one rec4 death into four).
        let jumped = dead && st.dead && st.last_dead_clock.is_some_and(|c| clock > c + 60);
        if dead && (!st.dead || jumped) {
            st.dead = true; st.reviewed = false;
            st.deaths.push(Death { clock, souls_dead: s.souls, ..Default::default() });
        }
        if dead {
            st.last_dead_clock = Some(clock);
            if s.respawn_secs.is_some() { st.last_respawn = s.respawn_secs; }
            // The panel fills in over a few seconds: the latest reading wins, for 25 s (rec5 death
            // 3 read a different panel a minute later, under the same killing blow).
            // Only a readable panel counts: vision reports rows with `death_panel` "open" (older
            // snapshots without the field are trusted). A panel equal to the previous death's
            // (rows and total) is a stale read of that death, never this one's cause.
            let n = st.deaths.len();
            let stale = n >= 2 && !s.death_damage.is_empty() && st.deaths[n - 2].panel == s.death_damage && st.deaths[n - 2].total == s.death_total_damage;
            let readable = s.death_panel.as_deref().is_none_or(|p| p == "open") && !stale;
            if let Some(d) = st.deaths.last_mut() {
                match s.death_panel.as_deref() {
                    Some("closed") => { d.closed_frames += 1; d.closed_since.get_or_insert(clock); }
                    Some(_) => { d.closed_frames = 0; d.closed_since = None; }
                    None => {}
                }
                if s.death_panel.is_some() { d.panel_state = s.death_panel.clone(); }
                if let Some(c) = s.death_cause.clone().filter(|_| readable) { d.cause = Some(c); d.cause_at.get_or_insert(clock); }
                if readable && !s.death_damage.is_empty() && clock <= d.clock + 25 {
                    d.panel = s.death_damage.clone();
                    for r in &s.death_damage { let l = r.source.to_lowercase(); if ["guardian", "walker", "shrine", "patron"].iter().any(|k| l.contains(k)) && !d.structures_seen.contains(&r.source) { d.structures_seen.push(r.source.clone()); } }
                    if s.death_killing_blow.is_some() { d.kb = s.death_killing_blow.clone(); }
                    if s.death_total_damage.is_some() { d.total = s.death_total_damage; }
                }
            }
            // Offered on every death-screen frame under a per-death key (the engine speaks a key
            // once): another High cue may take this frame's slot, and a rule cannot know whether
            // it was spoken. Ready 5 s in once the panel or cause is read (rows: once the killing
            // blow is tagged, or 8 s in), or 20 s in without it (rec5 death 3: panel 18 s in).
            // A collapsed panel waits the full 20 s the first time (the tip below may get it opened);
            // once the tip was said this match, 8 s (nothing more will be read). A panel on screen
            // but not confirmed yet ("reading": the second agreeing read is a frame away, or the
            // shop covers it) gets up to 40 s. Never past the respawn: with the timer at 4 s or less
            // the review goes out with what was read (rec13 6:07, a 10 s death).
            let tip_earlier = st.panel_tip_at.is_some_and(|c| st.deaths.last().is_some_and(|d| c < d.clock));
            let respawn_soon = s.respawn_secs.is_some_and(|r| r <= 4);
            let closed = |d: &Death| d.closed_frames >= PANEL_CLOSED_FRAMES && d.closed_since.is_some_and(|c| clock >= c + PANEL_CLOSED_SECS);
            let ready = st.deaths.last().is_some_and(|d| {
                // A cause without rows gets 4 s for the rows to be read (rec5 death 3: cause at 16 s, rows at 18 s).
                let read = if d.panel.is_empty() { d.cause_at.is_some_and(|c| clock >= c + 4) || tip_earlier && closed(d) && clock >= d.clock + 8 }
                    else { d.kb.is_some() || clock >= d.clock + 8 };
                let waiting = d.panel.is_empty() && d.panel_state.as_deref() == Some("reading") && clock < d.clock + 40;
                clock >= d.clock + 5 && (read || clock >= d.clock + 20 && !waiting || respawn_soon)
            });
            // The damage panel is collapsed on this death screen: tell the player once per match to
            // open it (the review cannot say what killed you without it).
            if st.panel_tip_at.is_none() && st.deaths.last().is_some_and(|d| closed(d) && d.panel.is_empty()) {
                let text = self.card("panel_closed").speech.clone();
                cues.push(Cue::card("review.panel_closed", "review:panel_closed", text, Priority::High, 3600));
            }
            if ready {
                let death = st.deaths.last().unwrap().clone();
                let (text, causes, summary) = self.death_review(&st, &death, s.souls);
                st.reviewed = true;
                if let Some(d) = st.deaths.last_mut() { d.summary = Some(summary); }
                // The skill of the cause the review names (the death is evidence the skill failed).
                let skill = causes.first().and_then(|id| match id.as_str() {
                    "cause" => death.cause.as_deref().and_then(|c| crate::skills::for_key(&format!("death:{}", crate::death_cause_advice(c).0))),
                    t if t.starts_with("threat:") => crate::skills::for_key("threat:"),
                    id => crate::skills::for_card(&format!("review.{id}")),
                });
                cues.push(Cue { key: format!("death:review:{}", death.clock), text, priority: Priority::High, cooldown_secs: 600, skill });
            }
            // Waiting to respawn is the natural moment for the bigger picture.
            // A new repeat killer / crowd-control pattern is worth saying after this death even if
            // this five-minute block already had its summary (rec5: Drifter's second kill came a
            // minute after the block's summary).
            if st.reviewed { if let Some((text, about_deaths, skill)) = self.pattern(&st, clock) {
                let key = if about_deaths { format!("review:pattern:death:{}", st.deaths.len()) } else { format!("review:pattern:{}", clock / t.pattern_window_secs) };
                if let Some(c) = Self::offer_pattern(&mut st, key, text, Priority::Normal, skill) { cues.push(c); }
            }}
        } else if s.screen.as_deref() == Some("Gameplay") {
            // Back alive after a Rejuvenator revive (vision `revived` on the first live frame): the
            // grey screen was not a death (rec17 26:12 got a death review 23 s later).
            if st.dead && s.revived && st.deaths.last().is_some_and(|d| clock <= d.clock + 10) && !st.reviewed {
                st.deaths.pop(); st.dead = false; st.last_respawn = None; st.last_dead_clock = None;
            }
            if st.dead { st.dead = false; st.respawned_at = Some(clock); st.last_respawn = None; st.last_dead_clock = None; }
            let me = sample.you;
            // Live: two teammates fell in the last 10 s while an enemy is on you.
            let earlier = st.samples.iter().filter(|x| x.clock + 10 >= clock).filter_map(|x| x.allies_dead).next();
            let enemy_close = me.is_some_and(|me| s.minimap.as_ref().is_some_and(|m| m.heroes.iter().any(|h| !h.ally && h.confirmed() && h.unseen_ms <= 3000 && dist(h.pos, me) <= 0.2)));
            // A frontliner falling with an enemy on you is the same exit call (rec13 40:35: "Bebop
            // and Rem are down, back out now").
            let was: Vec<String> = st.samples.iter().filter(|x| x.clock + 10 >= clock).find(|x| x.allies_dead.is_some()).map(|x| x.allies_dead_heroes.clone()).unwrap_or_default();
            let fell: Vec<String> = sample.allies_dead_heroes.iter().filter(|h| !was.contains(h)).cloned().collect();
            // One frontliner is enough only in a team fight around you (a teammate and two enemies
            // within 0.2): top-bar slots have no position, a death across the map is not yours.
            let near = |ally: bool| me.map_or(0, |me| s.minimap.as_ref().map_or(0, |m| m.heroes.iter().filter(|h| h.ally == ally && h.confirmed() && h.unseen_ms <= 3000 && dist(h.pos, me) <= 0.2).count()));
            let front_fell = fell.iter().any(|h| self.catalog.roles.frontline.contains(h)) && near(true) >= 1 && near(false) >= 2;
            // Only a lost trade: more of ours fell in those 10 s than of theirs (rec8 13:51 was
            // 3-for-3 and 33:40 2-for-2 at +33k, both called "back out").
            // The trade is the last 60 s: top-bar slots that turned dead on each side (slots, not
            // hero names: rec8 13:3x read one ally as Paige and as another hero).
            let trade = |pick: &dyn Fn(&Sample) -> Option<Vec<String>>| {
                let mut seen: Option<Vec<String>> = None; let mut fell: Vec<String> = vec![];
                for x in st.samples.iter().filter(|x| x.clock + 60 >= clock).chain(std::iter::once(&sample)) {
                    let Some(now) = pick(x) else { continue };
                    if let Some(prev) = &seen { for d in &now { if !prev.contains(d) && !fell.contains(d) { fell.push(d.clone()); } } }
                    seen = Some(now);
                }
                fell.len()
            };
            let theirs_fell = trade(&|x: &Sample| x.enemies_dead_slots.as_ref().map(|v| v.iter().map(|s| s.to_string()).collect()));
            let ours_fell = trade(&|x: &Sample| x.allies_dead_slots.as_ref().map(|v| v.iter().map(|s| s.to_string()).collect()));
            if let (Some(a), Some(b)) = (earlier, sample.allies_dead) { // Lost = more of ours fell than of theirs in the last minute, or two heroes down now (rec13 13:40).
            let down_now = sample.enemies_dead.is_some_and(|e| b > e + 1);
            if (b >= a + 2 || b > a && front_fell) && (ours_fell.max(b - a) > theirs_fell || down_now) && enemy_close {
                let data = game_data();
                let names: Vec<String> = fell.iter().take(2).map(|h| data.hero_name(h)).collect();
                let down = match (names.len(), b - a) {
                    (1, 1) => format!("{}'s down", names[0]),
                    (n, k) if n >= 2 && n == k => format!("{} are down", and_list(&names)),
                    (_, k) => format!("{} are down", plural(k, "teammate", "teammates")),
                };
                // Rotating phrasings (rec8 said "Back out: Shiv and Baba are down." twice, 20 minutes apart).
                let c = self.card("live_fight_lost");
                let all: Vec<&String> = std::iter::once(&c.speech).chain(c.variants.iter()).collect();
                let text = all[st.fight_lost_said % all.len()].replace("{down}", &down);
                cues.push(Cue::card("review.live_fight_lost", "review:fight-lost", text, Priority::High, 30));
            }}
            // Live: leaving base alone right after a respawn with enemies unseen.
            let fresh = st.respawned_at.is_some_and(|r| clock.saturating_sub(r) <= t.respawn_window_secs);
            if fresh && me.is_some_and(|p| p.y < 0.75) && sample.nearest_ally.is_none_or(|d| d > t.missed_fight_distance) && sample.unseen.is_some_and(|u| u >= t.unseen_enemies) {
                let (text, _) = self.fill("live_respawn_alone", &[("count", plural(sample.unseen.unwrap(), "enemy", "enemies"))]);
                cues.push(Cue::card("review.live_respawn_alone", "review:respawn-alone", text, Priority::Normal, 60));
            }
            // Calm moments (healthy, no enemy close) every five minutes: the play pattern.
            // One summary per five-minute block of game time (the key carries the block).
            let calm = sample.hp.is_some_and(|h| h >= 0.6) && !enemy_close;
            if calm { if let Some((text, _, skill)) = self.pattern(&st, clock) {
                if let Some(c) = Self::offer_pattern(&mut st, format!("review:pattern:{}", clock / t.pattern_window_secs), text, Priority::Low, skill) { cues.push(c); }
            }}
        }
        st.samples.push_back(sample);
        let keep = t.pattern_window_secs + 60;
        while st.samples.front().is_some_and(|x| x.clock + keep < clock) { st.samples.pop_front(); }
        cues
    }
    /// The calls a death can ignore, as spoken by the engine (docs/coach-review-8.md D1).
    fn spoken(&self, cue: &Cue) {
        let mut st = self.state.borrow_mut();
        let Some(now) = st.last_clock else { return };
        let kind = match cue.key.as_str() {
            "active:hold_base" | "active:hold_base:out" => "hold",
            "active:urn_hold" => "urn",
            "pos:outnumbered" => "outnumbered",
            k if k.starts_with("active:forward_exit") || k == "active:ult_out" || k == "active:escape_in" => "exit",
            "review:fight-lost" => { st.fight_lost_said += 1; return; }
            "review:panel_closed" => { st.panel_tip_at = st.deaths.last().map(|d| d.clock); return; }
            _ => return,
        };
        st.calls.push((now, kind));
        st.calls.retain(|c| c.0 + 120 >= now);
    }
}
