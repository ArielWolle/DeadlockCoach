//! Coaching engine: GameSnapshot in, spoken Cues out. Pure logic, no I/O,
//! so it can be tested with fake snapshots and replayed from recorded sessions.
use coach_core::GameSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub mod map_rules;
pub mod laning;
pub mod positioning;
pub mod jungle;
pub mod economy;
pub mod review;
pub mod active;
pub mod praise;
pub mod plans;
pub mod skills;
mod counter_explanations;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority { Low, Normal, High }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cue {
    /// Dedup key: the same key is not repeated within its cooldown.
    pub key: String,
    pub text: String,
    pub priority: Priority,
    pub cooldown_secs: u32,
    /// The skill this cue coaches (data/coaching/skills.json), so the browser can stay quiet about
    /// mastered skills. None only for summaries of several habits (review:pattern) and the
    /// cause-less death review.
    #[serde(default)]
    pub skill: Option<String>,
}
impl Cue {
    /// A cue whose skill comes from its key ([`skills::for_key`]).
    pub fn new(key: impl Into<String>, text: impl Into<String>, priority: Priority, cooldown_secs: u32) -> Cue {
        let key = key.into();
        let skill = skills::for_key(&key);
        Cue { key, text: text.into(), priority, cooldown_secs, skill }
    }
    /// A cue spoken from a coaching card ("<module>.<card id>"): the card's skill, else the key's.
    pub fn card(card: &str, key: impl Into<String>, text: impl Into<String>, priority: Priority, cooldown_secs: u32) -> Cue {
        let key = key.into();
        let skill = skills::of(Some(card), &key);
        Cue { key, text: text.into(), priority, cooldown_secs, skill }
    }
}

/// A scheduled objective, loaded from data/timings.json (interval 0 means one-shot; values are config,
/// not hardcoded; verify them against the current patch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerDef {
    pub name: String,
    pub first_spawn_secs: u32,
    pub interval_secs: u32,
    pub warn_ahead_secs: u32,
}

pub trait Rule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue>;
    /// Called by the engine for every cue it speaks (from any rule), after `evaluate` on the same
    /// snapshot. Rules that say something once per situation commit here, so a cue that lost the
    /// step to another one is offered again instead of being lost.
    fn spoken(&self, _cue: &Cue) {}
}

pub struct TimerRule(pub TimerDef);

impl Rule for TimerRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let (Some(t), d) = (snap.game_clock_secs, &self.0) else { return vec![] };
        if t + d.warn_ahead_secs < d.first_spawn_secs { return vec![]; }
        if d.interval_secs==0 && t>=d.first_spawn_secs {return vec![];}
        // Next spawn at or after now.
        let n = if t <= d.first_spawn_secs || d.interval_secs==0 { 0 } else { (t - d.first_spawn_secs).div_ceil(d.interval_secs) };
        let next = d.first_spawn_secs + n * d.interval_secs;
        let until = next - t;
        if until <= d.warn_ahead_secs && until > 0 {
            vec![Cue::new(format!("timer:{}:{}", d.name, next), format!("{} in {} seconds", d.name, until), Priority::Low, d.interval_secs.max(30))]
        } else { vec![] }
    }
}

/// "If the enemy has `when_enemy_has`, suggest `suggest`." Data-driven (data/counters.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterDef {
    pub when_enemy_has: String,
    pub suggest: String,
    pub reason: String,
    /// Item class_name of `suggest`, used to skip if already owned.
    #[serde(default)]
    pub suggest_class: String,
    /// Spoken name of the enemy (hero display name); falls back to `when_enemy_has`.
    #[serde(default)]
    pub label: String,
}

/// Counter-item suggestions. What actually hurt you comes first: enemy heroes named on recent
/// death panels get the item for that damage (data/coaching/review.json threats and counters:
/// Haze's Bullet Dance -> bullet resist), then the statistical counters of data/counters.json.
/// Melee items are never suggested to a long-range hero (rec8 offered Lifestrike to Grey Talon).
pub struct CounterRule { defs: Vec<CounterDef>, memo: std::cell::RefCell<CounterMemo> }
impl CounterRule { pub fn new(defs: Vec<CounterDef>) -> Self { Self { defs, memo: Default::default() } } }
/// Deaths seen: (clock, panel rows attributed to one enemy hero: (hero, ability, damage)), your
/// hero by vote, and the death state.
#[derive(Default)]
struct CounterMemo { deaths: Vec<(u32, Vec<(String, String, u32)>)>, dead: bool, me: Option<String>, votes: HashMap<String, u32>, enemies: Vec<String>, last_clock: u32 }
pub(crate) fn review_catalog() -> &'static review::Catalog {
    static C: std::sync::OnceLock<review::Catalog> = std::sync::OnceLock::new();
    C.get_or_init(|| serde_json::from_str(include_str!("../../../data/coaching/review.json")).expect("shipped review policy"))
}
/// How long a death panel keeps steering counter suggestions.
const COUNTER_DEATH_SECS: u32 = 600;
impl CounterRule {
    fn remember(&self, snap: &GameSnapshot) {
        let mut m = self.memo.borrow_mut();
        let Some(clock) = snap.game_clock_secs else { return };
        if clock + 120 < m.last_clock { *m = CounterMemo::default(); }
        m.last_clock = clock;
        // Your hero by majority over the match: a misread for a few minutes (rec8 read Grey Talon
        // as another hero 34:56-41:41) must not unlock melee items for a ranged hero.
        if let Some(h) = &snap.hero { *m.votes.entry(h.clone()).or_default() += 1; }
        m.me = m.votes.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(h, _)| h.clone());
        if snap.enemy_heroes.len() >= 3 { m.enemies = snap.enemy_heroes.clone(); }
        if !snap.dead { m.dead = false; return; }
        if !m.dead { m.deaths.push((clock, vec![])); m.dead = true; }
        if snap.death_damage.is_empty() { return; }
        // Panel rows naming one enemy hero's ability (shared abilities and items stay unattributed).
        let data = review::game_data();
        let per: Vec<(String, String, u32)> = snap.death_damage.iter()
            .filter_map(|r| data.ability_hero(&r.source, &m.enemies).map(|h| (h, r.source.clone(), r.damage))).collect();
        if let Some(d) = m.deaths.last_mut() { d.1 = per; }
    }
}

