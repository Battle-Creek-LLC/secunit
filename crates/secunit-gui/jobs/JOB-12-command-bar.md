# JOB-12 — Command bar (`⌘K`) with Tantivy `RamDirectory`

## Goal

A palette that searches across controls, runs, findings text, inventory entries, and artifact filenames. Result kinds are typed and grouped. `↵` opens in the right pane; `⌘↵` reveals on disk.

## Deliverables

- `crates/secunit-gui/src-tauri/src/search.rs`:
  - Tantivy index in `RamDirectory`.
  - Schema: `kind` (string), `id` (string), `title` (text, boost=4), `tags` (text, boost=3), `body` (text, boost=1), `path` (string), `mtime` (i64), `status` (string).
  - Built on `select_project` after the registry loads. Patched on each `notify` event (re-index the affected docs, never the whole corpus).
  - Query: BM25 with the field boosts above; tokenized with the default tokenizer (no stemmer for v1 — operator queries are short and literal).
- IPC commands:
  - `search(query: String, limit: usize, kinds?: Vec<String>) -> Vec<SearchHit>`.
  - `index_status() -> { ready: bool, doc_count: usize, last_updated: DateTime<Utc> }`.
- Frontend `web/src/components/CommandPalette.tsx`:
  - Triggered by `⌘K` / `Ctrl+K` from anywhere.
  - Results grouped by kind: Controls, Runs, Findings, Inventory, Artifacts.
  - Keyboard: `↑/↓` move selection across groups; `↵` opens in the right view; `⌘↵` reveals on disk; `Esc` closes.
  - Empty query state shows three sections: "Recent" (last 5 viewed entities), "Overdue" (top 5 from `due_rows`), "Quick actions" (open inventory, open WISP, switch project).

## Non-goals

- No persistent on-disk index. `MmapDirectory` is the spec-blessed escape hatch only if cold start gets uncomfortable.
- No fuzzy matching beyond what BM25 + the default tokenizer give. Trigram fuzz is a future enhancement.

## Acceptance criteria

- Indexing a fixture project cold-starts in under 500 ms on a developer laptop. (Rough budget — we re-evaluate against a real-org corpus later.)
- A query for an inventory entry's exact name returns it as the top hit.
- A query for a substring of a `findings.md` heading returns the corresponding finding within the top 5.
- Editing a control's title surfaces the new title in the next palette query within the debounce window.
- `⌘↵` on a result invokes `reveal_in_finder` against the canonical path.

## Test plan

- **Rust unit (`tests/search.rs`):** index against the fixture; assert ordering for several known queries; assert patching after a single doc update changes ordering as expected.
- **Frontend unit:** keyboard nav across grouped results; empty state renders the three sections.
- **Manual smoke:** open the palette, type a control id substring, see it surface as the top Controls hit.

## Files touched

```
crates/secunit-gui/src-tauri/src/search.rs
crates/secunit-gui/src-tauri/src/main.rs              (build on select_project)
crates/secunit-gui/src-tauri/tests/search.rs
crates/secunit-gui/web/src/components/CommandPalette.tsx
crates/secunit-gui/web/src/lib/keys.ts
crates/secunit-gui/web/src/__tests__/CommandPalette.test.tsx
```
