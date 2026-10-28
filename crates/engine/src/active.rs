//! Active coaching: short calls that tell the player to fix something right now, like a coach
//! watching the screen. Each call names what was seen (a debuff badge, a timer, a souls count, a
//! damage panel) and what to do about it.
//!
//! - Debuffs on you: Victor's Aura of Suffering, Drifter's Stalker's Mark.
//! - Hard crowd control (stun, silence, knocked off a zipline) while health falls.
//! - Enemy-dead windows used from your own half: push the deepest lane.
//! - In base with souls to spend; heading out of a shop with low health.
//! - Unsecured souls carried into danger.
//! - A repeat killer (same enemy hero topping several death panels) with a counter item, or a
//!   long-range hero dying up close.
//! - A team fight far away without you.
//!
//! Evidence rules: every call is built from readings on this frame or remembered from earlier
//! frames; an unread value never counts as safe. Status effects and death panels are only
//! refreshed by the full scan, so the fast path ([`ActiveCoach::fast`]) runs only the position-
//! and souls-based calls.
use crate::{laning::Source, positioning::TeamPicture, Cue, Priority, Rule};
use coach_core::{GameSnapshot, HeroTrack, Lane, Pt, ZoneArea};
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap, sync::{Arc, OnceLock}};

#[derive(Clone, Deserialize)]
pub struct Thresholds {
    pub cc_drop: f32, pub cc_low: f32, pub window_min_secs: u32, pub healthy: f32, pub near_radius: f32,
    pub base_y: f32, pub spend_early: u32, pub spend_late: u32, pub spend_late_clock: u32,
    pub heal_below: f32, pub purchase_drop: u32, pub purchase_window_secs: u32,
    pub unsecured_min: u32, pub unsecured_low_hp: f32,
    pub killer_share: f32, pub killer_margin: f32, pub close_share: f32, pub repeat_deaths: usize, pub lesson_delay_secs: u32,
    pub fight_allies: usize, pub fight_enemies: usize, pub fight_radius: f32, pub fight_far: f32, pub fight_hold_secs: u32,
    pub loss_deaths: usize, pub loss_window_secs: u32, pub loss_far: f32, pub loss_radius: f32,
    pub hold_dead: usize, pub hold_min_secs: u32, pub hold_soon_secs: u32, pub hold_exit_secs: u32, pub hold_exit_y: f32,
    pub raid_enemies: usize, pub raid_y: f32, pub raid_far: f32, pub raid_min_clock: u32,
    pub urn_dead: usize, pub urn_enemy_radius: f32,
}
#[derive(Clone, Deserialize)]
pub struct Debuff { pub status: String, pub hero: String, pub card: String }
#[derive(Clone, Deserialize)]
pub struct Killer { pub hero: String, pub tip: String, pub items: Vec<String> }
#[derive(Clone, Deserialize)]
pub struct Role { pub hero: String, pub role: String, pub source: Source }
#[derive(Clone, Deserialize)]
pub struct Card { pub id: String, pub speech: String, pub priority: String, pub cooldown_secs: u32, pub rationale: String, pub sources: Vec<Source> }
#[derive(Clone, Deserialize)]
pub struct Catalog {
    pub schema_version: u32, pub thresholds: Thresholds, pub debuffs: Vec<Debuff>, pub killers: Vec<Killer>,
    pub roles: Vec<Role>, pub close_sources: Vec<String>, pub generic_sources: Vec<String>, pub cards: Vec<Card>,
}

