# JOB-04 — `notify` watcher + reactive store

## Goal

A single, debounced `notify` watcher per open project emits typed events to the webview. The frontend keeps a reactive in-memory index keyed by `control_id` and `run_id`, patched by those events.

## Deliverables

- `crates/secunit-gui/src-tauri/src/watcher.rs`:
  - One watcher per active project; the previous watcher is dropped when the project switches.
  - 200 ms trailing debounce, configurable via `SECUNIT_GUI_WATCH_DEBOUNCE_MS` for tests.
  - Emits typed events on the Tauri event bus:
    - `control_changed { id, path }` — file under `controls/`.
    - `inventory_changed`.
    - `state_json_changed`.
    - `run_state_changed { control_id, run_id, kind: "prepared" | "sealed" | "aborted" | "pending" | "removed" }` — derived from `.run-pending`, `manifest.json`, `abort.json` appearing or disappearing.
    - `findings_changed { control_id, run_id }` — file under `evidence/**/findings.md`.
- Events carry just enough to invalidate cache; the frontend re-fetches via the IPC commands from JOB-03 to get fresh data. (Single source of truth = `secunit-core`.)
- Frontend `web/src/store/`:
  - A small reactive store (Zustand or hand-rolled — pick one and stick to it) keyed by `control_id` and `run_id`.
  - On `select_project`, prime the store with `list_controls`, `due_rows`, `list_runs`. On each event, surgically patch the affected slice.
  - A subscribe-by-key API for views to consume specific entries without re-rendering on unrelated changes.

## Non-goals

- No views consuming the store yet — JOB-05 and after wire them.
- No persistence of the store across reloads. The store is rebuilt every project open.

## Acceptance criteria

- Switching projects in the switcher (JOB-02) drops the previous watcher; only one watcher is active at a time. Verified via a counter in app state.
- Touching `controls/ac-annual-access-review.yaml` in the fixture org while the app is open emits exactly one `control_changed` event within 250 ms.
- Bursty edits (10 rapid writes within the debounce window) coalesce into a single event per file.
- Adding a `manifest.json` to a run dir emits `run_state_changed { kind: "sealed" }`.
- Frontend store patches the corresponding control row without a full reload.

## Test plan

- **Rust integration test** (`tests/watcher.rs`): copy the fixture org into a tempdir, start the watcher, perform the touch / burst / new-manifest scenarios, assert event sequence and timing.
- **Frontend unit:** dispatch synthetic events into the store, assert the indexed slice updates and that subscribers for *other* keys did not re-render (mock subscription counter).
- **Manual smoke:** with the app open against `testdata/orgs/multi-system/`, edit a control in the editor, see the table update without reload.

## Files touched

```
crates/secunit-gui/src-tauri/src/watcher.rs
crates/secunit-gui/src-tauri/src/main.rs            (start/stop on selection)
crates/secunit-gui/src-tauri/tests/watcher.rs
crates/secunit-gui/web/src/store/index.ts
crates/secunit-gui/web/src/store/events.ts
crates/secunit-gui/web/src/__tests__/store.test.ts
```
