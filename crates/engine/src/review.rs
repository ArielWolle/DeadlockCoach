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
pub struct Card { pub id: String, pub speech: String, pub lesson: String, pub sources: Vec<Source> }
/// An ability with its own death reason and lesson (matched by panel source or debuff badge).
#[derive(Clone, Deserialize)]
pub struct Threat { pub source: String, pub hero: String, pub close: bool, pub counter: String, pub reason: String, pub lesson: String,
    pub lesson_ranged: String, pub rank: u8, pub replaces: Vec<String>, pub sources: Vec<Source> }
/// Counter items for one need ("spirit", "debuff", "cc", "bullet"), in order of preference.
#[derive(Clone, Deserialize)]
pub struct Counter { pub need: String, pub items: Vec<String>, pub keyword: String, pub why: String, pub sources: Vec<Source> }
#[derive(Clone, Default, Deserialize)]
pub struct Roles { #[serde(default)] pub ranged: Vec<String> }
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
}

/// What one death came down to, kept for repeat killers and pattern summaries.
#[derive(Debug, Clone, Default)]
struct Summary { main: Option<String>, cc: Vec<String>, close: Vec<String> }

#[derive(Debug, Clone, Default)]
struct Death { clock: u32, cause: Option<String>, cause_at: Option<u32>, panel: Vec<DamageRow>, kb: Option<String>, total: Option<u32>,
    /// Souls on the first death-screen frame (before any shopping while dead).
    souls_dead: Option<u32>, summary: Option<Summary> }

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
}

/// The review coach. One instance per engine; it keeps its own history.
pub struct ReviewCoach { catalog: Arc<Catalog>, state: RefCell<State> }
impl Default for ReviewCoach {
    fn default() -> Self { Self { catalog: Arc::new(serde_json::from_str(include_str!("../../../data/coaching/review.json")).expect("shipped review policy")), state: RefCell::new(State::default()) } }
}

