# secunit — Specification

`secunit` is the operational layer for an organization's Written Information Security Program (WISP). It turns the policies, procedures, and review cycles defined in a WISP into a tracked, evidence-backed schedule of recurring activities.

It is designed to be **agent-paired**: the agent reads the registry, executes each control's runbook through a dedicated **skill**, captures evidence, files findings, and updates state. Workflows live in skills; the binary stays narrow.

`secunit` is delivered as a single Rust binary the agent invokes for filesystem-level chores — registry inspection, scope resolution, run-directory allocation, hashing, manifest assembly, hash-chain verification, and native evidence capture. The binary never invokes the agent.

`secunit` boots from an existing WISP via a `bootstrap` skill that walks the policy/procedure documents, extracts cadence-bearing obligations, and emits a draft registry. The same skill is re-runnable to keep the registry in sync as the WISP evolves.

## Goals

1. Make every WISP-mandated activity discoverable as a discrete **control** with an owner, cadence, runbook, scope, and evidence requirements.
2. Track completion against the schedule so nothing in the WISP silently lapses.
3. Capture **tamper-evident evidence** for assessors (SOC2, customer security questionnaires, pentest assessors).
4. Produce the artifacts the WISP itself promises — monthly control assessment summaries, quarterly leadership reports, annual policy review status, risk register snapshots.
5. Avoid hard tooling dependencies. The store is files. The workflows are agent skills. Integrations (issue trackers, ticketing systems, cloud APIs) are optional and live inside skills, not the core.

## Non-goals

- Replace the WISP. The policies in the org's WISP repo remain the source of truth.
- Replace the org's existing issue tracker. Risk registers and access tracking continue to live wherever the WISP says they live; `secunit` references those records by URL, not by API.
- Be a daemon, server, or scheduled job. It is a static registry plus an agent that walks it on demand.
- Ship a custom CLI. A thin convenience wrapper may be added later, but the canonical interface is "an agent reading the registry."

## Concepts

### Control

A discrete, recurring obligation derived from a WISP. One YAML file per control under `controls/`. A control declares:

- `id` — kebab-case, stable, used as evidence path component
- `policy` — relative path or URL to the policy/procedure that mandates the control
- `nist` — NIST control identifiers (`AU-6`, `CA-2`, etc.) for traceability, when applicable
- `owner` — role responsible (e.g. `cto`, `owner`, `bct`)
- `cadence` — `continuous | weekly | monthly | quarterly | semi-annual | annual | scheduled`
- `due` — ISO date(s) for `scheduled` cadence; derived from cadence otherwise
- `skill` — name of the agent skill that executes the runbook
- `scope` — what to iterate over from the inventory (kind + tag filter, or `all`); omit if org-wide
- `evidence_required` — list of expected evidence artifacts (`kind`, optional `path`/`prompt`/`cmd`)
- `outputs` — what the control produces (findings file, risk entries, report section)
- `references` — links into the WISP source

See `examples/controls/` for shapes.

### Inventory

Most controls operate over a set of systems — repos, cloud accounts, SaaS providers, physical sites. `inventory.yaml` is the single source of truth for what's in scope. The `inventory-seed` skill (run alongside `bootstrap`) populates it from the org's GitHub, cloud accounts, and the WISP's own access dictionaries.

Each entry carries a `name`, a `kind`, optional `tags`, and lifecycle dates (`in_scope_since`, `retired_on`). Controls reference inventory by **kind + tag filter**, never by name, so onboarding a new system is one inventory edit and every relevant control automatically picks it up.

See `storage.md` for the full schema and `examples/inventory.yaml` for shape.

### Skill

A markdown file in the Claude Code Skills format that encapsulates the workflow for one control (or a family of related controls). Skills:

- Are the **only** place that knows how to actually gather evidence — what cloud calls to make, what dashboards to check, what files to grep, what humans to ask.
- Are invoked by the agent when starting a control session, with the resolved scope passed in.
- Iterate over the resolved scope, capturing per-system evidence under `by-system/<name>/`.
- Produce a single rollup `findings.md` with per-system sections.
- Can be edited freely without changing control YAML or core schema.

