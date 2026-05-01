---
name: inventory-seed
description: Use when secunit invokes the inventory-seed control to populate (or refresh) `inventory.yaml` from the operator's GitHub org, cloud accounts, SaaS access dictionaries, and the WISP's site/endpoint lists. Reads connection details from `_config.yaml` integrations and from the WISP `org.wisp_repo`. Emits a draft `inventory.yaml` into the run directory's `raw/` tree alongside a per-kind `inventory-diff.md`. Re-runnable: existing entries are preserved, new entries are added with `in_scope_since: today`, and entries that no longer appear upstream are flagged for retirement (never deleted automatically).
requires_features: [github]
---

# Seed and refresh inventory

This skill produces a draft `inventory.yaml`. It never writes outside the run directory — promotion is `secunit registry import` (which merges new entries into the live `inventory.yaml`).

## Inputs

- `run_dir` — absolute path to the allocated run directory.
- `prior_run_dir` — absolute path to the previous inventory-seed run, or empty.
- `control` — parsed YAML for `inventory-seed`.
- `_config.yaml` — must contain `integrations.github.org` (and, if AWS captures are wired in, `integrations.aws.accounts`). Falls back to environment variables (`GITHUB_ORG`, `AWS_PROFILES`) when fields are absent.
- `live_registry_root` — the secunit registry root (so the skill can diff against the existing `inventory.yaml`).

## Procedure

For each inventory **kind**, follow the procedure below; collect drafts in memory and write them all at once in step 7.

### 1. `source_repos` — from GitHub

- For each org in `integrations.github.org`, list active (non-archived) repositories: `secunit capture github org-members --org <org> --out <run_dir>/raw/github-org-<org>.json` is a starting point, but the canonical capture is `gh repo list <org> --json name,url,isArchived,primaryLanguage` written to `raw/github-repos-<org>.json`.
- For each repo, propose:
  - `name`: the repo name (already kebab-case in GitHub).
  - `url`: `github.com/<org>/<name>`.
  - `stack`: derived from `primaryLanguage` (`Python` → `python`, `TypeScript` → `typescript`, `Rust` → `rust`, ...). Mark unknown languages as `<lang>` and flag in the diff.
  - `tags`: at minimum `[production]` if the repo is a default branch `main`/`master` and not archived. The `has-sca` and `has-sast` tags are operator decisions — leave the existing tags untouched on a refresh, default to `[]` on a new entry.
  - `in_scope_since`: today, on first appearance.
- Skip archived repos.

### 2. `cloud_accounts` — from `_config.yaml`

- The cloud-account list is operator-curated (there is no upstream "list of accounts I own"). Read `integrations.aws.accounts` (and `integrations.gcp.accounts` if present) and emit one entry per account.
- For new entries, set:
  - `name`: account-config key (e.g. `prod`, `staging`).
  - `provider`: `aws` / `gcp` / ...
  - `profile`: the SSO/IAM profile name from config.
  - `tags`: from the config block's `tags` field; default `[]`.
  - `in_scope_since`: today.

### 3. `saas` — from the WISP access dictionary

- The WISP's access-control policy or procedure typically contains a section like "Approved SaaS providers" or an access dictionary. Walk the WISP under `_config.yaml` `org.wisp_repo` and grep for that section.
- Extract one entry per SaaS provider:
  - `name`: kebab-case from the provider name.
  - `owner`: from the dictionary, if recorded.
  - `tags`: derive from category (`source-control`, `infrastructure`, `observability`, `communications`).
- If no access dictionary exists, write `raw/saas-needs-operator.md` listing what was searched and ask the operator to seed the section manually. Do not invent providers.

### 4. `sites` — from the WISP

- Walk the WISP for facility/physical-security policies. Extract any explicitly named site (often the org's HQ).
- Emit one entry per named site with `name`, optional `address`, and `tags: [physical]`.
- If none are named, leave the section empty with a comment.

### 5. `endpoints` — from MDM (optional)

- If `_config.yaml` configures an MDM integration, capture the endpoint list (`secunit capture http get …` against the MDM API).
- Otherwise leave the section empty with a comment that endpoints are operator-maintained.

### 6. Diff against live inventory

For each kind, compare proposed entries against `live_registry_root/inventory.yaml`:

- **kept** — entry exists in live; carry the live entry through unchanged (preserve operator edits to `tags`, `excludes`, etc.).
- **added** — entry does not exist; include with `in_scope_since: today` and conservative defaults.
- **missing-upstream** — entry exists in live but did not appear in the upstream scan (e.g. a repo was archived or transferred). Do **not** delete; flag in `inventory-diff.md` and propose `secunit inventory retire --kind <k> --name <n> --on <today> --reason "missing from <source>"`.

### 7. Write outputs

- `raw/inventory.yaml` — the merged draft. Top-level keys in stable order (`source_repos`, `cloud_accounts`, `saas`, `sites`, `endpoints`). Within each kind, entries sorted by `name`. YAML formatting matches the canonical examples under `docs/examples/inventory.yaml`.
- `raw/inventory-diff.md` — human-readable summary using the template below.
- `raw/sources/` — one file per upstream source consulted (`github-repos-<org>.json`, `wisp-saas-section.md`, ...) so the report is auditable.

### 8. Return

Structured result with `status`, `scope_layout: flat`, and `draft_issues` for any "needs operator" gaps surfaced.

## Diff template

```markdown
# Inventory seed diff — <YYYY-MM-DD>

## Summary
<2-4 sentences: counts per kind (kept/added/missing-upstream), notable gaps.>

## source_repos
- Added (<n>): app-api, app-ui, ...
- Kept (<n>): ...
- Missing upstream (<n>): legacy-pipeline (archived in GitHub on <date>) — propose retire on <today>.

## cloud_accounts
...

## saas
...

## sites
...

## endpoints
...

## Promotion
Run `secunit registry import <run-dir>` to merge `raw/inventory.yaml` into the
live registry. Existing entries are preserved; only new entries are appended.
For any "missing upstream" line, run the proposed `secunit inventory retire`
command after operator review.
```

## Anti-patterns

- Do not delete entries from the live inventory. Retirement is an operator decision via `secunit inventory retire`, which preserves history.
- Do not overwrite operator-curated tags. On a refresh, kept entries pass through verbatim.
- Do not invent SaaS providers, endpoints, or sites. If the upstream source is missing or empty, surface the gap rather than filling it with plausible-looking defaults.
- Do not change `in_scope_since` on entries that already have it. Lifecycle dates are write-once.
- Do not run captures requiring credentials the operator has not provisioned. Skip the kind and surface the gap; the rest of the seed is still useful.
