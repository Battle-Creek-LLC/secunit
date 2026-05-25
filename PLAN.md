# Build and Test Plan

How to take `secunit` from spec to running against the operator's WISP. Phased, value-incremental, each phase ends with something you can use.

## Goals

1. Working single Rust binary that exercises every documented subcommand.
2. End-to-end: bootstrap from the operator's WISP → first weekly runs producing hash-chained evidence → first quarterly report assembled from real evidence.
3. CI-enforced schema and hash-chain integrity from day one.
4. Test surface that catches regressions in registry math, hash chaining, and capture canonicalization without requiring live cloud credentials.

## Out of scope for this plan

- Open-source release, marketing, or distribution beyond release artifacts for the operator's own machines.
- A second org. The first target org is the only target until everything below is solid.
- Migration tooling between schema versions. Schemas are at v1; bumps are a future concern.

## Repo layout

Single cargo workspace at `secunit/`. The existing `secunit/docs/` stays put. Add:

```
secunit/
  Cargo.toml                 # workspace
  rust-toolchain.toml        # pin the rust version
  .github/workflows/         # ci.yml, release.yml
  crates/
    secunit-core/            # model, registry, evidence, hashing — library
    secunit-cli/             # clap dispatch — binary
    secunit-capture/         # capture subsystems, behind cargo features
  testdata/
    orgs/<scenario>/         # fixture trees for integration tests
    fixtures/captures/       # recorded upstream responses for capture tests
  schemas/                   # control, manifest, inventory JSON Schemas
  PLAN.md                    # this file
  docs/                      # spec, cli, storage, skills, examples
```

The split into three crates is for testability — `secunit-core` stays library-shaped so integration tests can drive it directly without going through `clap`.

## Phases at a glance

| Phase | Goal | Days |
|---|---|---|
| 0 | Foundations: workspace, schemas, CI, sccache | 1–2 |
| 1 | Registry math: parse, validate, due, scope | 2–3 |
| 2 | Run lifecycle: prepare, finalize, verify | 2–3 |
| 3 | Bootstrap from the target WISP into a real registry | 2–3 |
| 4 | GitHub + deps captures; first live `sca-weekly` run | 3–4 |
| 5 | AWS captures; first live `aa-weekly` run | 3–4 |
| 6 | Reports: first quarterly report from real evidence | 2–3 |
| 7 | Hardening: pre-commit, release packaging, ergonomics | ongoing |
| 8 | Internal risk register: append-only log, CLI, verify, GUI view | 3–4 |

Total: roughly three weeks of focused work to a useful system, then you mostly write skills. Phase 8 is an additive feature phase, designed in [`docs/risks.md`](docs/risks.md); it depends on Phases 2 (run lifecycle, root lock, hashing) and 6 (report assembly).

---

## Phase 0 — Foundations

**Goal.** A workspace that compiles, with empty crates wired up, schemas in place, CI green.

**Deliverables.**