#[derive(Deserialize)] struct HeroName { class_name: String, name: String }
#[derive(Deserialize)] struct Ability { name: String, #[serde(default)] kind: String, #[serde(default)] heroes: Vec<String> }
#[derive(Deserialize)] struct Item { class_name: String, name: String, cost: u32 }
fn heroes() -> &'static [HeroName] {
    static D: OnceLock<Vec<HeroName>> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(include_str!("../../../data/heroes.json").trim_start_matches('\u{feff}')).expect("shipped heroes"))
}
fn abilities() -> &'static [Ability] {
    static D: OnceLock<Vec<Ability>> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(include_str!("../../../data/abilities.json").trim_start_matches('\u{feff}')).expect("shipped abilities"))
}
fn items() -> &'static [Item] {
    static D: OnceLock<Vec<Item>> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(include_str!("../../../data/items.json").trim_start_matches('\u{feff}')).expect("shipped items"))
}
/// Spoken hero name ("Victor" for hero_frank); the class name when unknown.
pub fn hero_name(class: &str) -> String {
    heroes().iter().find(|h| h.class_name == class).map(|h| h.name.clone()).unwrap_or_else(|| class.trim_start_matches("hero_").replace('_', " "))
}
/// 4200 -> "4,200".
pub fn thousands(n: u32) -> String {
    let s = n.to_string(); let mut out = String::new();
    for (i, c) in s.chars().enumerate() { if i > 0 && (s.len() - i) % 3 == 0 { out.push(','); } out.push(c); }
    out
}
fn norm(s: &str) -> String { s.to_lowercase().replace(['\'', '\u{2019}'], "") }

/// One death as read from the incoming-damage panel. The panel fills in over a few frames and is
/// sometimes misread, so every distinct reading is kept with the number of frames it was seen on.
#[derive(Debug, Clone, Default)]
struct Death { clock: u32, panels: Vec<(Vec<coach_core::DamageRow>, u32)> }
impl Death {
    /// The reading seen on the most frames (the later one on a tie). rec5 showed one panel for
    /// 48 s of a death screen and a different one on a single frame.
    fn rows(&self) -> &[coach_core::DamageRow] {
        self.panels.iter().enumerate().max_by_key(|(i, p)| (p.1, *i)).map(|(_, p)| p.0.as_slice()).unwrap_or(&[])
    }
}

#[derive(Default)]
struct State {
    last_clock: Option<u32>, dead: bool, deaths: Vec<Death>, last_dead_clock: Option<u32>,
    /// Last read souls and item bar (both are often unread on the death screen).
    souls: Option<u32>, items: Vec<String>,
    /// Last read own hero and enemy line-up (the death screen may not show them).
    hero: Option<String>, enemies: Vec<String>,
    /// Last alive Gameplay reading: (clock, souls, in base).
    last_alive: Option<(u32, Option<u32>, Option<bool>)>,
    /// Clock of the last detected purchase (souls dropped while alive) or base exit.
    left_shop: Option<u32>,
    fight_since: Option<u32>,
    /// Allied top-bar slots, and the deaths seen on them: (clock, slot).
    allies: [Slot; 6], ally_deaths: Vec<(u32, usize)>,
    /// Newest ally death already named in a "losing a fight" call.
    loss_said: u32,
    /// Last in-base reading, the clock you walked out of base, and the hold-in-base calls of
    /// this wipe (1 = said in base, 2 = said after walking out).
    in_base: Option<bool>, left_base: Option<u32>, hold_said: u8,
    /// What the calls offered on this step would commit when spoken: hold-in-base bits and the
    /// newest ally death of a losing-fight call.
    hold_offer: u8, loss_offer: u32,
    /// The enemy-dead window named so far (shared with the positioning coach's call).
    window: crate::positioning::WindowMemo,
}

/// One allied top-bar slot across frames: whether it is dead, the last read respawn timer
/// (seconds, clock read), and since when it shows dead.
#[derive(Default, Clone, Copy)]
struct Slot { known: bool, dead: bool, timer: Option<(u32, u32)>, dead_since: u32, counted: bool }
/// Respawn timer cap (wiki: "cap 90s"); a timer read lower than this after the slot has shown
/// dead for longer is a new death.
const MAX_RESPAWN: u32 = 90;

