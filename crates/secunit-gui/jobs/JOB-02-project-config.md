# JOB-02 — Project config + project switcher

## Goal

The app reads `~/.config/secunit-gui/projects.yaml`, populates a top-bar project switcher, and remembers the last-selected project across launches.

## Deliverables

- New module `crates/secunit-gui/src-tauri/src/projects.rs`:
  - `struct ProjectsConfig { projects: Vec<ProjectEntry>, default: Option<String> }`.
  - `struct ProjectEntry { name: String, path: PathBuf }`.
  - `load() -> ProjectsConfig` — reads the YAML, expands `~`, validates that each `path` exists, returns a structured error otherwise. Missing file → empty config (not an error).
- IPC commands:
  - `#[tauri::command] fn list_projects() -> ProjectsConfig`
  - `#[tauri::command] fn select_project(name: String) -> Result<ProjectSelection, String>` — records the selection in app state and persists `~/.config/secunit-gui/state.json` with `{ "last_selected": "<name>" }`.
- Frontend:
  - Top bar renders a project switcher (a shadcn-style listbox) populated from `list_projects()`.
  - On mount, the app calls `list_projects()`, picks `default → last_selected → first → none` in that order, and calls `select_project()`.
  - When the user picks a project, the switcher updates and `select_project()` fires.
  - Empty / missing `projects.yaml` → render a centered explainer card with the exact path to create the file and a copy-pasteable example.

## Non-goals

- The selection does not yet trigger registry loading or watchers — those land in JOB-03 and JOB-04. JOB-02 only proves the path through to the binary.

## Acceptance criteria

- A real `~/.config/secunit-gui/projects.yaml` with two entries renders both entries in the switcher; selecting either fires `select_project()` (verified with a `tracing` log line).
- A missing file shows the explainer with the literal path `~/.config/secunit-gui/projects.yaml`.
- A malformed YAML shows a structured error message with the line/column from `serde_yaml`.
- After selecting project B and quitting, relaunch picks project B as the active selection.
- `~` in `path` is expanded.
- A non-existent `path` for a project surfaces a warning badge on that switcher row but does not block the rest of the list.

## Test plan

- **Rust unit:** parse three fixture YAMLs (empty, two-projects, malformed). Round-trip persisted state. `~` expansion against a tempdir-mocked HOME.
- **Frontend unit:** the switcher mounts with three states (loaded list, empty config, error) and renders each as snapshot HTML.
- **Manual smoke:** create the fixture YAML against the in-repo `testdata/orgs/multi-system/`, launch, confirm the switcher works.

## Files touched

```
crates/secunit-gui/src-tauri/src/projects.rs
crates/secunit-gui/src-tauri/src/main.rs            (register commands)
crates/secunit-gui/src-tauri/src/state.rs           (app state container)
crates/secunit-gui/web/src/lib/ipc.ts               (typed wrappers)
crates/secunit-gui/web/src/components/ProjectSwitcher.tsx
crates/secunit-gui/web/src/App.tsx
crates/secunit-gui/web/src/__tests__/ProjectSwitcher.test.tsx
```
