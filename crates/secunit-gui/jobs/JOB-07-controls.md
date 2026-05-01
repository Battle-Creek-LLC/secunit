# JOB-07 — Controls view

## Goal

A table of every control with status badge; row click opens a detail pane on the right with recent runs and metadata.

## Deliverables

- `web/src/routes/Controls.tsx`:
  - Two-pane layout. Left: searchable, filterable table. Right: detail pane (or empty state).
  - Columns: id (mono), title, cadence, owner, next_due (relative + absolute on hover), status badge, last run (relative).
  - Filters along the top: status (all / overdue / due-soon / pending / sealed / never-run), cadence (all / weekly / monthly / quarterly / semi-annual / annual / scheduled / continuous).
  - Filters reflect URL search params so deep links from Overview tiles work.
  - Sort: by next_due ascending by default; column-header click toggles. Stable secondary sort on id.
- Detail pane (right):
  - Card with title, owner, policy reference (linkable — opens in editor via `open_in_editor` IPC), cadence, next_due, NIST tags.
  - "Recent runs" sub-section with the last five runs from `list_runs(control_id)`. Each links into Evidence.
  - "Scope (today)" sub-section listing resolved systems by name and kind.
- Status badge logic lives **in Rust** (`secunit-core`-backed `ControlSummary.status`); the frontend renders the enum, no derivation.

## Non-goals

- No editing. Read-only.
- No bulk actions.

## Acceptance criteria

- The table matches `secunit due` output against the same fixture and `today`.
- URL `?status=overdue` lands on the overdue subset and the chip filter reflects it.
- Selecting a row updates the detail pane and the URL hash so the selection is preservable across reloads.
- Live update: editing a control yaml in the fixture changes the title in the table within the debounce window without a reload.
- Keyboard: `↑/↓` move selection; `↵` opens detail; `Esc` clears selection.

## Test plan

- **Frontend unit:** `Controls` mounted with mock `ControlSummary[]`; filter matrix gives expected row counts.
- **Snapshot:** detail pane for one control with three runs.
- **Manual smoke:** same as the spec example — open the fixture, every cadence shows expected next_due.

## Files touched

```
crates/secunit-gui/web/src/routes/Controls.tsx
crates/secunit-gui/web/src/components/ControlsTable.tsx
crates/secunit-gui/web/src/components/ControlDetail.tsx
crates/secunit-gui/web/src/components/StatusBadge.tsx
crates/secunit-gui/web/src/__tests__/Controls.test.tsx
```
