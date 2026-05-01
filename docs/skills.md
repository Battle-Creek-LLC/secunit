# Skills

Skills are how `secunit` actually does work. Every control declares one skill by name; the agent loads that skill when starting a session for the control.

A skill is a single markdown file using the [Claude Code Skills](https://docs.claude.com/) frontmatter format. It owns:

1. **Triggering rules** — when the agent should reach for this skill (frontmatter `description`).
2. **Required capabilities** — `requires_features:` listing the cargo features the skill needs from `secunit` (e.g. `[aws, github]`). `secunit validate` flags missing features before a run starts.
3. **The runbook** — the prose-and-code procedure the agent follows. Capture steps invoke `secunit capture <subsystem> <action>`; non-captured evidence (operator-attested attestations, screenshots, transcripts) is written under `raw/` directly.
4. **Evidence shape** — what to write into `raw/` and `findings.md`.
5. **Findings template** — the structure the run's `findings.md` must follow so quarterly/annual reports can summarize across runs.

Skills are the only place that knows how to interpret captures, contact humans, or compose narrative. The control YAML is declarative metadata; the skill is the procedure; `secunit` provides deterministic primitives.

## Skill responsibilities

A skill MUST:

- Write all captured artifacts under the run directory passed in by the agent. Never write outside it.
- Produce a `findings.md` with at minimum: `Summary`, `Evidence captured`, `Anomalies`, `Recommended actions`.
- Surface anything that should become a risk register entry as a structured block in `findings.md` (the agent files it; the skill drafts it).
- Be deterministic about evidence filenames so diffs across runs are meaningful.

A skill SHOULD:

- Prefer `secunit capture` commands over ad-hoc API calls — captured artifacts are canonicalized and diff-stable across runs.
- Capture raw output verbatim before summarizing. Narrative is the last step, not the first.
- Note when the operator must perform a step the agent cannot (e.g. a physical walkthrough).
- Declare `requires_features:` in the frontmatter so missing capabilities fail validation, not mid-run.

A skill MUST NOT:

- Modify production systems.
- File external issues directly. Draft the body; let the agent or operator file.
- Mutate `state.json`, `manifest.json`, or any sibling control's evidence.

## Skill families

Most controls map 1:1 to a skill. A few share:

- **`policy-annual-review`** — reused across all 16 policy review controls. The control YAML differs only by `policy` path; the skill walks the policy, checks last review date, opens an editing session for the operator, captures the diff as evidence.
- **`backup-verify`** — used by both `cp-weekly-full-backup` and `cp-monthly-full-backup` with a `scope` parameter passed from the control.
- **`report-quarterly` / `report-annual`** — read-only skills that aggregate prior run evidence into a report under `reports/`.

## Inputs the agent passes to a skill

The agent obtains the run context by calling `secunit run prepare <control-id>`. The output of that command is the structured context passed to the skill:

```yaml
control_id: sca-weekly-dependency-scan
run_dir: <org>/evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001/
prior_run_dir: <org>/evidence/2026/q2/sca-weekly-dependency-scan/2026-04-27-run-001/
control: { ...full YAML... }
operator: <operator handle>
resolved_scope:
  - { name: app-api, kind: source_repo, tags: [production, customer-data, has-sca], url: ..., stack: python-django }
  - { name: app-ui, kind: source_repo, tags: [production, customer-data, has-sca], url: ..., stack: typescript-react }
registry_git_sha: <hex>
```

The skill returns:

```yaml
status: complete | needs-operator | blocked
scope_layout: by-system | flat
artifacts: [list of top-level files written under run_dir]
by_system:
  - name: app-api
    status: complete
    artifacts: [list of files under by-system/app-api/]
findings_path: <run_dir>/findings.md
draft_risks: [list of structured risk entries]
draft_issues: [list of structured external issues to file]
```

The skill writes this as `result.json` at the run-dir root. `secunit run finalize` reads it, hashes the captured artifacts, writes `manifest.json`, updates `state.json`, and prompts the operator for any external filings.

## Multi-system skills

When `resolved_scope` contains more than one entry, the skill iterates:

```
for system in resolved_scope:
    sub_dir = run_dir / "by-system" / system.name
    sub_dir.mkdir(parents=True)
    capture_evidence(system, sub_dir)
    diff = compare_to_prior(system, prior_run_dir / "by-system" / system.name, sub_dir)
    section = summarize(system, sub_dir, diff)
    sections.append(section)

write_findings(run_dir, sections, cross_system_anomalies(sections))
```

Rules:

- One `findings.md` at the run root, with the multi-system template from `storage.md`.
- Per-system raw artifacts under `by-system/<name>/raw/` only. Never write outside the system's subdir during its iteration.
- Compare each system to its prior-run subdir at `prior_run_dir/by-system/<system.name>/`. If the system did not exist in the prior run (newly added to inventory), note that in the per-system section.
- Cross-system anomalies — patterns only visible when comparing across systems (e.g. one repo lacks SCA config that all others have) — go in their own section of `findings.md`.
- A skill MAY write `scope_layout: flat` when scope resolves to a single entry, in which case raw artifacts live at `run_dir/raw/` and the multi-system template collapses to the org-wide one. The agent records the chosen layout in the manifest.

## Single-system / org-wide skills

When `resolved_scope` is empty (control has no `scope` block), the skill runs once and writes evidence under `run_dir/raw/` using the org-wide `findings.md` template.

## Authoring conventions

- One skill per file. Filename matches the skill name.
- Frontmatter `description` should make the trigger condition unambiguous — the agent picks skills by reading descriptions.
- Keep procedural detail in the body, not the description.
- If a skill grows past ~200 lines, split it. Reuse via composition (one skill `include`s commands from another by reference, not by import).

See `examples/skills/aa-weekly-audit-review.md` for the canonical shape.
