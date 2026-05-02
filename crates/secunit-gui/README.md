# `secunit-gui`

A read-only Tauri 2 desktop app for inspecting `secunit` projects. Embeds
[`secunit-core`](../secunit-core) so registry parsing, cadence math, scope
resolution, and run-state derivation never leave the canonical Rust path.

The GUI **never writes** inside a project tree — see the spec's
"Read-only contract" and [`PLAN.md`](./PLAN.md). The CLI and direct git
edits remain the only paths that mutate state.

## Layout

```
crates/secunit-gui/
  package.json        # frontend at the crate root (Vite + React + TS + Tailwind)
  vite.config.ts
  tsconfig.json
  index.html
  src/                # frontend source
  src-tauri/          # Tauri Rust shell + config
    Cargo.toml        # workspace member; the Tauri shell crate
    src/              # Rust shell (lib + bin)
    capabilities/     # Tauri 2 permission set
    tauri.conf.json
    icons/            # bundle icon (placeholder; replace before release)
    tests/
  jobs/               # one JOB-XX-*.md per atomic commit
  PLAN.md
```

## Develop

Frontend deps:

```sh
pnpm --dir crates/secunit-gui install
```

Run the app in dev mode (Tauri spawns Vite via `beforeDevCommand`):

```sh
cd crates/secunit-gui
pnpm tauri:dev
```

The Tauri CLI ships as a devDependency, so `pnpm install` is enough to
provision it — no `cargo install` step required. The Rust shell can be
type-checked without launching the window:

```sh
cargo check -p secunit-gui
```

Frontend type-check and tests:

```sh
pnpm --dir crates/secunit-gui typecheck
pnpm --dir crates/secunit-gui test
```

## Build

```sh
pnpm --dir crates/secunit-gui build
cd crates/secunit-gui && cargo build --release
```

The default workspace `cargo build` deliberately **excludes** this crate
(see `default-members` in the root `Cargo.toml`) so headless CI and
core development do not pull in WebKitGTK / Tauri native deps.

## Linux native deps

```
libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libssl-dev pkg-config
```

## Read-only contract

The GUI never writes inside a project tree. The CLI (`secunit run …`,
`secunit registry import …`) and direct git edits are the only paths
that mutate state, so the hash-chained audit trail is unaffected.

Three guardrails enforce this:

1. **Allow-listed IPC surface.** Every `#[tauri::command]` in `src/api/`
   must appear in `tests/readonly.rs#ALLOWLIST`. The test fails if a
   new command is added without listing it (or if the list goes stale).
   Adding a command requires reviewing it against this contract in the
   PR.

2. **No write-shaped name.** A second test rejects names starting with
   `write_`, `create_`, `delete_`, `remove_`, `edit_`, `save_`, `set_`,
   `update_`, `patch_`, `mutate_`, `commit_`, `push_`. Catches slips
   during review.

3. **Capabilities deny fs writes.** `capabilities/default.json` is
   asserted to contain none of `fs:write`, `fs:create`, `fs:remove`,
   `fs:rename`, `fs:write-text-file`, `fs:write-binary-file`,
   `fs:allow-write`. The plugin set is `core:default`, `shell:default`,
   `dialog:default`, `opener:default` — none of which grants writes
   inside a project tree.

Path-typed commands (`get_run`, `read_findings`, `read_artifact`)
canonicalise the requested path and reject anything that lands outside
the project root.

Mutations to controls, schedule, inventory, evidence, or state always
go through `secunit` CLI invocations or direct git edits.
