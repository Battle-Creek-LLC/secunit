# Storage

`secunit` stores everything as files. No database, no service, no daemon. This document defines the on-disk contract; the spec covers the conceptual model.

## Tree

```
<org>/
  controls/                  # one YAML per control, kebab-case filenames
  skills/                    # one markdown per skill, kebab-case filenames
  inventory.yaml             # in-scope systems
  schedule.yaml              # date overrides and one-off insertions
  _config.yaml               # owners, thresholds, terminology, integration URLs
  state.json                 # last-run pointer per control
  risks/
    index.json               # derived register cache (regenerable from the logs)
    <risk-id>/events.jsonl   # append-only risk event log
  evidence/
    <year>/
      <quarter>/             # q1, q2, q3, q4
        <control-id>/
          <run-id>/          # <YYYY-MM-DD>-run-NNN
            .run-pending     # sentinel; present only between prepare and finalize
            prepare.json     # written by `secunit run prepare`
            result.json      # written by the skill before finalize
            manifest.json    # written by `secunit run finalize`
            findings.md
            by-system/<name>/raw/<artifact>.{json,txt,csv,png,...}
            raw/<artifact>.{json,txt,csv,png,...}     # when control is org-wide
  reports/
    <year>-<qN>-quarterly.md
    <year>-annual.md
    <year>-policy-review-status.md
```

Everything in the org directory is checked into git. Treat the repo as the system of record.

## Run-dir lifecycle

A run directory passes through three states, each marked by which files exist:

| State | Files present | Set by |
|---|---|---|
| **Pending — prepared** | `.run-pending`, `prepare.json`, empty `by-system/*/raw/` (or `raw/`) | `secunit run prepare` |
| **Pending — captured** | + skill-written artifacts under `raw/` and `by-system/`, `findings.md`, `result.json` | The skill executed by the agent |
| **Sealed** | + `manifest.json`; `.run-pending` removed | `secunit run finalize` |

`secunit run abort` is the only legitimate way to discard a pending run. It writes an `abort.json` with the reason and removes `.run-pending` but preserves the rest of the directory so the abort itself is auditable.

`secunit run resume` is a no-op that re-emits the prepare context — useful when the agent's session is restarted mid-run.

## Inventory schema

`inventory.yaml` is a flat dictionary keyed by **kind**. Each kind contains a list of entries; every entry has at minimum `name` and optional `tags`. Kind-specific fields are added as needed and consumed by skills.

```yaml
source_repos:
  - name: app-api
    url: github.com/<org>/app-api
    stack: python-django
    tags: [production, customer-data, has-sca, has-sast]
    in_scope_since: 2026-01-01
  - name: app-ui
    url: github.com/<org>/app-ui
    stack: typescript-react
    tags: [production, customer-data, has-sca, has-sast]
    in_scope_since: 2026-01-01
  - name: marketing-site
    url: github.com/<org>/marketing-site
    stack: static
    tags: [production, marketing]
    excludes: [sca]
  - name: data-pipeline
    stack: python
    tags: [internal, has-sca]
    in_scope_since: 2024-06-01
    retired_on: 2026-09-01

cloud_accounts:
  - name: prod
    provider: aws
    profile: prod
    tags: [production, customer-data]
  - name: staging
    provider: aws
    profile: staging
    tags: [staging]

saas:
  - name: github-org
    owner: <handle>
    tags: [production, source-control]
  - name: cloud-console
    owner: <handle>
    tags: [production, infrastructure]
  - name: error-monitoring
    owner: <handle>
    tags: [production, observability]

sites:
  - name: hq
    address: <city>
    tags: [physical]

endpoints:
  # populated by inventory-seed from MDM or manually
```

Field reference:

- `name` — stable, kebab-case. Used in evidence path `by-system/<name>/`. Renames go through `aliases:`.
- `tags` — free-form, used by control `scope:` filters. Conventional tags: `production`, `staging`, `customer-data`, `internal`, `has-sca`, `has-sast`, `marketing`, `infrastructure`, `observability`, `source-control`, `physical`.
- `in_scope_since` — ISO date. Skills skip the entry for runs dated before this.
- `retired_on` — ISO date. Skills skip the entry for runs dated on or after this.
- `aliases` — list of prior names, for discovering historical evidence after a rename.
- `excludes` — list of skill names this entry opts out of, even if tag filter would match.
- Kind-specific fields (`url`, `stack`, `provider`, `profile`, `owner`) are read by skills that target that kind.

The `inventory-seed` skill populates this from GitHub, cloud account discovery, and the WISP's access dictionaries. The operator confirms before commit.

## Scope resolution

A control's `scope` block selects inventory entries:

```yaml
# Iterate all production cloud accounts
scope:
  kind: cloud_account
  has_tags: [production]

# Iterate every SaaS provider
scope:
  kind: saas
  all: true

# Iterate every source repo with SCA enabled, but not the marketing site
scope:
  kind: source_repo
  has_tags: [has-sca]
  excludes: [marketing-site]

# Iterate inline (no inventory lookup) — useful for one-off pentest scope
scope:
  inline:
    - name: app-api
      kind: source_repo
    - name: customer-portal
      kind: external-surface
```