impl Rule for CounterRule {
    /// One explained item, deferred during combat and gated by observed shopping budget.
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        self.remember(snap);
        if snap.hp.zip(snap.max_hp).is_some_and(|(hp,max)|max>0&&(hp as f32)/(max as f32)<0.65)
            ||snap.health_delta_ratio.is_some_and(|d|d < -0.05){return vec![];}
        if snap.screen.as_deref()==Some("Gameplay") {
            if snap.game_clock_secs.unwrap_or(0)<180||snap.souls.unwrap_or(0)<800{return vec![];}
            if snap.minimap.as_ref().is_some_and(|m|m.you.is_some_and(|me|m.enemies.iter().any(|p|(me.x-p.x).hypot(me.y-p.y)<0.18))){return vec![];}
            // A purchase tip is for a shop moment only: dead or back in base (rec13 review item 11;
            // a recorded match spoke both counters mid-fight). Unknown position is not a shop moment.
            let map = snap.minimap.as_ref();
            let in_base = map.and_then(|m|m.you).is_some_and(|me|me.y>=0.8) || map.is_some_and(|m|m.zone_area==Some(coach_core::ZoneArea::OwnBase));
            if !in_base {return vec![];}
        }
        // The scoreboard is a shop moment only while dead (Tab mid-fight is not).
        if snap.screen.as_deref()==Some("Scoreboard") && !snap.dead {return vec![];}
        // A suggestion must be affordable from souls actually read (a recorded match suggested
        // items with no souls reading at all), and fit the build you are seen buying: a damage
        // damage item is skipped when you own more items of the other damage category (rec3
        // suggested Long Range, a weapon item, to a spirit Baba). Vitality items fit everyone.
        let owned = |kind: &str| snap.items.iter().filter(|i| counter_explanations::slot(i) == Some(kind)).count();
        let (weapon, spirit) = (owned("weapon"), owned("spirit"));
        let fits = |class: &str| match counter_explanations::slot(class) {
            Some("weapon") => spirit <= weapon,
            Some("spirit") => weapon <= spirit,
            _ => true,
        };
        let affordable = |class: &str| counter_explanations::cost(class).is_none_or(|cost| snap.souls.is_some_and(|souls| souls >= cost));
        let m = self.memo.borrow();
        let roles = &review_catalog().roles;
        let ranged = m.me.as_ref().is_some_and(|h| roles.ranged.contains(h));
        let usable = |class: &str| !(ranged && roles.melee_items.iter().any(|x| x == class));
        let owned_item = |class: &str, name: &str| snap.items.iter().any(|i| i == class || i == name);
        let mut seen: Vec<String> = vec![];
        let mut cues = vec![];
        // 1. Heroes on recent death panels, most damage first: the item for that damage.
        let clock = snap.game_clock_secs.unwrap_or(0);
        let mut hurt: Vec<(String, String, u32)> = vec![];
        for (_, rows) in m.deaths.iter().filter(|d| d.0 + COUNTER_DEATH_SECS >= clock) {
            for (h, a, d) in rows { match hurt.iter_mut().find(|x| &x.0 == h) { Some(x) => { x.2 += d; } None => hurt.push((h.clone(), a.clone(), *d)) } }
        }
        hurt.sort_by(|a, b| b.2.cmp(&a.2));
        let catalog = review_catalog();
        let data = review::game_data();
        for (hero, _, _) in &hurt {
            // The hero's ability with a known counter need, as seen on a panel.
            // (ability, need): the review's threats first, then the named threats of the live plans
            // (data/coaching/plans.json: Afterburn, Grasping Hands, Shoulder Charge...).
            let plan_threats = plans::catalog();
            let need = m.deaths.iter().rev().filter(|d| d.0 + COUNTER_DEATH_SECS >= clock).flat_map(|d| d.1.iter()).filter(|r| &r.0 == hero)
                .find_map(|r| catalog.threats.iter().find(|t| &t.hero == hero && !t.counter.is_empty() && t.source.eq_ignore_ascii_case(&r.1)).map(|t| (t.source.clone(), t.counter.clone()))
                    .or_else(|| plan_threats.threats.iter().find(|t| &t.hero == hero && !t.need.is_empty() && t.ability.eq_ignore_ascii_case(&r.1)).map(|t| (t.ability.clone(), t.need.clone()))));
            let Some((source, need)) = need else { continue };
            let Some(counter) = catalog.counters.iter().find(|c| c.need == need) else { continue };
            let item = counter.items.iter().filter_map(|n| data.item(n))
                .filter(|i| !owned_item(&i.class_name, &i.name) && usable(&i.class_name) && fits(&i.class_name) && snap.souls.is_some_and(|s| s >= i.cost)).next();
            let Some(item) = item else { continue };
            let name = data.hero_name(hero);
            let owner = if name.ends_with('s') { format!("{name}'") } else { format!("{name}'s") };
            // Short and threat-based (rec13 review item 11: <= 15 words at a shop moment); "most"
            // only for the hero with the most panel damage.
            let text = if hurt.first().is_some_and(|h| &h.0 == hero) { format!("{owner} {source} hurt you most: buy {}, {} souls.", item.name, thousands_souls(item.cost)) }
                else { format!("Against {owner} {source}: buy {}, {} souls.", item.name, thousands_souls(item.cost)) };
            seen.push(hero.clone());
            cues.push(Cue::new(format!("counter:{hero}:{}", item.class_name), text, Priority::Low, 1200));
        }
        // 2. Statistical counters. Once death panels name who hurts you, only those heroes
        //    ("Counter-buy against what is actually hurting you", moba-fundamentals): rec8
        //    suggested Metal Skin against Deadman Danny while Yamato and Pocket did the damage.
        let triggers: Vec<&String> = if hurt.is_empty() { snap.enemy_items.iter().chain(snap.enemy_heroes.iter()).collect() } else { hurt.iter().map(|h| &h.0).collect() };
        for trigger in triggers {
            if seen.contains(trigger) { continue; }
            seen.push(trigger.clone());
            let mine: Vec<&CounterDef> = self.defs.iter()
                .filter(|c| &c.when_enemy_has == trigger
                    && !owned_item(&c.suggest_class, &c.suggest)
                    && usable(&c.suggest_class)
                    && affordable(&c.suggest_class) && fits(&c.suggest_class))
                .take(1).collect();
            let Some(first) = mine.first() else { continue };
            let who = if first.label.is_empty() { trigger.as_str() } else { first.label.as_str() };
            // Item, cost and (when it fits in 15 words) what it does: the curated summary's first
            // clause, else the game's tooltip. The win-rate evidence stays in counters.json.
            let what=counter_explanations::summary(&first.suggest_class).map(str::to_string)
                .or_else(||counter_explanations::tooltip(&first.suggest_class))
                .map(|w|w.split([',',':',';']).next().unwrap_or("").trim().trim_end_matches('.').to_string());
            let cost=counter_explanations::cost(&first.suggest_class).map(|c|format!(", {} souls",thousands_souls(c))).unwrap_or_default();
            let base=format!("Against {who}: buy {}{cost}.",first.suggest);
            // The clause, else its part before " and " ("Its barrier absorbs damage"), if it fits.
            let fit=|w:&str|{let t=format!("{base} {w}.");(t.split_whitespace().count()<=15).then_some(t)};
            let text=what.and_then(|w|fit(&w).or_else(||w.split(" and ").next().and_then(|p|fit(p.trim())))).unwrap_or(base);
            cues.push(Cue::new(format!("counter:{trigger}:{}",if first.suggest_class.is_empty(){&first.suggest}else{&first.suggest_class}), text, Priority::Low, 1200));
        }
        cues
    }
}
pub fn counter_rule_from_json(json: &str) -> Result<Box<dyn Rule>, String> {
    Ok(Box::new(CounterRule::new(serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?)))
}

/// Parse data/timings.json into timer rules.
pub fn rules_from_timings(json: &str) -> Result<Vec<Box<dyn Rule>>, String> {
    let defs: Vec<TimerDef> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(defs.into_iter().map(|d| Box::new(TimerRule(d)) as Box<dyn Rule>).collect())
}

/// Tunable thresholds (data/rules.json). Values are starting guesses, not game facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    pub unspent_souls: u32,
    pub lead_pct: f32,
    pub lead_min_clock_secs: u32,
    /// Minimum absolute souls gap for a lead/behind call.
    pub lead_min_abs: u32,
}
impl Default for RulesConfig {
    fn default() -> Self { RulesConfig { unspent_souls: 1500, lead_pct: 15.0, lead_min_clock_secs: 300, lead_min_abs: 3000 } }
}

