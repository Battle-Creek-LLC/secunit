---
name: policy-annual-review
description: Use when a secunit control named `policy-annual-review-*` invokes this skill. Walks the single policy named in skill_args.policy_path, records the operator's edits as a diff, captures attestation, and surfaces drift between policy and procedure. Reused across every WISP policy-review control — the calling control supplies the policy path and due date. Trigger only when a run dir is allocated.
---

# Annual policy review

Executes the annual review of one WISP policy under `security/`. Invoked by every
`policy-annual-review-*` control; `skill_args.policy_path` says which policy.

## Inputs

- `run_dir`, `prior_run_dir`, `control`.
- `skill_args.policy_path` — relative path to the policy in this repo. Required.

## Procedure

1. **Snapshot before.** Copy the policy to `raw/policy-before.md`; record the
   repo git sha in `raw/policy-before.gitsha`.
2. **Walk it with the operator**, section by section. Capture keep/edit/remove
   decisions in `raw/walkthrough.md`. Check the `EFFECTIVE DATE` /
   `REVIEW CYCLE` frontmatter is current.
3. **Apply edits** the operator explicitly accepts. The agent may propose; the
   operator decides. Substantive edits are the operator's call.
4. **Snapshot after.** Copy to `raw/policy-after.md`; write `raw/policy.diff`.
5. **Procedure alignment.** If a procedure exists for this policy, check for
   clauses with no implementing step and steps with no authorizing clause.
   Record gaps in `raw/policy-procedure-gaps.md`.
6. **Attestation.** Operator confirms in writing the policy was read end-to-end.
   Write `raw/attestation.md` (timestamp + handle).
7. **Bump the review date** only with explicit operator confirmation (set
   `EFFECTIVE DATE` to today; keep the annual `REVIEW CYCLE`).
8. **Draft follow-up issues** for any gap.
9. **Write `findings.md`** (template below) and drop `result.json`.

## findings.md template

```markdown
# Annual policy review: <Policy title> — <YYYY-MM-DD>

## Summary
<2–4 sentences. Current? Material edits? Procedure aligned?>

## Evidence captured
- raw/policy-before.md, raw/policy-before.gitsha, raw/policy-after.md, raw/policy.diff
- raw/walkthrough.md, raw/attestation.md (+ raw/policy-procedure-gaps.md if reviewed)

## Material changes
<One bullet per substantive edit. "None — minor only" if clean.>

## Procedure alignment
<Per gap: clause → step (or absence). "Aligned" if clean.>

## Draft external issues
<One block per gap or follow-up; omit if none.>
```

## Anti-patterns

- Don't edit the policy without a per-section operator decision.
- Don't skip the procedure-alignment check — silent policy/procedure drift is how a WISP rots.
- Don't bump effective dates without explicit confirmation.