Resolution rules:

1. Filter inventory entries of the given `kind`.
2. If `all: true`, include every active entry; otherwise require `has_tags` to be a subset of the entry's `tags`.
3. Drop entries where `in_scope_since > run_date` or `retired_on <= run_date`.
4. Drop entries listed in the control's `excludes`, or whose own `excludes` list names the skill being run.
5. Sort by `name` for stable evidence-path ordering across runs.

If `scope` is omitted entirely, the control is **org-wide** — the skill runs once, writes evidence to `raw/` (no `by-system/`), and `findings.md` has no per-system sections.

If scope resolves to exactly one entry, the skill MAY flatten to `raw/`. Either layout is valid; the manifest records which was used.

## Cadence resolution

A control's `cadence` resolves to a target date as follows. `schedule.yaml` overrides any computed date for a specific run.

| Cadence | Default firing |
|---|---|
| `continuous` | Never fires; surfaced as "ongoing" in dashboards. The skill is invoked on demand to capture an evidence snapshot. |
| `weekly` | Each calendar week. The org may pin to a weekday (e.g. Monday); default is Monday. |
| `monthly` | First business day of the month. |
| `quarterly` | First business day of Q1/Q2/Q3/Q4 (Jan, Apr, Jul, Oct). |
| `semi-annual` | First business day of January and July. |
| `annual` | The control may declare `due_by` (e.g. `december-31`); default is the policy's effective-date anniversary. |

A control is **due** from its target date forward; it becomes **overdue** after a per-cadence grace period:

| Cadence | Grace |
|---|---|
| `weekly` | 3 days |
| `monthly` | 7 days |
| `quarterly` | 14 days |
| `semi-annual` | 21 days |
| `annual` | 30 days |

## `state.json`

```json
{
  "schema_version": 1,
  "controls": {
    "<control-id>": {
      "last_run_id": "2026-05-01-run-001",
      "last_run_path": "evidence/2026/q2/<control-id>/2026-05-01-run-001/",
      "last_run_at": "2026-05-01T14:32:00Z",
      "last_status": "complete",
      "next_due": "2026-05-08"
    }
  },
  "updated_at": "2026-05-01T14:32:00Z"
}
```

The agent rebuilds `state.json` from manifests when `next_due` is missing or when an inconsistency is detected.

## `manifest.json`

```json
{
  "schema_version": 1,
  "control_id": "sca-weekly-dependency-scan",
  "run_id": "2026-05-04-run-001",
  "started_at": "2026-05-04T13:05:00Z",
  "completed_at": "2026-05-04T13:42:00Z",
  "operator": "<operator handle>",
  "agent": {
    "model": "claude-opus-4-7",
    "skill": "sca-weekly-dependency-scan",
    "skill_sha256": "<hex>",
    "control_sha256": "<hex>"
  },
  "registry_git_sha": "<git sha of secunit repo at run time>",
  "scope_layout": "by-system",
  "resolved_scope": [
    { "name": "app-api", "kind": "source_repo", "tags": ["production", "customer-data", "has-sca"] },
    { "name": "app-ui",  "kind": "source_repo", "tags": ["production", "customer-data", "has-sca"] }
  ],
  "prior_run": {
    "run_id": "2026-04-27-run-001",
    "manifest_sha256": "<hex>"
  },
  "artifacts": [
    { "path": "findings.md", "sha256": "<hex>", "bytes": 4321 }
  ],
  "by_system": [
    {
      "name": "app-api",
      "status": "complete",
      "artifacts": [
        { "path": "by-system/app-api/raw/pip-audit.json", "sha256": "<hex>", "bytes": 8412 }
      ]
    },
    {
      "name": "app-ui",
      "status": "complete",
      "artifacts": [
        { "path": "by-system/app-ui/raw/pnpm-audit.json", "sha256": "<hex>", "bytes": 6391 }
      ]
    }
  ],
  "status": "complete",
  "draft_risks": [],
  "draft_issues": [],
  "external_links": []
}
```

Notes:

- `scope_layout` is `by-system` or `flat`. `flat` means the skill chose to write directly under `raw/` (only legal when scope is empty or resolves to one entry).
- `registry_git_sha` plus per-entry `in_scope_since`/`retired_on` lets an assessor reconstruct exactly which systems were in scope on the run date, even after the inventory has changed: `git checkout <sha>` and the inventory is what it was at run time.
- `prior_run.manifest_sha256` chains the manifests, so any retroactive edit of an earlier run breaks verification of every later run for that control.
- `by_system` mirrors the run's evidence layout. For `flat` runs, `artifacts` lives at the top level only.

## Risk register (`risks/`)

The risk register is authoritative state held in the store as an append-only event
log per risk. It reuses two patterns already defined above: the manifest hash
chain (here applied per risk via `prev_sha256`) and the regenerable `state.json`
cache (here `risks/index.json`). Full design and CLI/UI surface in
[`risks.md`](risks.md); the on-disk contract is below.

