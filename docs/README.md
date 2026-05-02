# secunit docs

Specification and reference shapes for `secunit`, the operational layer that drives a Written Information Security Program (WISP) on a recurring schedule via agent skills.

`secunit` boots from an existing WISP via the `bootstrap` skill, which walks the policy/procedure documents and emits a draft registry. An accompanying `inventory-seed` skill populates `inventory.yaml` with the in-scope systems (repos, cloud accounts, SaaS, sites). The schema, storage layout, and skill contract documented here are the same shape across orgs; per-org details live in that org's own `controls/`, `skills/`, `inventory.yaml`, and `_config.yaml`.

## Documents

- [`spec.md`](spec.md) — what `secunit` is, the concepts (control, inventory, skill, schedule, evidence, state), the runtime architecture (single Rust binary, two-phase run model, capture commands), the workflow, multi-system runs, and what is in/out of scope.
- [`cli.md`](cli.md) — full CLI reference: subcommands, flags, output modes, exit codes, cargo features, end-to-end session.
- [`skills.md`](skills.md) — how skills work, the skill contract, the multi-system iteration pattern, the input/output structure passed between agent and skill, `requires_features` declaration.
- [`storage.md`](storage.md) — the on-disk layout, run-dir lifecycle, inventory schema, scope resolution rules, cadence resolution rules, manifest hash chaining, file-naming conventions.

## Examples

Under [`examples/`](examples/):

- [`inventory.yaml`](examples/inventory.yaml) — in-scope systems, organized by kind with tags and lifecycle dates.
- `controls/` — reference control YAMLs spanning weekly, quarterly, annual, multi-system, and policy-review cadences:
  - [`aa-weekly-audit-review.yaml`](examples/controls/aa-weekly-audit-review.yaml) — single-system (resolves to one cloud account)
  - [`sca-weekly-dependency-scan.yaml`](examples/controls/sca-weekly-dependency-scan.yaml) — multi-system, iterates source repos
  - [`ca-quarterly-vuln-scan.yaml`](examples/controls/ca-quarterly-vuln-scan.yaml) — scope by cloud-account tag
  - [`ac-annual-access-review.yaml`](examples/controls/ac-annual-access-review.yaml) — scope by SaaS, all entries
  - [`cp-annual-bcp-test.yaml`](examples/controls/cp-annual-bcp-test.yaml) — scope declared inline
  - [`policy-annual-review-access-control.yaml`](examples/controls/policy-annual-review-access-control.yaml) — org-wide, no scope
- `skills/` — two reference skill markdowns showing the canonical shapes:
  - [`aa-weekly-audit-review.md`](examples/skills/aa-weekly-audit-review.md) — control-specific, single-system
  - [`sca-weekly-dependency-scan.md`](examples/skills/sca-weekly-dependency-scan.md) — control-specific, multi-system iteration
  - [`policy-annual-review.md`](examples/skills/policy-annual-review.md) — reusable across all 16 policy reviews
- [`schedule.yaml`](examples/schedule.yaml) — date overrides, skip and insert directives.
- [`state.json`](examples/state.json) — last-run pointer per control.
- Two example evidence runs demonstrating both layouts:
  - `evidence/2026/q2/aa-weekly-audit-review/2026-05-01-run-001/` — **flat layout** (single-system resolution): [manifest](examples/evidence/2026/q2/aa-weekly-audit-review/2026-05-01-run-001/manifest.json), [findings](examples/evidence/2026/q2/aa-weekly-audit-review/2026-05-01-run-001/findings.md)
  - `evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001/` — **by-system layout** (multi-system iteration): [manifest](examples/evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001/manifest.json), [findings](examples/evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001/findings.md), per-system raw artifacts under `by-system/<name>/raw/`
- [`reports/2026-q1-quarterly.md`](examples/reports/2026-q1-quarterly.md) — a generated quarterly report.

## How to read these in order

1. `spec.md` first — the conceptual model, including the runtime architecture, inventory, and multi-system runs.
2. `cli.md` — what the binary actually exposes; this anchors the rest.
3. `examples/inventory.yaml` — see what an in-scope set looks like.
4. Browse `examples/controls/` to see how controls reference inventory via `scope:`.
5. Read `examples/skills/sca-weekly-dependency-scan.md` to see how a skill iterates resolved scope and invokes `secunit capture` per system.
6. Walk the multi-system evidence run end-to-end (manifest → findings → per-system raw artifacts).
7. Compare against the single-system flat run for `aa-weekly-audit-review` to see the alternate layout.
8. Read the generated quarterly report to see what those evidence runs aggregate into.
9. Finish with `storage.md` and `skills.md` for the precise on-disk and contract details when authoring real registry entries.