/// New ally deaths from the top bar, counted once a respawn timer confirms them (rec8 41:29: a
/// slot flickered dead for one frame without a timer). A slot turning dead while its previous
/// timer still runs is the same death (a misread alive frame); a timer jumping up, or a first
/// timer lower than the time the slot has already shown dead allows, is a new death.
fn track_ally_deaths(st: &mut State, s: &GameSnapshot, clock: u32) {
    if s.hero_status.len() != 12 { return; }
    for h in s.hero_status.iter().filter(|h| h.ally) {
        let (Some(slot), Some(dead)) = (st.allies.get_mut(h.slot), h.dead) else { continue };
        let left = slot.timer.map(|(t, at)| t.saturating_sub(clock.saturating_sub(at)));
        if dead {
            if !slot.dead {
                let same = left.is_some_and(|l| l > 3);
                if !same { slot.dead_since = clock; slot.timer = None; slot.counted = !slot.known; }
            } else if let Some(t) = h.respawn_secs {
                let again = match left { Some(l) => t > l + 10, None => clock.saturating_sub(slot.dead_since) > MAX_RESPAWN.saturating_sub(t) + 10 };
                if again { slot.dead_since = clock; slot.counted = false; }
            }
            if let Some(t) = h.respawn_secs {
                if !slot.counted { st.ally_deaths.push((slot.dead_since, h.slot)); slot.counted = true; }
                slot.timer = Some((t, clock));
            }
            slot.dead = true;
        } else { slot.dead = false; }
        slot.known = true;
    }
    st.ally_deaths.retain(|e| e.0 + 60 >= clock);
}

/// The active coach. One instance per engine; it keeps a short history.
pub struct ActiveCoach { catalog: Arc<Catalog>, state: RefCell<State>, full: bool }
impl Default for ActiveCoach {
    fn default() -> Self { Self { catalog: Arc::new(Self::shipped()), state: RefCell::new(State::default()), full: true } }
}

/// What a death panel says about the enemies: damage per enemy hero, and the close-range share.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribution {
    pub hero: Option<String>, pub ability: Option<String>,
    /// The top hero's share of all panel damage, and how many times the next hero's damage it is.
    pub share: f32, pub close_share: f32, pub margin: f32,
}
impl Attribution {
    /// This hero clearly did the most damage: a minimum share and a margin over the next hero.
    pub fn killer(&self, t: &Thresholds) -> Option<&str> {
        self.hero.as_deref().filter(|_| self.share >= t.killer_share && self.margin >= t.killer_margin)
    }
}

impl ActiveCoach {
    pub fn shipped() -> Catalog { serde_json::from_str(include_str!("../../../data/coaching/active.json")).expect("shipped active policy") }
    /// The fast-path subset: calls that use only positions, health and souls (all refreshed or
    /// safely held by the fast monitor). Status effects, top-bar timers and death panels are not.
    pub fn fast() -> Self { Self { full: false, ..Self::default() } }
    pub fn catalog(&self) -> &Catalog { &self.catalog }

    fn cue(&self, id: &str, key: String, vars: &[(&str, String)]) -> Cue {
        let c = self.catalog.cards.iter().find(|c| c.id == id).expect("active card");
        let mut text = c.speech.clone();
        for (k, v) in vars { text = text.replace(&format!("{{{k}}}"), v); }
        let priority = match c.priority.as_str() { "high" => Priority::High, "low" => Priority::Low, _ => Priority::Normal };
        Cue { key, text, priority, cooldown_secs: c.cooldown_secs }
    }

    /// Which enemy hero did the most damage in a death panel. Only rows naming a single hero that is
    /// on the enemy team count (an ability shared by several heroes, or a generic row like "Gun
    /// Damage", is never attributed).
    pub fn attribute(&self, rows: &[coach_core::DamageRow], enemies: &[String]) -> Attribution {
        let total: u32 = rows.iter().map(|r| r.damage).sum();
        let generic = |s: &str| self.catalog.generic_sources.iter().any(|g| g.eq_ignore_ascii_case(s));
        let mut per: HashMap<String, (u32, String, u32)> = HashMap::new(); // hero -> (damage, top ability, its damage)
        for r in rows.iter().filter(|r| !generic(&r.source)) {
            let Some(a) = abilities().iter().find(|a| a.name.eq_ignore_ascii_case(&r.source) && a.kind == "ability") else { continue };
            let cands: Vec<&String> = a.heroes.iter().filter(|h| enemies.is_empty() || enemies.contains(h)).collect();
            let [hero] = cands.as_slice() else { continue };
            let e = per.entry((*hero).clone()).or_insert((0, String::new(), 0));
            e.0 += r.damage;
            if r.damage > e.2 { e.1 = a.name.clone(); e.2 = r.damage; }
        }
        let close: u32 = rows.iter().filter(|r| self.catalog.close_sources.iter().any(|c| c.eq_ignore_ascii_case(&r.source))).map(|r| r.damage).sum();
        let close_share = if total > 0 { close as f32 / total as f32 } else { 0.0 };
        let mut ranked: Vec<(String, (u32, String, u32))> = per.into_iter().collect();
        ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
        let margin = match ranked.as_slice() { [a, b, ..] if b.1 .0 > 0 => a.1 .0 as f32 / b.1 .0 as f32, _ => f32::INFINITY };
        match ranked.into_iter().next() {
            Some((hero, (dmg, ability, _))) if total > 0 => Attribution { hero: Some(hero), ability: Some(ability), share: dmg as f32 / total as f32, close_share, margin },
            _ => Attribution { hero: None, ability: None, share: 0.0, close_share, margin },
        }
    }

