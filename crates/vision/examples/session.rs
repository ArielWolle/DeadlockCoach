use coach_vision::scan::Scanner;
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let out_path = std::env::args().nth(2).unwrap();
    let mut s = Scanner::load(std::path::Path::new(".")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    files.sort();
    let mut out = String::new();
    for p in &files {
        let snap = s.scan(&std::fs::read(p).unwrap());
        out += &serde_json::to_string(&snap).unwrap(); out.push(char::from(10));
    }
    std::fs::write(&out_path, out).unwrap();
}
