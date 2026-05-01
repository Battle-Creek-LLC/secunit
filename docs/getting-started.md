# Getting started

How to take an organization with an existing Written Information Security Program (WISP) and stand up a `secunit` registry against it. The whole journey, end to end, from "I have policy docs in a repo" to "`secunit due` tells me what to do this week."

> **Status.** The bootstrap flow described here is the **design** target (Phase 3 of `PLAN.md`). Today's CLI implements registry math (Phase 1) and run lifecycle (Phase 2). The `bootstrap` and `inventory-seed` skills, plus `secunit registry import` and the `inventory` subcommands, are not yet wired in. Read this as the walkthrough you will hand to a new operator once Phase 3 lands; the conceptual contract is stable.

## What you need before starting

- **An existing WISP** as files in a git repo — typically `<org>-docs/security/` containing policy markdown/PDF, procedures, and access dictionaries. `secunit` does not replace this; it operationalizes it.
- **The `secunit` binary** built with the cargo features your environment needs (`aws`, `github`, `deps`, `http` are the default set; see `cli.md` for the full list).
- **Standard credential chains** for the integrations you intend to capture against: AWS profile(s), a GitHub PAT or app, etc. Bootstrap itself only needs read access; capture phases need whatever the relevant skill declares in `requires_features`.
- **A target directory** for the secunit root. Conventionally this is its own repo (e.g. `<org>-secunit/`) so the registry's git history is independent of the WISP's.

## The mental model in one paragraph

`secunit` turns each cadence-bearing obligation in a WISP into a **control** (a YAML file with id, owner, cadence, skill name, scope), tracks completion in `state.json`, captures hash-chained **evidence** under `evidence/<year>/<quarter>/<id>/<run-id>/`, and resolves what each control operates over via `inventory.yaml`. The agent does the work by following the **skill** named on the control; `secunit` itself just allocates run dirs, hashes artifacts, assembles manifests, and verifies the chain. Bootstrap is the one-time (then re-runnable) step that walks your WISP and produces the initial registry.

## Step 1 — Stub the root

Create the directory tree:

```
<org>-secunit/
  controls/                 # empty for now
  skills/                   # empty for now
  inventory.yaml            # empty doc: `{}` or kind keys with empty lists
  _config.yaml              # see below
  .gitignore                # ignore target/, .DS_Store, etc.
```

`_config.yaml` declares integration handles — what your GitHub org is called, which AWS profiles back which cloud accounts, where the WISP source lives. The schema is `schemas/_config.schema.json`. Treat this file as the only place you hand-author secrets-adjacent identifiers; everything downstream resolves through it.

`git init`, commit the stub. The git history of this repo *is* the audit trail — the manifest format pins the registry's git sha into every run.

## Step 2 — Prepare the bootstrap run

```bash
secunit run prepare bootstrap
```

This allocates `evidence/<year>/<quarter>/bootstrap/<YYYY-MM-DD>-run-001/`, writes `prepare.json` and a `.run-pending` sentinel, and emits the prepare context as JSON to stdout. The agent reads that JSON to know where to drop drafts.

## Step 3 — Run the `bootstrap` skill

The agent loads `skills/bootstrap.md` and walks your WISP repo. For every cadence-bearing obligation it finds — weekly log review, quarterly vuln scan, annual policy reviews, fixed-date procedures — it writes one **draft control YAML** under `<run-dir>/drafts/controls/`. Fixed-date obligations land in `<run-dir>/drafts/schedule.yaml`.

The skill also writes a `bootstrap-report.md` summarizing:

- Obligations found and their proposed `id`, `cadence`, `owner`, `skill`.
- Ambiguities the operator must resolve (cadence unclear, owner unclear, scope unclear).
- Any policy references that didn't resolve to a file.

Reference shapes for the kinds of obligations a NIST 800-53 / 800-171 WISP surfaces are in `spec.md` ("Coverage from a WISP" table) and `examples/controls/`.

## Step 4 — Run the `inventory-seed` skill

In the same run dir (or a paired bootstrap run), the agent loads `skills/inventory-seed.md` and enumerates:

- **Source repos** via the GitHub org named in `_config.yaml`.
- **Cloud accounts** via the AWS profile chain.
- **SaaS providers** from `_config.yaml` integration data and the WISP's own access dictionaries.
- **Sites, endpoints** — typically operator-attested; the skill prompts.

It writes `<run-dir>/drafts/inventory.yaml` grouped by `kind`, with `name`, `tags`, and `in_scope_since` set per entry. The schema is `schemas/inventory.schema.json`.

## Step 5 — Review

Open `<run-dir>/bootstrap-report.md`. Walk every flagged ambiguity. Edit drafts in place — the bootstrap run dir is just files. This is the only human-in-the-loop step in the bootstrap, and it is non-optional: a bootstrapped registry the operator hasn't read is not a registry.

Tag conventions to settle here, since every later `scope:` filter resolves through them:

- `production`, `staging`, `internal`
- `customer-data`
- `has-sca`, `has-sast`
- `source-control`, `infrastructure`, `observability`

(Full list in `storage.md`.)

## Step 6 — Finalize the bootstrap run

```bash
secunit run finalize <run-dir>
```

`secunit` hashes the drafts, links the manifest to (no prior, since this is the first run), validates against `manifest.schema.json`, atomically writes `manifest.json`, updates `state.json`, removes `.run-pending`. The bootstrap is now itself an auditable, hash-chained run.

## Step 7 — Promote drafts into the live registry

```bash
secunit registry import <run-dir>
```

This validates each draft against its schema, cross-checks `control.skill` resolves to a `skills/` file, checks `control.policy` paths exist, checks `scope.kind` matches an inventory section, and refuses to promote anything that fails. Successful drafts are moved into `controls/` and `inventory.yaml` is moved into place. Failed drafts stay in the run dir with per-file diagnostics for you to fix.

Commit the new `controls/`, `inventory.yaml`, and any `schedule.yaml` to the secunit repo. The first registry checkpoint.

## Step 8 — Verify and probe

```bash
secunit validate              # schema + cross-ref check, clean exit
secunit due --within 7d       # what's due in the next week
secunit calendar --quarter $(date +%Y)-q$((($(date +%-m)-1)/3+1))
secunit scope <some-control>  # preview which inventory entries iterate
```

If `due` shows the right things on the right dates, bootstrap is done. You are ready to start running real controls — pick one (`sca-weekly-dependency-scan` is a good first), run `secunit run prepare <id>`, follow its skill, finalize, repeat.

## Re-running bootstrap

Bootstrap is **re-runnable** by design. As the WISP evolves:

- New obligations show up as new drafts on the next bootstrap run.
- Existing obligations are matched by `id`; if the YAML diverges, the bootstrap report flags it and the operator decides whether to accept the change.
- Obligations no longer appearing in the WISP are marked orphaned. `secunit registry rm <id> --reason ...` writes a tombstone but preserves the prior evidence — historical runs remain discoverable.

This is what keeps the registry in sync with the WISP without a separate "migration" tool.

## Where to look next

- `spec.md` for the conceptual model.
- `cli.md` for the full CLI surface and exit codes.
- `storage.md` for the on-disk contract: run-dir lifecycle, scope resolution, cadence rules.
- `skills.md` for how skills are authored and what contract they have with the agent.
- `examples/` for reference shapes you can copy-paste from.
