use coach_core::{DamageRow, GameSnapshot, HeroStatus, HeroTrack, Lane, MinimapState, Pt, ZoneArea};
use coach_engine::{active::ActiveCoach, default_rules, Cue, Engine, Priority, Rule, RulesConfig, UnspentSoulsRule};

/// Alive, healthy, on your own half of Blue lane, 25:00.
fn alive() -> GameSnapshot {
    let mut map = MinimapState { observations: 3, you: Some(Pt { x: 0.5, y: 0.62 }), your_lane: Some(Lane::Blue), zone_area: Some(ZoneArea::Lane), ..Default::default() };
    map.green.push = Some(0.4); map.blue.push = Some(0.5); map.yellow.push = Some(0.7);
    GameSnapshot { game_clock_secs: Some(1500), screen: Some("Gameplay".into()), hp: Some(2400), max_hp: Some(3000), health_observations: 3,
        souls: Some(900), hero: Some("hero_orion".into()), enemy_heroes: vec!["hero_frank".into(), "hero_drifter".into(), "hero_chrono".into()],
        minimap: Some(map), ..Default::default() }
}
fn dead(t: u32, rows: &[(&str, u32)]) -> GameSnapshot {
    GameSnapshot { game_clock_secs: Some(t), screen: Some("Dead".into()), dead: true, enemy_heroes: alive().enemy_heroes,
        death_damage: rows.iter().map(|(s, d)| DamageRow { source: s.to_string(), hits: None, damage: *d }).collect(), ..Default::default() }
}
fn at(mut s: GameSnapshot, t: u32) -> GameSnapshot { s.game_clock_secs = Some(t); s }
fn track(ally: bool, x: f32, y: f32) -> HeroTrack { HeroTrack { ally, pos: Pt { x, y }, hits: 5, unseen_ms: 0, ..Default::default() } }
fn team(enemy_dead: &[Option<u32>], ally_dead: usize) -> Vec<HeroStatus> {
    (0..12).map(|i| {
        let ally = i < 6; let slot = i % 6;
        let dead = if ally { slot < ally_dead } else { slot < enemy_dead.len() };
        HeroStatus { ally, slot, dead: Some(dead), respawn_secs: if !ally && dead { enemy_dead[slot] } else { None },
            bar_visible: Some(!dead), health_ratio: (!dead).then_some(0.9), ..Default::default() }
    }).collect()
}
fn eval(c: &ActiveCoach, s: &GameSnapshot) -> Vec<Cue> { c.evaluate(s) }
fn keys(v: &[Cue]) -> Vec<String> { v.iter().map(|c| c.key.clone()).collect() }
/// Every spoken call is short enough to say: at most 2 sentences and 25 words.
fn speakable(c: &Cue) {
    let words = c.text.split_whitespace().count();
    let chars: Vec<char> = c.text.chars().collect();
    let sentences = (0..chars.len()).filter(|&i| matches!(chars[i], '.' | '!' | '?') && chars.get(i + 1).is_none_or(|n| n.is_whitespace())).count();
    assert!(words <= 25, "{} words: {}", words, c.text);
    assert!(sentences <= 2, "{} sentences: {}", sentences, c.text);
    assert!(!c.text.contains('{'), "unfilled placeholder: {}", c.text);
}

#[test]
fn active_cards_quote_real_document_guidance() {
    let catalog = ActiveCoach::shipped();
    assert_eq!(catalog.schema_version, 1);
    assert_eq!(catalog.cards.len(), 12);
    let check = |id: &str, source: &coach_engine::laning::Source| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs").join(&source.path);
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains(&source.excerpt), "{id}: missing excerpt {}", source.excerpt);
        assert!(text.contains(&source.section), "{id}: missing section {}", source.section);
        assert!(!source.excerpt.contains("UNVERIFIED"));
        assert!(["WIKI", "COMMUNITY", "GENERAL"].contains(&source.basis.as_str()), "{id}: basis {}", source.basis);
    };
    for c in &catalog.cards {
        assert!(!c.sources.is_empty() && c.cooldown_secs >= 30, "{}", c.id);
        assert!(!c.rationale.is_empty());
        for s in &c.sources { check(&c.id, s); }
    }
    for r in &catalog.roles { check(&r.hero, &r.source); }
    // Debuffs point at real cards; suggested items exist in data/items.json.
    for d in &catalog.debuffs { assert!(catalog.cards.iter().any(|c| c.id == d.card), "{}", d.card); }
    let items = include_str!("../../../data/items.json");
    for k in &catalog.killers { for i in &k.items { assert!(items.contains(&format!("\"{i}\"")), "{i}"); } }
}