This separation means the registry stays declarative and stable; the procedures evolve as the environment evolves.

See `examples/skills/` for shapes.

### Schedule

Cadence is normally derived (weekly = a chosen weekday, quarterly = first business day of quarter, etc. — exact rules in `storage.md`). `schedule.yaml` is an override file used for:

- Specific dated activities mandated by the WISP (e.g. a procedure that fixes vulnerability audits to particular months of the year).
- One-off slips, postponements, or insertions.

### Evidence

Every control execution writes a **run directory** under `evidence/<year>/<quarter>/<control-id>/<run-id>/` containing:

- `manifest.json` — run metadata, agent identity, git sha of the registry, git sha of the inventory, hashes of every artifact, per-system status.
- `findings.md` — the agent's narrative summary, anomalies, recommended risk-register entries; per-system sections when scope iterates.
- `by-system/<name>/raw/` — captured artifacts per resolved scope entry.
- `raw/` — used when a control has no scope (org-wide controls), or when scope resolves to exactly one entry and the skill chooses to flatten.

Manifests are hash-chained to the previous run for the same control so an assessor can verify the timeline has not been rewritten.

### State

`state.json` records the last completed run per control. The agent uses it to compute "what's overdue" without scanning the entire evidence tree. State is regenerable from manifests if it ever gets out of sync.

### Risk register and access tracking

Most WISPs locate the risk register and access change log in an issue tracker (typical conventions: a `risk` label on an issue tracker; access changes filed as templated issues). `secunit` does not maintain its own copies. Skills that produce risk entries or access changes write a draft markdown body and link the resulting external issue URL into the run manifest.

## Runtime architecture

`secunit` is a single Rust binary delivered as a release artifact. Optional integrations (AWS SDK, GitHub, dependency audits, generic HTTP) are gated behind cargo features — operators install only what their org needs.

The binary is a **helper to the agent**, not a harness that spawns one. The agent (running in a Claude Code session, a `/schedule` routine, or any equivalent) is the orchestrator; it reaches for `secunit` to do the deterministic work that should not be reasoned through every time.

### Responsibility split

| Layer | Owns |
|---|---|
| `secunit` (Rust) | Registry parsing, schema validation, cadence + scope resolution, run-directory allocation, artifact hashing, manifest assembly + hash-chain integrity, native evidence capture against versioned upstream APIs |
| Agent | Reading skills, composing capture steps, narrative reasoning, diff interpretation, finding identification, risk drafting, operator handoff |
| Skills | The procedure for each control: which captures to run, in what order, with what flags; how to interpret results; what `findings.md` looks like |

### Two-phase run model

Every control execution is bracketed by two `secunit` calls with the agent's skill execution sandwiched between:

1. **`secunit run prepare <control-id>`** — resolves scope against the inventory, allocates `evidence/<y>/<q>/<id>/<run-id>/`, snapshots the registry and inventory git shas, writes `prepare.json` into the run dir, and emits the prepare context as JSON to stdout.
2. **Agent executes the skill.** The skill calls `secunit capture …` for each piece of evidence, writes per-system artifacts under `by-system/<name>/raw/`, composes `findings.md`, and drops a `result.json` describing status and any drafted risks/issues.
3. **`secunit run finalize <run-dir>`** — reads `prepare.json` and `result.json`, hashes every artifact, links the manifest to the prior run via `prior_run.manifest_sha256`, validates the assembled manifest against `manifest.schema.json`, atomically writes `manifest.json`, and updates `state.json`.

The seam is intentional: the binary owns the state-changing operations (allocation, hashing, manifest write); the agent owns judgment.

### Capture commands

Native upstream integrations live under `secunit capture <subsystem> <action>`. Each capturer:

