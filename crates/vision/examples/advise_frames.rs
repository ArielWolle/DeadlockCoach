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
        let Some(m) = &s.minimap else { println!("{name}: no minimap ({:?})", s.screen); continue };
        let fmt = |ally: bool| m.heroes.iter().filter(|h| h.ally == ally && h.confirmed()).map(|h| format!("({:.0},{:.0})", h.pos.x * 335.0, h.pos.y * 350.0)).collect::<Vec<_>>().join(" ");
        println!("{name}: screen={:?} clock={:?} hp={:?}/{:?} you={:?} lane={:?}", s.screen, s.game_clock_secs, s.hp, s.max_hp,
            m.you.map(|p| ((p.x * 335.0) as i32, (p.y * 350.0) as i32)), m.your_lane);
        println!("   allies [{}]  enemies [{}]", fmt(true), fmt(false));
        match PositionCoach::default().assess(&s) { Some(a) => println!("   ADVICE {}: {}\n      evidence: {:?}", a.id, a.text, a.evidence), None => println!("   (no positional advice)") }
        let lane = coach_engine::laning::LaneCoach::default().decide(&s);
        if let Some(d) = lane { println!("   lane plan {:?} speak={}: {}", d.action, d.speak, d.text); }
    }
}