    fn lesson(&self, s: &GameSnapshot, st: &State) -> Option<Cue> {
        let t = &self.catalog.thresholds;
        let now = st.deaths.last()?;
        if now.rows().is_empty() || s.game_clock_secs? < now.clock + t.lesson_delay_secs { return None; }
        let att: Vec<Attribution> = st.deaths.iter().filter(|d| !d.rows().is_empty()).map(|d| self.attribute(d.rows(), &st.enemies)).collect();
        let cur = self.attribute(now.rows(), &st.enemies);
        let ranged = st.hero.as_ref().is_some_and(|h| self.catalog.roles.iter().any(|r| &r.hero == h && r.role == "long_range"));
        let close_ability = |a: &str| self.catalog.close_sources.iter().any(|c| c.eq_ignore_ascii_case(a));
        if let (Some(hero), Some(ability)) = (cur.killer(t).map(str::to_string), cur.ability.clone()) {
            let n = att.iter().filter(|a| a.killer(t) == Some(hero.as_str())).count();
            if n >= t.repeat_deaths {
                let killer = self.catalog.killers.iter().find(|k| k.hero == hero);
                let tip = if ranged && close_ability(&ability) { "You're long-range: fight from max range".to_string() }
                    else { killer.map(|k| k.tip.clone()).unwrap_or_else(|| "Track him before you commit".into()) };
                let item = killer.and_then(|k| {
                    // The item bar and souls are often unread on the death screen: the last readings count.
                    let pool: Vec<&Item> = k.items.iter().filter(|c| !st.items.contains(c)).filter_map(|c| items().iter().find(|i| &i.class_name == c)).collect();
                    pool.iter().find(|i| st.souls.is_some_and(|v| v >= i.cost)).or_else(|| pool.iter().min_by_key(|i| i.cost)).copied()
                }).map(|i| format!("; consider {} for {} souls", i.name, thousands(i.cost))).unwrap_or_default();
                return Some(self.cue("repeat_killer", format!("active:repeat:{hero}"), &[("killer", hero_name(&hero)), ("ability", ability), ("count", n.to_string()), ("tip", tip), ("item", item)]));
            }
        }
        if ranged && cur.close_share >= t.close_share {
            let n = att.iter().filter(|a| a.close_share >= t.close_share).count();
            if n >= t.repeat_deaths { return Some(self.cue("close_deaths", "active:close".into(), &[("count", n.to_string())])); }
        }
        None
    }

    fn in_base(&self, s: &GameSnapshot) -> Option<bool> { in_base(s, &self.catalog.thresholds) }
}

/// Calls that already tell the player to get out, with the reason (debuff on you, hard CC). The
/// engine treats them as the health back-off call for its cooldown.
pub fn is_escape_call(key: &str) -> bool { key.starts_with("active:debuff:") || key == "active:cc" }

/// In your own base: the zone label says so, or your marker is at the bottom edge of the map.
/// None when neither is read.
fn in_base(s: &GameSnapshot, t: &Thresholds) -> Option<bool> {
    let m = s.minimap.as_ref()?;
    match (m.zone_area, m.you) {
        (Some(ZoneArea::OwnBase), _) => Some(true),
        // A base label of unknown owner: yours when your marker is on your half (rec8 48:11:
        // "Base" at y 0.81 right after respawning).
        (Some(ZoneArea::Base), Some(p)) => Some(p.y > 0.5),
        (Some(ZoneArea::Base), None) => None,
        (_, Some(p)) => Some(p.y > t.base_y),
        (Some(_), None) => Some(false),
        _ => None,
    }
}

