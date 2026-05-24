---
name: capture-sweep
description: Use when a secunit control names skill `capture-sweep`. The mechanical capture→diff→flag runbook shared by every automated control (SCA/software-vuln sweeps, audit-log review, baseline-drift checks, scheduled vuln scans). Runs the `secunit capture …` calls and/or shell `commands` declared in the control's skill_args across the resolved scope, diffs each artifact against the prior run, and emits findings. Read-only — never modifies any system. Trigger only when a run dir is allocated and scope resolved.
requires_features: [github, deps]
---

# Capture sweep

The reusable engine for automated, evidence-by-capture controls. The control is
declarative; this skill turns its `skill_args` into capture calls, runs them per
in-scope system, diffs against the prior run, and writes findings. It performs
**no** mutations: no PRs, no rule changes, no remediation.

## Inputs (from `secunit run prepare`)

- `run_dir`, `prior_run_dir` — allocated paths (`prior_run_dir` empty on first run).
- `control` — parsed control YAML.
- `resolved_scope` — inventory entries to iterate; **empty for org-wide controls**.
- `control.skill_args`:
  - `target` — `repo` (pass `--repo <entry.url>`) or `account` (pass `--account <entry.name>`). Tells the skill which flag identifies the system for `secunit capture`.
  - `captures[]` — `{ tool, action, args[], out }` → `secunit capture <tool> <action> <system-flag> <args> --out <dir>/raw/<out>`.
  - `commands[]` — `{ cmd, out, description }` → run `cmd`, redirect stdout to `<dir>/raw/<out>`. For non-`secunit` evidence (e.g. `repocat audit --json`).
- `control.remediation_thresholds` / `_config.thresholds` — SLA days per severity; used to flag overdue findings and to decide what becomes a draft risk (`_config.thresholds.draft_risk_at`).
- `control.skill_args.extend` — optional name of a **fragment skill** holding control-specific steps this runbook can't express declaratively. If set, resolve it with `secunit skills show <name>` and run its steps at the hook points below. The fragment owns *only the delta*; this spine still owns capture→diff→findings. Don't fork the spine — extend it.

## Hook points

A fragment named in `skill_args.extend` runs at two points: **post-capture** (after step 2/3, before diffing — e.g. derive an extra artifact from the raw captures) and **pre-findings** (after step 5, before writing `findings.md` — e.g. add a bespoke section or risk). The fragment writes under the same `raw/` (or `by-system/<name>/raw/`) and returns its sections to fold in. Resolution is uniform: local `skills/<name>.md` wins over the bundled copy.

## Procedure

### Scoped controls (`resolved_scope` non-empty)

For each `system` in `resolved_scope`:

1. `sub_dir = run_dir/by-system/<system.name>` (already created by prepare).
2. For each `capture` in `skill_args.captures`, run:
   - `target: repo` → `secunit capture <tool> <action> --repo <owner/repo from system.url> <args…> --out <sub_dir>/raw/<out>`
   - `target: account` → `secunit capture <tool> <action> --account <system.name> <args…> --out <sub_dir>/raw/<out>`
   - **`requires_tag`**: if a capture declares `requires_tag: <tag>`, skip it for any system whose `tags` don't include `<tag>` (e.g. `codeql-alerts` with `requires_tag: has-sast` runs only on SAST-capable repos, never on Rust). Skipping is silent — not a failure.
   - **`github audit-log` needs an absolute `--since`** (ISO-8601). Compute it: the prior run's finalized timestamp, or `today − 7 days` (or the control's period) on the first run. Append it to the capture's `args`.
3. For each `command` in `skill_args.commands`, run it; write stdout to `<sub_dir>/raw/<out>`
   (capture-sweep handles the redirection — the command itself just prints to stdout).
   Before running, export these for the template to reference:
   - `$NAME` — the system name; `$REPO` — `owner/repo` for source repos;
     `$SINCE` — the prior-run ISO-8601 timestamp (or window start on first run).
4. **Diff** each artifact against `prior_run_dir/by-system/<system.name>/raw/<out>`; write `<sub_dir>/raw/diff-<out>.txt`. Captures are canonical, so a diff is real change. If the system is new since the prior run, record "first run for this system".
5. Compose a per-system section (hold in memory): counts by severity, new/persisting/resolved, config-missing flags, anything past its remediation SLA.

After iterating, run the **cross-system anomaly check**: a system missing a control all peers have (e.g. no Dependabot config), a finding spanning multiple systems (list once), notable divergence.

### Org-wide controls (`resolved_scope` empty)

Run `skill_args.captures` / `commands` once, writing to `run_dir/raw/`. Diff against `prior_run_dir/raw/`. `scope_layout: flat`.

### Finish

6. Write `findings.md` (template below).
7. Draft a risk for every finding at or above `_config.thresholds.draft_risk_at` (default High). SLA = `remediation_thresholds[severity]`; internet-facing High uses 30d, internal High 60d.
8. Draft an issue for every config gap (repo missing Dependabot/CodeQL, drifted from the repocat baseline, over-broad firewall rule).
9. Drop `result.json` and return.

## findings.md template

```markdown
# <control title> — <YYYY-MM-DD>

## Summary
<2–4 sentences: posture across <N> systems, worst finding, config gaps.>

## Scope
<N> <kind> in scope (<tag filter>). Inventory git sha: <hex>.

## Per-system results
### <system>
- Critical: <n> | High: <n> | Medium: <n> | Low: <n>
- New: <n> | Resolved: <n> | Persisting: <n> | Past SLA: <n>
- Config: <present/MISSING notes>
- Evidence: by-system/<system>/raw/<files>

## Cross-system anomalies
<Bulleted; "None" if clean.>

## Recommended actions
<Bulleted; "None" if clean.>

## Draft risk entries
<One block per finding ≥ threshold; omit if none. Use the risk template.>

## Draft external issues
<One block per config gap; omit if none.>
```

## Risk entry template

```markdown
### Risk: <subject> — <id/CVE>
- Affected systems: <names>
- Severity: Critical | High
- Impact: <1-5> · Likelihood: <1-5> · Score: <product>
- What / Why it matters / Suggested remediation: <…>
- SLA: <days from thresholds> · Evidence: <paths under run_dir>
```

## Operator-only / failure handling

If a capture cannot complete (missing token, account not configured, repo not
cloned), write `<dir>/raw/<out>.pending.txt` with the exact `secunit capture …`
command to run, set that system's status to `needs-operator`, and continue. Never
block the whole run on one system.

## Anti-patterns

- Capture first, summarize second. Never narrate before captures complete.
- Never open PRs or change rules to "fix" a finding — that's a separate operator decision.
- Never flatten to `raw/` when scope is multi-system; always `by-system/<name>/raw/`.
- Never parse an upstream API directly when a `secunit capture` exists for it — captures normalize output so diffs mean something.
