//! Headless Agent for the gaming PC. Must run in the logged-in desktop session.
//! Env: COACH_TOKEN (required), COACH_BIND (default 0.0.0.0:7878), COACH_INTERVAL_SECS (default 5).
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--devices") {
        for d in coach_audio::list_devices() { println!("{d}"); }
        return;
    }
    let token = std::env::var("COACH_TOKEN").expect("set COACH_TOKEN");
    let bind = std::env::var("COACH_BIND").unwrap_or_else(|_| format!("0.0.0.0:{}", coach_transport::DEFAULT_PORT));
    let secs: u64 = std::env::var("COACH_INTERVAL_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(5);

    let (tx, _) = broadcast::channel::<Arc<Vec<u8>>>(2);
    let txc = tx.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(secs));
        loop {
            tick.tick().await;
            if txc.receiver_count() == 0 { continue; }
            match tokio::task::spawn_blocking(|| coach_capture::capture_frame(1600, 80)).await {
                Ok(Ok(f)) => { let _ = txc.send(Arc::new(coach_transport::encode_frame(&f.meta, &f.jpeg))); }
                Ok(Err(e)) => eprintln!("capture failed: {e}"),
                Err(e) => eprintln!("capture task failed: {e}"),
            }
        }
    });
    // One worker thread plays queued audio sequentially (no overlapping cues).
    let (atx, arx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        for wav in arx {
            if let Err(e) = coach_audio::play_wav(wav) { eprintln!("playback failed: {e}"); } else { eprintln!("played cue"); }
        }
    });
    eprintln!("agent listening on {bind}, interval {secs}s");
    coach_transport::serve(&bind, token, tx, move |wav| { let _ = atx.send(wav); }).await.expect("server failed");
}