Each `risks/<risk-id>/events.jsonl` is JSON Lines, one immutable event per line:

```json
{"seq":1,"ts":"2026-05-25T14:40:00Z","actor":"jstockdi","agent":null,"type":"opened","prev_sha256":null,"data":{"finding_ref":{"control_id":"ra-vuln-audit","run_id":"2026-05-25-run-001","manifest_sha256":"<hex>","finding_id":"S032","body_path":"findings.md#risk-1"},"title":"S032 — pickle deserialization RCE (CWE-502)","severity":"critical","impact":3,"likelihood":3,"affected_systems":["app-api"],"sla_days":30,"due_at":"2026-06-24"}}
{"seq":2,"ts":"2026-05-25T14:41:00Z","actor":"jstockdi","agent":null,"type":"owner-assigned","prev_sha256":"<hex>","data":{"owner":"cto"}}
{"seq":3,"ts":"2026-06-02T16:00:00Z","actor":"jstockdi","agent":null,"type":"status-changed","prev_sha256":"<hex>","data":{"from":"open","to":"remediated","reason":"pickle replaced; verified in 2026-06-02-run-003"}}
```

Notes:

- `prev_sha256` is the SHA-256 of the previous canonicalised line; `null` on `seq: 1`. Lines are only ever appended — corrections are new events, never edits. `secunit verify` walks this chain.
- `finding_ref` binds the risk to immutable evidence by content hash. The fingerprint `<control_id>:<finding_id>` is the risk's cross-run identity; a re-observation in a later run appends `evidence-linked` rather than opening a new risk.
- Current state is folded from the events; nothing stores "the current status" mutably.

`risks/index.json` is the derived cache — same role as `state.json`:

```json
{
  "schema_version": 1,
  "risks": {
    "R-0007": {
      "title": "S032 — pickle deserialization RCE (CWE-502)",
      "fingerprint": "ra-vuln-audit:S032",
      "severity": "critical",
      "status": "open",
      "owner": "cto",
      "due_at": "2026-06-24",
      "source_control": "ra-vuln-audit",
      "first_run_id": "2026-05-25-run-001",
      "external": [{ "system": "linear", "id": "SEC-412", "url": "https://…" }],
      "log_head_sha256": "<hex>"
    }
  },
  "updated_at": "2026-05-25T14:41:00Z"
}
```

Rebuilt from the logs with `secunit risks rebuild` whenever it drifts.
`log_head_sha256` pins the event each entry was built from, so readers can detect
staleness without re-folding.

## `findings.md` template

Every skill emits findings in this shape. Fixed headings let `report-quarterly` aggregate across runs.

For **multi-system** runs:

```markdown
# <Control title> — <Run date>

## Summary
<2-4 sentences across the full run. Headline result.>

## Scope
<count> systems in scope: <comma-separated names>.
Inventory git sha: <hex>.

## Per-system results

### app-api
<per-system summary, anomalies, evidence pointers>

### app-ui
<per-system summary, anomalies, evidence pointers>

## Cross-system anomalies
<Patterns visible only when comparing across systems. "None" if not applicable.>

## Recommended actions
<Bulleted; "None" if clean>

## Draft risk entries
<Optional; one block per proposed risk>
```

For **org-wide** runs (no scope):

```markdown
# <Control title> — <Run date>

## Summary
<2-4 sentences>

## Evidence captured
- raw/<artifact> — <description>

## Anomalies
<Bulleted; "None" if clean>

## Recommended actions
<Bulleted; "None" if clean>

## Draft risk entries
<Optional>
```

## `schedule.yaml`

```yaml
overrides:
  - control_id: ra-2026-03-vuln-audit
    due: 2026-03-15
    note: "Pinned to mid-March per WISP procedure schedule."

  - control_id: ca-quarterly-vuln-scan
    skip:
      quarter: 2026-q3
      reason: "Subsumed into pentest engagement."

  - control_id: cp-annual-bcp-test
    insert:
      run_at: 2026-08-01
      reason: "Mid-year tabletop in addition to year-end test."
```

## File naming

- Control YAML: `<id>.yaml` (kebab-case, prefixed with the policy family — `aa-`, `ca-`, `ac-`, `cp-`, `ir-`, `pe-`, `at-`, `si-`, `sc-`, `sa-`, `ra-`, `sca-`, `sast-`, `policy-`).
- Skill markdown: `<skill-name>.md`. Skill name matches `control.skill` exactly.
- Run id: `<YYYY-MM-DD>-run-<NNN>`. The `NNN` counter is per control per day; `001` in almost all cases.
- Risk id: `R-<NNNN>`, globally sequential, allocated at `secunit risks open`. The log lives at `risks/<risk-id>/events.jsonl`.
- Inventory `name`: kebab-case, stable. Used directly as `by-system/<name>/` path.
- Evidence artifacts under `raw/` or `by-system/<name>/raw/`: descriptive kebab-case, with extension matching format. No timestamps in filenames — they live in `manifest.json`.
