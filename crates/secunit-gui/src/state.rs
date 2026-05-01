//! App-wide state held by Tauri (`tauri::Manager` resolves it via type).

use std::sync::Mutex;

/// Currently-selected project. Future jobs will hang the loaded
/// `secunit-core` registry and the `notify` watcher off the same struct.
#[derive(Debug, Default)]
pub struct AppState {
    pub selected: Mutex<Option<String>>,
}
