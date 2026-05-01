---
name: policy-annual-review
description: Use when secunit invokes any annual policy review control (`policy-annual-review-*`). Walks the policy referenced in `skill_args.policy_path`, records the operator's edits as a diff, captures attestation, and surfaces drift between policy and procedure. Reusable across every policy in the engagement's WISP — the calling control supplies the policy path.
---

# Annual policy review

This skill executes the annual review for a single policy document. It is invoked by every `policy-annual-review-*` control; the control's `skill_args.policy_path` (and optional `procedure_path`) identifies which policy.

## Inputs

- `run_dir` — absolute path to the allocated run directory.
- `prior_run_dir` — absolute path to the previous run, or empty.
- `control` — parsed YAML for the calling control.
- `skill_args.policy_path` — relative path to the policy file in the WISP repo. Required.
- `skill_args.procedure_path` — relative path to the corresponding procedure, if any.

## Procedure

1. **Snapshot the policy** at the start of the review.
   - Copy current contents to `raw/policy-before.md`.
   - Record git sha of the WISP repo at this point in `raw/policy-before.gitsha`.
2. **Walk the policy with the operator.**
   - Present the policy section by section.
   - Capture decisions: keep / edit / remove. Record in `raw/walkthrough.md`.
3. **Apply edits.** The operator (not the agent) makes substantive edits. The agent may propose edits but the operator must accept them explicitly.
4. **Snapshot the policy** at the end.
   - Copy revised contents to `raw/policy-after.md`.
   - Compute `raw/policy.diff` from before/after.
5. **Check procedure alignment** (if `procedure_path` provided).
   - Walk the procedure for references to the policy.
   - Identify any clause in the policy that is not implemented by a step in the procedure, or any step in the procedure that is not authorized by the policy.
   - Record gaps in `raw/policy-procedure-gaps.md`.
6. **Capture attestation.** Operator confirms in writing that the policy was read end-to-end. Record in `raw/attestation.md` with timestamp.
7. **Update review-cycle metadata.** If the policy frontmatter has fields like `EFFECTIVE DATE` or `REVIEW CYCLE`, propose an update reflecting today's review. Apply only with operator confirmation.
8. **Draft follow-up issues** for any gaps found in step 5. Use the issue template below.
9. **Write `findings.md`** using the template below.
10. **Return** structured result.

## Findings template

```markdown
# Annual policy review: <Policy title> — <YYYY-MM-DD>

## Summary
<2–4 sentences. Was the policy current? Material edits? Procedure aligned?>

## Evidence captured
- raw/policy-before.md, raw/policy-before.gitsha
- raw/policy-after.md
- raw/policy.diff
- raw/walkthrough.md
- raw/policy-procedure-gaps.md (if procedure reviewed)
- raw/attestation.md

## Material changes
<List of substantive edits, one per bullet. "None — minor only" if clean.>

## Procedure alignment
<For each gap: clause → procedure step (or absence). "Aligned" if clean.>

## Draft follow-up issues
<One block per gap or change requiring downstream work.>
```

## Issue template

```markdown
### Issue: <short title>
- Trigger: <which gap or change>
- Suggested target: <procedure file or system to update>
- Body:
  **Context:** <reference back to this run by path>
  **Required change:** <concrete next step>
  **Owner:** <role>
  **Due:** <ISO date>
```

## Anti-patterns

- Do not edit the policy without an operator decision per section.
- Do not skip the procedure-alignment check; missed drift here is the most common reason WISPs rot.
- Do not bump effective dates without the operator's explicit confirmation.