- Reads credentials from the standard chain or `_config.yaml` integration block; never persists secrets across invocations.
- Writes canonical JSON to `--out` with stable shape `{ capturer, version, captured_at, args, result }`.
- Sorts arrays by stable id, normalizes timestamps to ISO-8601 UTC, strips ephemeral fields (request ids, pagination tokens), so diffs across runs reflect real change rather than serialization drift.
- Streams paginated results to disk without buffering the full response.

See [`cli.md`](cli.md) for the full subcommand list.

When a control needs evidence no capture command covers, the skill instructs the agent to obtain it ad hoc (a screenshot, a transcript, an attestation from the operator) and write it under `raw/` directly. Ad-hoc evidence is still valid evidence; the hash chain doesn't care how it arrived.

### Validation and verification

- `secunit validate` — schema check on every YAML, cross-references between controls, skills, policies, inventory kinds, schedule overrides, and skill-declared `requires_features`. Run as a pre-commit hook.
- `secunit verify` — walks every run for a control (or all controls) in chronological order, recomputes artifact hashes, checks each `prior_run.manifest_sha256` against the recomputed sha of the prior manifest. Single point of integrity for an assessor.

Combined with git history and signed commits, the hash chain provides a tamper-evident audit trail with no proprietary infrastructure.

## Multi-system runs

When a control has a `scope` block, the skill iterates the resolved inventory list. For each entry it captures evidence under `by-system/<name>/`, then composes a top-level `findings.md` with one section per system plus a rollup summary.

The agent records the **inventory git sha** at run time in the manifest. Combined with the per-entry `in_scope_since` / `retired_on` dates, an assessor can reconstruct exactly which systems were in scope on any prior run date, even after the inventory has changed.

Inventory changes mid-cycle:

- **New system added** — picked up by the next run of every control whose tag filter matches.
- **System retired** — `retired_on` set; future runs skip it; historical evidence remains intact and discoverable.
- **System renamed** — `name` changes, an `aliases:` list preserves discoverability of historical evidence under the prior name.
- **Split or merge** — old entry retired, new entries added, with a one-time bootstrap-report-style transition note committed alongside.

