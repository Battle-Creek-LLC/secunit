# Getting started

How to take an organization with an existing Written Information Security Program (WISP) and stand up a `secunit` registry against it. The whole journey, end to end, from "I have policy docs in a repo" to "`secunit due` tells me what to do this week."

> **Status.** Phases 0–3 of `PLAN.md` are landed: registry math, run lifecycle, the `bootstrap` and `inventory-seed` skills, `secunit registry import`, and the `inventory` subcommands. Native captures (Phase 4+) are next; until they ship, the capture step in any control's skill is the operator running `gh`/`aws` by hand and writing the canonical JSON output into the run dir.

## Install

Each tagged release on [GitHub Releases](https://github.com/Battle-Creek-LLC/secunit/releases/latest) carries both the `secunit` CLI (macOS + Linux) and an unsigned macOS build of the `secunit-gui` desktop viewer.

### CLI

Pick the archive for your platform — `secunit-<target>.tar.gz`, with a matching `.sha256`. On Apple Silicon macOS:

```bash
gh release download --repo Battle-Creek-LLC/secunit \
  --pattern 'secunit-aarch64-apple-darwin.tar.gz*'
shasum -a 256 -c secunit-aarch64-apple-darwin.sha256   # verify
tar xzf secunit-aarch64-apple-darwin.tar.gz
sudo mv secunit /usr/local/bin/
secunit --version
```

Substitute `x86_64-apple-darwin` (Intel mac) or `*-unknown-linux-gnu` (Linux) as needed. No `gh`? Download the same files from the Releases page in a browser.

### Desktop app (macOS)

Download the `.dmg` for your architecture — `secunit-aarch64-apple-darwin.dmg` (Apple Silicon) or `secunit-x86_64-apple-darwin.dmg` (Intel) — open it, and drag **secunit.app** into `/Applications`.

The app is **not code-signed**, so on first launch macOS Gatekeeper will refuse it. Either right-click the app → **Open** → **Open** to approve it once, or clear the quarantine flag from the terminal:

```bash
xattr -dr com.apple.quarantine /Applications/secunit.app
```

The GUI is read-only — it never writes inside a project tree; the CLI remains the only path that mutates registry state.

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
  controls/
    bootstrap.yaml          # copy from docs/examples/controls/bootstrap.yaml
    inventory-seed.yaml     # copy from docs/examples/controls/inventory-seed.yaml
  inventory.yaml            # empty doc: `{}` or kind keys with empty lists
  _config.yaml              # see below
  .gitignore                # ignore target/, .DS_Store, etc.
```

No `skills/` directory is needed to start: `bootstrap` and `inventory-seed` — like every standard-library runbook — ship **bundled in the `secunit` binary**, so the controls resolve their skills out of the box (`secunit skills show bootstrap`). You only add a `skills/<name>.md` file later if you want to override a bundled runbook or author a bespoke one; it then takes precedence (see `docs/skills.md`).

The `bootstrap` and `inventory-seed` controls have `cadence: continuous`, so they never fire on schedule — they're invoked on demand to prepare runs. The bundled skills are what the agent reads to actually walk the WISP and the upstream sources.

`_config.yaml` declares integration handles — what your GitHub org is called, which AWS profiles back which cloud accounts, where the WISP source lives. The schema is `schemas/_config.schema.json`. Treat this file as the only place you hand-author secrets-adjacent identifiers; everything downstream resolves through it. The `bootstrap` skill needs `org.wisp_repo` set.

`git init`, commit the stub. The git history of this repo *is* the audit trail — the manifest format pins the registry's git sha into every run, and `run prepare` refuses to allocate a run dir outside a real repo.

## Step 2 — Prepare the bootstrap run

```bash
secunit run prepare bootstrap
```

This allocates `evidence/<year>/<quarter>/bootstrap/<YYYY-MM-DD>-run-001/`, writes `prepare.json` and a `.run-pending` sentinel, and emits the prepare context as JSON to stdout. The agent reads that JSON to know where to drop drafts.

## Step 3 — Run the `bootstrap` skill

The agent loads `skills/bootstrap.md` and walks your WISP repo. For every cadence-bearing obligation it finds — weekly log review, quarterly vuln scan, annual policy reviews, fixed-date procedures — it writes one **draft control YAML** under `<run-dir>/raw/controls/`. Fixed-date obligations land in `<run-dir>/raw/schedule.yaml`. (The skill is org-wide, so the run uses `raw/` rather than `by-system/<n>/raw/` — see `storage.md` on scope layouts.)

The skill also writes `<run-dir>/raw/bootstrap-report.md` summarizing:

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

It writes `<run-dir>/raw/inventory.yaml` grouped by `kind`, with `name`, `tags`, and `in_scope_since` set per entry, plus `<run-dir>/raw/inventory-diff.md` for human review. The schema is `schemas/inventory.schema.json`.

## Step 5 — Review

Open `<run-dir>/raw/bootstrap-report.md` and `<run-dir>/raw/inventory-diff.md`. Walk every flagged ambiguity. Edit drafts in place under `<run-dir>/raw/` — the run dir is just files. This is the only human-in-the-loop step in the bootstrap, and it is non-optional: a bootstrapped registry the operator hasn't read is not a registry.

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

This validates each draft against its schema and **copies** what passes into the live registry. Drafts stay in the run dir for audit; the run's manifest (and its hash) cover them too. Specifically:

- New control YAMLs are written into `controls/`. Controls that already exist by the same id are left untouched (`= aa-weekly-audit-review`).
- `inventory.yaml` is **merged**: missing kinds are added wholesale, and within an existing kind, entries are appended by `name` — the operator's existing tags, aliases, and excludes are preserved.
- `schedule.yaml` and `_config.yaml` are copied **only if absent** in the live registry (operator edits trump drafts).
- Drafts that fail schema validation are listed under `Drafts rejected (...)` with per-file diagnostics; nothing is promoted for those.

`secunit registry import --json <run-dir>` emits the same summary as structured JSON, useful for piping into reviews.

Commit the new `controls/`, `inventory.yaml`, and any `schedule.yaml` to the secunit repo. The first registry checkpoint.

## Step 8 — Verify and probe

```bash
secunit validate              # schema + cross-ref check, clean exit
secunit inventory check       # duplicate names, lifecycle-date sanity
secunit due --within 7        # what's due in the next 7 days
secunit scope <some-control>  # preview which inventory entries iterate
```

If `due` shows the right things on the right dates, bootstrap is done. You are ready to start running real controls — pick one (`sca-weekly-dependency-scan` is a good first), run `secunit run prepare <id>`, follow its skill, finalize, repeat.

## Re-running bootstrap

Bootstrap is **re-runnable** by design. As the WISP evolves:

- New obligations show up as drafts under `<run-dir>/raw/controls/` and `registry import` promotes them.
- Existing obligations whose live control already covers them are reported as **kept** — no draft is emitted, so an operator's hand edits to that control survive the next bootstrap untouched.
- Obligations no longer appearing in the WISP are reported as **orphaned** in `bootstrap-report.md`. The skill never deletes; the operator decides whether to retire (a future `secunit registry rm` will write a tombstone — until then, hand-edit the control or remove the YAML).

The same pattern holds for inventory-seed: new entries are added with `in_scope_since: today`; entries that disappeared upstream are flagged in `inventory-diff.md` with a proposed `secunit inventory retire` command. Retirement is always an operator step, never automatic.

This is what keeps the registry in sync with the WISP without a separate "migration" tool.

## Where to look next

- `spec.md` for the conceptual model.
- `cli.md` for the full CLI surface and exit codes.
- `storage.md` for the on-disk contract: run-dir lifecycle, scope resolution, cadence rules.
- `skills.md` for how skills are authored and what contract they have with the agent.
- `examples/` for reference shapes you can copy-paste from.
