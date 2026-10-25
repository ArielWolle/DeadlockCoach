use coach_vision::scan::Scanner;
fn main() {
    let mut s = Scanner::load(std::path::Path::new(".")).unwrap();
    let mut files: Vec<_> = std::fs::read_dir("frames").unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "jpg").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("0")).collect();
    files.sort();
    let mut out = String::new();
    for p in &files { out += &serde_json::to_string(&s.scan(&std::fs::read(p).unwrap())).unwrap(); out.push(char::from(10)); }
    std::fs::write("samples/session1.jsonl", out).unwrap();
}
