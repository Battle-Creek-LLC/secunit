---
name: attestation-review
description: Use when a secunit control names skill `attestation-review`. The shared runbook for human-judgment controls that walk a checklist, capture the operator's attestation, and draft follow-ups — used by the quarterly physical-access review, semi-annual restore test, and the annual risk assessment, vendor review, IR test, BCP test, access review, training, monitoring-tool review, and program review. Trigger only when a run dir is allocated.
requires_features: [github]
---

# Attestation review

The reusable runbook for controls where the evidence is the operator's
documented judgement, not a machine capture. The agent prepares the ground
(pulls any helpful captures, presents the checklist, structures the record);
the operator makes the decisions and attests.

## Inputs

- `run_dir`, `prior_run_dir`, `control`, `resolved_scope` (may be empty).
- `control.skill_args.checklist[]` — the steps to walk with the operator.
- `control.skill_args.capture_hint[]` — optional `{ tool, action, args, out }` captures the agent may run to inform the review (e.g. `github org-members` for the access review). Best-effort; not required.

## Procedure

1. **Prep.** If `resolved_scope` is non-empty, this is per-system: create
   `run_dir/by-system/<name>/` for each entry. Run any `capture_hint` captures
   into `raw/` (or `by-system/<name>/raw/`) to give the operator real data to
   review — e.g. the current org roster, vendor list, firewall rules.
2. **Walk the checklist.** Present each `checklist` item to the operator. For
   per-system controls, walk it once per system. Record decisions verbatim in
   `raw/walkthrough.md` (or per-system).
3. **Capture attestation.** The operator confirms in writing what was reviewed
   and what they decided. Write `raw/attestation.md` with an ISO-8601 timestamp
   and the operator handle. This file is the primary evidence.
4. **Identify follow-ups.** Anything requiring downstream work — a revocation, a
   new risk, a policy/procedure gap, a coverage gap — becomes a draft (below).
   The agent drafts the body; the operator files it in the tracker.
5. **Write `findings.md`** (template below).
6. Drop `result.json`; if the operator could not complete a step, set
   `status: needs-operator` and record what's outstanding.

## findings.md template

```markdown
# <control title> — <YYYY-MM-DD>

## Summary
<2–4 sentences: what was reviewed, the headline decision, anything outstanding.>

## Evidence captured
- raw/attestation.md (+ raw/walkthrough.md, any capture_hint artifacts)

## Review results
<Per checklist item (or per system): what was found, what was decided.>

## Recommended actions
<Bulleted; "None" if clean.>

## Draft risk entries
<One block per new/raised risk; omit if none.>

## Draft external issues
<One block per revocation / gap / follow-up; omit if none.>
```

## Issue template

```markdown
### Issue: <short title>
- Trigger: <which checklist item / finding>
- Suggested target: <system / policy / procedure to change>
- Required change: <concrete next step>
- Owner: <role> · Due: <ISO date>
- Context: <reference back to this run by path>
```

## Anti-patterns

- The agent never attests on the operator's behalf. Attestation is the operator's words.
- The agent never files external issues directly — it drafts; the operator files.
- Don't skip writing `raw/attestation.md` even when the result is "all clear" — the dated, signed record *is* the control's evidence.