// ---- Debuffs -------------------------------------------------------------------------------

#[test]
fn aura_of_suffering_on_you_is_a_high_priority_back_out_call() {
    let c = ActiveCoach::default();
    let mut s = alive(); s.status_effects = vec!["Aura of Suffering".into()];
    let cues = eval(&c, &s);
    assert_eq!(cues[0].key, "active:debuff:aura");
    assert_eq!(cues[0].priority, Priority::High);
    assert!(cues[0].text.contains("Victor") && cues[0].text.contains("Back out"), "{}", cues[0].text);
    speakable(&cues[0]);
    // No badge, a menu, or the death screen: nothing.
    assert!(eval(&ActiveCoach::default(), &alive()).is_empty());
    let mut menu = s.clone(); menu.screen = Some("Shop".into()); assert!(eval(&ActiveCoach::default(), &menu).is_empty());
    let mut gone = s.clone(); gone.dead = true; assert!(!keys(&eval(&ActiveCoach::default(), &gone)).iter().any(|k| k.starts_with("active:debuff")));
}
#[test]
fn stalkers_mark_tells_you_to_group_up() {
    let mut s = alive(); s.status_effects = vec!["Stalker\u{2019}s Mark".into()];
    let cues = eval(&ActiveCoach::default(), &s);
    assert_eq!(cues[0].key, "active:debuff:stalkers_mark");
    assert!(cues[0].text.contains("Drifter") && cues[0].text.contains("teammate"));
    speakable(&cues[0]);
}
#[test]
fn a_debuff_is_said_once_per_cooldown_not_every_frame() {
    let mut e = Engine::new(vec![Box::new(ActiveCoach::default())]);
    let mut said = 0;
    for t in 1500..1530 { let mut s = at(alive(), t); s.status_effects = vec!["aura of suffering".into()]; said += e.step(&s).len(); }
    assert_eq!(said, 1);
    let mut s = at(alive(), 1545); s.status_effects = vec!["aura of suffering".into()];
    assert_eq!(e.step(&s).len(), 1, "again after the 40 s cooldown");
}

// ---- Crowd control -------------------------------------------------------------------------

#[test]
fn hard_cc_is_called_only_while_health_is_low_or_falling_fast() {
    let mut s = alive(); s.status_effects = vec!["stunned".into(), "knocked_off_zipline".into()]; s.hp = Some(1700); s.health_delta_ratio = Some(-0.31);
    let cues = eval(&ActiveCoach::default(), &s);
    assert_eq!(keys(&cues), ["active:cc"]); assert!(cues[0].text.starts_with("Stunned")); speakable(&cues[0]);
    // A lane trade: stunned at 81%, losing 8% (rec3 1:49).
    let mut trade = alive(); trade.status_effects = vec!["stunned".into()]; trade.hp = Some(2430); trade.health_delta_ratio = Some(-0.08);
    assert!(eval(&ActiveCoach::default(), &trade).is_empty());
    // Unread health is not evidence of danger.
    let mut unread = s.clone(); unread.hp = None; assert!(eval(&ActiveCoach::default(), &unread).is_empty());
    // Silence and zipline knock-offs have their own advice.
    let mut sil = alive(); sil.status_effects = vec!["silenced".into()]; sil.hp = Some(1500);
    assert!(eval(&ActiveCoach::default(), &sil)[0].text.starts_with("Silenced"));
    let mut zip = alive(); zip.status_effects = vec!["knocked_off_zipline".into()]; zip.hp = Some(1500);
    assert!(eval(&ActiveCoach::default(), &zip)[0].text.contains("zipline"));
}

// ---- Enemy-dead window -----------------------------------------------------------------------

