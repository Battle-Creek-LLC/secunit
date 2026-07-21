# secunit

`secunit` is the operational layer for an organization's **Written
Information Security Program (WISP)**. It turns the policies, procedures,
and review cycles defined in a WISP into a tracked, evidence-backed schedule
of recurring security activities — so nothing the WISP mandates silently
lapses, and every completed activity leaves tamper-evident evidence an
assessor (SOC 2, customer security questionnaires, pentesters) can verify.

It is designed to be **agent-paired**. The agent reads the registry,
executes each control's runbook through a dedicated **skill**, captures
evidence, files findings, and updates state. The workflows live in skills;
the binary stays narrow — it does the filesystem-level chores: registry
inspection, scope resolution, run-directory allocation, hashing, manifest
assembly, hash-chain verification, and native evidence capture. **The binary
never invokes the agent.**

## Concepts

- **Control** — a cadence-bearing obligation from the WISP, expressed as a
  YAML file with an id, owner, cadence, skill name, scope, and evidence
  requirements.
- **Inventory** — the in-scope systems (source repos, cloud accounts, SaaS,
  sites) in `inventory.yaml`; a control's `scope:` resolves against it.
- **Skill** — the runbook the agent follows to actually do the work. The
  reusable standard-library runbooks ship bundled in the binary; an org
  overrides any of them, or adds its own, with a local `skills/<name>.md`.
- **Run** — a two-phase lifecycle (`prepare` → agent executes skill →
  `finalize`) that allocates `evidence/<year>/<quarter>/<id>/<run-id>/`.
- **Evidence** — hash-chained artifacts and a `manifest.json` per run; each
  manifest pins the prior run's manifest sha and the registry git sha.
- **State** — `state.json` tracks last-run / next-due per control.

`secunit` boots from an existing WISP via the bundled `bootstrap` skill,
which walks the policy/procedure documents and emits a draft registry; an
`inventory-seed` skill populates `inventory.yaml`. Bootstrap is re-runnable
to keep the registry in sync as the WISP evolves.

## Install / build

`secunit` is a single Rust binary (the CLI crate is published as
`bcl-secunit`; the installed binary is named `secunit`). It targets the Rust
toolchain pinned in `rust-toolchain.toml`.

Build from source:

```bash
git clone https://github.com/Battle-Creek-LLC/secunit.git
cd secunit
cargo build --release        # builds the default workspace members
./target/release/secunit --version
```

Install the CLI with cargo:

```bash
cargo install bcl-secunit
```

Cargo features gate the native capturers — the default set is `deps`,
`github`, and `aws`, with `http` available as an opt-in. (`aws` is currently a
placeholder so skills that declare `requires_features: [aws]` validate
cleanly; no AWS capturers are compiled in yet.) Build with only what your
environment needs (`cargo build --no-default-features --features
github,deps`). Skills declare `requires_features:` so `secunit validate` flags
missing capabilities before a run starts. See
[`docs/cli.md`](docs/cli.md#cargo-features) for the full list.

> The desktop GUI (`secunit-gui`, Tauri) is intentionally excluded from the
> default `cargo build` so headless CI and core development stay fast. Build
> it explicitly with `cargo build -p secunit-gui` or `cargo tauri dev`.

## Usage

`secunit [OPTIONS] <COMMAND>`. Global options include `-C, --root <DIR>`
(treat DIR as the secunit root), `--config <FILE>`, `--json` (machine-
readable output where applicable), and `-v/-vv/-vvv` for verbosity.

### Inspect the registry

```bash
secunit due --within 7d           # controls coming due
secunit status [<CONTROL_ID>]     # registry-wide or per-control status
secunit show <CONTROL_ID>         # one control's full configuration
secunit scope <CONTROL_ID>        # preview resolved scope without allocating
secunit coverage <CONTROL_ID>     # period-by-period coverage for one control
secunit features                  # which integrations are compiled in
```

### Run a control (two-phase)

```bash
secunit run prepare <CONTROL_ID>   # allocate run dir, resolve scope+skill, emit context
# ... the agent loads the resolved skill and executes it, writing
#     findings.md / result.json and raw artifacts into the run dir ...
secunit run finalize <RUN_DIR>     # hash artifacts, chain manifest, update state
secunit run abort <RUN_DIR> --reason "<why>"   # the only way to discard a pending run
```

### Capture evidence

Native integrations write canonical JSON to `--out`. A few examples (full
matrix in [`docs/cli.md`](docs/cli.md#capture)):

```bash
secunit capture github dependabot-alerts --repo <org>/<repo> --out <path>
secunit capture deps cargo-audit --path <dir> --out <path>
secunit capture deps pip-audit --path <dir> --out <path>
```

### Skills

```bash
secunit skills list          # bundled ∪ local runbooks
secunit skills show <NAME>    # print resolved runbook markdown (agent front door)
secunit skills path <NAME>
```

### Validate and verify

```bash
secunit validate [--strict]   # schema + cross-ref checks (run as a pre-commit hook)
secunit verify [<CONTROL_ID>]  # recompute and check the manifest hash chains
```

`verify` is the single point of integrity for an assessor: it walks every
run in chronological order, recomputes each artifact hash, and confirms each
run's `prior_run.manifest_sha256` matches the recomputed sha of the prior
manifest.

### Reports

```bash
secunit report data --week 2026-W30 --out raw/report-data.json   # also --month / --quarter / --year
```

Aggregates one period's coverage, runs, overdue controls, and risk-register
delta into JSON; the bundled `report` skill renders it to a stakeholder
report under `reports/` and — when `report.publish` is configured in
`_config.yaml` — files it as a GitLab or Linear issue, recording the issue
URL in the run's evidence. The binary assembles data only; prose and
publishing are the agent's job.

### Other commands

`secunit registry` and `secunit inventory` manage controls, the schedule, and
the inventory. See the CLI reference for details.

## Documentation

- [`docs/spec.md`](docs/spec.md) — what `secunit` is, the concepts, runtime
  architecture, two-phase run model, workflow, and scope.
- [`docs/cli.md`](docs/cli.md) — full CLI reference: subcommands, flags,
  output modes, exit codes, cargo features, end-to-end session.
- [`docs/getting-started.md`](docs/getting-started.md) — stand up a registry
  against an existing WISP, end to end (bootstrap → inventory-seed → import).
- [`docs/skills.md`](docs/skills.md) — how skills work, the skill contract,
  multi-system iteration, and `requires_features` declaration.
- [`docs/storage.md`](docs/storage.md) — on-disk layout, run-dir lifecycle,
  inventory/scope/cadence resolution, manifest hash chaining.
- [`docs/examples/`](docs/examples/) — reference inventory, controls, skills,
  schedule, state, evidence runs, and a generated quarterly report.
- [`PLAN.md`](PLAN.md) — phased implementation plan.
- [`CHANGELOG.md`](CHANGELOG.md) — notable changes.

## Security

To report a vulnerability, see [`SECURITY.md`](SECURITY.md).

## License

Dual-licensed under either of [MIT or Apache-2.0](LICENSE) at your option.
Copyright (c) 2026 Battle Creek LLC.
