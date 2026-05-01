---
name: aa-weekly-audit-review
description: Use when secunit invokes the weekly Audit and Accountability review. Captures cloud audit evidence (firewall alerts, configuration compliance, identity-access analyzer findings, object-store access logs), compares to the prior run, and emits findings.md with anomalies and draft risk entries. Trigger only when the calling control's id is `aa-weekly-audit-review` and the agent has allocated a run directory.
requires_features: [aws]
---

# Weekly audit and accountability review

This skill executes the weekly review prescribed by the engagement's Audit and Accountability Procedure. It is read-only — never modify cloud resources.

## Inputs

- `run_dir` — absolute path to the allocated run directory (provided by `secunit run prepare`).
- `prior_run_dir` — absolute path to the previous run, or empty for the first run.
- `control` — the parsed YAML for `aa-weekly-audit-review`.
- `resolved_scope` — list of cloud accounts matching `kind: cloud_account, has_tags: [production]`. For most orgs this resolves to a single entry; the skill uses the flat layout when so.
- Org-specific configuration (account profile names, log scope) is read from `_config.yaml`. If a referenced field is missing, ask the operator before proceeding.

## Procedure

For most orgs `resolved_scope` is a single cloud account, so this skill uses the flat layout — artifacts go to `run_dir/raw/`. If `resolved_scope` is multi-account, swap `raw/` for `by-system/<account.name>/raw/` and add per-account sections to `findings.md` per the multi-system template.

1. **Confirm scope.** Print the resolved cloud account(s) and ask the operator to confirm before issuing any cloud calls.
2. **Capture access analyzer findings.**
   - `secunit capture aws access-analyzer --account <name> --out raw/access-analyzer.json`
3. **Capture network firewall state.**
   - `secunit capture aws network-firewall --account <name> --since 7d --out raw/network-firewall-alerts.json`
4. **Capture configuration compliance.**
   - `secunit capture aws config --account <name> --out raw/config-compliance.json`
5. **Capture object-store access log sample.**
   - For each in-scope bucket: `secunit capture aws s3-access-logs --bucket <name> --since 7d --out raw/object-store-<bucket>.json`
6. **Diff against the prior run.**
   - For each artifact above, compute a structural diff vs `prior_run_dir/raw/<same name>`.
   - Write diffs to `raw/diff-<artifact>.txt`. Because captures are canonical, diffs reflect real change.
7. **Identify anomalies.** A finding is anomalous if any of the following hold:
   - A new entry appears in access analyzer findings.
   - Firewall alert volume exceeds the trailing 4-week mean by more than 2σ, or any alert with `severity: high` is present.
   - A configuration rule transitions to `NON_COMPLIANT`.
   - Object-store access shows a new principal or a new bucket.
8. **Draft risk entries** (if applicable). Use the template under "Risk entry template". Score Impact and Likelihood 1–5 each.
9. **Write `findings.md`** following the template under "Findings template".
10. **Write `result.json`** at the run-dir root with the structured return the agent expects (status, scope_layout, artifact list, draft_risks). `secunit run finalize` reads this.

## Findings template

```markdown
# Weekly audit and accountability review — <YYYY-MM-DD>

## Summary
<2–4 sentences. Headline result. Did we find anything that needs action?>

## Evidence captured
- raw/access-analyzer-findings.json — <count> active findings
- raw/network-firewall-alerts.json — <count> alerts, severity breakdown
- raw/config-compliance.json — <compliant>/<non-compliant>/<total> rules
- raw/object-store-access-sample.txt — <count> distinct principals across <count> buckets
- raw/diff-*.txt — diffs vs prior run

## Anomalies
<Bulleted list. "None" if clean.>

## Recommended actions
<Bulleted list. "None" if clean.>

## Draft risk entries
<One block per anomaly that warrants risk register entry. Omit section if none.>
```

## Risk entry template

```markdown
### Risk N: <short title>
- Impact: <1-5>
- Likelihood: <1-5>
- Score: <product>
- Body:
  **What:** <observed condition>
  **Why it matters:** <consequence if exploited or unaddressed>
  **Suggested remediation:** <concrete next step>
  **Evidence:** <relative paths under run_dir>
```

## Operator-only steps

If a `secunit capture` call fails for credential or permission reasons, write a stub artifact (`raw/<artifact>.pending.txt`) describing exactly what the operator must run, return `status: needs-operator`, and stop. Do not proceed with partial evidence.

## Anti-patterns

- Do not summarize before captures complete. Captures first, narrative second.
- Do not file external issues from this skill. Hand drafts back to the agent via `result.json`.
- Do not delete or rewrite prior run artifacts under any circumstance.
- Do not inflate severity to manufacture findings; "None" is a valid result and should be common.