#[test]
fn enemy_dead_window_on_your_half_names_the_deepest_lane() {
    let mut s = alive(); s.hero_status = team(&[Some(40), Some(22)], 0);
    let cues = eval(&ActiveCoach::default(), &s);
    assert_eq!(keys(&cues), ["pos:enemies_down"]);
    assert_eq!(cues[0].text, "Two enemies are dead for 22 more seconds. Group up and push Yellow lane now.");
    speakable(&cues[0]);
}
#[test]
fn enemy_dead_window_needs_time_health_your_half_and_fewer_allies_down() {
    let quiet = |f: &dyn Fn(&mut GameSnapshot)| { let mut s = alive(); s.hero_status = team(&[Some(40), Some(22)], 0); f(&mut s); eval(&ActiveCoach::default(), &s).is_empty() };
    assert!(quiet(&|s| s.hero_status = team(&[Some(40), Some(10)], 0)), "second timer too short");
    assert!(quiet(&|s| s.hero_status = team(&[Some(40), None], 0)), "an unread timer is not a window");
    assert!(quiet(&|s| s.hero_status = team(&[Some(40), Some(22)], 2)), "your team is down as many");
    assert!(quiet(&|s| s.hp = Some(1300)), "weak");
    assert!(quiet(&|s| s.minimap.as_mut().unwrap().you = Some(Pt { x: 0.5, y: 0.4 })), "already on their half");
    assert!(quiet(&|s| s.minimap.as_mut().unwrap().you = None), "position unknown");
    assert!(quiet(&|s| s.minimap.as_mut().unwrap().heroes = vec![track(false, 0.52, 0.6)]), "an enemy is on you");
}
#[test]
fn the_specific_window_call_wins_over_the_generic_one_and_silences_it() {
    let mut e = Engine::new(default_rules(RulesConfig::default()));
    let mut texts = vec![];
    for t in (1500..1540).step_by(2) { let mut s = at(alive(), t); s.hero_status = team(&[Some(60), Some(50)], 0); texts.extend(e.step(&s).into_iter().map(|c| c.text)); }
    let windows: Vec<&String> = texts.iter().filter(|t| t.contains("enemies are dead") || t.contains("are dead,")).collect();
    assert_eq!(windows.len(), 1, "{texts:?}");
    assert!(windows[0].contains("push Yellow lane"), "{texts:?}");
}

// ---- Base and shop ---------------------------------------------------------------------------

#[test]
fn in_base_with_souls_for_an_upgrade_says_spend_before_walking_out() {
    let rule = UnspentSoulsRule(RulesConfig::default());
    let mut s = alive(); s.game_clock_secs = Some(506); s.souls = Some(2187);
    s.minimap.as_mut().unwrap().zone_area = Some(ZoneArea::OwnBase); s.minimap.as_mut().unwrap().you = Some(Pt { x: 0.43, y: 0.87 });
    let cues = rule.evaluate(&s);
    assert_eq!(keys(&cues), ["unspent:base"]);
    assert_eq!(cues[0].text, "You're in base holding 2,187 souls. Spend them before you walk out.");
    speakable(&cues[0]);
    // Late game a 1,600 item is not the bar: 2,900 at 44:00 in base says nothing (not even the generic call).
    s.game_clock_secs = Some(2640); s.souls = Some(2900);
    assert!(rule.evaluate(&s).is_empty());
    // Out of base the generic field reminder is unchanged.
    s.minimap.as_mut().unwrap().zone_area = Some(ZoneArea::Lane); s.minimap.as_mut().unwrap().you = Some(Pt { x: 0.43, y: 0.6 });
    assert_eq!(keys(&rule.evaluate(&s)), ["unspent"]);
}
#[test]
fn shopping_at_low_health_then_walking_out_gets_a_heal_call_once() {
    let mut e = Engine::new(vec![Box::new(ActiveCoach::default())]);
    let mut s = at(alive(), 2288); s.souls = Some(6585); s.hp = Some(1350);
    assert!(e.step(&s).is_empty(), "no purchase yet");
    let mut after = at(alive(), 2294); after.souls = Some(250); after.hp = Some(1380);
    let cues = e.step(&after);
    assert_eq!(keys(&cues), ["active:heal"]);
    assert_eq!(cues[0].text, "You're heading out at 46% health. Heal before your next fight: standing in base restores health fast.");
    speakable(&cues[0]);
    let mut said = 0;
    for t in 2296..2330 { let mut x = at(after.clone(), t); x.hp = Some(1400); said += e.step(&x).len(); }
    assert_eq!(said, 0, "once per purchase");
}
#[test]
fn healthy_or_no_purchase_means_no_heal_call() {
    let c = ActiveCoach::default();
    let mut s = at(alive(), 2288); s.souls = Some(6585); eval(&c, &s);
    let mut after = at(alive(), 2294); after.souls = Some(250); // 80% health
    assert!(eval(&c, &after).is_empty());
    let c = ActiveCoach::default();
    let mut s = at(alive(), 2288); s.hp = Some(1350); eval(&c, &s);
    assert!(eval(&c, &at(s.clone(), 2290)).is_empty(), "same souls: nothing bought");
}

// ---- Unsecured souls -------------------------------------------------------------------------

