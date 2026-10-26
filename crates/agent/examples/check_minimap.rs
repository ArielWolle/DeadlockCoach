//! Checks the extended Agent wire format without playing audio or saving screenshots.
#[tokio::main]
async fn main() {
    let addr = std::env::args().nth(1).expect("agent address");
    let token = std::env::var("COACH_TOKEN").expect("COACH_TOKEN");
    let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut received = false;
    let result = tokio::time::timeout(std::time::Duration::from_secs(12),
        coach_transport::connect_with_minimap(&addr, &token, &mut rx, |meta, jpeg, png| {
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "Agent did not supply the lossless minimap");
            assert!(jpeg.starts_with(&[255, 216]), "main image is not JPEG");
            let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
            if !received {
                println!("frame {}x{}; minimap {}x{} lossless PNG ({} bytes)",
                    meta.width, meta.height, width, height, png.len());
            }
            received = true;
        })).await;
    if let Ok(Err(e)) = result { panic!("connection: {e}"); }
    assert!(received, "no frame received");
}
