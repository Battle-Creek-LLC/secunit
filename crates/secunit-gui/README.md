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
  Cargo.toml          # workspace member; the Tauri shell crate
  src/                # Rust shell (lib + bin)
  capabilities/       # Tauri 2 permission set
  tauri.conf.json
  icons/              # bundle icon (placeholder; replace before release)
  web/                # frontend: Vite + React + TypeScript + Tailwind
  jobs/               # one JOB-XX-*.md per atomic commit
  PLAN.md
```

## Develop

Frontend deps:

```sh
npm --prefix crates/secunit-gui/web install
```

Run the app in dev mode (Tauri spawns Vite via `beforeDevCommand`):

```sh
cd crates/secunit-gui
cargo tauri dev
```

If you don't have `cargo-tauri` installed, `cargo install tauri-cli`
fixes that. The Rust shell can be type-checked without launching the
window:

```sh
cargo check -p secunit-gui
```

Frontend type-check and tests:

```sh
npm --prefix crates/secunit-gui/web run typecheck
npm --prefix crates/secunit-gui/web test
```

## Build

```sh
npm --prefix crates/secunit-gui/web run build
cd crates/secunit-gui && cargo build --release
```

The default workspace `cargo build` deliberately **excludes** this crate
(see `default-members` in the root `Cargo.toml`) so headless CI and
core development do not pull in WebKitGTK / Tauri native deps.

## Linux native deps

```
libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libssl-dev pkg-config
```
