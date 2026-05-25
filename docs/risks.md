# Risk register

`secunit` maintains the risk register **inside the store**, as an append-only event
log per risk. This is a deliberate reversal of the earlier "link out to the org's
tracker" model: the register is now authoritative and lives next to the evidence
that produced it; external trackers become mirrors synced *out* to. See the
amended non-goal in [`spec.md`](spec.md) for the decision.

The design keeps the two integrity properties the rest of `secunit` relies on:

- **Append-only, hash-chained history** — the same tamper-evident chain as
  `manifest.json` (`prior_run.manifest_sha256`), applied per risk. A risk's
  current state is never stored; it is *folded* from its events.
- **Mutable, regenerable index** — `risks/index.json` is a derived cache,
  exactly like `state.json`: fast to read, rebuilt from the logs whenever it
  drifts, and outside the chain.

What makes the register trustworthy is the binding to evidence: every risk
references the finding that produced it by **content hash** (`manifest_sha256` +
`finding_id`), not by a loose URL. The risk and the sealed evidence live in the
same git tree, cryptographically bound.

## Tree

`risks/` sits at the org root, sibling to `evidence/`.

```
<org>/
  risks/
    index.json                 # derived cache — regenerable from the logs (the state.json analogue)
    R-0007/
      events.jsonl             # append-only event log — the source of truth
    R-0008/
      events.jsonl
```

One **JSON Lines** file per risk. The only write that ever touches it is "append
one line"; lines are never rewritten or deleted. The risk id `R-NNNN` is allocated
sequentially at `risks open`, the way run ids are `…-run-NNN`.

## Event log

Each line is one immutable event:

```json
{"seq":1,"ts":"2026-05-25T14:40:00Z","actor":"jstockdi","agent":null,"type":"opened","prev_sha256":null,"data":{ ... }}
```

| Field | Meaning |
|---|---|
| `seq` | 1-based, monotonic within the file. |
| `ts` | ISO-8601 UTC. |
| `actor` | Operator handle responsible for the change. |
| `agent` | `{ model, skill }` when an agent appended the event; `null` for a direct operator action. |
| `type` | Event type (below). |
| `prev_sha256` | SHA-256 of the previous canonicalised line; `null` on `seq: 1`. This is the per-risk hash chain. |
| `data` | Type-specific payload. |

### Event types

| `type` | `data` payload | Effect on the fold |
|---|---|---|
| `opened` | `finding_ref`, `title`, `severity`, `impact`, `likelihood`, `affected_systems`, `sla_days`, `due_at` | Creates the risk; status → `open`. |
| `owner-assigned` | `owner` | Sets owner. |
| `score-changed` | `impact`, `likelihood`, `severity`, `reason` | Supersedes the score; recomputes `due_at` if `sla_days` derives from severity. |
| `sla-set` | `due_at`, `basis` | Overrides the SLA due date. |
| `status-changed` | `from`, `to`, `reason` | Moves through the status machine (below). |
| `evidence-linked` | `finding_ref` | Appends another finding ref — how a risk re-observed in a later run is recorded as *persisting*. |
| `external-linked` | `system`, `external_id`, `url` | Records the tracker mirror created by sync-out. |
| `external-status-observed` | `system`, `status`, `observed_at` | **Advisory** inbound status from a tracker. Never authoritative — see *Sync seam*. |
| `note` | `text` | Free-text note; no state change. |
| `remediated` | `resolved_run_ref` (optional), `note` | Shorthand for `status-changed → remediated` with evidence. |
| `reopened` | `reason` | `remediated → open`. |
| `exception-documented` | `rationale`, `approved_by`, `expires_at` | Status → `accepted-exception`. |

`finding_ref` binds the risk to immutable evidence:

```json
{
  "control_id": "ra-vuln-audit",
  "run_id": "2026-05-25-run-001",
  "manifest_sha256": "9f3c…",
  "finding_id": "S032",
  "body_path": "findings.md#risk-1"
}
```

### Status machine

```
            ┌─────────────► accepted-exception
            │                       │
opened ──► open ──► in-progress ──► remediated ──► reopened ──► open
            │            │              │
            └────────────┴──────────────┴──► false-positive
```

`status-changed` events that violate this machine are rejected at append time.
`accepted-exception` carries `expires_at`; an expired exception surfaces as
overdue in `risks list` until re-documented or remediated.

### Fingerprint and persistence

A risk's identity across runs is its **fingerprint** `<control_id>:<finding_id>`
(e.g. `ra-vuln-audit:S032`), carried in the first `finding_ref`. When a later run
re-observes the same finding, the agent appends `evidence-linked` to the existing
risk rather than opening a new one. New / persisting / resolved across runs is
therefore derivable from the logs, not re-narrated each run in `findings.md`.

### The fold

Current state is a left-fold over events in `seq` order. Last-write-wins per
field; status follows the latest lifecycle event; `finding_ref`s accumulate.

