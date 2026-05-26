---
name: inventory-seed
description: Use when seeding or refreshing inventory.yaml — the in-scope systems (repos, cloud accounts, SaaS, sites). Pulls the GitHub org and any cloud accounts from `_config.integrations`, the repo-hardening baseline's managed-repo list, and known SaaS, proposes inventory entries with tags, and diffs against the live inventory. Read-only against live state — proposes edits for the operator to apply.
requires_features: [github]
---

# Inventory seed

Keeps `inventory.yaml` honest: what's actually in scope, tagged so controls pick
it up by `kind` + tag without naming systems individually.

## Procedure

1. **Source repos.** Let `$ORG = _config.integrations.github.org`. Cross-reference two lists:
   - Managed repos from the repo-hardening baseline named in `_config.integrations`
     (e.g. a `repocat` config such as `.repo.yml` — its `repos:` keys are the governed set).
   - Live GitHub org repos: `secunit capture github org-members --org $ORG` and the repo list via `gh repo list $ORG`.
   For each repo, detect stack (Cargo.toml → rust, package.json → node, pyproject → python, go.mod → go, pubspec.yaml → flutter). Tag `has-sca` only if it has a dependency manifest; otherwise `excludes:` the SCA/vuln-scan controls that need one.
2. **Cloud accounts.** If `_config.integrations` declares cloud accounts (an
   `aws`/`gcp` block), enumerate the accounts/profiles, add a `cloud_accounts:`
   section, and tag `production` where customer data lives. If the org runs no
   cloud infrastructure, omit the section entirely. Adding cloud later means
   re-running `bootstrap` to regenerate the cloud controls.
3. **SaaS.** List the vendors that hold company/customer data or provide
   production services (source control, identity, comms, project tracking, AI
   subprocessors). Tag by role; flag subprocessors.
4. **Sites.** Physical/facility locations (remote home office counts — needed for
   the quarterly physical-access review).
5. **Diff against live `inventory.yaml`:** added / retired / re-tagged. Set
   `retired_on` (never delete) for anything no longer in scope; add `aliases` for
   renames so historical evidence stays discoverable.
6. **Propose the edits** to the operator (write the proposed `inventory.yaml` to
   the run dir for review). Apply only what the operator accepts. Then
   `secunit inventory check` and `secunit validate`.

## Conventions

- `name` is kebab-case and stable; it's an evidence path component.
- Set `in_scope_since`; never back-date it past when the system truly entered scope.
- Prefer tags over per-control wiring — a new repo with `has-sca` is automatically swept; no control edit needed.

## Anti-patterns

- Don't delete retired entries — set `retired_on` so prior evidence remains valid.
- Don't tag a repo `has-sca` if it has no dependency manifest; the scan will error every week.
