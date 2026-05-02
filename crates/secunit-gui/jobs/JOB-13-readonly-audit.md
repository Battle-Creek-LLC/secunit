# JOB-13 — Read-only contract + audit

## Goal

A test that fails the build if any future commit adds a write-shaped IPC command. Plus a documented sweep of the surface as it stands.

## Deliverables

- `crates/secunit-gui/src-tauri/tests/readonly.rs`:
  - Compile-time enumerated allow-list of IPC command names.
  - Test that `tauri::Builder` registrations match the allow-list (extracted via a small registry helper rather than scraping the macro).
  - Allow-list:
    ```
    list_projects, select_project, load_project,
    list_controls, get_control,
    due_rows, schedule_horizon,
    get_inventory,
    list_runs, recent_runs, get_run, list_run_tree,
    get_findings, read_findings,
    read_artifact,
    search, index_status,
    open_in_editor, reveal_in_finder, copy_path,
    ```
  - Adding a new command requires adding its name here and reviewing it for read-only-ness in the PR.
- `crates/secunit-gui/src-tauri/src/sandboxing.rs` (or a section in `api.rs`):
  - Single `canonicalise_inside_root` helper that every IPC command which accepts a path **must** call. The test above asserts that helper is invoked on every path-typed argument (via a reflection helper or a hand-maintained list with a doc-test).
- `crates/secunit-gui/README.md` — a "Read-only contract" section spelling out the rule, the allow-list, and how to add a new command (must be reviewed against the rule).
- A pre-merge checklist item in the PR template (or repo's existing template) reminding reviewers to grep `fn ` against the allow-list when changing `api.rs`.

## Non-goals

- No runtime sandbox enforcement beyond the canonicalise helper. The OS-level Tauri `fs` allow-list configuration belongs in `tauri.conf.json` and is the second line of defence; covered by JOB-01's config but documented here.

## Acceptance criteria

- `cargo test -p secunit-gui` passes with the current command set.
- Adding a new command without updating the allow-list fails the test with a clear diff.
- Removing a command without updating the allow-list also fails (so the list does not rot).
- `tauri.conf.json`'s `app.security` denies write capabilities (`fs.allow` is read-only against the project root); a unit test parses the config and asserts the deny.

## Test plan

- **Rust:** the readonly test above; plus a regression test that adds a stub `write_thing` command in a separate test-only module and asserts the production registry rejects it (prove the test catches what it is supposed to catch).
- **Manual:** code review against `api.rs` to confirm every command name is read-shaped; add the result to the PR description.

## Files touched

```
crates/secunit-gui/src-tauri/src/sandboxing.rs
crates/secunit-gui/src-tauri/tests/readonly.rs
crates/secunit-gui/src-tauri/tauri.conf.json            (security review)
crates/secunit-gui/README.md
.github/PULL_REQUEST_TEMPLATE.md                        (if a template exists)
```
