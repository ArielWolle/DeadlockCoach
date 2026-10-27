//! End-to-end positional advice on saved frames: each frame is scanned twice (1 s apart) so
//! hero tracks confirm, then the positioning coach and full engine are evaluated.
use coach_engine::positioning::PositionCoach;
fn main() {
    let root = std::path::Path::new(".");
    let reader = coach_vision::ocr::Reader::load(&root.join("data/models")).unwrap();
    for f in std::env::args().skip(1) {
        let img = image::open(&f).unwrap();
        let mut auto = coach_vision::auto::AutoScanner::new(coach_vision::scan::Scanner::load(root).unwrap(), reader.clone());
        auto.scan(&img, 0);
        let s = auto.scan(&img, 1000);
        let name = std::path::Path::new(&f).file_name().unwrap().to_string_lossy().to_string();
        if s.screen.as_deref() != Some("Gameplay") {
            println!("{name}: screen={:?} clock={:?} souls={:?}", s.screen, s.game_clock_secs, s.souls);
            for r in auto.regions() { println!("   region {} {:?} {:?}", r.id, r.bounds, r.label); }
        }
        let Some(m) = &s.minimap else { println!("{name}: no minimap ({:?})", s.screen); continue };
        let fmt = |ally: bool| m.heroes.iter().filter(|h| h.ally == ally && h.confirmed()).map(|h| format!("({:.0},{:.0})", h.pos.x * 335.0, h.pos.y * 350.0)).collect::<Vec<_>>().join(" ");
        println!("{name}: screen={:?} clock={:?} hp={:?}/{:?} you={:?} lane={:?}", s.screen, s.game_clock_secs, s.hp, s.max_hp,
            m.you.map(|p| ((p.x * 335.0) as i32, (p.y * 350.0) as i32)), m.your_lane);
        println!("   allies [{}]  enemies [{}]", fmt(true), fmt(false));
        let slots: Vec<String> = s.hero_status.iter().map(|h| format!("{}{}:{}{}{}", if h.ally { "A" } else { "E" }, h.slot,
            if h.dead == Some(true) { format!("DEAD({})", h.respawn_secs.map(|v| v.to_string()).unwrap_or("?".into())) }
            else if h.bar_visible == Some(false) { "unseen".into() }
            else { h.health_ratio.map(|v| format!("{:.0}%", v * 100.0)).unwrap_or("?".into()) },
            if h.critical == Some(true) { " CRIT" } else { "" },
            format!("{}{}", h.hero.as_deref().map(|n| format!(" {}", n.trim_start_matches("hero_"))).unwrap_or_default(),
                h.position.map(|p| format!(" @({:.0},{:.0})", p.x * 335.0, p.y * 350.0)).unwrap_or_default()))).collect();
        println!("   slots {}", slots.join(" | "));
        match PositionCoach::default().assess(&s) { Some(a) => println!("   ADVICE {}: {}\n      evidence: {:?}", a.id, a.text, a.evidence), None => println!("   (no positional advice)") }
        let lane = coach_engine::laning::LaneCoach::default().decide(&s);
        if let Some(d) = lane { println!("   lane plan {:?} speak={}: {}", d.action, d.speak, d.text); }
    }
}
