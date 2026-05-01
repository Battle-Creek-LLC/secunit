//! Project-config IPC: list, select, current. Mirrors what JOB-02
//! introduced; the registry-loading commands live next door in
//! `registry_cmd.rs` so the read-only audit can keep the surfaces tidy.

use tauri::State;

use crate::projects::{self, PersistedState, ProjectsError, ProjectsView};
use crate::state::AppState;

fn stringify(err: ProjectsError) -> String {
    err.to_string()
}

#[tauri::command]
pub fn list_projects() -> Result<ProjectsView, String> {
    let yaml_path = projects::projects_yaml_path().map_err(stringify)?;
    let state_path = projects::state_json_path().map_err(stringify)?;
    let cfg = projects::load_config(&yaml_path).map_err(stringify)?;
    let persisted = projects::load_state(&state_path).map_err(stringify)?;
    Ok(projects::view_for(&cfg, &persisted, &yaml_path))
}

#[tauri::command]
pub fn select_project(name: String, state: State<'_, AppState>) -> Result<String, String> {
    let yaml_path = projects::projects_yaml_path().map_err(stringify)?;
    let cfg = projects::load_config(&yaml_path).map_err(stringify)?;
    if !cfg.projects.iter().any(|p| p.name == name) {
        return Err(format!("unknown project `{name}`"));
    }
    {
        let mut sel = state
            .selected
            .lock()
            .expect("AppState.selected mutex poisoned");
        *sel = Some(name.clone());
    }
    let state_path = projects::state_json_path().map_err(stringify)?;
    projects::save_state(
        &state_path,
        &PersistedState {
            last_selected: Some(name.clone()),
        },
    )
    .map_err(stringify)?;
    tracing::info!(project = %name, "selected project");
    Ok(name)
}

#[tauri::command]
pub fn current_project(state: State<'_, AppState>) -> Option<String> {
    state
        .selected
        .lock()
        .expect("AppState.selected mutex poisoned")
        .clone()
}
