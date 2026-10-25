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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_role])
        .run(tauri::generate_context!())
        .expect("error while running DeadlockCoach");
}