/// Spend reminders, scaled to the game clock ([`economy::spend_target`]) and said per "buy
/// episode": once when you can afford the tier normally bought now, at most one reminder 3
/// minutes later, then nothing until you buy (souls drop) or die. A recorded match said "You are
/// holding N souls" 11 times, including 1,633 at 32:18 and while saving for a 6,400 item.
pub struct UnspentSoulsRule(pub RulesConfig, std::cell::Cell<SoulsEpisode>);
impl UnspentSoulsRule { pub fn new(cfg: RulesConfig) -> Self { Self(cfg, Default::default()) } }
#[derive(Default, Clone, Copy)]
pub struct SoulsEpisode { peak: u32, said: u8, said_at: u32, dead_said: bool, base_said: bool, dead_since: Option<u32>, clock: u32 }
fn thousands_souls(v: u32) -> String { if v >= 1000 { format!("{},{:03}", v / 1000, v % 1000) } else { v.to_string() } }
impl Rule for UnspentSoulsRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        if snap.minimap.as_ref().is_some_and(|m| m.urn == Some(coach_core::UrnState::CarriedByYou)) { return vec![]; }
        let (Some(s), Some(clock)) = (snap.souls, snap.game_clock_secs) else { return vec![] };
        let mut cur = self.1.get();
        cur.clock = clock;
        let out = (|| { let ep = &mut cur;
            // A purchase (souls fall well below the episode's peak) or a new match starts over.
            if s + 400 < ep.peak || clock + 120 < ep.said_at { let dead_since = ep.dead_since; *ep = SoulsEpisode::default(); ep.dead_since = dead_since; }
            ep.peak = ep.peak.max(s);
            let target = economy::spend_target(clock).max(self.0.unspent_souls);
            if snap.dead {
                // The death screen is a shop window: named once per death when the souls buy at
                // least the tier below what you would normally buy now.
                let since = *ep.dead_since.get_or_insert(clock);
                ep.base_said = false; // the next base visit is a new shop window
                let need = economy::death_spend(clock);
                if s < need || ep.dead_said { return vec![]; }
                // A revive (rec8 39:17: health back 5 s later, no timer ever read) is not a
                // shop window: without a respawn timer, wait 5 s into the death screen.
                if snap.respawn_secs.is_none() && clock < since + 5 { return vec![]; }
                let wait = snap.respawn_secs.map(|r| format!(" You are back in {r} seconds.")).unwrap_or_default();
                return vec![Cue::new("unspent-dead", format!("You are dead with {} souls. Buy now.{wait}", thousands_souls(s)), Priority::High, 90)];
            }
            if snap.screen.as_deref() == Some("Gameplay") && snap.hp.is_some() { ep.dead_since = None; ep.dead_said = false; }
            // In your own base the call is specific ("spend them before you walk out"), from the
            // tier below the usual buy (the base is the shop: rec8 walked out at 21:39 with 4,522
            // souls, enough for a 3,200 item), once per buy episode.
            if let Some(base) = active::base_spend(snap) {
                let Some(c) = base else { return vec![] };
                if ep.base_said { return vec![]; }
                return vec![c];
            }
            if s < target { return vec![]; } // saving for the next item
            if ep.said >= 2 || ep.said == 1 && clock < ep.said_at + 180 { return vec![]; }
            // Under half health the next call is about surviving, not shopping (rec5 38:04 named
            // 6,585 souls at 44% health with unsecured souls on the line).
            if snap.hp.zip(snap.max_hp).is_some_and(|(h, m)| m >= 100 && h * 2 < m) { return vec![]; }
            if let Some(lane)=laning::focus(snap) {
                if clock<900 && snap.minimap.as_ref().is_some_and(|m| m.lane(lane).push.is_none_or(|p|p<0.7)) {return vec![];}
            }
            let text = if ep.said == 0 {
                format!("You have {} souls: that is a {}-soul item. Shop when the wave is safe.", thousands_souls(s), thousands_souls(target))
            } else {
                format!("Still holding {} souls. Buy at the next safe moment; unspent souls do nothing.", thousands_souls(s))
            };
            vec![Cue::new("unspent", text, Priority::Normal, 150)]
        })();
        self.1.set(cur);
        out
    }
    /// Committed when spoken: a call the engine held back (cadence, a death review first) is offered again.
    fn spoken(&self, cue: &Cue) {
        let mut ep = self.1.get();
        match cue.key.as_str() {
            // The death review names the buy for this death.
            k if k == "unspent-dead" || k.starts_with("death:review") => ep.dead_said = true,
            "unspent:base" => { ep.base_said = true; ep.said = ep.said.max(1); ep.said_at = ep.clock; }
            "unspent" => { ep.said += 1; ep.said_at = ep.clock; }
            _ => {}
        }
        self.1.set(ep);
    }
}

/// Explains what killed the player, from the "Incoming damage (last death)" panel.
pub struct DeathCauseRule;
impl Rule for DeathCauseRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let (true, Some(cause)) = (snap.dead, snap.death_cause.as_deref()) else { return vec![] };
        let (tag, text) = death_cause_advice(cause);
        vec![Cue::new(format!("death:{tag}"), text, Priority::High, 60)]
    }
}
/// What a death-panel cause means for the player: (tag, advice). "heroes" is the generic case.
pub fn death_cause_advice(cause: &str) -> (&'static str, &'static str) {
        let c = cause.to_lowercase();
        if c.contains("temple") || c.contains("midboss") || c.contains("mid boss") {
            ("midboss", "The Midboss killed you. Next life: take it only with all of us, after its shield drops.")
        } else if c.contains("walker") {
            ("walker", "Their Walker killed you. Next life: let our troopers tank its shots first.")
        } else if c.contains("guardian") || c.contains("shrine") || c.contains("patron") {
            ("structure", "Their structure killed you. Next life: stay out of its range until our troopers are in.")
        } else if c.contains("trooper") {
            ("trooper", "Troopers killed you. Next life: fight from behind our wave, not inside theirs.")
        } else {
            ("heroes", "Enemy heroes killed you. Next life: check the scoreboard before you go back in.")
        }
}
/// A non-hero killer as a death-review cause: ("because" fragment, next-life change).
pub fn death_cause_parts(cause: &str) -> Option<(&'static str, &'static str)> {
    match death_cause_advice(cause).0 {
        "midboss" => Some(("the Midboss hit you", "take it only with all of us, after its shield drops")),
        "walker" => Some(("their Walker shot you", "let our troopers tank its shots first")),
        "structure" => Some(("you stood in their structure's range", "stay out of its range until our troopers are in")),
        "trooper" => Some(("you fought inside their wave", "fight from behind our wave, not inside theirs")),
        _ => None,
    }
}
/// Speaks only after the same state (ahead/behind) is seen on 4 snapshots in a row and the gap
/// is at least `lead_min_abs` souls, so noisy reads near parity do not flip-flop. The same state
/// is repeated at most every 8 minutes (the cue cooldown; rec12 said "your team is ahead" five
/// times in 28 minutes); a change of state (ahead -> behind) is a new key and is said at once.
pub struct NetworthLeadRule { cfg: RulesConfig, streak: std::cell::Cell<(i8, u8)>, last: std::cell::Cell<Option<(u32, u32)>> }
impl NetworthLeadRule { pub fn new(cfg: RulesConfig) -> Self { NetworthLeadRule { cfg, streak: std::cell::Cell::new((0, 0)), last: std::cell::Cell::new(None) } } }
impl Rule for NetworthLeadRule {
    fn evaluate(&self, snap: &GameSnapshot) -> Vec<Cue> {
        let (Some(a), Some(e), Some(t)) = (snap.ally_networth, snap.enemy_networth, snap.game_clock_secs) else { return vec![] };
        if t < self.cfg.lead_min_clock_secs || a == 0 || e == 0 { return vec![]; }
        let diff = (a as f32 - e as f32) / e as f32 * 100.0;
        // One team at under half the other's souls is a digit drop, not a match state (a
        // recorded match read ~100k as 1000 and called "behind"): say nothing.
        if a.min(e) * 2 < a.max(e) { self.streak.set((0, 0)); return vec![]; }
        // Either total jumping more than 8% since the previous snapshot is a misread or a
        // correction in progress: start over (rec5 called "ahead" right after 121k -> 173k).
        let jumped = self.last.get().is_some_and(|(pa, pe)| a.abs_diff(pa) * 100 > pa * 8 || e.abs_diff(pe) * 100 > pe * 8);
        self.last.set(Some((a, e)));
        if jumped { self.streak.set((0, 0)); return vec![]; }
        let state: i8 = if diff >= self.cfg.lead_pct && a.saturating_sub(e) >= self.cfg.lead_min_abs { 1 }
            else if diff <= -self.cfg.lead_pct && e.saturating_sub(a) >= self.cfg.lead_min_abs { -1 } else { 0 };
        let (prev, n) = self.streak.get();
        let n = if state == prev { n.saturating_add(1) } else { 1 };
        self.streak.set((state, n));
        if state == 0 || n < 4 { return vec![]; }
        let (key, text) = if state > 0 {
            ("lead", "We are ahead in souls. Take objectives, skip risky fights.")
        } else {
            ("behind", "We are behind in souls. Farm safe and fight near our structures.")
        };
        vec![Cue::new(key, text, Priority::Normal, LEAD_REPEAT_SECS)]
    }
}
/// Minimum seconds between two "your team is ahead / behind" calls of the same state.
pub const LEAD_REPEAT_SECS: u32 = 480;
pub fn default_rules(cfg: RulesConfig) -> Vec<Box<dyn Rule>> {
    // The active coach goes first: among equal priorities its specific call (the debuff on you,
    // the lane to push in an enemy-dead window, souls to spend in base) wins the step over the
    // generic one, and the shared keys keep the generic one quiet for its cooldown. Praise comes
    // before the farm-pace compliment, which it outranks for the shared praise budget.
    vec![Box::new(active::ActiveCoach::default()), Box::new(positioning::PositionCoach::default()), Box::new(jungle::JungleCoach::default()), Box::new(laning::LaneCoach::default()), Box::new(map_rules::MapRule), Box::new(UnspentSoulsRule::new(cfg.clone())), Box::new(review::ReviewCoach::default()), Box::new(NetworthLeadRule::new(cfg)), Box::new(praise::PraiseCoach::default()), Box::new(economy::PaceRule::default()),Box::new(economy::WaveReportRule::default()), Box::new(plans::PlanCoach::default())]
}