fn shared() -> &'static ActiveCoachCards {
    static D: OnceLock<ActiveCoachCards> = OnceLock::new();
    D.get_or_init(|| ActiveCoachCards(ActiveCoach::shipped()))
}
struct ActiveCoachCards(Catalog);

/// The in-base shopping call, for the souls reminder ([`crate::UnspentSoulsRule`]): alive in your
/// own base holding at least the cost of a useful upgrade for the game time (the death-screen
/// tier, [`crate::economy::death_spend`]). `Some(None)` means "in base, nothing to say" so the generic
/// field reminder stays quiet there; `None` means not known to be in base.
pub fn base_spend(s: &GameSnapshot) -> Option<Option<Cue>> {
    let c = &shared().0; let t = &c.thresholds;
    // The scoreboard is read in base too (rec8 21:39: Tab open in base with 4,522 souls).
    if s.dead || !matches!(s.screen.as_deref(), Some("Gameplay" | "Scoreboard")) || in_base(s, t) != Some(true) { return None; }
    if s.minimap.as_ref().is_some_and(|m| m.urn == Some(coach_core::UrnState::CarriedByYou)) { return Some(None); }
    // In the shop a purchase is one click away: the tier below the usual buy (as on the death
    // screen) is worth spending before walking out; less is saving.
    let need = crate::economy::death_spend(s.game_clock_secs.unwrap_or(0));
    let _ = (t.spend_early, t.spend_late, t.spend_late_clock);
    let Some(v) = s.souls.filter(|v| *v >= need) else { return Some(None) };
    let card = c.cards.iter().find(|c| c.id == "base_spend").expect("base_spend card");
    Some(Some(Cue { key: "unspent:base".into(), text: card.speech.replace("{souls}", &thousands(v)), priority: Priority::Normal, cooldown_secs: card.cooldown_secs }))
}

fn health(s: &GameSnapshot) -> Option<f32> { s.hp.zip(s.max_hp).filter(|(h, m)| *m >= 100 && h <= m).map(|(h, m)| h as f32 / m as f32) }
fn live(h: &HeroTrack) -> bool { h.confirmed() && h.unseen_ms <= 1000 }
fn dist(a: Pt, b: Pt) -> f32 { (a.x - b.x).hypot(a.y - b.y) }
fn count_word(n: usize) -> String { match n { 2 => "Two".into(), 3 => "Three".into(), 4 => "Four".into(), 5 => "Five".into(), 6 => "Six".into(), n => n.to_string() } }

