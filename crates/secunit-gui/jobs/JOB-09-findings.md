# JOB-09 — Findings view

## Goal

A reverse-chronological feed of every `findings.md` produced under `evidence/`, rendered as safe HTML, filterable by control / system / quarter.

## Deliverables

- `web/src/routes/Findings.tsx`:
  - Two-pane layout. Left: filter rail. Right: feed.
  - Filters: control (multi-select), system (multi-select), quarter (single, default "all"), text (debounced free-text query that delegates to JOB-12's search index when present, else falls back to a substring filter on the loaded summaries).
  - Feed: card per `findings.md`, header (control, run, sealed/aborted badge, completed_at), body (rendered HTML), footer ("open in editor", "reveal in finder", "copy path").
  - Lazy-load: 20 cards initially; infinite scroll loads the next 20.
- Backend:
  - `read_findings(control_id, run_id) -> SafeHtml` from JOB-03 is the source of truth.
  - HTML produced by `pulldown-cmark` then run through `ammonia` with an allow-list (`p`, `h1..h6`, `ul`, `ol`, `li`, `a[href starting with https? or path:]`, `code`, `pre`, `strong`, `em`, `blockquote`, `table`, `thead`, `tbody`, `tr`, `th`, `td`, `hr`, `img[src starting with file:// path inside the run dir only]`).
  - `path:` URLs in `findings.md` are rewritten to a Tauri-safe `convertFileSrc` reference that points inside the run dir; everything else is stripped.

## Non-goals

- No edit. No comment threads.
- No syntax highlighting on code blocks (deferred — `prism` is a 100 KB dep we don't need yet).

## Acceptance criteria

- The fixture org's example findings render with headings, lists, and tables intact and no raw HTML escapes.
- A relative path in `findings.md` to `by-system/foo/raw/bar.json` opens that file via the editor IPC, not via the webview navigation.
- Filtering by control narrows the feed; clearing filters restores the full list.
- A new `findings.md` appearing in the fixture surfaces in the feed within the debounce window.
- HTML allow-list test: a `findings.md` containing `<script>` and `<iframe>` tags renders neither, with no console errors.

## Test plan

- **Rust unit (`tests/sanitiser.rs`):** every dangerous tag/attribute combination from the OWASP cheat sheet, plus a happy-path full-fidelity round-trip.
- **Frontend unit:** filter matrix; lazy-load triggers when the sentinel intersects.
- **Manual smoke:** browse the fixture findings; copy a path; confirm sysclipboard.

## Files touched

```
crates/secunit-gui/src-tauri/src/api.rs                 (read_findings)
crates/secunit-gui/src-tauri/src/sanitiser.rs
crates/secunit-gui/src-tauri/tests/sanitiser.rs
crates/secunit-gui/web/src/routes/Findings.tsx
crates/secunit-gui/web/src/components/FindingsFilters.tsx
crates/secunit-gui/web/src/components/FindingCard.tsx
crates/secunit-gui/web/src/__tests__/Findings.test.tsx
```
