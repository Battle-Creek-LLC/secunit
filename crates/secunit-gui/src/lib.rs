//! `secunit-gui` — Tauri shell that embeds `secunit-core` and exposes a
//! read-only IPC surface to the webview. The shell never mutates anything
//! inside a project tree; the only state-changing paths remain the CLI
//! and direct git edits.

pub mod api;
pub mod projects;
pub mod state;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("SECUNIT_GUI_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tauri=warn")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            api::list_projects,
            api::select_project,
            api::current_project,
            api::load_project,
            api::list_controls,
            api::get_control,
            api::due_rows,
            api::get_inventory,
            api::list_runs,
            api::recent_runs,
            api::get_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running secunit-gui");
}
