# CLI Reference

`secunit` is a single Rust binary that serves as the agent's helper for registry inspection, run orchestration, evidence capture, and audit verification. The agent invokes it; it does not invoke the agent. All long-form workflow logic lives in skills.

## Top-level

```
secunit [OPTIONS] <COMMAND>

OPTIONS:
    -C, --root <DIR>        Treat DIR as the secunit root [default: cwd]
        --config <FILE>     Alternate _config.yaml path
        --json              Machine-readable output (where applicable)
    -v, --verbose...        Increase verbosity (-v info, -vv debug, -vvv trace)
    -h, --help
    -V, --version

COMMANDS:
    due          Show controls coming due
    status       Show registry-wide or per-control status
    show         Show one control's full configuration
    scope        Preview resolved scope for a control
    features     Show which integrations are compiled in
    skills       List, show, or locate runbook skills

    run          Allocate / finalize / abort runs
    capture      Capture evidence via native integrations

    validate     Validate the registry (schema + cross-refs)
    verify       Verify manifest hash chains

    report       Assemble report data
    registry     Manage controls and schedule
    inventory    Manage the inventory
    risks        Manage the risk register
```

## Inspection

Pure read commands. Default human tables; `--json` flips to structured output suitable for the agent.

```
secunit due [--within <DURATION>] [--overdue-only] [--owner <ROLE>] [--json]
secunit status [<CONTROL_ID>] [--json]
secunit show <CONTROL_ID> [--json]
secunit scope <CONTROL_ID> [--at <DATE>] [--json]
secunit features [--json]
```

`scope` previews what a run would iterate over without allocating a run dir — useful for validating inventory changes before committing.

## Runs

Two-phase, with the agent's skill execution sandwiched between.

```
secunit run prepare <CONTROL_ID> [--note <STRING>] [--at <DATE>] [--human]
secunit run finalize <RUN_DIR> [--json]
secunit run abort <RUN_DIR> --reason <STRING>
secunit run resume <RUN_DIR>
secunit run list --pending
```

`run prepare`:

