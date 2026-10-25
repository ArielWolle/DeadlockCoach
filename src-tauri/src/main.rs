#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use coach_core::Role;

/// Role is chosen at launch: `--role agent` or `--role brain` (default brain)
/// or the DEADLOCK_COACH_ROLE env var.
fn resolve_role() -> Role {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--role") {
        if let Some(r) = args.get(i + 1).and_then(|s| Role::parse(s)) {
            return r;
        }
    }
    std::env::var("DEADLOCK_COACH_ROLE")
        .ok()
        .and_then(|s| Role::parse(&s))
        .unwrap_or(Role::Brain)
}

#[tauri::command]
fn get_role() -> Role {
    resolve_role()
}

/// Brain: connect to the Agent forever (reconnecting) and emit frames to the UI.
/// Env: DEADLOCK_AGENT_ADDR (default 192.168.2.17:7878), COACH_TOKEN.
struct AudioTx(tokio::sync::mpsc::UnboundedSender<Vec<u8>>);

/// Speak `text` on the gaming PC: synthesize with Piper here, send WAV to the Agent.
#[tauri::command]
async fn say(text: String, state: tauri::State<'_, AudioTx>) -> Result<(), String> {
    let wav = tauri::async_runtime::spawn_blocking(move || coach_audio::Piper::from_env()?.synth(&text))
        .await
        .map_err(|e| e.to_string())??;
    state.0.send(wav).map_err(|e| e.to_string())
}

fn spawn_brain_link(app: tauri::AppHandle, mut audio_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) {
    use base64::Engine;
    use tauri::Emitter;
    tauri::async_runtime::spawn(async move {
        let addr = std::env::var("DEADLOCK_AGENT_ADDR").unwrap_or_else(|_| "192.168.2.17:7878".into());
        let token = std::env::var("COACH_TOKEN").unwrap_or_default();
        loop {
            let a = app.clone();
            let _ = a.emit("link", format!("connecting to {addr}"));
            let res = coach_transport::connect(&addr, &token, &mut audio_rx, |meta, jpeg| {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
                let _ = a.emit("frame", serde_json::json!({
                    "meta": meta,
                    "src": format!("data:image/jpeg;base64,{b64}"),
                }));
            })
            .await;
            let _ = app.emit("link", format!("disconnected: {res:?}; retrying"));
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tauri::Manager::manage(app, AudioTx(tx));
            if resolve_role() == Role::Brain {
                spawn_brain_link(app.handle().clone(), rx);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_role, say])
        .run(tauri::generate_context!())
        .expect("error while running DeadlockCoach");
}