When per-system divergence is itself the finding (e.g. one repo has SCA, another doesn't), the skill flags it in `findings.md` and drafts a risk register entry. The operator decides whether to remediate or document an exception.

## Workflow

### A typical session

```
1. Operator (or scheduled agent invocation) asks: "what's due this week?"
   → Agent runs `secunit due --within 7d`.
2. Operator picks a control, e.g. sca-weekly-dependency-scan.
3. Agent runs `secunit run prepare sca-weekly-dependency-scan`.
   secunit resolves scope against inventory.yaml, allocates the run dir,
   writes prepare.json, emits the run context as JSON.
4. Agent loads skills/sca-weekly-dependency-scan.md and follows it:
   for each in-scope system, runs `secunit capture …` to write canonical
   JSON into by-system/<name>/raw/, then composes findings.md and result.json.
5. Agent runs `secunit run finalize <run-dir>`.
   secunit hashes every artifact, links to the prior run via
   prior_run.manifest_sha256, writes manifest.json, updates state.json.
6. If findings warrant risk entries, the agent drafts them and asks the
   operator to file in the org's tracker; the URL is pasted back into the
   manifest's external_links.
7. Done.
```

### Quarterly reporting

`reports/<year>-<qN>-quarterly.md` is generated by an agent walking the quarter's evidence directories, summarizing per-control activity, listing open risks, listing overdue controls, and listing upcoming controls. The skill `report-quarterly` knows how to assemble it. Same pattern for `report-annual` and `report-policy-review-status`.

## Storage layout

```
<org>/
  controls/                                      # one YAML per control
    aa-weekly-audit-review.yaml
    ca-quarterly-vuln-scan.yaml
    sca-weekly-dependency-scan.yaml
    ...
  skills/                                        # one markdown per workflow
    aa-weekly-audit-review.md
    sca-weekly-dependency-scan.md
    report-quarterly.md
    bootstrap.md
    inventory-seed.md
    ...
  inventory.yaml                                 # in-scope systems (repos, cloud accounts, SaaS, sites)
  schedule.yaml                                  # date overrides + one-offs
  state.json                                     # last-run pointer per control
  _config.yaml                                   # owners, thresholds, terminology, integration URLs
  evidence/<year>/<quarter>/<control-id>/<run-id>/
    manifest.json
    findings.md
    by-system/<name>/raw/...                     # when control has scope
    raw/...                                      # when control is org-wide
  reports/<year>-<qN>-quarterly.md
  reports/<year>-annual.md
  README.md                                      # operator quickstart
```

The whole tree is plain files. Versioning the registry under git gives free auditability of policy/runbook changes and makes the inventory git sha referenced from manifests meaningful.

## Coverage from a WISP

A typical WISP grounded in NIST 800-53 / SP 800-171 surfaces this set of cadence-bearing obligations. The reference shapes in `examples/controls/` are derived from this list; an org's actual registry is built by the `bootstrap` skill walking the org's WISP and emitting one control per obligation.

| Source family | Example control id | Cadence | Scope |
|---|---|---|---|
| Audit and Accountability — periodic log review | `aa-weekly-audit-review` | weekly | cloud_account, production |
| Assessment, Authorization, and Monitoring — control assessment | `ca-monthly-control-assessment` | monthly | org-wide |
| Assessment, Authorization, and Monitoring — leadership status | `ca-quarterly-program-status` | quarterly | org-wide |
| Assessment, Authorization, and Monitoring — vulnerability scans | `ca-quarterly-vuln-scan` | quarterly | cloud_account, production |
| Assessment, Authorization, and Monitoring — penetration testing | `ca-annual-pentest` | annual | declared inline |
| Risk Assessment — fixed-date activities from a procedure | `ra-<yyyy-mm>-<activity>` | scheduled | varies |
| Access Control — entitlement review | `ac-annual-access-review` | annual | saas, all |
| Physical and Environmental — facility access list review | `pe-quarterly-physical-access-review` | quarterly | site, all |
| Awareness and Training — annual training, status reporting | `at-annual-training`, `at-quarterly-training-status` | annual / quarterly | org-wide |
| System and Information Integrity — SCA / dependency scan | `sca-weekly-dependency-scan` | weekly | source_repo, has-sca |
| System and Information Integrity — SAST | `sast-weekly` | weekly | source_repo, has-sast |
| System and Information Integrity — endpoint scan | `si-weekly-endpoint-scan` | weekly | endpoint, all |
| System and Information Integrity — monitoring tool review | `si-annual-monitoring-tool-review` | annual | org-wide |
| System and Communications Protection — firewall review | `sc-semiannual-firewall-review` | semi-annual | cloud_account, production |
| Contingency Planning — BCP test, training, backups | `cp-annual-bcp-test`, `cp-weekly-full-backup` | annual / weekly / monthly | declared inline / cloud_account |
| Incident Response — annual test | `ir-annual-test` | annual | org-wide |
| System and Service Acquisition — vendor review | `sa-annual-vendor-review` | annual | saas, all |
| Policy lifecycle — annual review of each policy document | `policy-annual-review-<slug>` | annual | org-wide |

A reference subset is shown under `examples/controls/`. The full registry is built incrementally; nothing is gated on completeness.

## Out of scope for v1

- Automated scheduling / cron. Cadence is computed; firing is operator-initiated (or driven by a Claude Code `/schedule` routine that simply asks the agent to run `secunit due`).
- Encrypted evidence storage. Files are plain markdown/JSON in a private repo.
- Multi-owner workflows beyond the single `owner` field. Extend the schema and add an assignment skill if required.
- Cloud providers beyond the initial set. The first release wires `aws`, `github`, and dependency audits behind cargo features; `gcp` and others land as feature additions when the first org needs them.
- A long-lived daemon, server, or API layer in front of the binary. Each `secunit` invocation is a one-shot process; secrets never persist across runs.