- Loads and validates the control YAML.
- Resolves `scope` against `inventory.yaml`, filtered by run date against `in_scope_since` / `retired_on`.
- Allocates `evidence/<y>/<q>/<id>/<run-id>/`, creates `by-system/<name>/raw/` per resolved entry, drops a `.run-pending` sentinel.
- Captures `registry_git_sha` (the repo's HEAD; pins inventory.yaml too since it lives in the same repo).
- Resolves the control's `skill` (local-first, then bundled — see **Skills**) and embeds it in the context as `skill: { name, source, sha256 }`, so the agent loads the runbook without knowing where it lives. Fails if the skill resolves to nothing.
- Writes `prepare.json` into the run dir.
- Prints the prepare context to stdout (JSON by default; `--human` for tables).

`run finalize`:

- Reads `prepare.json` and `result.json` from the run dir.
- Hashes every artifact under `raw/` and `by-system/`.
- Reads the prior run's `manifest.json`, computes its sha, embeds in `prior_run.manifest_sha256`.
- Validates the assembled manifest against `manifest.schema.json`.
- Atomically writes `manifest.json` and updates `state.json` (write-temp, fsync, rename).
- Removes the `.run-pending` sentinel.

`run abort` is the only legitimate way to discard a pending run. Records a reason; preserves the run dir so the abort itself is auditable.

## Capture

Native integrations gated behind cargo features. Each capturer:

- Reads credentials from the standard chain or `_config.yaml` integration block — never persisted across invocations.
- Writes canonical JSON to `--out` with shape `{ capturer, version, captured_at, args, result }`.
- Sorts arrays by stable id, ISO-8601 UTC timestamps, strips ephemeral fields.
- Streams paginated results to disk without buffering the full response.
- Prints a one-line stderr summary; exits 0 / 1 / 2.

```
secunit capture aws access-analyzer  --account <NAME> --out <PATH>
secunit capture aws guardduty        --account <NAME> --since <DURATION> --out <PATH>
secunit capture aws config           --account <NAME> --out <PATH>
secunit capture aws network-firewall --account <NAME> --since <DURATION> --out <PATH>
secunit capture aws cloudtrail       --account <NAME> --query <FILTER> --since <DURATION> --out <PATH>
secunit capture aws s3-access-logs   --bucket <NAME> --since <DURATION> --out <PATH>

secunit capture github dependabot-alerts --repo <ORG/REPO> [--state open] --out <PATH>
secunit capture github branch-protection --repo <ORG/REPO> --branch <NAME> --out <PATH>
secunit capture github org-members        --org <NAME> --out <PATH>
secunit capture github audit-log          --org <NAME> --since <DURATION> --out <PATH>
secunit capture github codeql-alerts      --repo <ORG/REPO> --out <PATH>

secunit capture deps pip-audit   --path <DIR> --out <PATH>
secunit capture deps pnpm-audit  --path <DIR> --out <PATH>
secunit capture deps cargo-audit --path <CARGO_LOCK> [--db-path <ADVISORY_DB>] --out <PATH>
secunit capture deps osv-query   --ecosystem <NAME> --package <NAME> --version <STR> --out <PATH>
secunit capture deps sbom        --path <DIR> [--ecosystem <rust|python|node>] --out <PATH>
secunit capture deps sbom        --path <DIR> --discover --out-dir <DIR>

secunit capture http get <URL> [--header <K=V>...] [--auth-from-env <VAR>] --out <PATH>
secunit capture snapshot file <SRC> --out <PATH>
```

### Required external tools

Every `capture deps` action shells out to a tool the operator installs.
`secunit capture deps --help` prints this table, and `secunit doctor` reports
which entries are on `PATH`. A missing binary fails the capture with its install
command and exit 2 — never a partial artifact.

| Binary | Needed by | Install |
|---|---|---|
| `pip-audit` | `deps pip-audit` | `pip install pip-audit` |
| `pnpm` | `deps pnpm-audit` | `npm install -g pnpm` |
| `cargo-cyclonedx` | `deps sbom --ecosystem rust` | `cargo install cargo-cyclonedx` |
| `cyclonedx-py` | `deps sbom --ecosystem python` | `pipx install cyclonedx-bom` |
| `cyclonedx-npm` | `deps sbom --ecosystem node` | `npm install -g @cyclonedx/cyclonedx-npm` |

`deps cargo-audit` and `deps osv-query` need no external binary: the first runs
the `rustsec` library in-process, the second queries OSV.dev over HTTPS.

### SBOM (`capture deps sbom`)

Generates a [CycloneDX](https://cyclonedx.org/) bill of materials — the ECMA-424
International Standard — by shelling out to the CycloneDX project's own
generator for the ecosystem. It is the only capture that writes **two** files:

| File | Contents |
|---|---|
| `<out>` | the bare CycloneDX document, schema-valid and standalone, so an assessor's tooling reads it unchanged |
| `<out>.envelope.json` | the canonical capture envelope: ecosystem, generator name and version, declared spec version, component count, and the BOM's sha256 |

The BOM stays unwrapped because nesting it inside `result` would make it
unreadable to every CycloneDX consumer; the envelope sits beside it and pins its
hash instead.

`--ecosystem` is inferred from the manifests present (`Cargo.toml`,
`requirements.txt` / `poetry.lock` / `uv.lock` / `pyproject.toml`,
`package.json`) and is **required** when a tree carries more than one —
capturing the wrong ecosystem's SBOM is worse than capturing none.

### Which Python projects are supported

`cyclonedx-py` dispatches on input format, so the capturer picks its subcommand
from what the project provides, in this order:

| Project provides | Subcommand | `result.describes` |
|---|---|---|
| `poetry.lock` | `poetry` | `poetry.lock` |
| `requirements.txt` | `requirements` | `requirements.txt` |
| `Pipfile.lock` | `pipenv` | `Pipfile.lock` |
| `.venv/` or `venv/` | `environment` | `.venv` |

**uv, PDM, and hatch projects are supported through their virtualenv.**
Upstream does not read `pyproject.toml` or `uv.lock` directly — the manifest and
lockfile "are not explicitly supported" while the virtual environments "are
fully supported" — so a PEP-621 project is captured from its own `.venv`. Run
`uv sync` (or `pdm install`, `hatch env create`) once, and the capture works
from then on.

A declared manifest wins over an installed venv: it describes what the project
*asks for* and needs no install step. `result.describes` records which was used,
because resolved-from-a-lockfile and read-from-an-install are different
evidentiary claims — the second includes whatever is actually installed, the
first only what is declared.

Pointing at an in-project `.venv` is not the ambient-interpreter hazard
`capture deps pip-audit` refuses: the path is explicit and belongs to the
project, so the BOM describes that tree rather than whichever interpreter
happens to be on `$PATH`. A PEP-621 project with no venv and no lockfile fails
with the command to run rather than a "manifest unsupported" error.

Spec version follows what each generator supports, and the artifact is validated
against whatever version it declares:

| Ecosystem | Generator | CycloneDX |
|---|---|---|
| `rust` | `cargo-cyclonedx` | 1.5 — its `--spec-version` accepts no higher |
| `python` | `cyclonedx-py` | 1.6 |
| `node` | `cyclonedx-npm` | 1.6 |

Two captures of an unchanged tree are byte-identical: CycloneDX's `serialNumber`
and `metadata.timestamp` regenerate on every run, so both are stripped and the
component and dependency arrays are sorted by `purl` before the BOM is written.

Each generator reads a different thing, so the checkout has to be in the right
state:

| Ecosystem | Reads | Prepare the checkout with |
|---|---|---|
| `rust` | `Cargo.toml` + `Cargo.lock` | `cargo generate-lockfile` if no lockfile is committed |
| `python` | `requirements.txt`, `poetry.lock`, or `Pipfile.lock` | nothing — the manifest is parsed, not installed |
| `python` (uv / PDM / hatch) | the project's own `.venv` or `venv` | `uv sync`, `pdm install`, or `hatch env create` |
| `node` | the **installed** `node_modules` tree, via `npm ls` | `npm install` (or `npm ci`) first |

The Node arm is the one that needs an install step: `cyclonedx-npm` resolves
against `node_modules`, not `package-lock.json`, and fails with npm's
`ELSPROBLEMS` if packages are missing. A sweep over repos that are cloned but
not installed should record `needs-operator` for those systems rather than treat
the failure as a finding.

The Rust arm is the one `deps` capture that is not strictly read-only against
the target tree. `cargo-cyclonedx` has no stdout mode and no output-directory
flag — it writes a BOM next to every `Cargo.toml` — so the capturer writes under
a process-unique filename, reads it, and deletes it, including on the error
path. It refuses to start if such a file already exists rather than overwrite
it. A crash between write and cleanup can leave a `secunit-sbom-<pid>.json`
behind.

For a cargo workspace, the BOM at the workspace root is used. A **virtual
manifest** (`[workspace]` with no `[package]`, which is secunit's own shape)
yields one BOM per member and no root BOM, so the capture fails rather than
invent a merged document — point `--path` at a single member.

A generator that exits non-zero fails the capture and writes nothing, even when
it produced a parseable BOM. `cyclonedx-npm` against a partially-installed
`node_modules` emits a schema-valid document that is missing a subtree;
recording it would understate `component_count` and make the next sweep diff it
as "dependencies removed".

### Repos with no manifest at the root (`--discover`)

`--path` reads only the directory it is given. A repo whose projects live one or
more levels down — `api/` python, `client/` node, nothing at the root — produces
no SBOM at all, and the operator has to know the layout, run one capture per
project, and name each output by hand.

```
secunit capture deps sbom --path . --discover --out-dir evidence/sbom
```

walks the tree and writes **one artifact per project** — `<out-dir>/<relative
path>.cdx.json` plus its `.envelope.json` sidecar — with an index at
`<out-dir>/index.json` listing every project's path, ecosystem, status, and its
BOM's sha256, so a control diffs one file instead of enumerating N.

`--discover` and `--out` are mutually exclusive, as are `--discover` and
`--ecosystem`: discovery infers the ecosystem per project.

There is no merged BOM, and there will not be one. A CycloneDX document carries
a single `metadata.component`, so folding a Python and a Node dependency graph
into one would invent a deployable that does not exist. One BOM per deployable
is also the model `inventory.yaml` and the `capture-sweep` skill already assume.

**Descent stops at each project root.** An npm workspace root is captured
*once*: `cyclonedx-npm` already aggregates every member and their transitive
deps into the root's BOM, so walking on into `packages/*` would emit the same
components again under a second artifact. The same holds for a cargo workspace
with a root `[package]`.

**What the walk never enters:**

| Skipped | Why |
|---|---|
| `node_modules/`, `venv/` | installed trees — every vendored package has its own manifest |
| `target/` | cargo's build directory, which vendors manifests the same way |
| any dot-directory | `.claude/worktrees/` holds git worktrees of the same repo, whose projects would be captured a second time under a different path; `.venv` and `.tox` fall out of the same rule |
| symlinked directories | a link back into the tree would loop the walk; a link out of it would capture something the repo does not own |

The root itself is exempt from the dot rule — a checkout under `~/.local/src` or
a temp dir must still be walkable.

A directory carrying manifests for more than one toolchain becomes one project
per ecosystem, and only then does the artifact name carry the ecosystem
(`service.node.cdx.json`, `service.python.cdx.json`) — two BOMs of one directory
cannot share a filename. A project at the discovery root is written as
`sbom.cdx.json`.

**One project's failure costs only that project.** A missing generator or a
`node_modules` that was never installed is recorded in the index as
`needs-operator`, with the generator's error and the exact `secunit capture deps
sbom …` command that captures it once the operator has acted — the same
treatment `capture-sweep` gives a system it could not capture. The run exits 0
as long as the index was written: a repo where one subproject is uninstalled
still yields real evidence for the rest, and the index already says what is
missing. Discovery fails (exit 2) only when the tree holds no supported manifest
at all.

Two runs over an unchanged tree are byte-identical, index included: projects are
sorted by path, and relative paths are recorded `/`-separated so the index does
not depend on which OS captured it.

## Audit

```
secunit validate [--strict]
secunit verify [<CONTROL_ID>] [--from <DATE>] [--json]
secunit doctor [--json]
```

`validate` checks:

- Every YAML parses against its JSON schema.
- Every `control.skill` resolves — local `skills/<name>.md` or a bundled skill.
- Every `control.policy` path exists.
- Every `scope.kind` matches an inventory section.
- No `id` collisions across controls; no `name` collisions within an inventory kind.
- Every `requires_features:` listed by a skill is present in `secunit features`.
- `schedule.yaml` overrides reference real control ids.

Run as a pre-commit hook. `--strict` adds opinionated checks (NIST id format, descriptive title length, scope minimum-tag rules).

`verify` walks every run for a control (or all controls) in chronological order, recomputes every artifact hash, and checks each `prior_run.manifest_sha256` against the recomputed sha of the prior manifest. It also walks each risk's `events.jsonl` chain and confirms every `finding_ref` resolves to a sealed manifest whose recomputed sha matches. Single point of integrity for an assessor.

`doctor` is a read-only preflight that automates the Part B audit in [`setup-checklist.md`](setup-checklist.md). It groups checks into five sections — **Environment** (secunit version, compiled-in capture features, root is a git repo, declared integrations vs. compiled features), **Repo structure** (`_config.yaml`/`org.wisp_repo`, a non-empty `controls/`, `inventory.yaml`/`schedule.yaml`/`state.json` presence, `.gitignore` ignores `.secunit.lock`), **Registry** (the same schema + cross-reference + skill `requires_features` checks `validate` runs), **Evidence integrity** (the `verify` hash-chain walk over runs and risk logs), and **Risk register** (folds every `risks/<id>/events.jsonl` — validating the append-only event-log format — and confirms `risks/index.json` is a fresh projection of the logs). Each line is `✓`/`ℹ`/`⚠`/`✗`; doctor exits 1 if any check fails (warnings and notes do not fail it). Run it when standing up or inheriting a registry, or as a periodic health check.

Every `⚠`/`✗` line carries a `fix:` (a `fix` field under `--json`) with the concrete next action, so an agent can remediate from the report alone. The fixes distinguish trouble that is **safe to auto-repair** (`git init`, `secunit risks rebuild`, editing `_config.yaml`) from **integrity failures it must not auto-repair** — a broken manifest or risk-log hash chain means evidence was altered, and the fix says to investigate rather than re-finalize or hand-edit the append-only logs.

## Risk register

The risk register is authoritative state held under `risks/` as an append-only
event log per risk; see [`risks.md`](risks.md) for the model. Mutating verbs each
append one event under the root lock and refresh `risks/index.json`; read verbs
are pure folds.

```
secunit risks open <CONTROL_ID> --from <RUN_DIR> --finding <ID> [--owner <ROLE>] [--sla-days <N>]
secunit risks assign <RISK_ID> --owner <ROLE>
secunit risks score <RISK_ID> --impact <N> --likelihood <N> --reason <STRING>
secunit risks retitle <RISK_ID> --title <STRING> [--reason <STRING>]
secunit risks status <RISK_ID> --to <STATUS> --reason <STRING>
secunit risks relink <RISK_ID> --from <RUN_DIR> --finding <ID>
secunit risks link <RISK_ID> --system <NAME> --id <EXT_ID> --url <URL>
secunit risks observe <RISK_ID> --system <NAME> --status <STATUS>
secunit risks note <RISK_ID> --text <STRING>
secunit risks remediate <RISK_ID> [--evidence <RUN_DIR>] --note <STRING>
secunit risks reopen <RISK_ID> --reason <STRING>
secunit risks except <RISK_ID> --rationale <STRING> --approved-by <WHO> --expires <DATE>

secunit risks list [--status <S>] [--severity <LIST>] [--owner <ROLE>] [--past-sla] [--json]
secunit risks show <RISK_ID> [--json]
secunit risks rebuild
```

`risks open --from` reads the named `draft_risk` from the sealed manifest,
allocates the next `R-NNNN`, and verifies the manifest exists with a matching sha
before writing the `opened` event — a risk cannot be bound to absent or fabricated
evidence. `status` transitions are validated against the status machine in
[`risks.md`](risks.md#status-machine). `rebuild` regenerates `index.json` from the
logs, the way `state.json` rebuilds from manifests.

## Skills

```
secunit skills list [--json]
secunit skills show <NAME>
secunit skills path <NAME>
```

Skills are runbook markdown files. The standard-library runbooks
(`capture-sweep`, `attestation-review`, `policy-annual-review`, `report`,
`bootstrap`, `inventory-seed`) are **bundled into the binary** and ship with each
release — no install step. An org overrides any of them, or adds its own, by
dropping `<root>/skills/<name>.md`.

Every skill reference — a control's `skill:`, a runbook's `skill_args.extend:`,
`validate`, `run prepare`, and `skills show/path` — resolves through **one order:
local `<root>/skills/<name>.md` first, then bundled.** That uniform lookup is what
lets `run prepare` hand the agent a runbook by name and lets a release update the
standard library without touching any org repo.

- `skills list` — every skill available to this root (bundled ∪ local), each
  tagged `bundled` or `local`. `--json` adds `requires_features` and `description`.
- `skills show <name>` — print the resolved skill markdown to stdout. This is the
  call the agent / `/ciso` front door uses to load a runbook.
- `skills path <name>` — print a filesystem path; bundled skills are materialised
  into a per-version cache dir so tools that need a real path can read them.

## Reports

```
secunit report data --week <YYYY-Wnn> [--out <PATH>]
secunit report data --month <YYYY-MM> [--out <PATH>]
secunit report data --quarter <YYYY-qN> [--out <PATH>]
secunit report data --year <YYYY> [--out <PATH>]
```

Period selectors are case-insensitive and tolerate missing zero-padding (`2026-Q3`, `2026-W5` normalize to `2026-q3`, `2026-W05`). Aggregates one period's manifests, coverage, state, and the risk register into JSON the `report` skill renders to markdown — per-control period status (satisfied/late/gap/open), sealed runs with draft-risk/issue counts, overdue controls, the register's opened/closed delta, and what's due next. Exactly one period selector is required; output goes to `--out` or stdout, always as JSON. The binary never composes prose — and it never publishes; filing the rendered report as a tracker issue (GitLab, Linear) is the `report` skill's job, configured under `report.publish` in `_config.yaml`.

## Registry / inventory management

```
secunit registry add <YAML_PATH>
secunit registry rm <CONTROL_ID> --reason <STRING>
secunit registry import <BOOTSTRAP_RUN_DIR>
secunit registry diff <CONTROL_ID>

secunit inventory list [--kind <NAME>] [--json]
secunit inventory add --kind <NAME> --name <NAME> [--tags <TAG>...] [--url <URL>]
secunit inventory retire --kind <NAME> --name <NAME> --on <DATE> --reason <STRING>
secunit inventory check
```

`registry rm` does not delete history — it marks the control orphaned and preserves prior evidence. Reuse the same id only after operator confirmation.

`registry import` promotes drafts written by a `bootstrap` run into the live registry, validating each before commit.

`inventory retire` sets `retired_on` rather than deleting; historical evidence remains discoverable.

## Output and exit conventions

| Subcommand | Default | `--json` flips to |
|---|---|---|
| `due`, `status`, `show`, `scope`, `coverage` | human tables | structured JSON |
| `run prepare` | structured JSON | (already JSON; `--human` for tables) |
| `run finalize` | human checklist | structured JSON |
| `capture *` | writes JSON to `--out`; stderr summary | (no flip — `--out` is the contract) |
| `validate`, `verify` | human report | structured JSON |
| `risks list`, `risks show` | human tables | structured JSON |
| `features` | table | structured JSON |

Exit codes:

| Exit | Meaning |
|---|---|
| 0 | Success |
| 1 | Validation or verification failure (data is wrong) |
| 2 | Runtime failure (network, auth, missing dep) |
| 3 | Usage error (bad flags) |
| 4 | Pending run prevents action |

## Cargo features

```toml
[features]
default = ["aws", "github", "deps", "http"]
aws    = ["aws-config", "aws-sdk-accessanalyzer", "aws-sdk-guardduty",
          "aws-sdk-config", "aws-sdk-networkfirewall", "aws-sdk-cloudtrail",
          "aws-sdk-s3"]
github = ["octocrab"]
deps   = ["reqwest"]    # OSV + PyPA advisory DB
http   = ["reqwest"]
gcp    = []
```

Operators install with the features they need; skills declare `requires_features:` in their frontmatter so `secunit validate` flags missing capabilities before a run starts.

## End-to-end session

```bash
$ secunit due --within 3d
ID                              CADENCE   DUE      STATUS  LAST RUN
sca-weekly-dependency-scan      weekly    today    due     2026-04-27

$ secunit run prepare sca-weekly-dependency-scan > /tmp/prep.json
$ jq -r .run_dir /tmp/prep.json
evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001/

# agent loads skills/sca-weekly-dependency-scan.md, executes:
$ secunit capture deps pip-audit --path ../app-api \
    --out evidence/.../by-system/app-api/raw/pip-audit.json
$ secunit capture github dependabot-alerts --repo <org>/app-api \
    --out evidence/.../by-system/app-api/raw/dependabot.json
$ secunit capture deps pnpm-audit --path ../app-ui \
    --out evidence/.../by-system/app-ui/raw/pnpm-audit.json
# ... agent writes findings.md and result.json into the run dir ...

$ secunit run finalize evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001/
✓ hashed 8 artifacts
✓ chained to prior 2026-04-27-run-001
✓ wrote manifest.json
✓ updated state.json (next_due 2026-05-11)

$ secunit verify sca-weekly-dependency-scan
✓ 14 runs verified, hash chain intact
```