/// One death is one event: from the first death screen to a confirmed respawn. rec13 41:05-42:34
/// showed Unknown, Shop, Scoreboard and single Gameplay frames (no health read) inside one death
/// and produced two extra death reviews with a false "died 0 / 5 s after respawning". A respawn is
/// confirmed by the respawn timer running out ([`review::respawn_estimate`] over the timer reads),
/// or by [`RESPAWN_HP_FRAMES`] gameplay frames in a row with your health read while no timer says
/// you are still dead; a gameplay frame with health read within 10 s of the death screen and no
/// longer timer is the grey "Respawn in: 1" revive (rec11 25:17), not a death. Without any health
/// read, [`RESPAWN_BLIND_SECS`] of gameplay frames without a death screen end it. Frames inside the
/// death that do not confirm the respawn reach the rules as death frames (dead, no personal HUD).
pub const RESPAWN_HP_FRAMES: u8 = 2;
pub const RESPAWN_BLIND_SECS: u32 = 20;
#[derive(Default, Clone, Debug)]
pub struct LifeTracker {
    /// First death-screen clock of the current death (None while alive).
    pub dead_since: Option<u32>,
    /// Clock of the last confirmed respawn, and the start of the last death.
    pub respawned_at: Option<u32>, pub last_death: Option<u32>,
    ends: Vec<u32>, last_dead: u32, hp_frames: u8, first_hp: Option<u32>, blind_since: Option<u32>, last_clock: Option<u32>,
}
impl LifeTracker {
    /// The snapshot as the rules should see it: None = this frame unchanged.
    pub fn observe(&mut self, s: &GameSnapshot) -> Option<GameSnapshot> {
        let Some(clock) = s.game_clock_secs else { return None };
        if self.last_clock.is_some_and(|c| clock + 120 < c) { *self = LifeTracker::default(); }
        self.last_clock = Some(clock);
        let death_frame = s.dead || s.screen.as_deref() == Some("Dead");
        if death_frame {
            // More than a minute without any frame on a death screen is a new death (a recording gap).
            if self.dead_since.is_none() || clock > self.last_dead + 60 { self.dead_since = Some(clock); self.last_death = Some(clock); self.ends.clear(); }
            if let Some(r) = s.respawn_secs.filter(|r| *r <= 120) { self.ends.push(clock + r); }
            self.last_dead = clock; self.hp_frames = 0; self.first_hp = None; self.blind_since = None;
            return None;
        }
        let Some(start) = self.dead_since else { return None };
        let est = review::respawn_estimate(&self.ends, self.last_dead);
        let mut respawn = None;
        if s.screen.as_deref() == Some("Gameplay") {
            let hp = s.hp.zip(s.max_hp).is_some_and(|(h, m)| m >= 100 && h <= m && h > 0);
            if hp { self.hp_frames = self.hp_frames.saturating_add(1); self.first_hp.get_or_insert(clock); } else { self.hp_frames = 0; self.first_hp = None; }
            let blind = *self.blind_since.get_or_insert(clock);
            let timer_far = est.is_some_and(|e| e > clock + 10);
            if est.is_some_and(|e| clock >= e) { respawn = Some(est.unwrap_or(clock).max(self.last_dead).min(clock)); }
            else if hp && !timer_far && (self.hp_frames >= RESPAWN_HP_FRAMES || clock < start + 10 && est.is_none_or(|e| e < start + 10)) { respawn = self.first_hp; }
            else if self.hp_frames >= RESPAWN_HP_FRAMES + 1 { respawn = self.first_hp; }
            else if est.is_none() && clock >= blind + RESPAWN_BLIND_SECS { respawn = Some(blind); }
        }
        if let Some(r) = respawn {
            self.dead_since = None; self.respawned_at = Some(r.max(start)); self.ends.clear(); self.hp_frames = 0; self.first_hp = None; self.blind_since = None;
            return None;
        }
        // Still dead: the frame is part of the death, without the personal HUD.
        let mut d = s.clone();
        d.dead = true;
        if d.screen.as_deref() == Some("Gameplay") { d.screen = Some("Dead".into()); }
        d.hp = None; d.max_hp = None; d.health_delta_ratio = None; d.health_observations = 0; d.status_effects.clear(); d.unsecured_souls = None;
        Some(d)
    }
    /// Seconds since the current or last death ended (None: dead now or no death yet).
    pub fn since_respawn(&self, now: u32) -> Option<u32> { if self.dead_since.is_some() { None } else { self.respawned_at.map(|r| now.saturating_sub(r)) } }
}

/// Fight detection for the arbitration (Engine::step): a fight is >= [`FIGHT_HEROES`] heroes (at
/// least one enemy) within [`FIGHT_RANGE`] of you on the minimap, your health falling fast, or a
/// teammate dying in the last [`FIGHT_ALLY_DEATH_SECS`]; it holds [`FIGHT_HOLD_SECS`] after the
/// last signal (a frame without reads in the middle of a fight is still the fight).
pub const FIGHT_HEROES: usize = 4;
pub const FIGHT_RANGE: f32 = 0.15;
pub const FIGHT_FALL: f32 = 0.10;
pub const FIGHT_ALLY_DEATH_SECS: u32 = 8;
pub const FIGHT_HOLD_SECS: u32 = 6;
#[derive(Default, Clone)]
struct FightState { last_signal: Option<u32>, allies_dead: Option<usize>, ally_death_at: Option<u32> }
impl FightState {
    fn observe(&mut self, s: &GameSnapshot, now: u32) -> bool {
        if s.dead || s.screen.as_deref() != Some("Gameplay") { self.last_signal = None; return false; }
        let map = s.minimap.as_ref();
        let me = map.and_then(|m| m.you);
        let near: Vec<&coach_core::HeroTrack> = me.zip(map).map(|(me, m)| m.heroes.iter()
            .filter(|h| h.confirmed() && h.unseen_ms <= 3000 && h.distance(me) <= FIGHT_RANGE).collect()).unwrap_or_default();
        let crowd = near.len() >= FIGHT_HEROES && near.iter().any(|h| !h.ally);
        let falling = s.health_delta_ratio.is_some_and(|d| d < -FIGHT_FALL);
        if let Some(p) = positioning::TeamPicture::from(s) {
            let n = p.allies_dead.len();
            if self.allies_dead.is_some_and(|prev| n > prev) { self.ally_death_at = Some(now); }
            self.allies_dead = Some(n);
        }
        let ally_died = self.ally_death_at.is_some_and(|t| now >= t && now - t <= FIGHT_ALLY_DEATH_SECS);
        if crowd || falling || ally_died { self.last_signal = Some(now); }
        self.last_signal.is_some_and(|t| now >= t && now - t <= FIGHT_HOLD_SECS)
    }
}

