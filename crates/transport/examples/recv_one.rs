// Usage: COACH_TOKEN=... recv_one <addr>   -> writes remote.jpg from the first frame.
#[tokio::main]
async fn main() {
    let addr = std::env::args().nth(1).unwrap_or("127.0.0.1:7878".into());
    let token = std::env::var("COACH_TOKEN").unwrap_or_default();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), coach_transport::connect(&addr, &token, |m, j| {
        std::fs::write("remote.jpg", &j).unwrap();
        println!("frame {}x{} scale {} bytes {}", m.width, m.height, m.dpi_scale, j.len());
        std::process::exit(0);
    }))
    .await
    .map(|r| println!("{r:?}"));
    println!("no frame");
}
