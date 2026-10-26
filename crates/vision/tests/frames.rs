use coach_vision::scan::Scanner;
use std::path::Path;

fn frame(prefix: &str) -> Vec<u8> {
    let dir = Path::new("../../samples/frames");
    let f = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).find(|e| e.file_name().to_string_lossy().starts_with(prefix)).unwrap();
    std::fs::read(f.path()).unwrap()
}

#[test]
fn reads_real_frames() {
    let mut s = Scanner::load(Path::new("../..")).unwrap();
    let a = s.scan(&frame("00014"));
    assert_eq!(a.game_clock_secs, Some(88));
    assert_eq!(a.souls, Some(900));
    let mut s = Scanner::load(Path::new("../..")).unwrap();
    let b = s.scan(&frame("00059"));
    assert_eq!(b.game_clock_secs, Some(313));
    assert_eq!(b.souls, Some(1732));
    assert_eq!(b.ally_networth, Some(21000));
    assert_eq!(b.enemy_networth, Some(16000));
    assert!(b.items.contains(&"upgrade_pristine_emblem".to_string()), "items: {:?}", b.items);
}

#[test]
fn classifies_shop_and_gameplay() {
    use coach_vision::screen::Screen;
    let s = Scanner::load(Path::new("../..")).unwrap();
    assert_eq!(s.classify(&frame("00048")), Screen::Shop);
    assert_eq!(s.classify(&frame("00014")), Screen::Gameplay);
}
