# JOB-03 — IPC: expose `secunit-core` read APIs

## Goal

Tauri commands surface every read needed by the views, all backed by `secunit-core`. No status logic is written on the frontend.

## Deliverables

A `crates/secunit-gui/src-tauri/src/api.rs` module with these commands. All take a `project_name: String` (already selected via JOB-02) and resolve the path through app state.

| Command | Returns | Notes |
|---|---|---|
| `load_project(name)` | `LoadSummary { errors, warnings, controls_count, inventory_count, has_state, has_config }` | Loads the registry, caches the `LoadedRegistry` in app state, returns a summary. |
| `list_controls()` | `Vec<ControlSummary>` | Per-control: id, title, cadence, owner, current status, next_due, overdue, last_run_id, last_run_at. |
| `get_control(id)` | `ControlDetail` | Full control + most-recent-N runs (manifests parsed, by_system block flattened). |
| `due_rows(today?: NaiveDate)` | `Vec<DueRow>` | `today` defaults to system local date. |
| `get_inventory()` | `InventoryView` | Grouped by kind, each entry annotated with `is_active_today`. |
| `list_runs(control_id?, quarter?)` | `Vec<RunRow>` | Walks `evidence/<y>/<q>/...`, returns `RunRow { control_id, run_id, started_at, status, sealed, manifest_sha256? }`. Pending and aborted runs included with their state. |
| `get_run(control_id, run_id)` | `RunDetail` | Manifest (if sealed), `prepare.json`, abort sidecar (if any), and a tree of artifact paths under the run dir. |
| `get_findings(filter)` | `Vec<FindingsRow>` | Reverse-chron list of `findings.md` files; `filter` allows `control_id`, `system`, `quarter`. |
| `read_findings(control_id, run_id)` | `String` (sanitised HTML) | Renders `findings.md` to HTML via `pulldown-cmark` with an allow-list. |
| `read_artifact(path)` | `ArtifactBytes` | Path must canonicalise inside the project root; otherwise error. Capped at 5 MiB; larger artifacts return a sentinel. |

Every type that crosses the IPC boundary is `Serialize` + `ts-rs`-exported, and the frontend consumes the generated TypeScript via a `web/src/lib/bindings/` directory checked in.

## Non-goals

- No `notify` plumbing yet — registry is loaded once on `load_project`. Live updates land in JOB-04.
- No write commands. Ever.

## Acceptance criteria

- Against `testdata/orgs/multi-system/` (mounted as a project in `projects.yaml`), every listed command returns non-empty, well-typed data.
- Errors from `LoadReport` surface to the frontend with the exact path and message; loading does not silently succeed when soft-validation fails.
- `read_artifact` rejects paths that escape the project root with a precise error.
- All TypeScript bindings in `web/src/lib/bindings/` regenerate cleanly; CI fails if they drift.
- `clippy -p secunit-gui -- -D warnings` is clean.

## Test plan

- **Rust unit (per command):** drive against `testdata/orgs/multi-system/`. Assert known counts (e.g. `list_controls().len() == 8`) and verify well-known fields (`ac-annual-access-review.cadence == Annual`).
- **Path traversal test:** call `read_artifact("../../../etc/passwd")`, assert error.
- **Property test:** for any control returned by `list_controls`, `get_control` succeeds.
- **TypeScript:** `tsc --noEmit` against the generated bindings.

## Files touched

```
crates/secunit-gui/src-tauri/src/api.rs
crates/secunit-gui/src-tauri/src/api/types.rs
crates/secunit-gui/src-tauri/src/state.rs
crates/secunit-gui/src-tauri/src/main.rs            (register commands)
crates/secunit-gui/src-tauri/build.rs               (regenerate ts-rs)
crates/secunit-gui/web/src/lib/bindings/*.ts        (generated)
crates/secunit-gui/web/src/lib/api.ts               (typed wrappers)
crates/secunit-gui/src-tauri/tests/api_smoke.rs
```
