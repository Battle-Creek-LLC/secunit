---
name: report
description: Use when a secunit control names skill `report` (e.g. ca-quarterly-program-status), or when the operator asks for a quarterly/annual security program report. Read-only — aggregates prior run evidence, state, and risk links into a leadership-facing report under reports/. Never captures or mutates. Trigger when a run dir is allocated or when explicitly asked to assemble a report.
requires_features: []
---

# Program report

Assembles a leadership-facing status report from evidence already on disk. This
skill reads; it never captures. The binary produces the data; the agent writes
the prose.

## Inputs

- `run_dir`, `control`.
- `skill_args.kind` — `quarterly` | `annual`.
- `skill_args.sections[]` — which sections to include (e.g. `program-status`,
  `open-risks`, `training-status`, `kpis`, `steering-committee`).

## Procedure

1. **Pull the data.** Run `secunit report data` for the period:
   - quarterly → `secunit report data --quarter <YYYY-Qn> --out raw/report-data.json`
   - annual → `secunit report data --year <YYYY> --out raw/report-data.json`
   This aggregates per-control activity, manifests, state, overdue controls, and
   risk-register links.
2. **Read supporting state** as needed: `secunit status --json` for current
   coverage, `secunit due --within 90d --json` for what's upcoming.
3. **Compose the report** for each requested section (template below). Title it
   with `org.name` from `_config.yaml`. Pull open-risk and training-status numbers
   from the period's findings; don't invent figures — cite the run that produced
   each number.
4. **Write** the report to `reports/<YYYY>-<qN>-quarterly.md` (or
   `reports/<YYYY>-annual.md`) **and** copy it to `run_dir/raw/quarterly-status.md`
   so it's part of this control's evidence.
5. Note any control that is overdue or never run as a gap. Drop `result.json`.

## Report template

```markdown
# <org.name from _config.yaml> security program — <period>

## Program status
- Controls: <run on time> / <total>. Overdue: <list or none>.
- Coverage by family (AU, CA, SI, CP, …): <one line each>.

## Open risks
<Table from the period's draft_risks / risk-register links: subject, severity, SLA, status.>

## Training status
<Annual training complete? Acknowledgements current?>

## KPIs
<Findings opened/closed, mean time to remediate vs SLA, drift incidents.>

## Steering committee notes
<Decisions, exceptions accepted, priorities for next period.>

## Upcoming
<Controls due in the next period, especially staggered annual policy reviews.>
```

## Anti-patterns

- Never run a `secunit capture` here — reports read existing evidence only.
- Don't state a number you can't trace to a run. Cite the evidence path.
- Don't mark a control "done" that has no finalized run this period.
