---
name: bootstrap
description: Use when standing up or re-syncing the secunit registry from the WISP under security/. Walks every policy, extracts cadence-bearing obligations, and emits draft control YAMLs plus a kept/added/orphaned diff against the live controls/. Re-runnable as the WISP evolves. Read-only against live state — drafts go under the run dir for the operator to import.
requires_features: []
---

# Bootstrap registry from WISP

Derives the control registry from the policies in `security/`. It writes drafts;
the operator promotes them with `secunit registry import`. It never edits live
`controls/` directly.

## Procedure

1. **Walk the WISP.** For each `security/*-policy.md`, scan for cadence-bearing
   obligations — sentences with `daily | weekly | monthly | quarterly |
   semi-annual | annually | every N | by <month>`. Record source file + line.
2. **Map cadence.** daily/weekly → `weekly`; every-6-months → `semi-annual`;
   "annually by December 31" → `annual` + `due_by`. Pin a `weekday` for weekly.
3. **Map to a skill.** Capture-driven obligation (alerts, scans, logs, drift) →
   `capture-sweep`. Human-judgement obligation (review, test, attest) →
   `attestation-review`. Policy document review → `policy-annual-review`.
   Period report → `report`.
4. **Resolve scope.** Per-repo → `source_repo`; per-account → `cloud_account`;
   per-vendor → `saas`; per-site → `site`; else org-wide (omit scope).
5. **Emit drafts** under `raw/controls/<id>.yaml`, one per obligation, with
   `policy:` pointing at the source file and `nist:` from the policy's References.
6. **Diff against live.** Compare draft ids to `controls/*.yaml`:
   - **kept** — obligation still present, control unchanged.
   - **added** — new obligation with no control yet.
   - **orphaned** — live control with no matching obligation (policy changed?).
   Write the summary to `raw/bootstrap-report.md`.
7. **Stub missing registry files** (`raw/inventory.yaml`, `raw/schedule.yaml`,
   `raw/_config.yaml`) only if the live registry lacks them.
8. Drop `result.json`. The operator reviews `raw/bootstrap-report.md`, then runs
   `secunit registry import <run-dir>` to promote the drafts they accept.

## Anti-patterns

- Never write into live `controls/` — only `raw/` under the run dir.
- Don't drop an orphaned control silently; surface it so the operator decides retire vs keep.
- Stagger new annual policy reviews across the year via `due_by`; don't cluster them.
