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
fn spawn_brain_link(app: tauri::AppHandle) {
    use base64::Engine;
    use tauri::Emitter;
    tauri::async_runtime::spawn(async move {
        let addr = std::env::var("DEADLOCK_AGENT_ADDR").unwrap_or_else(|_| "192.168.2.17:7878".into());
        let token = std::env::var("COACH_TOKEN").unwrap_or_default();
        loop {
            let a = app.clone();
            let _ = a.emit("link", format!("connecting to {addr}"));
            let res = coach_transport::connect(&addr, &token, |meta, jpeg| {
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
            if resolve_role() == Role::Brain {
                spawn_brain_link(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_role])
        .run(tauri::generate_context!())
        .expect("error while running DeadlockCoach");
}
