# JOB-01 — Scaffold `secunit-gui`

## Goal

A new workspace member `crates/secunit-gui/` that builds a Tauri 2.x desktop app with a Vite/React/TypeScript frontend, Tailwind configured, Inter loaded as the UI font, and a hello-world window that opens.

## Deliverables

- `crates/secunit-gui/Cargo.toml` declared as workspace member in the root `Cargo.toml`. Defaults are off — workspace `cargo build` without `--all-features` does **not** pull in Tauri.
- `crates/secunit-gui/src-tauri/` (or Tauri 2 equivalent layout) with `main.rs`, `tauri.conf.json` (windowed app, fixed initial size 1280×800, resizable, title `secunit`), and an empty `tauri::Builder::default().run(...)` shell.
- `crates/secunit-gui/web/` Vite + React + TypeScript app:
  - `index.html` with `<html lang="en">`, dark/light auto.
  - `src/main.tsx`, `src/App.tsx` rendering a centered `<h1>secunit</h1>` and the project's `package.json#version`.
  - Tailwind configured via `tailwind.config.ts` and `postcss.config.cjs`. CSS variables for shadcn-style design tokens defined in `src/styles.css` (`--bg`, `--fg`, `--muted`, `--border`, `--ring`, status hues).
  - Inter loaded via `@fontsource/inter` (400/500/600). System mono fallback for `font-family: ui-monospace`.
  - Path alias `@/` → `web/src/`.
- `crates/secunit-gui/web/package.json` pinned to deterministic versions; lockfile committed.
- `crates/secunit-gui/README.md` — one paragraph + the dev/build commands.
- A `cargo xtask` is *not* required; document `npm --prefix crates/secunit-gui/web install && (cd crates/secunit-gui && cargo tauri dev)` in the README.

## Non-goals

- No IPC commands beyond the default `tauri::generate_handler![]` empty list.
- No `secunit-core` integration. That arrives in JOB-03.
- No views. The window shows the placeholder hello.
- No CI changes. CI continues to build the existing crates only.

## Acceptance criteria

- `cargo build -p secunit-gui` succeeds on Linux (Tauri 2 dependencies present).
- `cargo build` (no flags) on the workspace **still** succeeds and **does not** compile Tauri or its native deps.
- `npm --prefix crates/secunit-gui/web run build` produces `dist/` with `index.html`, hashed JS, hashed CSS.
- `cargo tauri dev` (or the equivalent Tauri 2 invocation) opens a window titled `secunit` containing the hello text rendered in Inter.
- `clippy -p secunit-gui -- -D warnings` is clean.
- `npm --prefix crates/secunit-gui/web run typecheck` passes.

## Test plan

- **Rust unit:** none — the Rust side is a stub.
- **Frontend unit:** Vitest test asserting `App` renders the literal "secunit".
- **Manual smoke:** open the window once on Linux; confirm Inter is the rendered font (not a system fallback) by checking the computed style of `<h1>` in DevTools.

## Files touched

```
Cargo.toml                                       (add workspace member)
crates/secunit-gui/Cargo.toml
crates/secunit-gui/src-tauri/main.rs
crates/secunit-gui/src-tauri/tauri.conf.json
crates/secunit-gui/src-tauri/build.rs
crates/secunit-gui/web/package.json
crates/secunit-gui/web/package-lock.json
crates/secunit-gui/web/tsconfig.json
crates/secunit-gui/web/vite.config.ts
crates/secunit-gui/web/tailwind.config.ts
crates/secunit-gui/web/postcss.config.cjs
crates/secunit-gui/web/index.html
crates/secunit-gui/web/src/main.tsx
crates/secunit-gui/web/src/App.tsx
crates/secunit-gui/web/src/styles.css
crates/secunit-gui/web/src/__tests__/App.test.tsx
crates/secunit-gui/README.md
```
