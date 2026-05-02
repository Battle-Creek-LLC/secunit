---
name: bootstrap
description: Use when secunit invokes the bootstrap control to seed (or re-seed) a registry from an organisation's WISP. Walks the WISP repo, extracts cadence-bearing obligations from each policy and procedure, and emits a draft `controls/`, `inventory.yaml` skeleton, `schedule.yaml`, `_config.yaml`, and a human `bootstrap-report.md` into the run directory's `raw/` tree. Re-runnable: existing controls in the live registry are surfaced as "kept", new obligations as "added", and obligations no longer present in the WISP as "orphaned". Trigger only when the calling control's id is `bootstrap` and the agent has allocated a run directory.
---

# Bootstrap a registry from a WISP

This skill produces draft registry files. It never writes outside the run directory — promotion into the live registry is done explicitly by `secunit registry import`.

The organisation's WISP repo is configured via `_config.yaml`:

```yaml
org:
  wisp_repo: ../<org>-docs
```

The skill reads the directory rooted at `org.wisp_repo` (relative to the secunit registry root, or absolute).

## Inputs

- `run_dir` — absolute path to the allocated run directory.
- `prior_run_dir` — absolute path to the previous bootstrap run, or empty.
- `control` — parsed YAML for `bootstrap`.
- `_config.yaml` — must contain `org.wisp_repo` pointing at the WISP source tree.
- `live_registry_root` — the secunit registry root (so the skill can diff against existing controls). Available as `--root`'s canonical path.

## Procedure

1. **Locate the WISP.** Resolve `org.wisp_repo`. Fail with `status: failed` if the path does not exist or is empty.
2. **Inventory the WISP.** Walk every `*.md` file under the WISP, classifying by filename pattern:
   - `*-policy.md` → policy document
   - `*-procedure.md` → procedure document
   - `*-plan.md` → plan document (e.g. `business-continuity-plan.md`)
   - other markdown → narrative; scan but do not require obligations
   Record the relative path of each policy/procedure under `raw/wisp-files.txt`.
3. **Extract cadence-bearing obligations.** For each document, scan headings and sentences for cadence keywords:
   - `weekly`, `monthly`, `quarterly`, `semi-annually`/`semi-annual`, `annually`/`annual`, `every <N> <unit>`, `at least once a <unit>`.
   - Fixed-date obligations: `in March each year`, `by December 31`, `<month> <day>`.
   - For each match, capture the surrounding sentence as `excerpt`, and the document's NIST family from filename (`access-control` → `AC`, `audit-and-accountability` → `AU`, `system-and-information-integrity` → `SI`, ...).
   Write the raw matches to `raw/extractions.json` for audit.
4. **Map obligations to controls.** For each extracted obligation, propose a control id following the conventions in `docs/storage.md`:
   - Family prefix from NIST mapping (`aa-`, `ac-`, `ca-`, `cp-`, `ir-`, `pe-`, `at-`, `si-`, `sc-`, `sa-`, `ra-`, `sca-`, `sast-`, `policy-`).
   - Cadence suffix (`-weekly`, `-monthly`, `-quarterly`, `-semiannual`, `-annual`).
   - Activity slug derived from the obligation's heading (kebab-case, ≤ 4 words).
   For policy review obligations, use `policy-annual-review-<policy-slug>` and reuse the shared `policy-annual-review` skill.
5. **Diff against the live registry.** Read existing `controls/*.yaml` from `live_registry_root`. For each proposed control:
   - **kept** — id already exists in the live registry; do not emit a draft.
   - **added** — id does not exist; emit a draft under `raw/controls/<id>.yaml`.
   - **orphaned** — control exists in the live registry but no obligation in the current WISP scan maps to it; record in `bootstrap-report.md` for operator review. Do not delete.
