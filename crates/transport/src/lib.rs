//! Agent <-> Brain transport: WebSocket, token auth, framed screenshots.
//!
//! Wire format of a frame message (binary): u32 BE meta_len | meta JSON | JPEG bytes.
//! First message from the client must be text `AUTH <token>`; server answers `OK`.
use coach_core::FrameMeta;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
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
    let len = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
    let meta = serde_json::from_slice(data.get(4..4 + len)?).ok()?;
    Some((meta, data.get(4 + len..)?.to_vec()))
}

/// Agent side: accept Brain connections, push every broadcast frame to each.
pub async fn serve(addr: &str, token: String, frames: broadcast::Sender<Arc<Vec<u8>>>) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    loop {
        let (stream, peer) = listener.accept().await?;
        let (token, mut rx) = (token.clone(), frames.subscribe());
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
                match rx.recv().await {
                    Ok(f) => {
                        if ws.send(Message::Binary((*f).clone())).await.is_err() { break; }
                    }
                    // Brain is slow: drop old frames rather than queue.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            eprintln!("brain disconnected: {peer}");
        });
    }
}

/// Brain side: connect, authenticate, and call `on_frame` for each frame.
/// Returns when the connection ends; caller decides whether to retry.
pub async fn connect<F: FnMut(FrameMeta, Vec<u8>)>(addr: &str, token: &str, mut on_frame: F) -> Result<(), String> {
    let stream = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let (mut ws, _) = tokio_tungstenite::client_async(format!("ws://{addr}/"), stream)
        .await
        .map_err(|e| e.to_string())?;
    ws.send(Message::Text(format!("AUTH {token}"))).await.map_err(|e| e.to_string())?;
    match ws.next().await {
        Some(Ok(Message::Text(t))) if t == "OK" => {}
        _ => return Err("authentication rejected".into()),
    }
    while let Some(msg) = ws.next().await {
        if let Message::Binary(b) = msg.map_err(|e| e.to_string())? {
            if let Some((meta, jpeg)) = decode_frame(&b) {
                on_frame(meta, jpeg);
            }
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
}
