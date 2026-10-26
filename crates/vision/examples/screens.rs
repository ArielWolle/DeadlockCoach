use coach_vision::scan::Scanner;
fn main() {
    let s = Scanner::load(std::path::Path::new(".")).unwrap();
    let dir = std::env::args().nth(1).unwrap_or("frames".into());
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("0")).collect();
    files.sort();
    for p in &files {
        let sc = s.classify(&std::fs::read(p).unwrap());
        println!("{} {:?}", p.file_stem().unwrap().to_string_lossy(), sc);
    }
}