6. **Emit draft controls.** For each "added" obligation, write `raw/controls/<id>.yaml` with the schema-required fields:
   - `id`, `title`, `policy` (relative path back into the WISP repo), `nist`, `owner` (from `_config.yaml` `owners` map by family, falling back to `owner`), `cadence`, `weekday` (for weekly), `due_by` (for annual), `skill` (matches the proposed control id unless a shared skill applies, e.g. `policy-annual-review`).
   - `scope` — best guess based on the obligation. If the policy mentions "every source repo", emit `kind: source_repo, has_tags: [has-<family>]`. If "every cloud account in production", emit `kind: cloud_account, has_tags: [production]`. Otherwise omit (org-wide) and flag in the report for operator review.
   - `evidence_required` — derive from the obligation text (e.g. "scan output", "attestation", "summary"). Always include a `kind: summary` entry.
   - `references` — at minimum, the source policy/procedure file the obligation came from.
7. **Emit `inventory.yaml` skeleton.** If the live registry has no `inventory.yaml`, write `raw/inventory.yaml` with empty lists for the kinds the proposed controls reference (`source_repos`, `cloud_accounts`, `saas`, `sites`, `endpoints`, ...). Add a comment line per kind pointing to the `inventory-seed` skill. If the live registry already has an inventory, do not overwrite — `inventory-seed` handles deltas.
8. **Emit `schedule.yaml` skeleton.** Stub with an empty `overrides: []` plus comments showing the override shapes from `docs/storage.md`. Skip if the live registry already has one.
9. **Emit `_config.yaml` skeleton.** If the live registry has no `_config.yaml`, write one populated with whatever the agent learned from the WISP — `org.name` (from policy headers), `org.wisp_repo` (the path the operator passed in), `owners` map (from policy "Owner:" lines), and `weekly_default_weekday: monday`. Skip if a `_config.yaml` already exists.
10. **Compose `bootstrap-report.md`.** Use the template below. This is the human-readable artifact the operator reviews before running `secunit registry import`.
11. **Return** the structured result the agent expects (`status`, `scope_layout: flat`, `draft_risks: []`, `draft_issues: []`).

## Bootstrap report template

```markdown
# Bootstrap report — <YYYY-MM-DD>

## Summary
<2-4 sentences. WISP repo path. Number of policy/procedure files scanned. Counts: kept / added / orphaned.>

## WISP files scanned
<count> files under <wisp_repo>:
- security/access-control-policy.md
- security/audit-and-accountability-procedure.md
- ...

## Proposed controls

### Added (<n>)
| id | cadence | source | scope | owner |
|---|---|---|---|---|
| sca-weekly-dependency-scan | weekly | system-and-information-integrity-policy.md | source_repo, has-sca | cto |
| ... | ... | ... | ... | ... |

### Kept (<n>)
Controls already present in the live registry whose obligations the current WISP scan still finds. No draft emitted.
- aa-weekly-audit-review
- ...

### Orphaned (<n>)
Controls present in the live registry but whose source obligation was not found in the current WISP scan. Operator review required — the WISP may have been edited, or the scan heuristics may have missed an obligation.
- <id> — last seen in <prior wisp file> (manual confirmation needed)
- ...

## Items requiring operator review
<Bulleted list of obligations the heuristic could not confidently classify.
Each item names the WISP file and excerpt, and proposes one or more candidate
control ids to choose between.>

## Promotion
Run `secunit registry import <run-dir>` to copy draft controls into the live
registry. Existing files are preserved; only missing controls/files are added.
```

## Idempotency

The skill is designed to be re-run as the WISP evolves:

- Same WISP, same registry → 0 added, all controls kept, 0 orphaned.
- New obligation appears in WISP → 1 added, rest kept.
- Obligation removed from WISP → 1 orphaned (operator decides whether to retire the control), rest kept.

Because draft files are emitted under `raw/controls/` (not the live `controls/`), re-running cannot clobber operator edits.

## Anti-patterns

- Do not write outside `run_dir`. Promotion is the operator's explicit step.
- Do not delete or modify entries in the live registry — only emit drafts and surface diffs in the report.
- Do not invent NIST control identifiers. If the policy text does not name one, leave the `nist:` list empty rather than guessing.
- Do not collapse multiple obligations into one control. If a policy says "weekly log review and monthly summary", emit two controls.
- Do not skip the orphaned section. A WISP that no longer mandates a control is information the operator needs to act on.