/// Normal / Low calls per phase (docs/coach-review-13.md, item 14): one per [`LANING_GAP_SECS`]
/// before [`LANING_END_SECS`], then one per [`MID_LATE_GAP_SECS`]. High safety calls bypass the
/// budget (their own cooldowns still dedupe them).
pub const LANING_END_SECS: u32 = 480;
pub const LANING_GAP_SECS: u32 = 60;
pub const MID_LATE_GAP_SECS: u32 = 45;
/// In a fight: one call per [`FIGHT_GAP_SECS`], High safety / threat / exit calls only, at most
/// [`FIGHT_WORDS`] words.
pub const FIGHT_GAP_SECS: u32 = 15;
pub const FIGHT_WORDS: usize = 10;
pub const FIGHT_PER_30: usize = 2;
/// At most one wave call (deep wave or overview) per [`WAVE_GAP_SECS`].
pub const WAVE_GAP_SECS: u32 = 90;
/// A timer readout waits while an objective plan (key prefix "plan:") was spoken this recently.
pub const PLAN_COVERS_SECS: u32 = 60;
/// Praise waits this long after a death (and after the opposite mistake).
pub const PRAISE_AFTER_DEATH_SECS: u32 = 60;
/// The dead-with-souls reminder waits this long into a death: the death review names the buy.
pub const DEAD_SOULS_WAIT_SECS: u32 = 25;

/// Calls that announce state rather than a plan: never in a fight or a siege.
pub fn is_chatter(key: &str) -> bool {
    const PREFIX: [&str; 12] = ["map:wave", "wave:", "timer:", "map:urn-ground", "map:urn-ally", "map:midboss", "map:farm-window", "pace:", "counter:", "praise:", "review:pattern", "jungle:spawn"];
    PREFIX.iter().any(|p| key.starts_with(p)) || matches!(key, "lead" | "behind")
}
/// "Back off" calls that say the same thing in different words: one per [`RETREAT_GAP_SECS`]
/// (rec11 30:35-30:52 said gank, outnumbered and hold your side in 17 s). The critical-health call
/// and the debuff / crowd-control escapes are not in it.
/// The forward-position calls (out of their base, ult down, escape spent) are in the family.
pub fn is_retreat_call(key: &str) -> bool { matches!(key, "pos:gank" | "pos:outnumbered" | "lane:hold" | "lane:back_off" | "review:fight-lost" | "active:ult_out" | "active:escape_in") || key.starts_with("active:forward_exit") }
pub const RETREAT_GAP_SECS: u32 = 20;
pub fn is_wave_call(key: &str) -> bool { key.starts_with("map:wave:") || key == "wave:overview" }
/// Calls that send you to an objective, a push or a fight (docs/coach-review-8.md D9).
pub fn is_go_call(key: &str) -> bool {
    const PREFIX: [&str; 8] = ["plan:urn", "plan:midboss", "plan:rejuv", "plan:guardian", "plan:walker", "map:urn-ground", "map:urn-ally", "map:midboss"];
    PREFIX.iter().any(|p| key.starts_with(p))
        || matches!(key, "pos:enemies_down" | "pos:go_in" | "pos:join" | "pos:kill_window" | "pos:help" | "lane:push" | "lane:objective" | "lane:rotate" | "lane:gank" | "active:fight" | "map:farm-window")
}
/// Calls that hold you back: while one applies (or [`HOLD_COVERS_SECS`] after it), no call sends
/// you to an objective or a push (rec8 40:42 "check who is missing before you grab the Urn" 3 s
/// before "Hold in base"; 27:09 "push a lane" while carrying the Urn; rec13 26:00 a Midboss plan
/// while carrying the Urn).
pub fn is_hold_call(key: &str) -> bool {
    matches!(key, "active:hold_base" | "active:hold_base:out" | "active:urn_hold" | "review:fight-lost" | "active:team_losing" | "active:ult_out" | "active:escape_in" | "pos:outnumbered")
        || key.starts_with("active:forward_exit")
}
pub const HOLD_COVERS_SECS: u32 = 20;
/// The same sentence is not spoken twice within this many seconds (docs/coach-review-8.md D10).
pub const SAME_TEXT_SECS: u32 = 180;
/// Lane-income advice that has no place in a fight.
fn is_farm_call(key: &str) -> bool {
    matches!(key, "lane:last_hit" | "lane:wave_good" | "lane:poke" | "lane:push" | "lane:shop" | "lane:rotate" | "lane:objective" | "map:farm-window" | "unspent" | "active:close") || key.starts_with("active:repeat")
}
/// Which Normal / Low call takes a budget slot: the time-sensitive, contextual ones first (a window
/// being used, a praise within a minute of the play, an objective plan), state readouts last (a
/// missing enemy, a wave, a timer, a summary). rec12 lost every window praise to "you're ahead of
/// your trooper wave" taking the slot first.
pub fn call_value(key: &str) -> u8 {
    const NOW: [&str; 9] = ["praise:", "plan:", "pos:enemies_down", "pos:kill_window", "pos:go_in", "pos:help", "lane:gank", "review:respawn-alone", "active:"];
    const STATE: [&str; 13] = ["pos:missing", "pos:unseen", "pos:join", "lane:overextended", "lane:last_hit", "lane:wave_good", "unspent", "review:pattern", "pace:", "counter:", "jungle:", "map:", "timer:"];
    if NOW.iter().any(|p| key.starts_with(p)) { 3 } else if STATE.iter().any(|p| key.starts_with(p)) || is_wave_call(key) || matches!(key, "lead" | "behind") { 1 } else { 2 }
}
/// A call cut to its first sentence (or clause) for a fight, at most [`FIGHT_WORDS`] words.
pub fn fight_text(text: &str) -> String {
    let first = text.split_inclusive(['.', '!', '?']).next().unwrap_or(text).trim().to_string();
    let first = if first.split_whitespace().count() > FIGHT_WORDS { first.split(':').next().unwrap_or(&first).trim().trim_end_matches(['.', ',']).to_string() + "." } else { first };
    let words: Vec<&str> = first.split_whitespace().collect();
    if words.len() <= FIGHT_WORDS { first } else { words[..FIGHT_WORDS].join(" ").trim_end_matches([',', ':']).to_string() + "." }
}
/// A siege: a structure / Patron banner up, or you in their half with two or more teammates.
fn sieging(s: &GameSnapshot) -> bool {
    s.objective_banner.as_ref().is_some_and(|b| !b.name.trim().is_empty()) || allies_near(s, 0.2) >= 2 && s.minimap.as_ref().and_then(|m| m.you).is_some_and(|p| p.y < 0.5)
}
fn allies_near(s: &GameSnapshot, r: f32) -> usize {
    let Some(m) = s.minimap.as_ref() else { return 0 };
    let Some(me) = m.you else { return 0 };
    m.heroes.iter().filter(|h| h.ally && h.confirmed() && h.unseen_ms <= 3000 && h.distance(me) <= r).count()
}
/// Wave calls are about lane income alone: not with three teammates around you, not in their
/// half, not while a structure banner is up.
fn wave_call_allowed(s: &GameSnapshot) -> bool {
    !(allies_near(s, 0.2) >= 3 || s.minimap.as_ref().and_then(|m| m.you).is_some_and(|p| p.y < 0.5) || s.objective_banner.as_ref().is_some_and(|b| !b.name.trim().is_empty()))
}