#[test]
fn unsecured_souls_near_an_enemy_or_at_low_health_say_back_off() {
    let mut s = alive(); s.unsecured_souls = Some(607); s.minimap.as_mut().unwrap().heroes = vec![track(false, 0.53, 0.6)];
    let cues = eval(&ActiveCoach::default(), &s);
    assert_eq!(keys(&cues), ["active:unsecured"]);
    assert_eq!(cues[0].text, "You carry 607 unsecured souls with an enemy close. Don't die with them: back off until they secure.");
    speakable(&cues[0]);
    let mut low = alive(); low.unsecured_souls = Some(900); low.hp = Some(1200);
    assert!(eval(&ActiveCoach::default(), &low)[0].text.contains("at 40% health"));
    let mut safe = alive(); safe.unsecured_souls = Some(900);
    assert!(eval(&ActiveCoach::default(), &safe).is_empty(), "healthy, nobody near");
    let mut small = s.clone(); small.unsecured_souls = Some(300);
    assert!(eval(&ActiveCoach::default(), &small).is_empty());
    let mut unread = s.clone(); unread.unsecured_souls = None;
    assert!(eval(&ActiveCoach::default(), &unread).is_empty());
}

// ---- Repeat killer / close-range deaths ------------------------------------------------------

const VICTOR_1: &[(&str, u32)] = &[("Aura of Suffering", 789), ("Still Life", 613), ("Lurking Terrors", 540), ("Knocked off Zipline", 332)];
const VICTOR_2: &[(&str, u32)] = &[("Aura of Suffering", 1051), ("Escalating Exposure", 495), ("Torment Pulse", 359), ("Pulse Grenade", 129)];

fn die(e: &mut Engine, from: u32, rows: &[(&str, u32)], souls: Option<u32>) -> Vec<Cue> {
    let mut out = vec![];
    for t in from..from + 20 { let mut d = dead(t, rows); d.souls = souls; out.extend(e.step(&d)); }
    out
}
#[test]
fn the_same_hero_topping_two_death_panels_is_named_with_a_counter_item() {
    let mut e = Engine::new(vec![Box::new(ActiveCoach::default())]);
    let mut a = alive(); a.items = vec!["upgrade_blitz_bullets".into()]; e.step(&a);
    assert!(die(&mut e, 1500, VICTOR_1, Some(700)).is_empty(), "first death: no pattern yet");
    e.step(&at(alive(), 1560));
    let cues = die(&mut e, 1977, VICTOR_2, Some(1229));
    assert_eq!(keys(&cues), ["active:repeat:hero_frank"]);
    assert_eq!(cues[0].text, "Victor's Aura of Suffering topped 2 of your deaths. You're long-range: fight from max range; consider Spirit Shielding for 1,600 souls.");
    speakable(&cues[0]);
}
#[test]
fn the_counter_item_fits_the_souls_and_skips_owned_items() {
    let run = |souls: u32, owned: &[&str]| {
        let mut e = Engine::new(vec![Box::new(ActiveCoach::default())]);
        let mut a = alive(); a.hero = Some("hero_haze".into()); a.items = owned.iter().map(|s| s.to_string()).collect(); e.step(&a);
        die(&mut e, 1500, VICTOR_1, None); e.step(&at(a.clone(), 1560));
        die(&mut e, 1977, VICTOR_2, Some(souls)).first().map(|c| c.text.clone()).unwrap_or_default()
    };
    assert!(run(3300, &[]).ends_with("Keep out of his aura; consider Dispel Magic for 3,200 souls."), "{}", run(3300, &[]));
    assert!(run(3300, &["upgrade_reduce_debuff_duration"]).contains("Spirit Resilience for 3,200"));
    assert!(run(3300, &["upgrade_reduce_debuff_duration", "upgrade_tech_purge", "upgrade_spirit_bubble"]).ends_with("Keep out of his aura."));
}
#[test]
fn generic_or_shared_damage_rows_never_name_a_killer() {
    let c = ActiveCoach::default();
    let enemies: Vec<String> = alive().enemy_heroes;
    let rows = |r: &[(&str, u32)]| r.iter().map(|(s, d)| DamageRow { source: s.to_string(), hits: None, damage: *d }).collect::<Vec<_>>();
    // "Gun Damage" and "Melee" name no hero; Pulse Grenade is Paradox's or Holliday's, and only Paradox is an enemy.
    let a = c.attribute(&rows(&[("Gun Damage", 900), ("Melee", 500), ("Pulse Grenade", 100)]), &enemies);
    assert_eq!(a.hero.as_deref(), Some("hero_chrono"));
    assert!(a.killer(&c.catalog().thresholds).is_none(), "100 of 1500 is not the killer");
    // Two heroes close in damage: neither is clearly the killer.
    let b = c.attribute(&rows(&[("Aura of Suffering", 600), ("Rend", 550)]), &enemies);
    assert!(b.killer(&c.catalog().thresholds).is_none());
}
#[test]
fn a_long_range_hero_dying_up_close_twice_is_told_to_keep_range() {
    let melee: &[(&str, u32)] = &[("Melee", 700), ("Gun Damage", 500), ("Rend", 300), ("Aura of Suffering", 280)];
    let run = |hero: &str| {
        let mut e = Engine::new(vec![Box::new(ActiveCoach::default())]);
        let mut a = alive(); a.hero = Some(hero.into()); e.step(&a);
        let mut out = die(&mut e, 1500, melee, None); e.step(&at(a.clone(), 1560));
        out.extend(die(&mut e, 1700, &[("Melee", 800), ("Gun Damage", 300), ("Rend", 290), ("Aura of Suffering", 260)], None)); out
    };
    let cues = run("hero_orion");
    assert_eq!(keys(&cues), ["active:close"]);
    assert!(cues[0].text.contains("long-range hero")); speakable(&cues[0]);
    assert!(run("hero_frank").is_empty(), "no range advice for heroes without a sourced role");
}
#[test]
fn a_new_death_across_a_recording_gap_is_still_a_new_death() {
    // rec5: the death screen at 25:31, then the next death screen at 32:57 with no frames between.
    let mut e = Engine::new(vec![Box::new(ActiveCoach::default())]);
    e.step(&alive());
    die(&mut e, 1500, VICTOR_1, None);
    let cues = die(&mut e, 1977, VICTOR_2, None);
    assert_eq!(keys(&cues), ["active:repeat:hero_frank"]);
}

