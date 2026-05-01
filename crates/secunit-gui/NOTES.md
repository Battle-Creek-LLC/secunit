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
