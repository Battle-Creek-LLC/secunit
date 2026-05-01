//! App-wide state held by Tauri (`tauri::Manager` resolves it via type).

use std::path::PathBuf;
use std::sync::Mutex;

use secunit_core::model::LoadedRegistry;

/// A project that has been opened — its registry is fully loaded and
/// cached. The watcher (JOB-04) will reach into this struct to trigger
/// reloads on disk events.
pub struct LoadedProject {
    pub name: String,
    pub root: PathBuf,
    pub registry: LoadedRegistry,
    /// Errors and warnings reported by the loader, kept around so the UI
    /// can surface them on demand without re-walking the tree.
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
    /// Currently-selected project name (set by `select_project`).
    pub selected: Mutex<Option<String>>,
    /// Currently-loaded registry. `None` until `load_project` runs.
    pub project: Mutex<Option<LoadedProject>>,
}
