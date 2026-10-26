use coach_vision::scan::Scanner;
fn main() {
    let s = Scanner::load(std::path::Path::new(".")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("0")).collect();
    files.sort();
    let mut counts = std::collections::BTreeMap::new();
    for (i, p) in files.iter().enumerate() {
        let sc = s.classify(&std::fs::read(p).unwrap());
        if !matches!(sc, coach_vision::screen::Screen::Gameplay) { println!("{i}: {:?}", sc); }
        *counts.entry(format!("{:?}", sc)).or_insert(0) += 1;
    }
    println!("{counts:?}");
}
