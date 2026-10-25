// Usage: COACH_TOKEN=.. record <addr> <outdir> <count>  -> saves frames as JPEGs.
#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let addr = a.next().unwrap(); let dir = a.next().unwrap();
    let max: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    std::fs::create_dir_all(&dir).unwrap();
    let token = std::env::var("COACH_TOKEN").unwrap_or_default();
    let mut n = 0usize;
    let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = coach_transport::connect(&addr, &token, &mut rx, |m, j| {
        std::fs::write(format!("{dir}/{:05}_{}.jpg", n, m.timestamp_ms), &j).unwrap();
        n += 1; println!("saved {n}");
        if n >= max { std::process::exit(0); }
    }).await;
}
