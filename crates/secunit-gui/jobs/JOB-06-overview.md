# JOB-06 — Overview view

## Goal

A landing view with health tiles and a last-N runs timeline, both driven entirely by `secunit-core` outputs.

## Deliverables

- `web/src/routes/Overview.tsx`:
  - Four tiles in a responsive grid:
    - **Overdue** — count of `due_rows().filter(overdue)`.
    - **Due this week** — count where `next_due ≤ today + 7d` and not overdue.
    - **In progress** — count of controls whose newest run is `.run-pending` (no manifest, no abort).
    - **Sealed last 30d** — count of manifests with `completed_at >= today − 30d`.
  - Each tile shows the count, a one-line caption, and clicks through to the relevant filtered view (e.g. Controls filtered by overdue).
- A timeline of the most recent 25 runs (sealed, aborted, pending) in reverse chronological order. Each row: control id (mono), short run id, status badge, relative time. Clicking opens the run in Evidence view.
- New IPC command `recent_runs(limit: usize) -> Vec<RunRow>` if not already covered by JOB-03's `list_runs`. (Spec keeps the API surface small — extend, do not duplicate.)

## Non-goals

- No charts. Counts and a list are enough for v1.
- No customisation of the time window.

## Acceptance criteria

- Against `testdata/orgs/multi-system/` with synthetic recent runs in `evidence/`, every tile shows a non-zero, hand-verifiable count.
- The timeline order matches `find evidence -name 'manifest.json' -newer $cutoff | sort -r` for sealed runs.
- Clicking the **Overdue** tile lands on `/controls?status=overdue` and the table filters accordingly. (Link wiring; the Controls view filtering itself ships with JOB-07 if not earlier.)
- Live update: dropping a new `manifest.json` into the fixture mid-session updates the **Sealed last 30d** count without a reload.

## Test plan

- **Rust unit:** `recent_runs` against the fixture; assert ordering and limit.
- **Frontend unit:** `Overview` rendered with mock data — four tile counts and a timeline of N rows.
- **Live-update integration:** with the watcher running, write a synthetic manifest into a tempdir copy of the fixture and assert the tile count rises within the debounce window.

## Files touched

```
crates/secunit-gui/src-tauri/src/api.rs                 (extend if needed)
crates/secunit-gui/web/src/routes/Overview.tsx
crates/secunit-gui/web/src/components/HealthTile.tsx
crates/secunit-gui/web/src/components/RunTimeline.tsx
crates/secunit-gui/web/src/__tests__/Overview.test.tsx
```