pub struct Engine {
    fight: FightState, in_fight: bool, life: LifeTracker,
    rules: Vec<Box<dyn Rule>>,
    last_said: HashMap<String, u32>,
    /// Max cues returned per step (rest are held back and retried next step).
    pub max_per_step: usize,
    /// Minimum game-seconds between any two spoken cues.
    pub min_gap_secs: u32,
    /// The phase cadence budget and fight mode (on by default; unit tests of single rules turn it off).
    pub budget: bool,
    last_cue_at: Option<u32>,
    /// Clocks of the calls spoken in fight mode (last 30 s), and of the last wave call.
    fight_calls: Vec<u32>, wave_at: Option<u32>,
    /// Text of the last spoken wave overview (which lanes it named).
    wave_text: Option<String>,
    /// Clocks of the praise calls spoken, and of the last compliment of any kind (praise or the
    /// farm-pace compliment): the praise budget ([`praise::budget_allows`]).
    praise_at: Vec<u32>, compliment_at: Option<u32>,
    /// Spoken sentences and when (the same-text guard).
    said_text: HashMap<String, u32>,
}

impl Engine {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self { Engine { fight: FightState::default(), in_fight: false, life: LifeTracker::default(), rules, last_said: HashMap::new(), max_per_step: 1, min_gap_secs: 8, budget: true, last_cue_at: None, fight_calls: vec![], wave_at: None, wave_text: None, praise_at: vec![], compliment_at: None, said_text: HashMap::new() } }
    /// Whether the last step was in fight mode.
    pub fn in_fight(&self) -> bool { self.in_fight }
    /// Deaths as one event each (first death screen to the confirmed respawn).
    pub fn life(&self) -> &LifeTracker { &self.life }
    /// Seconds between Normal / Low calls at this game clock.
    pub fn phase_gap(now: u32) -> u32 { if now < LANING_END_SECS { LANING_GAP_SECS } else { MID_LATE_GAP_SECS } }

