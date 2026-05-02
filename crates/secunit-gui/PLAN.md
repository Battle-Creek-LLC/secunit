# `secunit-gui` Implementation Plan

How to take the [Viewer (Tauri GUI)](../../docs/spec.md#viewer-tauri-gui) section of the spec from text to a working desktop app, in atomic, reviewable jobs.

The GUI is **read-only**. The CLI and direct git edits remain the only paths that mutate state. Every job here must preserve that contract.

## Repo placement

```
crates/
  secunit-gui/
    PLAN.md                   # this file
    jobs/JOB-XX-*.md          # one file per atomic commit
    package.json              # frontend (Vite + React + TS + Tailwind) at crate root
    vite.config.ts
    tsconfig.json
    index.html
    src/                      # frontend source
    src-tauri/                # Tauri Rust shell + config (workspace member)
      Cargo.toml
      src/                    # Rust shell (lib + bin)
      tauri.conf.json
      capabilities/
```

The crate name is `secunit-gui`. The binary built by Tauri is also `secunit-gui`. It is gated behind a default-off cargo feature on the workspace so headless CI (which already builds `secunit-cli`) is unaffected by Tauri/WebKit deps until the GUI is opted into explicitly.

## Architectural rules (from the spec)

1. The Tauri shell embeds `secunit-core` as a library — same crate the CLI uses. **No shelling out to the CLI; no logic duplicated on the frontend.**
2. **One** `notify` watcher per open project, debounced, emits typed events to the webview.
3. The webview keeps a reactive in-memory index keyed by `control_id` and `run_id`.
4. All status derivation (overdue / due / pending / sealed / aborted) happens in `secunit-core`.
5. `⌘K` palette is backed by a Tantivy `RamDirectory` index built on project open and patched by the same `notify` events.
6. Project list lives in `~/.config/secunit-gui/projects.yaml`.
7. Read-only contract: the only mutating-looking actions allowed are *open in editor*, *reveal in finder*, *copy path* — none of which touch the project tree.

## Visual system

- **Font.** Inter for UI text. JetBrains Mono (or system mono) for IDs, hashes, and paths.
- **Style.** shadcn-flavoured: neutral palette (zinc/slate), 1px borders at low contrast, rounded-md cards, subtle focus rings, generous whitespace, no gradients. Status colour is the only chroma — overdue red, due-soon amber, sealed/complete green, in-progress blue, neutral grey for everything else.
- **Components.** A small handwritten set (Button, Card, Badge, Table, Input, ScrollArea, Dialog, CommandPalette) modeled on shadcn primitives but cut down to what these views need. No ad-hoc Tailwind in views — go through the primitives.
- **Density.** Operator tool, not a marketing site. Tables show counts of rows on screen; sidebars are narrow; type tops out around 14px for body, 12px for metadata.

## Phases

| Phase | Jobs | Outcome |
|---|---|---|
| Foundation | 01–04 | Crate exists; window opens; project config loaded; live watcher wired; IPC reaches `secunit-core`. |
| Views | 05–11 | Six spec views render real data from a real project tree. |
| Polish | 12–13 | ⌘K palette indexes the project; read-only contract is audited and tested. |

Each job is a single commit with its own acceptance criteria and test plan. If a job grows beyond reasonable, split it and update the next job's number.

## Test strategy

- **Rust (cargo test).** Unit tests on every IPC handler against a fixture org under `testdata/orgs/multi-system/`. Integration test that boots the watcher, mutates a fixture file, and asserts the typed event arrives within the debounce window.
- **TypeScript.** Vitest unit tests on the reactive store reducers and the search-result shaping. No DOM tests in v1 — visual review is faster.
- **End-to-end (deferred).** A Playwright run over a packaged build is on the roadmap but not required to land any of the jobs below. Manual smoke against `testdata/orgs/multi-system/` is the gate.
- **Read-only enforcement.** A Rust test enumerates every registered IPC command and asserts the handler set is a subset of the allow-list (`load_project`, `list_*`, `get_*`, `search`, `open_in_editor`, `reveal_in_finder`, `copy_path`, …). New write-shaped command name → test fails.

## Risks

| Risk | Why it bites | Mitigation |
|---|---|---|
| WebKitGTK / native deps slow down Linux CI | `cargo build` for the workspace becomes 10× slower | Feature-gate Tauri so default workspace builds skip it |
| `notify` debounce eats real events under fast batched edits | UI goes stale until next manual reload | 200ms trailing debounce + integration test exercising bursty changes |
| Cross-platform path handling (case sensitivity, separators) | Paths from Rust look wrong in JS or vice versa | Always pass paths as strings produced by `Path::display()` from Rust; never reconstruct in JS |
| Tantivy index gets large on big findings corpora | Cold-start regression on real org repos | RamDirectory now; spec-blessed swap to MmapDirectory keyed by inventory git sha if it bites |
| Markdown rendering hits XSS / file-URL exfil | Findings come from skill output, but still reasoned-about content | Rust-side sanitiser (`pulldown-cmark` → safe HTML allow-list) instead of `dangerouslySetInnerHTML` against raw text |

## Open questions (raised at end)

Anything ambiguous or worth pushing back on lands at the bottom of [`NOTES.md`](./NOTES.md), not in commit messages.