```
opened                 → status:open, severity, impact, likelihood, due_at, finding_refs:[#1]
+ owner-assigned       → owner
+ external-linked      → external:{linear, SEC-412, url}
+ evidence-linked      → finding_refs:[#1, #2]   (persisting)
+ status-changed:remediated → status:remediated, resolved_at
```

Corrections are never edits. A wrong score is a later `score-changed`; a wrong
risk is `status-changed → false-positive`. The log is the full audit story of how
the risk was handled — which is what RA-3 / CA-5 (POA&M) want to see.

## `risks/index.json`

Derived projection of every risk's fold, for fast list/dashboard reads. Mutable,
not chained, regenerable with `secunit risks rebuild`.

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
      "log_head_sha256": "e5f6…"
    }
  },
  "updated_at": "2026-05-25T14:41:00Z"
}
```

`log_head_sha256` pins the latest event each index entry was built from, so the
GUI and `report` can tell at a glance whether the index is stale relative to the
log on disk.

## CLI

The CLI is the only writer; the GUI and `report` only read. See
[`cli.md`](cli.md#risk-register) for flags.

**Mutating verbs** each append exactly one event under the root file lock, then
refresh `index.json`:

| Command | Event appended |
|---|---|
| `secunit risks open --from <run-dir> --finding <id>` | `opened` (promotes a sealed `draft_risk`) |
| `secunit risks assign <risk-id> --owner <role>` | `owner-assigned` |
| `secunit risks score <risk-id> --impact N --likelihood N --reason …` | `score-changed` |
| `secunit risks status <risk-id> --to <status> --reason …` | `status-changed` |
| `secunit risks relink <risk-id> --from <run-dir> --finding <id>` | `evidence-linked` |
| `secunit risks link <risk-id> --system <name> --id <ext-id> --url <url>` | `external-linked` |
| `secunit risks observe <risk-id> --system <name> --status <s>` | `external-status-observed` |
| `secunit risks note <risk-id> --text …` | `note` |
| `secunit risks remediate <risk-id> [--evidence <run-dir>] --note …` | `remediated` |
| `secunit risks reopen <risk-id> --reason …` | `reopened` |
| `secunit risks except <risk-id> --rationale … --approved-by … --expires <date>` | `exception-documented` |

The append protocol: take the root lock → read the tail line for `seq` and
`prev_sha256` → validate the transition against the status machine and the
event schema → write the new line with `O_APPEND` → rebuild the index entry →
release. `risks open --from` additionally verifies the referenced manifest
exists and **recomputes its sha to match**, so a risk cannot be bound to absent
or fabricated evidence. SLA defaults from the source control's
`remediation_thresholds`.

**Read verbs** are pure folds and never write:

```
secunit risks list [--status <s>] [--severity <list>] [--owner <role>] [--past-sla] [--json]
secunit risks show <risk-id> [--json]      # current fold + full event timeline + finding refs
secunit risks rebuild                       # regenerate index.json from the logs
```

`secunit verify` is extended to cover the register: it walks each risk's
`events.jsonl` `prev_sha256` chain, and confirms every `finding_ref` resolves to a
real sealed manifest whose recomputed sha matches. One command verifies evidence
integrity *and* its links to the register.

## Read-only viewer

The GUI keeps its contract — it reads the same tree and never writes
([`spec.md` §Viewer](spec.md#viewer-tauri-gui)). It treats the log exactly as the
CLI's read verbs do: **fold in memory, render, never mutate.**

| View | Source | Purpose |
|---|---|---|
| Risks | `risks/index.json` | Register table: id, title, severity, status, owner, **SLA countdown** (red past due), source control, tracker link. Sort / filter. |
| Risk detail | `risks/<id>/events.jsonl` | Header from the fold; the event log rendered as a chronological **timeline** (the audit narrative, for free); bound findings deep-linking to `findings.md#anchor` + manifest. |
| Overview tile | `risks/index.json` | "Open risks: N (X past SLA)" beside the run-health tiles. |

The detail view recomputes the bound manifest's sha and shows a verified ✓/✗
badge — still read-only. Because the GUI never writes, what it shows is exactly
the on-disk log: every change happens via the CLI (operator or agent) and the GUI
re-reads. The append-only log is what makes "read-only viewer + CLI-only mutation"
clean — viewing is a pure function of the files; mutation is always one more line.

## Sync seam

Sync is a **projection outward**, keeping `secunit` authoritative:

- A skill (or the operator's own tool) reads register state, pushes Critical/High
  risks to the org's tracker, and writes the resulting id/URL back as an
  `external-linked` event.
- Inbound tracker status is recorded as `external-status-observed` and treated as
  **advisory only** — it is shown in the UI but never overrides the register's own
  status. This avoids true bidirectional-sync conflict resolution: there is one
  authoritative source, and the tracker is a mirror.

`external_links` on the manifest remains for backward compatibility but is
superseded by the register for risk tracking; `report` reads open risks from the
register, not from manifest links.
