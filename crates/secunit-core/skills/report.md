---
name: report
description: Use when a secunit control names skill `report` (e.g. rp-weekly-status, ca-quarterly-program-status), or when the operator asks for a weekly/monthly/quarterly/annual security program report. Read-only over evidence — aggregates prior run data, state, and risk links into a stakeholder-facing report under reports/, and optionally publishes it as an issue in the org's tracker per `_config.yaml`. Never captures or mutates evidence. Trigger when a run dir is allocated or when explicitly asked to assemble a report.
requires_features: []
---

# Program report

Assembles a stakeholder-facing status report from evidence already on disk.
This skill reads; it never captures. The binary produces the data; the agent
writes the prose — and, when publishing is configured, files the report as a
tracker issue using its own tooling. The binary has no tracker integration.

## Inputs

- `run_dir`, `control`.
- `skill_args.kind` — `weekly` | `monthly` | `quarterly` | `annual`.
- `skill_args.sections[]` — which sections to include (e.g. `program-status`,
  `open-risks`, `training-status`, `kpis`, `steering-committee`).
- `skill_args.publish` — `true` to publish per `_config.yaml` (below);
  omitted or `false` means write to `reports/` only.

## Procedure

1. **Pull the data.** The run's `period_id` (in `prepare.json`) is the period
   selector — a weekly control claims `2026-W30`, monthly `2026-07`, and so
   on. When reporting on the *previous* period (the usual case for a report
   run early in a new week/month), the operator prepares the run with
   `--period <prior-period>`. Write the data *into the run dir* — commands
   run from the store root, so a bare `raw/` would land outside the run and
   the numbers would never become hash-chained evidence:
   - weekly → `secunit report data --week <YYYY-Wnn> --out <run_dir>/raw/report-data.json`
   - monthly → `secunit report data --month <YYYY-MM> --out <run_dir>/raw/report-data.json`
   - quarterly → `secunit report data --quarter <YYYY-qn> --out <run_dir>/raw/report-data.json`
   - annual → `secunit report data --year <YYYY> --out <run_dir>/raw/report-data.json`
   This aggregates per-control coverage, sealed runs, overdue controls, the
   risk-register delta, and what's due next.
2. **Read supporting state** as needed: `secunit status --json` for current
   coverage, `secunit due --within 90d --json` for what's upcoming.
3. **Compose the report** for the kind (templates below). Title it with
   `org.name` from `_config.yaml`. Don't invent figures — every number must
   trace to `report-data.json` or a run's evidence path.
4. **Write** the report to `reports/` **and** copy it to the run dir so it's
   part of this control's evidence:
   - weekly → `reports/<YYYY-Wnn>-weekly.md`, copy `run_dir/raw/weekly-status.md`
   - monthly → `reports/<YYYY-MM>-monthly.md`, copy `run_dir/raw/monthly-status.md`
   - quarterly → `reports/<YYYY>-<qn>-quarterly.md`, copy `run_dir/raw/quarterly-status.md`
   - annual → `reports/<YYYY>-annual.md`, copy `run_dir/raw/annual-status.md`
5. Note any control that is overdue or never run as a gap (`overdue` and
   per-control `gaps` in the data call these out). If `risks.register_errors`
   is non-empty, put it in the headline: the risk counts understate the
   register, a broken log may mean evidence was altered, and the operator
   must investigate — never omit or soften this.
6. **Publish** if `skill_args.publish` is true — see below.
7. Drop `result.json`.

## Publishing (optional)

Publishing targets live in `_config.yaml`, not in the binary:

```yaml
report:
  publish:
    target: gitlab            # gitlab | linear | none
    gitlab:
      project: group/project  # where the issue is filed
      labels: [security-report]
    linear:
      team: SEC
      labels: [security-report]
```

Use your own tooling for the configured target — e.g. `glab issue create
--repo <project> --title "<title>" --description-file <report>` for GitLab,
or the Linear API/MCP for Linear. Issue title: `<org.name> security status —
<period label>`. Issue body: the composed report verbatim.

Record the created issue in `result.json` so it lands in the manifest's
`external_links` and the report stays traceable from the evidence chain:

```json
"external_links": [
  { "system": "gitlab", "kind": "report-issue",
    "id": "42", "url": "https://gitlab.com/group/project/-/issues/42" }
]
```

If issue creation fails (no auth, no network), still finalize the run with
the report as evidence, set `status: partial`, and say why in `findings.md`
— a written-but-unpublished report is a partial success, not a failure.

## Weekly / monthly template

Stakeholder-brief: one screen, numbers first.

```markdown
# <org.name> security status — <period label>

## Headline
<2-3 sentences: cadence held or not, notable findings, incidents or none.>

## Control activity
| Control | Cadence | Status | Notes |
|---|---|---|---|
<one row per control in the data: satisfied/open/gap/late + run notes>

## New findings
<Findings from this period's runs, or "none". Cite the run that produced each.>

## Risk register
- Open: <n> (<n> past SLA). Opened this period: <n>. Reopened: <n>. Closed: <n>.
<Table of open risks above threshold: id, severity, title, owner, due.>

## Upcoming
<Controls due through `period.horizon`, from `upcoming` in the data —
cite the horizon date as the section's through-date.>
```

## Quarterly / annual template

```markdown
# <org.name from _config.yaml> security program — <period>

## Program status
- Controls: <run on time> / <total>. Overdue: <list or none>.
- Coverage by family (AU, CA, SI, CP, …): <one line each>.

## Open risks
<Table from the period's risk register: subject, severity, SLA, status.>

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
- Don't put anything in a published issue that isn't in the report file —
  the issue is a mirror of the evidence, not a second document.
