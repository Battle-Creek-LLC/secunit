//! App-wide state held by Tauri (`tauri::Manager` resolves it via type).

use std::path::PathBuf;
use std::sync::Mutex;

use secunit_core::model::LoadedRegistry;

use crate::watcher::WatcherHandle;

pub struct LoadedProject {
    pub name: String,
    pub root: PathBuf,
    pub registry: LoadedRegistry,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub level: &'static str,
    pub path: String,
    pub message: String,
}

#[derive(Default)]
pub struct AppState {
    pub selected: Mutex<Option<String>>,
    pub project: Mutex<Option<LoadedProject>>,
    /// Active watcher; replaced when the project changes, dropped when
    /// the app shuts down.
    pub watcher: Mutex<Option<WatcherHandle>>,
}
