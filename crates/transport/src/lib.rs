//! Agent <-> Brain transport: WebSocket, token auth.
//!
//! Agent -> Brain binary: frame = u32 BE meta_len | meta JSON | JPEG bytes.
//! Brain -> Agent binary: WAV audio to play.
//! First message from the client must be text `AUTH <token>`; server answers `OK`.
use coach_core::FrameMeta;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

pub const DEFAULT_PORT: u16 = 7878;

pub fn encode_frame(meta: &FrameMeta, jpeg: &[u8]) -> Vec<u8> {
    let m = serde_json::to_vec(meta).unwrap();
    let mut out = Vec::with_capacity(4 + m.len() + jpeg.len());
    out.extend_from_slice(&(m.len() as u32).to_be_bytes());
    out.extend_from_slice(&m);
    out.extend_from_slice(jpeg);
    out
}

pub fn decode_frame(data: &[u8]) -> Option<(FrameMeta, Vec<u8>)> {
    let (meta, jpeg, _) = decode_frame_with_minimap(data)?;
    Some((meta, jpeg))
}

/// Extended header retains the original fields and adds the main image length.
/// Legacy senders have no jpeg_len and no crop; legacy readers can still decode JPEG
/// (JPEG readers tolerate the appended PNG). Updated readers split the payload exactly.
pub fn encode_frame_with_minimap(meta: &FrameMeta, jpeg: &[u8], png: &[u8]) -> Vec<u8> {
    let mut header = serde_json::to_value(meta).unwrap();
    header["jpeg_len"] = serde_json::json!(jpeg.len());
    let m = serde_json::to_vec(&header).unwrap();
    let mut out = Vec::with_capacity(4 + m.len() + jpeg.len() + png.len());
    out.extend_from_slice(&(m.len() as u32).to_be_bytes());
    out.extend_from_slice(&m);
    out.extend_from_slice(jpeg);
    out.extend_from_slice(png);
    out
}

pub fn decode_frame_with_minimap(data: &[u8]) -> Option<(FrameMeta, Vec<u8>, Vec<u8>)> {
    let len = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
    let end = 4usize.checked_add(len)?;
    let header: serde_json::Value = serde_json::from_slice(data.get(4..end)?).ok()?;
    let meta = serde_json::from_value(header.clone()).ok()?;
    let payload = data.get(end..)?;
    let n = match header.get("jpeg_len") {
        Some(n) => usize::try_from(n.as_u64()?).ok()?,
        None => payload.len(),
    };
    Some((meta, payload.get(..n)?.to_vec(), payload.get(n..)?.to_vec()))
}

/// Agent side: accept Brain connections, push every broadcast frame to each,
/// and hand incoming audio (WAV bytes) to `on_audio`.
pub async fn serve<A>(
    addr: &str,
    token: String,
    frames: broadcast::Sender<Arc<Vec<u8>>>,
    on_audio: A,
) -> std::io::Result<()>
where
    A: Fn(Vec<u8>) + Send + Sync + 'static,
{
    let on_audio = Arc::new(on_audio);
    let listener = TcpListener::bind(addr).await?;
    loop {
        let (stream, peer) = listener.accept().await?;
        let (token, mut rx, on_audio) = (token.clone(), frames.subscribe(), on_audio.clone());
        tokio::spawn(async move {
            let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else { return };
            let ok = matches!(ws.next().await, Some(Ok(Message::Text(t))) if t.trim() == format!("AUTH {token}"));
            if !ok {
                let _ = ws.close(None).await;
                eprintln!("rejected {peer}");
                return;
            }
            let _ = ws.send(Message::Text("OK".into())).await;
            eprintln!("brain connected: {peer}");
            loop {
                tokio::select! {
                    f = rx.recv() => match f {
                        Ok(f) => { if ws.send(Message::Binary((*f).clone())).await.is_err() { break; } }
                        // Brain is slow: drop old frames rather than queue.
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    },
                    m = ws.next() => match m {
                        Some(Ok(Message::Binary(b))) => on_audio(b),
                        Some(Ok(_)) => {}
                        _ => break,
                    },
                }
            }
            eprintln!("brain disconnected: {peer}");
        });
    }
}

/// Brain side: connect, authenticate, call `on_frame` for each frame, and
/// forward anything received on `audio_out` to the Agent. Returns when the
/// connection ends; caller decides whether to retry.
pub async fn connect<F: FnMut(FrameMeta, Vec<u8>)>(
    addr: &str,
    token: &str,
    audio_out: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    mut on_frame: F,
) -> Result<(), String> {
    connect_with_minimap(addr, token, audio_out, |meta, jpeg, _| on_frame(meta, jpeg)).await
}

pub async fn connect_with_minimap<F: FnMut(FrameMeta, Vec<u8>, Vec<u8>)>(
    addr: &str,
    token: &str,
    audio_out: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    mut on_frame: F,
) -> Result<(), String> {
    let stream = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let (mut ws, _) = tokio_tungstenite::client_async(format!("ws://{addr}/"), stream)
        .await
        .map_err(|e| e.to_string())?;
    ws.send(Message::Text(format!("AUTH {token}"))).await.map_err(|e| e.to_string())?;
    match ws.next().await {
        Some(Ok(Message::Text(t))) if t == "OK" => {}
        _ => return Err("authentication rejected".into()),
    }
    loop {
        tokio::select! {
            msg = ws.next() => match msg {
                Some(m) => {
                    if let Message::Binary(b) = m.map_err(|e| e.to_string())? {
                        if let Some((meta, jpeg, png)) = decode_frame_with_minimap(&b) { on_frame(meta, jpeg, png); }
                    }
                }
                None => break,
            },
            Some(wav) = audio_out.recv() => {
                ws.send(Message::Binary(wav)).await.map_err(|e| e.to_string())?;
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let meta = FrameMeta { width: 1920, height: 1080, dpi_scale: 1.0, timestamp_ms: 5 };
        let (m, j) = decode_frame(&encode_frame(&meta, &[1, 2, 3])).unwrap();
        assert_eq!((m.width, m.timestamp_ms, j), (1920, 5, vec![1, 2, 3]));
    }

    #[test]
    fn minimap_payload_and_legacy_decode() {
        let meta = FrameMeta { width: 2560, height: 1440, dpi_scale: 1.0, timestamp_ms: 20 };
        let packet = encode_frame_with_minimap(&meta, &[1, 2], &[3, 4, 5]);
        let (_, jpeg, png) = decode_frame_with_minimap(&packet).unwrap();
        assert_eq!(jpeg, vec![1, 2]); assert_eq!(png, vec![3, 4, 5]);
        assert_eq!(decode_frame(&packet).unwrap().1, jpeg);
        let old = encode_frame(&meta, &[8, 9]);
        assert!(decode_frame_with_minimap(&old).unwrap().2.is_empty());
        assert!(decode_frame_with_minimap(&packet[..packet.len() - 4]).is_none());
        assert!(decode_frame_with_minimap(&[255, 255, 255, 255]).is_none());
    }
}