    /// Evaluate all rules; returns cues not suppressed by cooldown, highest priority first.
    pub fn step(&mut self, raw: &GameSnapshot) -> Vec<Cue> {
        // Deaths first: menus and flicker frames inside a death reach the rules as death frames.
        let masked = self.life.observe(raw);
        let snap = masked.as_ref().unwrap_or(raw);
        if snap.screen.as_deref().is_some_and(|s|!matches!(s,"Gameplay"|"Scoreboard"|"Dead")){return vec![];}
        let Some(now) = snap.game_clock_secs else { return vec![] };
        if let Some(last)=self.last_cue_at {if now<last {
            if last-now>120 {self.last_said.clear();self.said_text.clear();self.last_cue_at=None;self.praise_at.clear();self.compliment_at=None;self.fight_calls.clear();self.wave_at=None;}
            else {return vec![];} // small OCR backsteps are not a new match
        }}
        self.in_fight = self.fight.observe(snap, now);
        let mut out: Vec<Cue> = self.rules.iter().flat_map(|r| r.evaluate(snap)).collect();
        // In danger only safety calls speak: rec3 said "secure last hits" at 38% health falling
        // 15 points a read, five seconds before dying.
        let health = snap.hp.zip(snap.max_hp).filter(|(h, m)| *m >= 100 && h <= m).map(|(h, m)| h as f32 / m as f32);
        let falling = snap.health_delta_ratio.is_some_and(|d| d < -0.03);
        if health.is_some_and(|h| h < 0.30 || h < 0.45 && falling) { out.retain(|c| c.priority == Priority::High); }
        out.retain(|c| match self.last_said.get(&c.key) {
            Some(&t) if now >= t && now - t < c.cooldown_secs => false,
            _ => true,
        });
        // The same sentence twice within 3 minutes is a repeat whatever its key (rec8 "Go defend our
        // Patron: it's under attack." 48:20 and 49:34); a death review is always new.
        // Debuff and crowd-control escapes are said on every new badge (their cooldown dedupes them).
        // So is the hold-in-base call of a new wipe (its own episode logic decides).
        out.retain(|c| c.key.starts_with("death:review") || c.key.starts_with("active:hold_base") || active::is_escape_call(&c.key) || self.said_text.get(&c.text).is_none_or(|t| now < *t || now - t >= SAME_TEXT_SECS));
        let recent = |k: &str, secs: u32| self.last_said.get(k).is_some_and(|t| now >= *t && now - t < secs);
        // Contradiction guard: no objective pickup or push while a hold / back-out call applies,
        // while three or more teammates are dead, or (other than the Urn itself) while you carry it.
        let held_back = self.last_said.iter().any(|(k, t)| is_hold_call(k) && now >= *t && now - t < HOLD_COVERS_SECS);
        let team_down = positioning::TeamPicture::from(snap).is_some_and(|p| p.allies_dead.len() >= 3);
        let carrying = snap.minimap.as_ref().is_some_and(|m| m.urn == Some(coach_core::UrnState::CarriedByYou)) || snap.status_effects.iter().any(|e| e == "carrying_urn");
        out.retain(|c| !is_go_call(&c.key) || !(held_back || team_down || carrying && !c.key.starts_with("plan:urn") && !c.key.starts_with("map:urn")));
        // Nor a wave chore ("catch the farm before you roam") that walks you out alone.
        if held_back || team_down { out.retain(|c| !is_wave_call(&c.key) && !is_farm_call(&c.key)); }
        // A summary waits while three of us are dead (rec8 40:42: "come help" 3 s before "Hold in base").
        if team_down { out.retain(|c| !c.key.starts_with("review:pattern")); }
        // One death, one message: the death review folds in the repeat killer and the close-range
        // pattern (rec8 28:50 / 47:38 spoke them as second death-screen messages).
        // (Only once the review was spoken for this death: without a review rule the lessons stay.)
        let reviewed = self.life.dead_since.is_some_and(|d| self.last_said.iter().any(|(k, t)| k.starts_with("death:review") && *t >= d));
        if reviewed { out.retain(|c| !(c.key.starts_with("active:") || c.key.starts_with("review:pattern:death"))); }
        // A throw call is for the living (rec8 42:30 named the lead to a dead player); it waits for the respawn with fresh numbers.
        if self.life.dead_since.is_some() || snap.dead { out.retain(|c| !c.key.starts_with("lead:falling")); }
        // Praise is rationed: at most 3 per 10 minutes, a minute apart (praise.rs); never during or
        // within a minute after a death, nor right after the opposite mistake (rec13: "keep it up,
        // you went 9 minutes without dying" 5 s after a death).
        out.retain(|c| praise::budget_allows(&c.key, &self.praise_at, self.compliment_at, now));
        let after_death = self.life.dead_since.is_some() || self.life.since_respawn(now).is_some_and(|s| s < PRAISE_AFTER_DEATH_SECS);
        let lost = recent("review:fight-lost", PRAISE_AFTER_DEATH_SECS) || recent("active:team_losing", PRAISE_AFTER_DEATH_SECS);
        out.retain(|c| !praise::is_compliment(&c.key) || !(after_death || lost
            || c.key.starts_with("praise:group:") && (recent("review:respawn-alone", PRAISE_AFTER_DEATH_SECS) || recent("active:hold_base:out", PRAISE_AFTER_DEATH_SECS))
            || c.key.starts_with("praise:lead:") && recent("behind", PRAISE_AFTER_DEATH_SECS)));
        // "Your lead grew from X to Y" says more than the generic "your team is ahead".
        if out.iter().any(|c| c.key.starts_with("praise:lead:")) { out.retain(|c| c.key != "lead"); }
        // A counter item is suggested once per match (rec8 suggested Long Range twice, 31 min apart).
        out.retain(|c|!c.key.starts_with("counter:")||(
            self.last_said.get("$counter-category").is_none_or(|t|now.saturating_sub(*t)>=180)
            &&!self.last_said.contains_key(&format!("$counter-item:{}",c.key.rsplit(':').next().unwrap_or("")))));
        // One wave call per 90 s across both wave families, and a deep-wave call is not said for a
        // lane the overview just named as coming toward you (rec8 spoke both families 25 times).
        // Wave calls are about lane income alone: never with three teammates around you, in their
        // half or during a structure push (rec13 "catch that wave" in late-game sieges).
        let overview = self.wave_text.clone().filter(|_| recent("wave:overview", 60)).unwrap_or_default();
        let wave_ok = wave_call_allowed(snap) && self.wave_at.is_none_or(|t| now < t || now - t >= WAVE_GAP_SECS);
        out.retain(|c| if !is_wave_call(&c.key) { true } else if !wave_ok { false }
            else if let Some(lane) = c.key.strip_prefix("map:wave:") { !overview.contains(&format!("{lane} is coming")) } else { true });
        // An objective plan covers the timer readout (plans.rs, "plan:*").
        let planned = self.last_said.iter().any(|(k, t)| k.starts_with("plan:") && now >= *t && now - t < PLAN_COVERS_SECS);
        if planned { out.retain(|c| !c.key.starts_with("timer:")); }
        if recent("$retreat", RETREAT_GAP_SECS) { out.retain(|c| !is_retreat_call(&c.key)); }
        // "Hold in base" already covers "play safe until they're back".
        let held = recent("active:hold_base", 60) || recent("active:hold_base:out", 60);
        out.retain(|c| !(held && c.key.starts_with("pos:allies_down")));
        // The death review names what to buy; the generic dead-with-souls reminder is the fallback.
        if self.budget { if let Some(d) = self.life.dead_since { if now < d + DEAD_SOULS_WAIT_SECS { out.retain(|c| c.key != "unspent-dead"); } } }
        if self.budget {
            let siege = sieging(snap);
            if self.in_fight {
                // One call per fight beat: a High safety / threat / exit call, short.
                // The critical-health exit may follow another call after 3 s; never more than
                // [`FIGHT_PER_30`] calls in 30 s.
                self.fight_calls.retain(|t| now >= *t && now - t < 30);
                let last = self.fight_calls.last().copied();
                let gap_ok = |c: &Cue| last.is_none_or(|t| now - t >= FIGHT_GAP_SECS || c.key == "lane:critical" && now - t >= 3 && !self.last_said.get("lane:critical").is_some_and(|x| *x == t));
                let room = self.fight_calls.len() < FIGHT_PER_30;
                out.retain(|c| room && gap_ok(c) && c.priority == Priority::High && !is_chatter(&c.key) && !is_farm_call(&c.key));
                for c in out.iter_mut() { c.text = fight_text(&c.text); }
            } else if siege {
                out.retain(|c| !is_chatter(&c.key));
            }
            // The phase budget for everything that is not a High safety call; a state readout
            // ([`call_value`] 1) needs twice the gap, leaving the slots to time-sensitive calls.
            let gap = Self::phase_gap(now);
            out.retain(|c| c.priority == Priority::High || self.last_cue_at.is_none_or(|l| now - l >= if call_value(&c.key) <= 1 { 2 * gap } else { gap }));
        }
        if let Some(l)=self.last_cue_at { if now-l<self.min_gap_secs {
            out.retain(|c| c.priority==Priority::High && (now-l>=3 || c.key=="lane:back_off" || c.key=="lane:critical"));
        }}
        // Among equal priorities the critical-health call goes first: at 24% falling, "get out"
        // beats a gank warning (rec3 spoke the gank and died before the critical call).
        // The death review goes before the generic dead-with-souls reminder (it names the souls too).
        let first = |c: &Cue| c.key == "lane:critical" || c.key.starts_with("death:review");
        out.sort_by(|a, b| b.priority.cmp(&a.priority).then(first(b).cmp(&first(a))).then(call_value(&b.key).cmp(&call_value(&a.key))));
        out.truncate(self.max_per_step);
        for c in &out { self.last_said.insert(c.key.clone(), now); self.said_text.insert(c.text.clone(), now); }
        self.said_text.retain(|_, t| now < *t || now - *t < SAME_TEXT_SECS);
        for c in &out { for r in &self.rules { r.spoken(c); } }
        // A debuff / crowd-control call already says "get out" with the reason: the generic
        // health back-off would repeat it a second later (rec5 17:53 aura, 17:54 back off).
        if out.iter().any(|c| active::is_escape_call(&c.key)) { self.last_said.insert("lane:back_off".into(), now); }
        // The specific call covers the generic one: holding the urn replaces "head for delivery".
        if out.iter().any(|c| c.key == "active:urn_hold") { self.last_said.insert("map:urn-you".into(), now); }
        // A growing-lead praise already says the team is ahead: the generic call waits its repeat time.
        if out.iter().any(|c| c.key.starts_with("praise:lead:")) { self.last_said.insert("lead".into(), now); }
        // The death review said what to buy: no second souls call on this death screen.
        if out.iter().any(|c| c.key.starts_with("death:review")) { self.last_said.insert("unspent-dead".into(), now); }
        for c in out.iter().filter(|c| praise::is_compliment(&c.key)) {
            self.compliment_at = Some(now);
            if praise::is_praise(&c.key) { self.praise_at.push(now); }
        }
        if let Some(c) = out.iter().find(|c| c.key == "wave:overview") { self.wave_text = Some(c.text.clone()); }
        if out.iter().any(|c| is_wave_call(&c.key)) { self.wave_at = Some(now); }
        if out.iter().any(|c| is_retreat_call(&c.key)) { self.last_said.insert("$retreat".into(), now); }
        for c in out.iter().filter(|c|c.key.starts_with("counter:")){
            self.last_said.insert("$counter-category".into(),now);
            self.last_said.insert(format!("$counter-item:{}",c.key.rsplit(':').next().unwrap_or("")),now);
        }
        if !out.is_empty() { self.last_cue_at = Some(now); if self.in_fight { self.fight_calls.push(now); } }
        out.sort_by(|a, b| b.priority.cmp(&a.priority));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: u32) -> GameSnapshot { GameSnapshot { game_clock_secs: Some(t), ..Default::default() } }
    fn eng() -> Engine {
        Engine::new(vec![Box::new(TimerRule(TimerDef {
            name: "Test boss".into(), first_spawn_secs: 600, interval_secs: 300, warn_ahead_secs: 30 }))])
    }

    #[test]
    fn warns_before_first_spawn_once() {
        let mut e = eng();
        assert!(e.step(&at(500)).is_empty());
        assert_eq!(e.step(&at(575))[0].text, "Test boss in 25 seconds");
        assert!(e.step(&at(580)).is_empty(), "deduped");
    }

    #[test]
    fn warns_again_for_next_spawn() {
        let mut e = eng();
        e.step(&at(575));
        assert_eq!(e.step(&at(880))[0].text, "Test boss in 20 seconds");
    }

    #[test]
    fn shipped_timings_parse() {
        let r = rules_from_timings(include_str!("../../../data/timings.json")).unwrap();
        let mut e = Engine::new(r);
        assert_eq!(e.step(&at(580))[0].text, "Soul Urn in 20 seconds");
    }

