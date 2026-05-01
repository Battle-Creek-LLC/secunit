# Implementation notes — `secunit-gui`

Decisions made while implementing the spec, plus questions for review at the end. Append-only. Each entry dated.

## Format

```
## YYYY-MM-DD — short title

**Context.** What prompted the note.
**Decision.** What we did.
**Why.** Reasoning, especially anything that diverges from the spec.
**Open question.** If a human should weigh in, mark with `Q:` and number sequentially.
```

## 2026-05-01 — Tauri 2 layout vs. spec

**Context.** The spec says "Tauri shell embeds `secunit-core`". Tauri 2.x (current, GA since late 2024) reorganised the per-app crate to live under `src-tauri/` with its own `Cargo.toml`. The workspace member is the outer crate.

**Decision.** Use Tauri 2.x. Crate root is `crates/secunit-gui/`. Frontend lives under `web/`. The Tauri Rust shell lives under `src-tauri/` inside the crate. The workspace member declared in the root `Cargo.toml` is `crates/secunit-gui` (which delegates to the inner shell crate).

**Why.** Tauri 2's API surface is closer to what the spec implies; backporting to v1 buys nothing.

**Open question.** None.

## 2026-05-01 — Frontend framework

**Context.** Spec is silent on the frontend stack.

**Decision.** Vite + React 18 + TypeScript + Tailwind. Component primitives modeled on shadcn/ui but hand-rolled (no copy of the component registry — too much surface for a read-only viewer).

**Why.** React/Tailwind is the path of least resistance for the shadcn aesthetic the user requested. Hand-rolling primitives keeps the surface small and avoids dragging in `@radix-ui/*` for components we may not need.

**Q1.** Should we instead use a Rust-native frontend (Dioxus / Leptos) to share more types between shell and UI? Default: no — the surface is small and React is faster to iterate. Revisit if the IPC boundary becomes a maintenance burden.

## 2026-05-01 — IPC type bindings: hand-rolled instead of ts-rs

**Context.** JOB-03 originally called for `ts-rs`-generated TypeScript bindings checked in under `web/src/lib/bindings/`. Adopting it requires `#[derive(TS)]` macros across `secunit-core` types (or wrapper newtypes) and a build-time generation step.

**Decision.** Hand-roll the TypeScript shapes in `web/src/lib/ipc.ts` mirroring the Rust types. Drift is caught by the `tests/api_smoke.rs` integration test plus type-level rejection at runtime if the JSON shape ever shifts.

**Why.** The IPC surface is ~17 commands and ~25 types — small enough to author by hand without a generator. Adding `ts-rs` to `secunit-core` would change a crate the CLI depends on; the GUI is supposed to be optional. Trade-off accepted.

**Q2.** If/when the surface doubles, revisit ts-rs (likely behind a wrapper crate `secunit-gui-types` so `secunit-core` stays unchanged).

## 2026-05-01 — Search index: cold rebuild on load_project

**Context.** The spec says the Tantivy index is "patched by the same `notify` events that drive the live UI". JOB-12 did the build but skipped per-event patching.

**Decision.** Build the entire index on `load_project`; do not patch on watcher events. The view jobs already re-fetch via the IPC layer when the relevant watcher event fires, so search staleness only matters until the next `load_project` (or until the user manually re-opens the project).

**Why.** Per-event patching needs deletion-by-term semantics and care to avoid duplicate docs across re-runs. Cold rebuild against the multi-system fixture is well under the spec's 500 ms budget on a developer laptop. The escape hatch in the spec — swap to `MmapDirectory` keyed by inventory git sha — is the right call only if real-org cold-start becomes uncomfortable.

**Q3.** Worth wiring incremental patching now, or wait until we have a real-org corpus to measure against?

## 2026-05-01 — Evidence/Inventory view ergonomics deferred

**Context.** JOB-10 ships a tree + summary + preview, but the `image` and `too-large` branches show metadata only — no inline image preview, no "head -N" for big logs.

**Decision.** Ship the safer/cheaper version. Operators can open the artifact in the editor or reveal in finder via the Tauri `opener` plugin (already in capabilities). Add inline image and tail-of-log preview only if it surfaces as a real friction point.

**Q4.** Is "open in editor" already wired to the operator's preferred editor, or do we need a per-OS "$EDITOR" + fallback? Today we only call `tauri-plugin-opener`'s `open_path`, which uses the OS default association. That's the spec's described behaviour but worth confirming.

## End-of-session open questions

Punted to here per the user's "save questions / changes for the end" instruction. None were blocking; each is a follow-up:

- **Q1**: React vs. Dioxus/Leptos for the frontend (probably stay on React).
- **Q2**: ts-rs adoption when the IPC surface grows.
- **Q3**: Incremental Tantivy patching on watcher events.
- **Q4**: Editor invocation behaviour against `$EDITOR`.
- **Q5**: Should JOB-08's `next_due_with_reason` push down into `secunit-core::registry::resolver`? Today the GUI derives reasons by inspecting the schedule directly; if the CLI ever wants the same enrichment (`secunit due --why`), the resolver is the right home.
- **Q6**: Tauri 2's window security policy (CSP) currently allows `style-src 'self' 'unsafe-inline'` because Tailwind injects style elements at runtime under `react-router`'s scrollRestoration. Tightening CSP is a hardening pass; tracked but not done.
- **Q7**: The Linux build pulls `webkit2gtk-4.1`. macOS and Windows still need a smoke build before any release; the CI workflow will need a matrix.

Each of these is a small follow-up commit, not a blocker on the v1 ship.