impl Rule for ActiveCoach {
    fn evaluate(&self, s: &GameSnapshot) -> Vec<Cue> {
        let Some(clock) = s.game_clock_secs else { return vec![] };
        let t = self.catalog.thresholds.clone();
        let mut st = self.state.borrow_mut();
        if st.last_clock.is_some_and(|c| clock + 120 < c) { *st = State::default(); }
        st.last_clock = Some(clock);
        if s.souls.is_some() { st.souls = s.souls; }
        if !s.items.is_empty() { st.items = s.items.clone(); }
        if s.hero.is_some() { st.hero = s.hero.clone(); }
        if !s.enemy_heroes.is_empty() { st.enemies = s.enemy_heroes.clone(); }
        let dead = s.dead || s.screen.as_deref() == Some("Dead");
        let mut out = vec![];
        st.hold_offer = 0;
        if self.full { track_ally_deaths(&mut st, s, clock); }
        // A respawn is a new walk out of base.
        if dead { st.in_base = None; st.left_base = None; st.hold_said = 0; }

        // --- Death panels (full scan only): remember them and, after the review had its turn,
        //     name a repeat killer or a pattern of close-range deaths.
        if self.full && dead {
            // A new death: entering the death screen, or more than 60 s without frames while it
            // shows (rec5 went from one death screen to the next across a 7-minute gap).
            let jumped = st.dead && st.last_dead_clock.is_some_and(|c| clock > c + 60);
            if !st.dead || jumped { st.deaths.push(Death { clock, panels: vec![] }); }
            st.dead = true; st.last_dead_clock = Some(clock); st.last_alive = None; st.left_shop = None; st.fight_since = None;
            if !s.death_damage.is_empty() {
                // A panel identical to the previous death's is a stale panel, not this death.
                let n = st.deaths.len();
                let stale = n >= 2 && st.deaths[n - 2].rows() == s.death_damage.as_slice();
                if let (false, Some(d)) = (stale, st.deaths.last_mut()) {
                    match d.panels.iter_mut().find(|p| p.0 == s.death_damage) { Some(p) => p.1 += 1, None => d.panels.push((s.death_damage.clone(), 1)) }
                }
            }
            if let Some(c) = self.lesson(s, &st) { out.push(c); }
            return out;
        }
        if s.screen.as_deref() != Some("Gameplay") || dead { return out; }
        st.dead = false;
        let hp = health(s);
        let confirmed = s.health_observations >= 2;
        let falling = s.health_delta_ratio.is_some_and(|d| d < -t.cc_drop);
        let map = s.minimap.as_ref();
        let me = map.and_then(|m| m.you);
        let enemy_near = me.is_some_and(|me| map.is_some_and(|m| m.heroes.iter().any(|h| !h.ally && live(h) && h.distance(me) <= t.near_radius)));
        let in_base = self.in_base(s);

        if self.full {
            // --- Debuff badges on you.
            let effects: Vec<String> = s.status_effects.iter().map(|e| norm(e)).collect();
            for d in &self.catalog.debuffs {
                if effects.iter().any(|e| e.contains(&norm(&d.status))) {
                    out.push(self.cue(&d.card, format!("active:debuff:{}", d.card), &[("hero", hero_name(&d.hero))]));
                }
            }
            // --- Hard CC while health falls (an unread health is not "falling").
            // rec3 1:49: a stun at 81% health dropping 8% in lane is a trade, not a crisis.
            let hurt = hp.is_some_and(|h| h < t.cc_low) || falling;
            if hurt && hp.is_some() {
                let card = if effects.iter().any(|e| e == "stunned") { Some("cc_stunned") }
                    else if effects.iter().any(|e| e == "knocked_off_zipline") { Some("cc_zipline") }
                    else if effects.iter().any(|e| e == "silenced") { Some("cc_silenced") } else { None };
                if let Some(c) = card { out.push(self.cue(c, "active:cc".into(), &[])); }
            }
            // --- Enemy-dead window from your own half.
            // Said once per window (rec8 repeated the call 11 times); the key is shared with the
            // positioning coach's generic window call, which commits the same way.
            let new_window = st.window.observe(crate::positioning::dead_set(s, false), clock, 2);
            if let (Some(p), Some(me), Some(m), true) = (TeamPicture::from(s), me, map, new_window) {
                let mut timers: Vec<u32> = p.enemies_dead.iter().flatten().copied().collect();
                timers.sort_unstable_by(|a, b| b.cmp(a));
                let window = timers.get(1).copied().unwrap_or(0);
                if timers.len() >= 2 && window >= t.window_min_secs && p.enemies_dead.len() > p.allies_dead.len()
                    && hp.is_some_and(|h| h >= t.healthy) && confirmed && me.y > 0.5 && !enemy_near {
                    let deepest = [Lane::Green, Lane::Blue, Lane::Yellow].into_iter().filter_map(|l| m.lane(l).push.map(|v| (l, v))).max_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((lane, _)) = deepest {
                        out.push(self.cue("window", "pos:enemies_down".into(), &[("count", count_word(p.enemies_dead.len())), ("secs", window.to_string()),
                            ("lane", format!("{} lane", crate::laning::lane_name(lane)))]));
                    }
                }
            }
            // --- Leaving a shop (souls dropped while alive) or the base with low health.
            if let Some((pc, ps, pb)) = st.last_alive {
                let bought = ps.zip(s.souls).is_some_and(|(a, b)| a >= b + t.purchase_drop) && clock.saturating_sub(pc) <= 30;
                let exited = pb == Some(true) && in_base == Some(false);
                if bought || exited { st.left_shop = Some(clock); }
            }
            if let (Some(at), Some(h)) = (st.left_shop, hp) {
                if clock.saturating_sub(at) <= t.purchase_window_secs && h < t.heal_below && confirmed && in_base == Some(false) {
                    let mut c = self.cue("heal_first", "active:heal".into(), &[("pct", format!("{:.0}", h.max(0.01) * 100.0))]);
                    // Walking out of a shop nearly dead is a safety call: under 30% health the
                    // engine keeps only high-priority cues (rec8 44:24 left a shop at 3% unwarned).
                    if h < 0.3 { c.priority = Priority::High; }
                    out.push(c);
                }
            }
            // --- Unsecured souls carried into danger.
            if let Some(u) = s.unsecured_souls.filter(|u| *u >= t.unsecured_min) {
                let low = hp.filter(|h| *h < t.unsecured_low_hp && confirmed);
                let why = if let Some(h) = low { Some(format!("at {:.0}% health", h * 100.0)) } else if enemy_near { Some("with an enemy close".into()) } else { None };
                if let Some(why) = why.filter(|_| in_base != Some(true)) {
                    out.push(self.cue("unsecured", "active:unsecured".into(), &[("souls", thousands(u)), ("why", why)]));
                }
            }
            let team = TeamPicture::from(s);
            // --- Walking out of base while the team is dead (rec8 41:50: three teammates on
            //     61-86 s timers, dead 15 s later) or about to respawn (48:07: two back in 8 s).
            if st.in_base == Some(true) && in_base == Some(false) { st.left_base = Some(clock); }
            if in_base.is_some() { st.in_base = in_base; }
            if let Some(p) = &team {
                if p.allies_dead.len() < 2 { st.hold_said = 0; }
                // Dead on a long or unread timer (a teammate back in a few seconds is not missing).
                let long: Vec<Option<u32>> = p.allies_dead.iter().copied().filter(|r| r.is_none_or(|r| r >= t.hold_min_secs)).collect();
                let walked_out = in_base == Some(false) && st.left_base.is_some_and(|c| clock <= c + t.hold_exit_secs) && me.is_some_and(|p| p.y > t.hold_exit_y && p.y < t.base_y - 0.1); // clearly out: the base label reaches y 0.78 (rec5 23:09)
                let next = long.iter().flatten().min().map(|n| format!(", the next back in {n} seconds")).unwrap_or_default();
                if long.len() >= t.hold_dead && (in_base == Some(true) && st.hold_said & 1 == 0 || walked_out && st.hold_said & 2 == 0) {
                    st.hold_offer = if in_base == Some(true) { 1 } else { 3 };
                    let (action, key) = if in_base == Some(true) { ("Hold in base", "active:hold_base") } else { ("Go back to base", "active:hold_base:out") };
                    out.push(self.cue("hold_base", key.into(), &[("count", count_word(long.len())), ("next", next), ("action", action.into())]));
                } else if in_base == Some(true) && st.hold_said == 0 && p.allies_dead.len() >= 2
                    && p.allies_dead.iter().all(|r| r.is_some_and(|r| (3..=t.hold_soon_secs).contains(&r))) {
                    st.hold_offer = 1;
                    let max = p.allies_dead.iter().flatten().max().copied().unwrap_or(0);
                    out.push(self.cue("wait_respawn", "active:hold_base".into(), &[("count", count_word(p.allies_dead.len())), ("secs", max.to_string())]));
                }
            }
            // --- Teammates dying in a fight away from you (rec8 34:46-34:52: two died at the
            //     Yellow side while you farmed the Green side jungle).
            let recent: Vec<(u32, usize)> = st.ally_deaths.iter().copied().filter(|e| e.0 + t.loss_window_secs >= clock).collect();
            if recent.len() >= t.loss_deaths && recent.iter().any(|e| e.0 > st.loss_said) {
                if let (Some(me), Some(m)) = (me, map) {
                    // Where the fight is: the enemy marker closest to an allied one.
                    let tracks: Vec<&HeroTrack> = m.heroes.iter().filter(|h| live(h)).collect();
                    let spot = tracks.iter().filter(|e| !e.ally).filter_map(|e| tracks.iter().filter(|a| a.ally).map(|a| a.distance(e.pos))
                        .min_by(f32::total_cmp).filter(|d| *d <= t.loss_radius).map(|d| (*e, d))).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
                    if let Some(e) = spot.filter(|e| e.distance(me) > t.loss_far) {
                        st.loss_offer = recent.iter().map(|e| e.0).max().unwrap_or(clock);
                        let first = recent.iter().map(|e| e.0).min().unwrap_or(clock);
                        out.push(self.cue("team_losing", "active:team_losing".into(), &[("area", crate::positioning::area(e.pos, e.lane)),
                            ("count", count_word(recent.len()).to_lowercase()), ("secs", clock.saturating_sub(first).max(1).to_string())]));
                    }
                }
            }
            // --- Your base under attack while you are away: the structure banner, else enemy
            //     heroes on the minimap inside your base.
            if in_base != Some(true) && clock >= t.raid_min_clock {
                if let Some(b) = s.objective_banner.as_ref().filter(|b| b.own && !b.name.trim().is_empty()) {
                    out.push(self.cue("base_banner", "active:base_attack".into(), &[("structure", b.name.trim().to_string())]));
                } else if let (Some(me), Some(m)) = (me, map) {
                    let raiders: Vec<&HeroTrack> = m.heroes.iter().filter(|h| !h.ally && live(h) && h.pos.y >= t.raid_y && (0.3..=0.7).contains(&h.pos.x)).collect();
                    if raiders.len() >= t.raid_enemies && raiders.iter().all(|h| h.distance(me) > t.raid_far) {
                        out.push(self.cue("base_attack", "active:base_attack".into(), &[("count", count_word(raiders.len()).to_lowercase())]));
                    }
                }
            }
            // --- Carrying the urn with teammates dead and an enemy close (rec8 27:12-27:39:
            //     two teammates dead, an enemy beside you; you died 18 s later).
            let carrying = map.is_some_and(|m| m.urn == Some(coach_core::UrnState::CarriedByYou)) || effects.iter().any(|e| e == "carrying_urn");
            if let (true, Some(p), Some(me), Some(m)) = (carrying, &team, me, map) {
                if p.allies_dead.len() >= t.urn_dead && m.heroes.iter().any(|h| !h.ally && live(h) && h.distance(me) <= t.urn_enemy_radius) {
                    out.push(self.cue("urn_hold", "active:urn_hold".into(), &[("count", count_word(p.allies_dead.len()).to_lowercase())]));
                }
            }
        }
        st.last_alive = Some((clock, s.souls, in_base));

        // --- A team fight far from you, held for a few seconds.
        let fight = me.zip(map).and_then(|(me, m)| {
            if hp.is_none_or(|h| h < t.healthy) || !confirmed || enemy_near { return None; }
            let allies: Vec<&HeroTrack> = m.heroes.iter().filter(|h| h.ally && live(h)).collect();
            let enemies: Vec<&HeroTrack> = m.heroes.iter().filter(|h| !h.ally && live(h)).collect();
            allies.iter().filter_map(|a| {
                let group: Vec<&&HeroTrack> = allies.iter().filter(|o| o.distance(a.pos) <= t.fight_radius).collect();
                let foes = enemies.iter().filter(|e| e.distance(a.pos) <= t.fight_radius).count();
                (group.len() >= t.fight_allies && foes >= t.fight_enemies && dist(a.pos, me) > t.fight_far).then_some((*a, group.len(), foes))
            }).max_by_key(|x| x.1 + x.2)
        });
        match fight {
            Some((a, allies, foes)) => {
                let since = *st.fight_since.get_or_insert(clock);
                if clock.saturating_sub(since) >= t.fight_hold_secs {
                    out.push(self.cue("fight_missed", "active:fight".into(), &[("allies", allies.to_string()), ("enemies", foes.to_string()), ("area", crate::positioning::area(a.pos, a.lane))]));
                }
            }
            None => st.fight_since = None,
        }
        out
    }
    fn spoken(&self, cue: &Cue) {
        let mut st = self.state.borrow_mut();
        match cue.key.as_str() {
            "active:hold_base" | "active:hold_base:out" => { let o = st.hold_offer; st.hold_said |= o; }
            "active:team_losing" => { st.loss_said = st.loss_said.max(st.loss_offer); }
            "pos:enemies_down" => st.window.said(),
            _ => {}
        }
    }
}
