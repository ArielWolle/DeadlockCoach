// Usage: COACH_TOKEN=... say <addr> "text"   -> TTS here, plays on the Agent's PC.
#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let addr = a.next().unwrap_or("127.0.0.1:7878".into());
    let text = a.next().unwrap_or("Coach audio test. Mid boss spawns in thirty seconds.".into());
    let token = std::env::var("COACH_TOKEN").unwrap_or_default();
    let wav = coach_audio::Piper::from_env().and_then(|p| p.synth(&text)).expect("tts");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tx.send(wav).unwrap();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(6), coach_transport::connect(&addr, &token, &mut rx, |_, _| {})).await;
    println!("sent");
}
