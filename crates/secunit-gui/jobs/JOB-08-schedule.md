# JOB-08 — Schedule view

## Goal

A calendar / list view of `next_due` per control with `schedule.yaml` overrides pinned and explained.

## Deliverables

- `web/src/routes/Schedule.tsx`:
  - Two tabs: **Calendar** (default) and **List**.
  - **Calendar** — a 12-month grid covering today − 1 month → today + 11 months. Each day cell shows pills for controls due that day (mono id, status hue). Hover → control + reason; click → Controls detail pane with that control selected.
  - **List** — flat reverse-chronological table of upcoming firings: date, control id, reason (`cadence` / `override-due` / `override-insert` / `override-skip` / `override-weekday`), note (if any).
  - Override badges: every row whose source is `schedule.yaml` shows a `pinned` badge with the override reason on hover.
- New IPC command `schedule_horizon(window_days: i64) -> Vec<ScheduleEntry>` returning every firing within the window, with reason tags above. Built on the existing resolver — reasons surface from a small audit hook in `next_due` (or a new `next_due_with_reason` sibling that returns both).

## Non-goals

- No iCal export, no .ics. Future.
- No long-range projection past 12 months. Annual cadences typically pin on `due_by`; horizon is a usability call.

## Acceptance criteria

- For the fixture org on a fixed `today`, the visible firings match a hand-computed list.
- An `insert` override appears on its date with reason `override-insert`.
- A `skip { quarter: 2026-q3 }` override removes the relevant cadence firing and shows a faint `skipped` chip in the empty cell on hover.
- Editing `schedule.yaml` while the view is open updates the calendar within the debounce window.

## Test plan

- **Rust unit:** `schedule_horizon` against the fixture for `today = 2026-05-01`, window 365 days. Assert exact list of dates per control.
- **Frontend unit:** Calendar grid renders correct number of cells across DST boundary; reason badges appear in the right places.
- **Manual smoke:** add a one-off `insert` to `schedule.yaml`, watch it appear on the right day.

## Files touched

```
crates/secunit-gui/src-tauri/src/api.rs                 (schedule_horizon)
crates/secunit-core/src/registry/resolver.rs            (next_due_with_reason)
crates/secunit-gui/web/src/routes/Schedule.tsx
crates/secunit-gui/web/src/components/Calendar.tsx
crates/secunit-gui/web/src/components/ScheduleList.tsx
crates/secunit-gui/web/src/__tests__/Schedule.test.tsx
crates/secunit-core/tests/resolver_reasons.rs
```
