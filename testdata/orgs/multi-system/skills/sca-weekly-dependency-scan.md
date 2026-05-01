---
name: sca-weekly-dependency-scan
description: Use when secunit invokes the weekly Software Composition Analysis control. Iterates the resolved scope (source repos with the has-sca tag), captures dependency audit output and Dependabot state per repo, diffs against prior run, and emits a multi-system findings.md. Trigger only when the calling control's id is `sca-weekly-dependency-scan` and the agent has allocated a run directory and resolved scope.
requires_features: [github, deps]
---

# Weekly software composition analysis

This skill executes the weekly dependency-vulnerability sweep across every source repo flagged for SCA. It is read-only — never modify repo state or open PRs.

## Inputs

- `run_dir` — absolute path to the allocated run directory.
- `prior_run_dir` — absolute path to the previous run, or empty for the first run.
- `control` — parsed YAML for `sca-weekly-dependency-scan`.
- `resolved_scope` — list of source-repo entries from `inventory.yaml` matching `kind: source_repo, has_tags: [has-sca]`. Each carries `name`, `url`, `stack`, `tags`.
- `registry_git_sha` — recorded by the agent into `manifest.json` (pins inventory.yaml too, since it lives in the same repo).

## Procedure

For each `system` in `resolved_scope`:

1. **Allocate** `sub_dir = run_dir / "by-system" / system.name` (already created by `secunit run prepare`).
2. **Capture dependency audit** appropriate to the stack:
   - `python-django` / `python` — `secunit capture deps pip-audit --path <repo path> --out <sub_dir>/raw/pip-audit.json`.
   - `typescript-react` / `typescript` / `node` — `secunit capture deps pnpm-audit --path <repo path> --out <sub_dir>/raw/pnpm-audit.json`.
   - `rust` — `secunit capture deps cargo-audit --path <repo path> --out <sub_dir>/raw/cargo-audit.json`.
   - Other stacks — record `sub_dir/raw/unsupported-stack.txt` with the stack name and return per-system `status: needs-operator`; do not block the rest.
3. **Capture Dependabot state**:
   - `secunit capture github branch-protection --repo <system.url> --branch main --out <sub_dir>/raw/branch-protection.json` (used downstream to confirm enforcement).
   - `secunit capture github dependabot-alerts --repo <system.url> --state open --out <sub_dir>/raw/dependabot-alerts.json`.
   - If the repo has no `.github/dependabot.yml`, the dependabot-alerts capture returns an empty result with `result.config_present: false`. Reflect this in the per-system summary; flag in cross-system anomalies.
4. **Diff against prior run**:
   - For each captured artifact, compare `sub_dir/raw/<artifact>.json` against `prior_run_dir/by-system/<system.name>/raw/<artifact>.json`.
   - Because captures are canonical, diffs reflect real change. Write diffs to `sub_dir/raw/diff-<artifact>.txt`.
   - If the system did not exist in the prior run, note "first run for this system" instead.
5. **Per-system summary**: compose a markdown section with totals (Critical/High/Medium/Low), new vs persisting findings, missing-config flags. Hold the section in memory; write all sections together in step 7.

After iterating:

6. **Cross-system anomaly check**:
   - Repos missing Dependabot configuration when the rest have it — flag.
   - A vulnerability present across more than one repo — flag once, list the repos.
   - Stack divergence (e.g. one Python repo on a major version newer than the others) — note.
7. **Write `findings.md`** at `run_dir` using the multi-system template under "Findings template".
8. **Draft risk entries** for any finding above remediation threshold (Critical or High). One block per finding, scoped to the affected repo(s).
9. **Draft issues** for any repo lacking required configuration (e.g. no Dependabot, missing audit step in CI). One block per gap.
10. **Return** the structured result the agent expects (`status`, `scope_layout: by-system`, per-system results, `draft_risks`, `draft_issues`).

## Findings template

```markdown
# Weekly software composition analysis — <YYYY-MM-DD>

## Summary
<2-4 sentences: overall posture across <N> repos. Worst finding. Configuration gaps.>

## Scope
<N> source repos in scope (tag: has-sca). Inventory git sha: <hex>.
- app-api, app-ui, ...

## Per-system results

### app-api
- Stack: python-django
- Critical: <n> | High: <n> | Medium: <n> | Low: <n>
- New this week: <n> | Resolved since prior: <n> | Persisting: <n>
- Dependabot config: present | MISSING
- Open Dependabot alerts: <n>
- Notes: <one-liner if applicable>
- Evidence: by-system/app-api/raw/pip-audit.json, ...

### app-ui
- Stack: typescript-react
- ...

## Cross-system anomalies
<Bulleted; "None" if not applicable.>

## Recommended actions
<Bulleted; "None" if clean.>

## Draft risk entries
<One block per finding above threshold; omit section if none.>

## Draft external issues
<One block per repo lacking required config; omit section if none.>
```

## Risk entry template

```markdown
### Risk N: <package>@<version> — <CVE id>
- Affected systems: <comma-separated repo names>
- Severity: Critical | High
- Impact: <1-5>
- Likelihood: <1-5>
- Score: <product>
- Body:
  **What:** <CVE summary, vulnerable version range, fixed version>
  **Why it matters:** <consequence in this repo's context>
  **Suggested remediation:** <upgrade path; mitigations if upgrade blocked>
  **SLA:** <30d for Critical, 90d for High>
  **Evidence:** <relative paths under run_dir>
```

## Operator-only steps

If a capture cannot complete (private registry, missing token, repo not cloned locally), write `sub_dir/raw/<artifact>.pending.txt` with the exact `secunit capture …` command the operator must run, set the per-system status to `needs-operator`, and continue with the remaining systems. Do not block the entire run on one repo.

## Anti-patterns

- Do not summarize before capture completes. Captures first, narrative second.
- Do not open PRs to fix findings; that is a separate operator decision.
- Do not skip the cross-system anomaly section; divergence between repos is the most actionable signal this control produces.
- Do not flatten to `raw/` when scope is multi-system. Always use `by-system/<name>/raw/`.
- Do not parse upstream APIs directly when a `secunit capture` command exists for the same data — captures normalize output so diffs across runs reflect real change.