    #[test]
    fn counter_rule_fires_once_and_skips_owned() {
        let r = counter_rule_from_json(r#"[{"when_enemy_has":"A","suggest":"B","reason":"test."}]"#).unwrap();
        let mut e = Engine::new(vec![r]);
        let mut s = at(100); s.enemy_items = vec!["A".into()];
        assert_eq!(e.step(&s)[0].text, "Against A: buy B.");
        assert!(e.step(&s).is_empty());
        let mut s2 = at(1000); s2.enemy_items = vec!["A".into()]; s2.items = vec!["B".into()];
        assert!(e.step(&s2).is_empty());
    }

    #[test]
    fn unspent_and_lead_rules() {
        let mut e = Engine::new(default_rules(RulesConfig::default()));
        let mut s = at(400);
        s.souls = Some(2000); s.ally_networth = Some(20000); s.enemy_networth = Some(15000);
        e.max_per_step = 5; e.budget = false;
        e.min_gap_secs = 0;
        let mut texts: Vec<String> = vec![];
        for t in [400, 405, 410, 415] { let mut s2 = s.clone(); s2.game_clock_secs = Some(t); texts.extend(e.step(&s2).into_iter().map(|c| c.text)); }
        assert!(texts.iter().any(|t| t.contains("2,000 souls")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("ahead")), "needs 3 consistent reads: {texts:?}");
        assert_eq!(texts.iter().filter(|t| t.contains("ahead")).count(), 1);
    }

    #[test]
    fn dead_player_is_told_to_shop_and_why() {
        let mut e = Engine::new(default_rules(RulesConfig::default()));
        e.max_per_step = 5;
        let mut s = at(850);
        s.dead = true; s.souls = Some(2000); s.respawn_secs = Some(20); s.death_cause = Some("Temple Guardia".into());
        // The death review names the buy: the generic souls reminder waits for it.
        let mut t: Vec<String> = e.step(&s).into_iter().map(|c| c.text).collect();
        assert!(t.is_empty(), "{t:?}");
        // The death review waits ~5 s for the damage panel to fill in.
        s.game_clock_secs = Some(856); s.respawn_secs = Some(14);
        t.extend(e.step(&s).into_iter().map(|c| c.text));
        assert!(t.iter().any(|x| x.contains("Midboss") && x.contains("Spend 2,000 souls now.")), "{t:?}");
        for c in [860, 866] { s.game_clock_secs = Some(c); t.extend(e.step(&s).into_iter().map(|c| c.text)); }
        assert!(!t.iter().any(|x| x.contains("dead with")), "one souls call per death: {t:?}");
        // Without a review (no rule for it), the reminder still comes, 25 s into the death.
        let mut e = Engine::new(vec![Box::new(UnspentSoulsRule::new(RulesConfig::default()))]);
        let mut s = at(850); s.dead = true; s.souls = Some(2000); s.respawn_secs = Some(30);
        let mut said = vec![];
        for c in (850..=880).step_by(5) { s.game_clock_secs = Some(c); s.respawn_secs = Some(880 - c); said.extend(e.step(&s).into_iter().map(|x| (c, x.text))); }
        assert!(said.len() == 1 && said[0].0 >= 875 && said[0].1.contains("dead with 2,000 souls"), "{said:?}");
    }

    #[test]
    fn counter_matches_enemy_hero() {
        let r = counter_rule_from_json(r#"[{"when_enemy_has":"hero_x","suggest":"Y","reason":"r."}]"#).unwrap();
        let mut s = at(10); s.enemy_heroes = vec!["hero_x".into()];
        assert_eq!(Engine::new(vec![r]).step(&s).len(), 1);
    }

    #[test]
    fn counter_needs_read_souls_and_a_fitting_build() {
        // Plated Armor (vitality, 6400) vs a spirit item; owned items are three weapon items.
        let json = r#"[{"when_enemy_has":"hero_x","suggest":"Plated Armor","suggest_class":"upgrade_deflecting_armor","reason":"r."},
            {"when_enemy_has":"hero_y","suggest":"Extra Spirit","suggest_class":"upgrade_improved_spirit","reason":"r."}]"#;
        let step = |souls: Option<u32>, enemy: &str| {
            let mut s = at(900); s.souls = souls; s.enemy_heroes = vec![enemy.into()];
            s.items = vec!["upgrade_close_range".into(), "upgrade_high_velocity_mag".into(), "upgrade_clip_size".into()];
            Engine::new(vec![counter_rule_from_json(json).unwrap()]).step(&s).len()
        };
        assert_eq!(step(None, "hero_x"), 0, "souls unread");
        assert_eq!(step(Some(5000), "hero_x"), 0, "cannot afford 6400");
        assert_eq!(step(Some(7000), "hero_x"), 1);
        assert_eq!(step(Some(7000), "hero_y"), 0, "spirit item for a weapon build");
    }

    #[test]
    fn implausible_team_gap_is_not_called() {
        let mut e = Engine::new(vec![Box::new(NetworthLeadRule::new(RulesConfig::default()))]);
        e.min_gap_secs = 0;
        let mut said = 0;
        for t in [600, 610, 620, 630] { let mut s = at(t); s.ally_networth = Some(1000); s.enemy_networth = Some(60000); said += e.step(&s).len(); }
        assert_eq!(said, 0, "a 98% gap is a misread, not a game state");
    }

    #[test]
    fn the_team_lead_is_repeated_at_most_every_eight_minutes_but_a_change_is_news() {
        // rec12 said "your team is ahead" five times in 28 minutes.
        let mut e = Engine::new(vec![Box::new(NetworthLeadRule::new(RulesConfig::default()))]);
        e.min_gap_secs = 0;
        let mut said: Vec<(u32, String)> = vec![];
        let mut step = |t: u32, a: u32, en: u32, said: &mut Vec<(u32, String)>| { let mut s = at(t); s.ally_networth = Some(a); s.enemy_networth = Some(en); said.extend(e.step(&s).into_iter().map(|c| (t, c.key))); };
        for t in (600..1500).step_by(10) { let a = 60000 + (t - 600) * 50; step(t, a, a * 100 / 125, &mut said); }
        let leads: Vec<u32> = said.iter().filter(|x| x.1 == "lead").map(|x| x.0).collect();
        assert_eq!(leads.len(), 2, "{said:?}");
        assert!(leads[1] >= leads[0] + LEAD_REPEAT_SECS, "{leads:?}");
        // Ahead -> behind is said at once (a new state, its own key).
        said.clear();
        for t in (1500..1560).step_by(10) { step(t, 100000, 125000 + (t - 1500) * 20, &mut said); }
        assert_eq!(said.iter().filter(|x| x.1 == "behind").count(), 1, "{said:?}");
    }

    #[test]
    fn a_growing_lead_praise_replaces_the_generic_lead_call() {
        let mut e = Engine::new(default_rules(RulesConfig::default()));
        e.min_gap_secs = 0; e.budget = false;
        let mut keys: Vec<String> = vec![];
        for t in (600..1100).step_by(10) {
            let mut s = at(t); s.screen = Some("Gameplay".into()); s.hp = Some(1800); s.max_hp = Some(2000); s.health_observations = 3;
            s.minimap = Some(coach_core::MinimapState { observations: 3, you: Some(coach_core::Pt { x: 0.5, y: 0.7 }), ..Default::default() });
            s.ally_networth = Some(50000 + (t - 600) * 40); s.enemy_networth = Some(50000 + (t - 600) * 10);
            keys.extend(e.step(&s).into_iter().map(|c| c.key));
        }
        let praise = keys.iter().position(|k| k.starts_with("praise:lead:")).unwrap_or_else(|| panic!("{keys:?}"));
        assert!(!keys[praise..].iter().any(|k| k == "lead"), "{keys:?}");
    }

    #[test]
    fn no_clock_no_cue() {
        assert!(eng().step(&GameSnapshot::default()).is_empty());
    }
}