- `Cargo.toml` workspace; three member crates with `lib.rs` / `main.rs` stubs.
- `rust-toolchain.toml` pinned to a current stable.
- JSON Schemas in `schemas/` for `control`, `manifest`, `inventory`, `state`, `result`, `prepare`, `_config`.
- GitHub Actions workflow: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`.
- `sccache` enabled in CI to keep AWS-SDK rebuild times sane in later phases.
- `pre-commit` config running `cargo fmt`, `cargo clippy`, schema validation on YAML/JSON in `docs/examples/`.

**Tests.**

- CI runs on the empty crates and passes.
- Schemas validate every example file under `docs/examples/`.

**Exit criteria.** `cargo build --release` produces a `secunit` binary that prints `--version` and `--help`. CI is green.

---

## Phase 1 — Registry math

**Goal.** Read-only subcommands work end-to-end against the example registry under `docs/examples/`.

**Deliverables.**

- `secunit-core::model` — Control, Inventory, Schedule, State, Config types with `serde` + schema validation.
- `secunit-core::registry::loader` — walks `controls/`, `inventory.yaml`, `schedule.yaml`, `state.json`, `_config.yaml`. Reports per-file errors.
- `secunit-core::registry::resolver`:
  - `next_due(control, schedule, state, today) -> Date` per cadence rules in `storage.md`.
  - `is_overdue(control, today) -> bool` with grace periods.
  - `resolve_scope(control, inventory, run_date) -> Vec<ResolvedSystem>`.
- `secunit-cli` subcommands: `due`, `calendar`, `status`, `show`, `scope`, `history`, `features`, `validate`.
- Output: human tables by default, `--json` produces structured output validated against published schemas.

**Tests.**

- **Unit:** table-driven cadence math (every cadence × edge dates × override matrix).
- **Unit:** scope resolution edge cases (retired, in_scope_since, excludes, aliases).
- **Golden:** `cargo insta` snapshots of human and JSON output for each subcommand against three fixture orgs in `testdata/orgs/`:
  - `minimal/` — one control, no inventory, no schedule overrides.
  - `multi-system/` — controls with `scope:` block, inventory with retired entries.
  - `with-overrides/` — schedule.yaml exercising skip/insert/override.
- **Property:** `proptest` checking that `next_due` is monotonic in time and that `validate` accepts every valid registry the property generator can produce.

**Exit criteria.** `secunit due`, `secunit scope`, `secunit validate` all run against `docs/examples/` and produce expected output. `secunit validate` is wired into pre-commit.

---

## Phase 2 — Run lifecycle

**Goal.** A control can be run end-to-end with manually authored evidence, and the hash chain holds across runs.

**Deliverables.**

- `secunit-core::evidence::runner`:
  - `prepare(control_id, root, today) -> RunContext`.
  - Allocates `<root>/evidence/<y>/<q>/<id>/<run-id>/`, creates `by-system/<name>/raw/` per resolved scope, writes `.run-pending` and `prepare.json`.
- `secunit-core::evidence::hasher`:
  - SHA-256 over every file under `raw/` and `by-system/`.
  - Atomic manifest write (write-temp, fsync, rename).
  - Prior-run linkage via `manifest_sha256`.
- `secunit-core::evidence::verifier` — walks runs in chronological order, recomputes every hash, checks chain.
- CLI: `run prepare`, `run finalize`, `run abort`, `run resume`, `run list --pending`, `verify`.
- File lock at `<root>/.secunit.lock` so concurrent invocations against the same root serialize state writes.

**Tests.**

- **Round-trip:** prepare → drop a hand-crafted `result.json` and known artifacts → finalize → verify. Repeat across 3 runs to exercise the chain.
- **Tamper detection:** modify a sealed manifest's body and confirm `verify` fails with a precise diagnostic.
- **Atomicity:** kill the finalize process at controlled points; assert manifest is either fully written or not present (no partial writes).
- **Abort path:** prepare → abort → confirm `.run-pending` removed, `abort.json` written, no manifest.
- **Resume:** prepare → restart agent → resume → confirm context matches original prepare exactly.
- **Concurrency:** two `run prepare` invocations against the same root; one wins, the other exits cleanly.

**Exit criteria.** Hand-crafted runs produce valid manifests; verify catches every form of tampering tested.

---

## Phase 3 — Bootstrap from the target WISP

**Goal.** Skill-driven bootstrap produces a real, curated registry from the operator's WISP repo (`<org>-docs/security/` or equivalent).

**Deliverables.**

- `skills/bootstrap.md` — agent-side skill: walks WISP, extracts cadence-bearing obligations, emits draft `controls/`, `inventory.yaml` skeleton, `schedule.yaml`, `_config.yaml` stubs, `bootstrap-report.md`.
- `skills/inventory-seed.md` — agent-side skill: enumerates source repos, cloud accounts, SaaS providers; populates `inventory.yaml` from `_config.yaml` integration data + WISP access dictionaries.
- CLI: `secunit registry import <bootstrap-run-dir>` — validates each draft and promotes to `controls/`.
- The bootstrap run itself is a `secunit run prepare → skill → secunit run finalize` cycle, so the bootstrap output is hash-chained from day one.

**Tests.**

- Run bootstrap against the operator's WISP repo. Manually review `bootstrap-report.md` for accuracy.
- Confirm `secunit validate` accepts the imported registry.
- Re-run bootstrap. Confirm idempotency: no duplicate controls, new obligations flagged, removed obligations marked orphaned.
- Modify a WISP policy (add a new "every six weeks" obligation). Re-run. Confirm new draft control appears.

**Exit criteria.** Target org's registry checked in. `secunit due` shows the right things due in the next 7 days against today's date. Bootstrap is re-runnable.

---

## Phase 4 — Source-side captures and first live `sca-weekly` run

**Goal.** `sca-weekly-dependency-scan` runs end-to-end against the target org's real repos, producing hash-chained evidence with canonical capture output.

**Deliverables.**

- `secunit-capture::deps`:
  - `pip-audit` — invokes pypa/pip-audit's library API or, if not feasible, the binary; canonicalizes output.
  - `pnpm-audit` — similar, using `pnpm audit --json` semantics.
  - `cargo-audit` — using the rustsec advisory DB directly.
  - `osv-query` — REST client against osv.dev.
- `secunit-capture::github` — uses `octocrab`:
  - `dependabot-alerts`, `branch-protection`, `org-members`, `audit-log`, `codeql-alerts`.
- Canonical output schema enforcement: every capture writes `{ capturer, version, captured_at, args, result }` with sorted arrays, ISO-8601 UTC timestamps, ephemeral fields stripped.
- CLI: full `capture deps …` and `capture github …` surface.

**Tests.**

- **Capturer unit tests:** recorded HTTP responses (via `wiremock` or static fixtures under `testdata/fixtures/captures/github/*.json`); assert byte-identical canonical output.
- **Schema tests:** every capturer's output validates against its published schema.
- **Idempotency:** run the same capture twice against the same fixture; assert byte-identical output.
- **Live smoke test:** run `sca-weekly-dependency-scan` against the target org's real repos. Manually inspect `findings.md`. Run a second time the next week. Confirm diff captures real change.

**Exit criteria.** Two consecutive `sca-weekly-dependency-scan` runs against the target org with intact hash chain and meaningful diffs between them.

---

## Phase 5 — AWS captures and first live `aa-weekly` run

**Goal.** `aa-weekly-audit-review` runs end-to-end against the target org's real AWS account.

**Deliverables.**

- `secunit-capture::aws` (gated behind `aws` cargo feature) using `aws-config` + `aws-sdk-*`:
  - `access-analyzer`, `guardduty`, `config`, `network-firewall`, `cloudtrail` (with `--query`), `s3-access-logs`.
- Streaming pagination — captures stream paginated SDK responses to disk without buffering full results.
- Credential discipline: standard credential chain via `aws-config`; never log credentials; `tracing` filters set to drop SDK debug output above `info`.
- CLI: full `capture aws …` surface.

**Tests.**

- **Capturer unit tests:** recorded SDK responses (using `aws-smithy-mocks` or hand-crafted JSON fixtures); assert canonical output.
- **Pagination test:** synthetic large response (1000+ findings) flows through streaming path without buffering — confirm with memory profile.
- **Auth failure path:** capture invoked without credentials; exit code 2 with redacted diagnostic.
- **Live smoke test:** run `aa-weekly-audit-review` against the target org's prod account. Manually inspect findings. Run again the following week. Confirm diff is meaningful.

**Exit criteria.** Two consecutive `aa-weekly-audit-review` runs with intact hash chain. Both `sca-weekly` and `aa-weekly` are running on cadence with no operator intervention beyond reviewing findings.

---

## Phase 6 — Reports

**Goal.** First quarterly report assembled from real evidence at the end of Q2 2026.

**Deliverables.**

- `secunit-core::reports::data_assembler`:
  - Walks `evidence/<y>/<q>/`, summarizes per-control activity, counts on-time vs late, extracts open risks from manifest `external_links` (superseded by the `risks/` register once Phase 8 lands; the assembler then reads open risks from `risks/index.json`).
  - Emits structured JSON.
- CLI: `secunit report data --quarter <yyyy-qN> --out <path>`, `--year`, `--policy-status`.
- `skills/report-quarterly.md` — agent-side skill that reads `report-data.json`, composes `reports/<y>-<qN>-quarterly.md` matching the example shape under `docs/examples/reports/`.
- `skills/report-annual.md`, `skills/report-policy-review-status.md` — same pattern, lower priority.

**Tests.**

- **Data assembler:** golden tests against the multi-system fixture org, three quarters of synthetic evidence runs, assert known JSON output.
- **End-to-end:** run `secunit report data --quarter 2026-q2`; load output through `report-quarterly` skill; manually review markdown against the example.
- **Late-control surfacing:** synthetic fixture with one control that missed two consecutive weekly runs; assert the report calls it out.

**Exit criteria.** Q2 2026 quarterly report committed and reviewed with the Owner.

---

## Phase 7 — Hardening

**Goal.** The system requires only routine attention — capture additions, skill edits — not core engineering.

**Deliverables.**

- `cargo-dist` (or equivalent) for release packaging: signed macOS + Linux binaries.
- Pre-commit hook ships in the repo template (`secunit validate` + `cargo fmt` + `cargo clippy`).
- Performance pass on `verify` if it becomes slow against a year of evidence (target: under 10 seconds for a full chain across all controls).
- Add new captures as skills demand them — each new capture follows the contract from Phase 4.
- Documentation polish: `cli.md` matches every flag the binary actually exposes; `storage.md` reflects any schema bumps.

**Tests.** Ongoing — every new capture follows the test pattern from Phase 4 (recorded fixtures, schema validation, idempotency).

**Exit criteria.** None — this phase is steady-state.

---

## Phase 8 — Internal risk register

**Goal.** `secunit` owns the risk register as an append-only, hash-chained event log under `risks/`. Findings flow into it; `verify` covers it; the GUI shows it read-only. Designed in [`docs/risks.md`](docs/risks.md). Reverses the original "link out to the tracker" non-goal — the register is authoritative, the tracker is a mirror synced out to.

**Deliverables.**

- Schemas: `schemas/risk-event.schema.json` (the `events.jsonl` line envelope + per-`type` `data` payloads) and `schemas/risk-index.schema.json`.
- `secunit-core::risks`:
  - Event model + `append(risk_id, event)` — takes the root lock, reads the tail line for `seq` + `prev_sha256`, validates the transition against the status machine and the event schema, writes one line with `O_APPEND`, refreshes the index entry.
  - `fold(events) -> RiskState` — deterministic left-fold; the single source of "current state".
  - `index` build / `rebuild` from the logs (the `state.json` rebuild analogue).
  - `R-NNNN` id allocation under the lock; fingerprint `<control_id>:<finding_id>` for cross-run identity.
- `secunit-core::evidence::verifier` extended: walk each risk's `prev_sha256` chain, and resolve every `finding_ref` to a sealed manifest whose recomputed sha matches.
- `secunit-cli`: `risks open | assign | score | status | relink | link | observe | note | remediate | reopen | except` (mutating) and `risks list | show | rebuild` (read). `risks open --from` reads the named `draft_risk` from the sealed manifest and defaults the SLA from the source control's `remediation_thresholds`.
- `secunit-core::reports::data_assembler` reads open risks from `risks/index.json` instead of manifest `external_links` (supersedes the Phase 6 source).
- `secunit-gui`: **Risks** register view (table + SLA countdown) and **Risk detail** (fold header + event-log timeline + finding deep-links + verified-sha badge), plus the Overview open-risks tile. All read-only — the view folds the same way `secunit-core` does; no writes.

**Tests.**

- **Unit:** `fold` determinism (events applied in `seq` order reproduce identical state); status-machine rejects every illegal transition; `prev_sha256` chaining is correct across a synthetic event sequence.
- **Round-trip:** `risks open` from a sealed fixture run → a sequence of mutating verbs → `risks show`/`list` reflect the fold → `risks rebuild` reproduces `index.json` byte-identically.
- **Tamper detection:** edit or delete a log line → `verify` fails with a precise diagnostic; point a `finding_ref` at a mutated or absent manifest → `verify` fails.
- **Concurrency:** two appends to the same risk serialize via the root lock; no torn or interleaved lines.
- **Golden:** `cargo insta` snapshots of `risks list` / `risks show` (human + JSON) against a fixture org carrying risks in every state — new, persisting (multiple `evidence-linked`), remediated, accepted-exception, past-SLA.
- **GUI parity:** the view's in-memory fold matches `secunit-core::risks::fold` for the fixture logs (snapshot/property).

**Exit criteria.** A real multi-finding run (e.g. the `ra-vuln-audit` Django review — 2 Critical, 16 High) is promoted into the register; `secunit risks list --past-sla` surfaces SLA breaches against control thresholds; `secunit verify` covers the register chain; the GUI renders the register and timeline read-only. The quarterly report's risk section is sourced from `risks/`.

---

## Testing strategy

Five layers, each cheap to run and each catching a different class of bug.

| Layer | What it catches | Tooling |
|---|---|---|
| Schema validation | Bad YAML, missing fields, wrong types | `jsonschema` against `schemas/*.json` |
| Unit (per crate) | Cadence math, scope resolution, hashing, parsing | `cargo test` |
| Golden file (per CLI subcommand) | Output format regressions, both human and JSON | `cargo insta review` |
| Property (specific invariants) | Cadence monotonicity, validate-then-load round-trip, hash-chain integrity under any sequence of runs | `proptest` |
| Live smoke (against target org) | Real upstream changes, canonicalization gaps, credential chain | scheduled GitHub Action against a sandbox AWS account |

CI runs the first four on every push. Live smokes run weekly on a schedule, gated on a separate set of test credentials with read-only scope.

**Test data discipline.**

- `testdata/orgs/<scenario>/` — checked-in registry fixtures for integration tests. Synthetic, not derived from any real org.
- `testdata/fixtures/captures/<subsystem>/<action>/` — recorded upstream responses, hand-edited to be deterministic.
- Golden snapshots committed under `crates/*/snapshots/`. Reviewed on diff via `cargo insta review`.
- The target org's registry lives in its own repo, not in `secunit/`.

**Coverage targets.**

- `secunit-core::model` and `registry::resolver`: 90%+ line coverage.
- `secunit-core::evidence::hasher`: 100% — this is integrity-critical.
- Capturers: golden-fixture coverage is the bar, not line coverage; every capturer round-trips its fixtures byte-identically.

---

## Risk areas

| Risk | Why it bites | Mitigation |
|---|---|---|
| AWS-SDK clean compile is slow | CI cold builds get unpleasant; local dev loop suffers | sccache from Phase 0; pin SDK minor versions to avoid spurious rebuilds |
| Capture canonicalization gaps | Phantom diffs across runs; assessor confusion | Property test: same fixture twice → byte-identical output. Failing this fails CI. |
| Hash chain semantics under retries / aborts | Retroactive edits go undetected, or false positives on legitimate retries | Phase 2 atomicity tests; aborts write `abort.json` not silently delete |
| Inventory drift mid-run | A system added/removed during a run leaves ambiguous evidence | `prepare` snapshots `registry_git_sha` (which pins inventory.yaml since it lives in the same repo); `finalize` does not re-resolve scope |
| Skill / binary version drift | Skills written against old capture flags break silently | Manifest records `skill_sha256` and `secunit_version`; `verify` flags mismatches |
| Credential leakage in logs | SDK debug output can include headers / signed URLs | `tracing` redaction filter; SDK loggers default to `info`; security review before each release |
| Concurrent runs of the same control | State.json corruption | File lock at root; `prepare` refuses if a `.run-pending` exists for the same control |

---

## Definition of done (per phase)

A phase is done when:

1. All deliverables listed above are merged.
2. Tests for the phase pass on CI.
3. The capability the phase promised is exercised end-to-end against either a fixture org (Phases 0–2) or the target org's real registry (Phases 3–6).
4. `cli.md` reflects any new subcommands or flags.
5. `PLAN.md` is updated with anything learned that changes later phases.

---

## Open decisions

- **Repo split.** Keep `secunit/` as one repo containing both the binary and the docs, or split into `secunit-tool/` (binary) and `secunit-docs/` (this directory)? Recommendation: stay single until Phase 7.
- **Where does the target org's registry live?** Sibling repo `<org>-secunit/`, or a private dir inside the org's docs repo? Recommendation: sibling repo so evidence does not pollute the WISP repo's git history.
- **Release artifacts.** `cargo-dist` is convenient but pulls in a release pipeline. GitHub Releases with manually attached binaries is fine for one-operator. Deferred to Phase 7.
- **Test credentials for live smokes.** Need a read-only AWS role and a fine-grained PAT scoped to one repo for the weekly smoke. Created during Phase 5 setup.
- **`secunit-capture` as a separate crate or module of `secunit-cli`?** Separate crate makes feature gating cleaner. Costs nothing.
- **Async runtime for AWS captures.** `tokio` is required by aws-sdk-rust. Either run inside `#[tokio::main]` for the whole binary or just for the capture subcommands. Recommendation: just the capture subcommands — keep core sync.
- **Risk id scheme (Phase 8).** Global sequential `R-NNNN` vs a content-derived id. Recommendation: global sequential allocated under the root lock at `risks open`, with the fingerprint `<control_id>:<finding_id>` stored inside for cross-run identity — human-friendly and tracker-friendly.
- **Sync-out tooling (Phase 8).** Ship a bundled `risk-sync` skill or leave the projection to the operator? Recommendation: bundle a thin skill that pushes Critical/High risks to the configured tracker and writes `external-linked` events back; inbound status stays advisory (`external-status-observed`), so there is never a true bidirectional conflict to resolve.