// ---- Missed team fight -----------------------------------------------------------------------

#[test]
fn a_team_fight_far_away_held_for_eight_seconds_says_rotate() {
    let mut e = Engine::new(vec![Box::new(ActiveCoach::fast())]);
    let fight = |t: u32, me: Pt| {
        let mut s = at(alive(), t);
        let m = s.minimap.as_mut().unwrap(); m.you = Some(me);
        m.heroes = vec![track(true, 0.2, 0.3), track(true, 0.25, 0.32), track(true, 0.22, 0.25), track(false, 0.28, 0.27), track(false, 0.2, 0.22)];
        s
    };
    let far = Pt { x: 0.8, y: 0.8 };
    assert!(e.step(&fight(1500, far)).is_empty());
    assert!(e.step(&fight(1505, far)).is_empty(), "not held long enough");
    let cues = e.step(&fight(1509, far));
    assert_eq!(keys(&cues), ["active:fight"]);
    assert!(cues[0].text.starts_with("Your team is fighting 3 against 2 at"), "{}", cues[0].text);
    speakable(&cues[0]);
    let mut said = 0; for t in 1510..1560 { said += e.step(&fight(t, far)).len(); }
    assert_eq!(said, 0, "60 s cooldown");
    // Within the positioning coach's help range: its help/join call owns that fight.
    let mut e = Engine::new(vec![Box::new(ActiveCoach::fast())]);
    for t in 1500..1520 { assert!(e.step(&fight(t, Pt { x: 0.5, y: 0.5 })).is_empty()); }
}

// ---- Fast path -----------------------------------------------------------------------------

#[test]
fn the_fast_path_never_speaks_from_status_effects_timers_or_death_panels() {
    let c = ActiveCoach::fast();
    let mut s = alive(); s.status_effects = vec!["Aura of Suffering".into(), "stunned".into()]; s.hp = Some(1000);
    s.hero_status = team(&[Some(40), Some(30)], 0); s.unsecured_souls = Some(2000);
    assert!(eval(&c, &s).is_empty());
    assert!(eval(&c, &dead(1600, VICTOR_2)).is_empty());
}
#[test]
fn default_rules_put_the_debuff_call_first() {
    let mut e = Engine::new(default_rules(RulesConfig::default()));
    let mut s = alive(); s.status_effects = vec!["Aura of Suffering".into()]; s.hp = Some(1500); s.health_delta_ratio = Some(-0.2);
    let cues = e.step(&s);
    assert_eq!(cues[0].key, "active:debuff:aura", "{cues:?}");
    // The generic back-off does not repeat it a second later; the critical call still can.
    let mut next = at(s.clone(), 1501); next.status_effects.clear(); next.hp = Some(1300);
    assert!(!keys(&e.step(&next)).contains(&"lane:back_off".to_string()));
    let mut critical = at(next, 1503); critical.hp = Some(300);
    assert_eq!(keys(&e.step(&critical)), ["lane:critical"]);
}