fn dist(a: Pt, b: Pt) -> f32 { (a.x - b.x).hypot(a.y - b.y) }
fn plural(n: usize, one: &str, many: &str) -> String { if n == 1 { format!("1 {one}") } else { format!("{n} {many}") } }
fn minutes(secs: u32) -> String { if secs < 90 { format!("{secs} seconds") } else { format!("{:.1} minutes", secs as f32 / 60.0) } }
fn num(v: u32) -> String { if v >= 1000 { format!("{},{:03}", v / 1000, v % 1000) } else { v.to_string() } }
fn norm(s: &str) -> String { s.to_lowercase().replace(['\'', '\u{2019}'], "").trim().to_string() }
/// "a", "a and b", "a, b and c".
fn and_list(v: &[String]) -> String {
    match v.len() { 0 => String::new(), 1 => v[0].clone(), n => format!("{} and {}", v[..n - 1].join(", "), v[n - 1]) }
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
    listed: u32, total: Option<u32>, zipline: bool, top: Option<(String, u32)>,
}
impl Attribution {
    /// The panel lists (nearly) everything: `total` read and the rows add up to 90% of it.
    fn complete(&self) -> bool { self.total.is_some_and(|t| self.listed * 10 >= t * 9) }
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
        let mut a = Attribution { total: d.total, ..Default::default() };
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
            enemies_dead: team.as_ref().map(|p| p.enemies_dead.len()), souls: s.souls, fight_at, deep_wave, effects }
    }

    /// Why the last death happened, from the samples before it and the damage panel.
    /// Returns (spoken text, reason ids, summary).
    fn death_review(&self, st: &State, death: &Death) -> (String, Vec<String>, Summary) {
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
        // (rank, id, text) and (rank, id, lesson); lower rank = stronger evidence.
        let mut reasons: Vec<(u8, String, String)> = vec![];
        let mut lessons: Vec<(u8, String, String)> = vec![];
        let mut summary = Summary::default();
        let add = |reasons: &mut Vec<(u8, String, String)>, lessons: &mut Vec<(u8, String, String)>, rank: u8, lrank: u8, id: &str, vars: &[(&str, String)]| {
            let (a, b) = self.fill(id, vars);
            if !a.is_empty() { reasons.push((rank, id.to_string(), a)); }
            if !b.is_empty() { lessons.push((lrank, id.to_string(), b)); }
        };
        if no_frames { reasons.push((0, "no_frames".into(), self.card("no_frames").speech.clone())); }

        // --- The damage panel: who did it.
        let attr = self.attribute(death, st);
        let mut threats_seen: Vec<&Threat> = vec![];
        if let Some(a) = &attr {
            for r in &death.panel { if let Some(th) = self.threat(&r.source) { if !threats_seen.iter().any(|x| x.source == th.source) { threats_seen.push(th); } } }
            let pct = |d: u32| (d * 100 + a.listed / 2) / a.listed.max(1);
            // A named hero leads the review only with a real share of the listed damage (rec3: an
            // unnamed gun did 461 and Viscous 20).
            let lead = a.heroes.first().filter(|x| x.1 * 5 >= a.listed);
            if lead.is_none() {
                if let Some((src, dmg)) = &a.top {
                    let id = if a.heroes.is_empty() { "panel_generic" } else { "panel_top" };
                    let src = if src.eq_ignore_ascii_case("gun damage") { "An enemy gun".to_string() } else { src.clone() };
                    add(&mut reasons, &mut lessons, 0, 9, id, &[("source", src), ("dmg", num(*dmg))]);
                }
            }
            if let Some((h, dmg, srcs)) = lead {
                let name = data.hero_name(h);
                let abilities = and_list(srcs);
                summary.main = Some(h.clone());
                if a.heroes.len() == 1 && a.guns <= 1 && dmg * 10 >= a.listed * 4 {
                    add(&mut reasons, &mut lessons, 0, 9, "solo", &[("hero", name.clone()), ("abilities", abilities)]);
                } else {
                    let id = if a.complete() { "killer" } else { "killer_partial" };
                    add(&mut reasons, &mut lessons, 0, 9, id, &[("hero", name.clone()), ("dmg", num(*dmg)), ("pct", pct(*dmg).to_string()), ("abilities", abilities)]);
                }
            }
            if a.min_heroes() >= 2 {
                let mut names: Vec<String> = a.heroes.iter().map(|x| data.hero_name(&x.0)).collect();
                let extra = a.min_heroes() - a.heroes.len();
                if extra == 1 && !names.is_empty() { names.push("another hero's gun".into()); }
                else if extra >= 1 { names.push(format!("{extra} {}", if names.is_empty() { "separate guns" } else { "more guns" })); }
                add(&mut reasons, &mut lessons, 1, 9, "involved", &[("count", a.min_heroes().to_string()), ("names", and_list(&names))]);
            }
            if let Some(kb) = death.kb.as_deref().filter(|k| !k.eq_ignore_ascii_case("gun damage")) {
                let what = match self.classify(kb, st) { Src::Hero(h) => format!("{}'s {kb}", data.hero_name(&h)), _ => kb.to_string() };
                add(&mut reasons, &mut lessons, 4, 9, "killing_blow", &[("what", what)]);
            }
            if a.troopers > 0 && (a.troopers * 10 >= a.listed || a.troopers >= 300) {
                add(&mut reasons, &mut lessons, 2, 2, "troopers", &[("dmg", num(a.troopers))]);
            }
            if let Some((src, dmg)) = a.structures.iter().max_by_key(|x| x.1) {
                add(&mut reasons, &mut lessons, 2, 2, "structure", &[("source", src.clone()), ("dmg", num(*dmg))]);
            }
            if let Some((src, dmg)) = a.neutral.iter().max_by_key(|x| x.1).filter(|x| x.1 * 10 >= a.listed) {
                add(&mut reasons, &mut lessons, 5, 9, "neutral", &[("source", src.clone()), ("dmg", num(*dmg))]);
            }
        }
        // Debuff badges seen on you before the death are threats too (Stalker's Mark 4-6 s
        // before the hit, rec5 death 4).
        for x in &before { for e in &x.effects { if let Some(th) = self.threat(e) { if !threats_seen.iter().any(|y| y.source == th.source) { threats_seen.push(th); } } } }
        let replaced: Vec<&str> = threats_seen.iter().flat_map(|t| t.replaces.iter().map(String::as_str)).collect();
        for th in &threats_seen {
            if th.close { summary.close.push(th.source.clone()); }
            if th.reason.is_empty() { continue; }
            let hero = data.hero_name(&th.hero);
            let text = th.reason.replace("{hero}", &hero);
            reasons.push((1, format!("threat:{}", th.source), text));
            let lesson = if ranged && !th.lesson_ranged.is_empty() { &th.lesson_ranged } else { &th.lesson };
            if !lesson.is_empty() { lessons.push((th.rank, format!("threat:{}", th.source), lesson.replace("{hero}", &hero).replace("{you}", &you))); }
        }

        // --- Crowd control in the window (status effects) and the zipline knock-off (panel row).
        let mut cc: Vec<(u8, &str, String)> = vec![]; // (lesson rank, card, spoken fragment)
        let stuns = before.iter().enumerate().filter(|(i, x)| x.effects.iter().any(|e| e == "stunned")
            && (*i == 0 || !before[i - 1].effects.iter().any(|e| e == "stunned"))).count();
        let has = |e: &str| before.iter().any(|x| x.effects.iter().any(|y| y == e));
        if has("knocked_off_zipline") || attr.as_ref().is_some_and(|a| a.zipline) { cc.push((2, "cc_zipline", String::new())); }
        if stuns > 0 { cc.push((3, "cc_stunned", match stuns { 1 => String::new(), 2 => " twice".into(), n => format!(" {n} times") })); }
        if has("silenced") { cc.push((3, "cc_silenced", String::new())); }
        if has("disarmed") { cc.push((3, "cc_disarmed", String::new())); }
        if has("out_of_stamina") { cc.push((3, "cc_out_of_stamina", String::new())); }
        if !cc.is_empty() {
            // Spoken in the order the player lives it: stun / silence first, then the knock-off.
            let order = ["cc_stunned", "cc_silenced", "cc_disarmed", "cc_zipline", "cc_out_of_stamina"];
            let mut frags: Vec<(usize, String)> = cc.iter().map(|(_, id, times)| (order.iter().position(|o| o == id).unwrap_or(9), self.card(id).speech.replace("{times}", times))).collect();
            frags.sort();
            let list: Vec<String> = frags.into_iter().map(|f| f.1).collect();
            add(&mut reasons, &mut lessons, 1, 9, "cc", &[("effects", and_list(&list))]);
            for (lr, id, _) in &cc { lessons.push((*lr, id.to_string(), self.card(id).lesson.clone())); }
            summary.cc = cc.iter().map(|c| c.1.trim_start_matches("cc_").to_string()).collect();
        }

        // --- Repeat killer by hero (and the counter item), else by cause family.
        let souls_at = before.iter().rev().filter_map(|x| x.souls).next().or(if no_frames { death.souls_dead } else { None });
        let mut repeat_hero = false;
        if let Some(main) = &summary.main {
            let n = 1 + st.deaths.iter().filter(|d| d.clock != death.clock).filter(|d| d.summary.as_ref().and_then(|s| s.main.as_ref()) == Some(main)).count();
            if n >= 2 {
                repeat_hero = true;
                let hero = data.hero_name(main);
                add(&mut reasons, &mut lessons, 2, 8, "repeat_killer", &[("count", n.to_string()), ("hero", hero.clone())]);
                // Need: the hero's named threat (Aura -> spirit, Mark -> debuff), else spirit for
                // ability damage.
                let th = threats_seen.iter().find(|t| &t.hero == main && !t.counter.is_empty());
                let need = th.map(|t| t.counter.as_str()).unwrap_or("spirit");
                if let Some(l) = self.counter(need, &hero, th.map(|t| t.source.as_str()).unwrap_or("these"), souls_at, st) { lessons.push((0, "counter_item".into(), l)); }
            }
        }
        if !repeat_hero && death.panel.is_empty() {
            if let Some(fam) = death.cause.as_deref().map(cause_family) {
                let n = st.deaths.iter().filter(|d| d.cause.as_deref().map(cause_family).as_deref() == Some(fam.as_str())).count();
                if n >= 2 { add(&mut reasons, &mut lessons, 3, 3, "repeat_cause", &[("count", n.to_string()), ("cause", fam)]); }
            }
        }

        // --- What the frames before the death showed.
        if !no_frames {
            // Burst: from the healthiest reading in the window (the earliest one at that level) to
            // the end; rec5 said "69% to dead in 2 s" for a 100% -> 0 fall over ~10 s.
            let peak = alive_known.iter().map(|x| x.hp.unwrap()).fold(0.0f32, f32::max);
            // From 30%: rec5 went back in at 49% and fell to dead in 6 s, which is still a burst.
            if let Some(h) = alive_known.iter().find(|x| x.hp.is_some_and(|h| h >= 0.3 && h >= peak - 0.03)) {
                // If even the last alive reading was healthy, the fall was between readings: count
                // to the death screen instead (an upper bound, never an invented shorter time).
                let secs = (if h.clock == end { death.clock } else { end }).saturating_sub(h.clock).max(2);
                // 15 s: the death screen is seen a few seconds after the killing blow.
                if secs <= 15 {
                    let (text, _) = self.fill("burst", &[("from", format!("{:.0}", h.hp.unwrap() * 100.0)), ("secs", secs.to_string())]);
                    reasons.push((3, "burst".into(), text));
                    if !replaced.contains(&"burst") {
                        let l = if ranged { self.fill("burst_ranged", &[("you", you.clone())]).1 } else { self.card("burst").lesson.clone() };
                        lessons.push((4, "burst".into(), l));
                    }
                }
            }
            // Teammates dying in the window before you.
            let dead_first = before.iter().filter_map(|x| x.allies_dead).next();
            let dead_last = before.iter().rev().filter_map(|x| x.allies_dead).next();
            if let (Some(a), Some(b)) = (dead_first, dead_last) { if b > a { add(&mut reasons, &mut lessons, 2, 2, "fight_lost", &[("count", plural(b - a, "teammate", "teammates"))]); } }
            // Alone = teammates were visible on the minimap and all far (unseen is not "far").
            let recent: Vec<&&Sample> = before.iter().rev().filter(|x| x.nearest_ally.is_some()).take(4).collect();
            let alone = !recent.is_empty() && recent.iter().all(|x| x.nearest_ally.is_some_and(|d| d > t.alone_distance));
            let after_respawn = st.respawned_at.is_some_and(|r| end.saturating_sub(r) <= t.respawn_window_secs);
            if alone && after_respawn { add(&mut reasons, &mut lessons, 2, 2, "respawn_alone", &[("secs", end.saturating_sub(st.respawned_at.unwrap()).to_string())]); }
            else if alone { add(&mut reasons, &mut lessons, 4, 4, "alone", &[]); }
            else if st.respawned_at.is_some_and(|r| end >= r && end - r <= 90) {
                add(&mut reasons, &mut lessons, 4, 5, "soon_after_respawn", &[("secs", (end - st.respawned_at.unwrap()).to_string())]);
            }
            // Went back in weak: every health reading of the last 30 s under 60% (rec5: 44-49% for
            // 30 s after shopping, then dead).
            let minute: Vec<f32> = st.samples.iter().filter(|x| x.clock + 30 >= end && x.clock <= end).filter_map(|x| x.hp).collect();
            if minute.len() >= 3 && minute.iter().all(|h| *h < 0.6) {
                let top = minute.iter().copied().fold(0.0f32, f32::max);
                add(&mut reasons, &mut lessons, 2, 1, "weak_entry", &[("pct", format!("{}", ((top * 100.0 / 5.0).ceil() * 5.0) as u32))]);
            }
            // In the enemy half with enemies unseen.
            if let Some(x) = before.iter().rev().find(|x| x.you.is_some_and(|p| p.y < 0.48) && x.unseen.is_some_and(|u| u >= t.unseen_enemies)) {
                add(&mut reasons, &mut lessons, 3, 3, "forward_missing", &[("count", plural(x.unseen.unwrap(), "enemy", "enemies"))]);
            }
        }
        let souls_reason = souls_at.filter(|v| *v >= t.souls_at_death.max(crate::economy::death_spend(death.clock))).map(|souls| self.fill("souls", &[("souls", num(souls))]));
        // Non-hero killers (Midboss, Walker, structures, troopers) have their own advice when the
        // panel rows were not read.
        if attr.is_none() {
            if let Some((tag, advice)) = death.cause.as_deref().map(crate::death_cause_advice).filter(|(tag, _)| *tag != "heroes") {
                if no_frames { lessons.push((0, format!("cause_{tag}"), advice.to_string())); } else { reasons.push((0, format!("cause_{tag}"), advice.to_string())); }
            }
        }
        if reasons.is_empty() && souls_reason.is_none() { add(&mut reasons, &mut lessons, 9, 9, "no_reason", &[]); }
        if no_frames && lessons.is_empty() { lessons.push((9, "no_frames".into(), self.card("no_frames").lesson.clone())); }
        // Only a souls lesson when no counter item already names what to buy.
        if let Some((_, lesson)) = &souls_reason { if !lessons.iter().any(|l| l.1 == "counter_item") { lessons.push((5, "souls".into(), lesson.clone())); } }
        // Lessons: the two with the strongest evidence, without repeats.
        lessons.sort_by_key(|l| l.0);
        let mut picked: Vec<&(u8, String, String)> = vec![];
        for l in &lessons { if !picked.iter().any(|p| p.2 == l.2) && picked.len() < 2 { picked.push(l); } }
        // Reasons: the strongest four, plus the one behind each picked lesson (a lesson is never
        // given without the observation it answers).
        let backs = |id: &str| -> String { if id.starts_with("cc_") { "cc".into() } else if id == "counter_item" { "repeat_killer".into() } else { id.to_string() } };
        let needed: Vec<String> = picked.iter().map(|l| backs(&l.1)).collect();
        reasons.sort_by_key(|r| r.0);
        let limit = if no_frames { 5 } else { 4 };
        let mut top: Vec<(u8, String, String)> = vec![];
        for r in reasons { if top.len() < limit || needed.contains(&r.1) { top.push(r); } }
        if let Some((text, _)) = souls_reason { top.push((5, "souls".into(), text)); }
        let why = top.iter().map(|r| r.2.as_str()).collect::<Vec<_>>().join(" ");
        let picked: Vec<&str> = picked.iter().map(|l| l.2.as_str()).collect();
        let cause = if death.panel.is_empty() { death.cause.as_deref().map(|c| format!("Damage panel: {c}. ")).unwrap_or_default() } else { String::new() };
        let next = if picked.is_empty() { String::new() } else { format!("Next time: {}", picked.join(" ")) };
        let lead = if no_frames { "" } else { "Why you died: " };
        (format!("{lead}{why} {cause}{next}").trim_end().to_string(), top.iter().map(|r| r.1.clone()).collect(), summary)
    }

    /// The last `pattern_window_secs` of play summarised in a few observations.
    fn pattern(&self, st: &State, now: u32) -> Option<(String, bool)> {
        let t = &self.catalog.thresholds;
        let data = game_data();
        let w: Vec<&Sample> = st.samples.iter().filter(|x| x.clock + t.pattern_window_secs >= now).collect();
        let span = w.last()?.clock.saturating_sub(w.first()?.clock);
        if span < t.pattern_window_secs * 4 / 5 { return None; }
        // Time per area: each sample stands for the gap to the next one (capped at 10 s).
        let mut time = [0u32; 5]; let mut hoard = 0u32; let mut deep_away = 0u32; let mut peak = 0u32;
        // Enemy-dead windows: (start, length, most dead, ever in their half).
        let mut windows: Vec<(u32, u32, usize, bool)> = vec![]; let mut open: Option<(u32, u32, usize, bool)> = None;
        let mut fights_missed = 0u32; let mut fights_joined = 0u32; let mut in_fight: Option<bool> = None;
        for (a, b) in w.iter().zip(w.iter().skip(1)) {
            let dt = b.clock.saturating_sub(a.clock).min(10);
            time[a.area as usize] += dt;
            if a.area != Area::Dead && a.souls.is_some_and(|v| v >= t.hoard_souls.max(crate::economy::spend_target(a.clock))) { hoard += dt; peak = peak.max(a.souls.unwrap()); }
            // Defending in base is not being away from a deep wave; only farming camps is.
            if a.deep_wave && a.area == Area::Jungle { deep_away += dt; }
            let alive = matches!(a.area, Area::Lane | Area::Jungle | Area::Base);
            match (a.enemies_dead.filter(|n| *n >= 2), alive, open.as_mut()) {
                (Some(n), true, Some(w)) => { w.1 += dt; w.2 = w.2.max(n); w.3 |= a.you.is_some_and(|p| p.y < 0.5); }
                (Some(n), true, None) => open = Some((a.clock, dt, n, a.you.is_some_and(|p| p.y < 0.5))),
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
        let unused = windows.into_iter().filter(|w| w.1 >= 20 && !w.3).max_by_key(|w| w.1);
        // Deaths = death-screen runs inside the window (a death just before it still counts).
        // (Unreadable frames between death screens do not split a death.)
        let deaths = (0..w.len()).filter(|&i| w[i].area == Area::Dead
            && w[..i].iter().rev().find(|x| x.area != Area::Unknown).is_none_or(|p| p.area != Area::Dead)).count();
        let known = (time[Area::Lane as usize] + time[Area::Jungle as usize] + time[Area::Base as usize]).max(1);
        let pct = |a: Area| time[a as usize] * 100 / known;
        let min = (span + 30) / 60;
        // Reviewed deaths in the window first: repeat killers, crowd control, close range.
        let mut out: Vec<(String, String)> = vec![];
        let recent: Vec<&Summary> = st.deaths.iter().filter(|d| d.clock + t.pattern_window_secs >= now).filter_map(|d| d.summary.as_ref()).collect();
        let mut killers: Vec<(&String, usize)> = vec![];
        for s in &recent { if let Some(m) = &s.main { match killers.iter_mut().find(|k| k.0 == m) { Some(k) => k.1 += 1, None => killers.push((m, 1)) } } }
        if let Some((hero, n)) = killers.into_iter().filter(|k| k.1 >= 2).max_by_key(|k| k.1) {
            let name = data.hero_name(hero);
            let (text, generic) = self.fill("pattern_killer", &[("hero", name.clone()), ("count", n.to_string()), ("min", min.to_string())]);
            let need = self.catalog.threats.iter().find(|th| &th.hero == hero && !th.counter.is_empty());
            let lesson = self.counter(need.map(|t| t.counter.as_str()).unwrap_or("spirit"), &name, need.map(|t| t.source.as_str()).unwrap_or("these"), None, st).unwrap_or(generic);
            out.push((text, lesson));
        }
        let with_cc: Vec<&&Summary> = recent.iter().filter(|s| !s.cc.is_empty()).collect();
        if recent.len() >= 2 && with_cc.len() >= 2 {
            let mut effects: Vec<String> = vec![];
            for s in &with_cc { for e in &s.cc { let f = self.card(&format!("cc_{e}")).speech.replace("{times}", ""); if !effects.contains(&f) { effects.push(f); } } }
            let which = match (with_cc.len(), recent.len()) { (2, 2) => "both deaths".to_string(), (a, b) if a == b => format!("all {b} deaths"), (a, b) => format!("{a} of your {b} deaths") };
            out.push(self.fill("pattern_cc", &[("which", which), ("effects", and_list(&effects))]));
        }
        if self.ranged(st) && recent.len() >= 2 && recent.iter().all(|s| !s.close.is_empty()) {
            let mut srcs: Vec<String> = vec![];
            for s in &recent { for c in &s.close { if !srcs.contains(c) { srcs.push(c.clone()); } } }
            let you = data.hero_name(st.me.as_deref().unwrap_or_default());
            let count = if recent.len() == 2 { "Both".to_string() } else { format!("All {}", recent.len()) };
            out.push(self.fill("pattern_close", &[("count", count), ("sources", and_list(&srcs)), ("you", you)]));
        }
        let about_deaths = !out.is_empty();
        if deaths >= 2 || time[Area::Dead as usize] >= 60 {
            out.push(self.fill("pattern_deaths", &[("min", min.to_string()), ("deaths", plural(deaths, "time", "times")), ("dead", minutes(time[Area::Dead as usize]))]));
        }
        // A summary never repeats an item the previous summary already said (rec6 said "held over
        // 6,400 souls" in two summaries in a row; rec5 repeated the same 38:21 window).
        // The previous summary: the last one offered for an earlier 5-minute block, else the one before.
        let block = format!(":{}", now / t.pattern_window_secs);
        let previous = st.last_pattern.as_ref().filter(|(k, _)| !k.ends_with(&block)).map(|(_, text)| text).or(st.prev_pattern.as_ref());
        let said_before = |needle: &str| previous.is_some_and(|text| text.contains(needle));
        let unused = unused.filter(|(start, ..)| !said_before(&format!("At {}:{:02},", start / 60, start % 60)));
        let hoard = if said_before("souls for") { 0 } else { hoard };
        if let Some((start, secs, n, _)) = unused {
            out.push(self.fill("pattern_window", &[("time", format!("{}:{:02}", start / 60, start % 60)), ("count", plural(n, "enemy", "enemies")), ("secs", secs.to_string())]));
        }
        if fights_missed >= 2 { out.push(self.fill("pattern_missed", &[("fights", plural(fights_missed as usize, "time", "times"))])); }
        if pct(Area::Jungle) >= 50 && known >= 120 { out.push(self.fill("pattern_jungle", &[("jungle", pct(Area::Jungle).to_string()), ("min", min.to_string())])); }
        if deep_away >= 30 { out.push(self.fill("pattern_waves", &[("secs", deep_away.to_string())])); }
        if hoard >= 60 { out.push(self.fill("pattern_hoard", &[("hoard", num(t.hoard_souls.max(crate::economy::spend_target(now)))), ("secs", hoard.to_string()), ("peak", num(peak))])); }
        if out.is_empty() {
            if deaths > 0 || known < 120 { return None; }
            out.push(self.fill("pattern_good", &[("min", min.to_string()), ("lane", pct(Area::Lane).to_string()), ("jungle", pct(Area::Jungle).to_string()), ("joined", if fights_joined > 0 { format!(", with your team for {}", plural(fights_joined as usize, "fight", "fights")) } else { String::new() })]));
        }
        let picked: Vec<_> = out.into_iter().take(3).collect();
        let text = picked.iter().map(|(a, b)| format!("{a} {b}")).collect::<Vec<_>>().join(" ");
        Some((text, about_deaths))
    }
}

impl ReviewCoach {
    /// The same summary is never offered under a new key (a new block saying the same thing);
    /// under its own key it is re-offered until the engine speaks it (another cue may have taken
    /// the frame's slot).
    fn offer_pattern(st: &mut State, key: String, text: String, priority: Priority) -> Option<Cue> {
        if st.last_pattern.as_ref().is_some_and(|(k, t)| *t == text && *k != key) { return None; }
        // The previous block's summary (not this one, which may be re-offered) gates repeats.
        if let Some((k, t)) = st.last_pattern.take() { if k != key { st.prev_pattern = Some(t); } }
        st.last_pattern = Some((key.clone(), text.clone()));
        Some(Cue { key, text, priority, cooldown_secs: 600 })
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
        let sample = self.sample(s, clock);
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
            if let Some(d) = st.deaths.last_mut() {
                if let Some(c) = s.death_cause.clone() { d.cause = Some(c); d.cause_at.get_or_insert(clock); }
                if !s.death_damage.is_empty() && clock <= d.clock + 25 {
                    d.panel = s.death_damage.clone();
                    if s.death_killing_blow.is_some() { d.kb = s.death_killing_blow.clone(); }
                    if s.death_total_damage.is_some() { d.total = s.death_total_damage; }
                }
            }
            // Offered on every death-screen frame under a per-death key (the engine speaks a key
            // once): another High cue may take this frame's slot, and a rule cannot know whether
            // it was spoken. Ready 5 s in once the panel or cause is read (rows: once the killing
            // blow is tagged, or 8 s in), or 20 s in without it (rec5 death 3: panel 18 s in).
            let ready = st.deaths.last().is_some_and(|d| {
                // A cause without rows gets 4 s for the rows to be read (rec5 death 3: cause at 16 s, rows at 18 s).
                let read = if d.panel.is_empty() { d.cause_at.is_some_and(|c| clock >= c + 4) } else { d.kb.is_some() || clock >= d.clock + 8 };
                clock >= d.clock + 5 && (read || clock >= d.clock + 20)
            });
            if ready {
                let death = st.deaths.last().unwrap().clone();
                let (text, _, summary) = self.death_review(&st, &death);
                st.reviewed = true;
                if let Some(d) = st.deaths.last_mut() { d.summary = Some(summary); }
                cues.push(Cue { key: format!("death:review:{}", death.clock), text, priority: Priority::High, cooldown_secs: 600 });
            }
            // Waiting to respawn is the natural moment for the bigger picture.
            // A new repeat killer / crowd-control pattern is worth saying after this death even if
            // this five-minute block already had its summary (rec5: Drifter's second kill came a
            // minute after the block's summary).
            if st.reviewed { if let Some((text, about_deaths)) = self.pattern(&st, clock) {
                let key = if about_deaths { format!("review:pattern:death:{}", st.deaths.len()) } else { format!("review:pattern:{}", clock / t.pattern_window_secs) };
                if let Some(c) = Self::offer_pattern(&mut st, key, text, Priority::Normal) { cues.push(c); }
            }}
        } else if s.screen.as_deref() == Some("Gameplay") {
            if st.dead { st.dead = false; st.respawned_at = Some(clock); st.last_respawn = None; st.last_dead_clock = None; }
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
            if calm { if let Some((text, _)) = self.pattern(&st, clock) {
                if let Some(c) = Self::offer_pattern(&mut st, format!("review:pattern:{}", clock / t.pattern_window_secs), text, Priority::Low) { cues.push(c); }
            }}
        }
        st.samples.push_back(sample);
        let keep = t.pattern_window_secs + 60;
        while st.samples.front().is_some_and(|x| x.clock + keep < clock) { st.samples.pop_front(); }
        cues
    }
}
