# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] — 2026-07-23

### Added

- `secunit report data --week|--month|--quarter|--year`: assemble one
  period's per-control coverage, sealed runs, overdue controls, risk-register
  delta, and upcoming due dates into JSON for a report skill to render
  (PLAN Phase 6, extended with weekly/monthly selectors).
- The bundled `report` skill now supports `kind: weekly | monthly` with a
  one-screen stakeholder template, and an opt-in publish step
  (`skill_args.publish: true`) that files the rendered report as a GitLab or
  Linear issue per `report.publish` in `_config.yaml` and records the issue
  URL in the run's `external_links`. Publishing is agent-side only — the
  binary has no tracker integration.
- Example controls `rp-weekly-status` / `rp-monthly-status` under
  `docs/examples/controls/`.

### Changed

- A due date that passes without a completed run now **holds** until the run
  completes or a `schedule.yaml` skip covers it, for every cadence — so
  `secunit due --overdue` (and the report's `overdue` section) actually fire.
  Previously most cadences rolled a missed date forward to the next firing,
  silently forgiving the miss.
- `run prepare --period` canonicalizes operator spellings (`2026-Q3` →
  `2026-q3`, `2026-w5` → `2026-W05`) before validating and storing, and
  zero-padded/signed quarter digits are rejected — sealed period ids always
  match the derive-minted form coverage compares against.
- A corrupt risk event log no longer takes every register surface down:
  `risks list` and the GUI register table render the readable risks with a
  loud warning (list exits 1), report data carries the breakage in-band, and
  `risks rebuild` stays strict. `verify`/`doctor` now warn about event logs
  in `risks/` dirs outside the register's `R-NNNN` membership rule.

### Removed

- The planned-but-never-implemented `secunit report data --policy-status`
  mode. Policy-review status reads out of `secunit status` and the annual
  report; the docs no longer advertise the flag.

## [0.5.0] — 2026-05-30

### Added

- `secunit wisp init` and `secunit wisp export`: render the WISP markdown set
  under `security/` into a single branded PDF, entirely in Rust (markdown →
  Typst → PDF via the `typst`/`typst-pdf` crates behind an on-by-default `pdf`
  feature — no external toolchain). Includes a cover page, a native table of
  contents with page numbers and PDF bookmarks, running header/footer, bundled
  Inter / JetBrains Mono fonts, and a git-commit + SHA-256 provenance stamp.
  Branding lives in required, operator-owned partials scaffolded by `wisp init`
  (`theme/header/footer/cover/toc.typ` + logo). The policy index and inline
  `.md` cross-references resolve to internal document links, and `--allow-dirty`
  gates exporting from an uncommitted tree. (#51)

## [0.4.2] — 2026-05-26

### Fixed

- The bundled skill library now ships inside `secunit-core` (moved from the
  workspace root to `crates/secunit-core/skills/`), so the crate compiles from
  its published tarball. Previously the `include_str!` paths reached above the
  package root and `cargo publish` verification failed — which is why no release
  past 0.1.2 reached crates.io.

### Changed

- CI (`release.yml`) now publishes `secunit-core`, `secunit-capture`, and
  `bcl-secunit` to crates.io on `v*` tags. Requires a `CARGO_REGISTRY_TOKEN`
  repository secret.

## [0.4.1] — 2026-05-26

### Security

- Dismissed `glib` advisory GHSA-wrw7-89jp-8q8g (RUSTSEC-2024-0429,
  `VariantStrIter` unsoundness, medium) as tolerable risk. `glib` 0.18.5 is
  transitive, pulled only by the Tauri 2 Linux GTK3 stack (gtk-rs 0.18
  generation via tao/wry); the fix lands only in `glib` 0.20 (the gtk-rs 0.20
  generation), which Tauri 2 does not ride, and there is no 0.18.x backport —
  upstream-blocked. Not reachable in any shipped artifact: the GTK/glib stack
  is Linux-only, while the GUI ships solely as a macOS `.dmg` (WKWebView/Cocoa,
  no GTK), and the only Linux release artifact is the CLI, which is not in the
  Tauri dependency tree. Revisit once Tauri 2 adopts gtk-rs ≥ 0.20.

## [0.4.0] — 2026-05-26

### Added

- `secunit doctor` — a read-only environment + registry health preflight that
  automates the Part B audit in `docs/setup-checklist.md`, grouped into five
  sections (Environment, Repo structure, Registry, Evidence integrity, Risk
  register). It reuses the existing building blocks (`validate`'s skill checks,
  `verify`'s hash-chain walk, the risk-register fold) and verifies the git
  HEAD the same way `run prepare` does. Every `⚠`/`✗` line carries a `fix:`
  (a `fix` field under `--json`) with the concrete next action, distinguishing
  safe auto-repairs (`git init`, `risks rebuild`) from integrity failures the
  agent must investigate rather than repair.

### Security

- Dropped the `gix-reqwest` feature from the `rustsec` dependency in the deps
  capturer, removing the transitive `gix` 0.72.x tree that was the sole source
  of 8 `gix`-family advisories (gix RCE/path-traversal, `gix-fs`, `gix-pack`,
  `gix-transport`, `gix-date`). `rustsec` now runs library-only with a gix-free
  advisory-db fetch (reqwest + tar + flate2); canonical capturer output is
  byte-identical. The only remaining `gix` is secunit-core's patched 0.83 (#48).

## [0.3.1] — 2026-05-25

### Security

- Bumped `tauri` 2.11.0 → 2.11.2 (GHSA-7gmj-67g7-phm9 — IPC origin confusion).
- Bumped `octocrab` 0.41 → 0.51, moving `jsonwebtoken` 9 → 10.4.0 and clearing the
  advisory; the GitHub capturers are unchanged (octocrab is used only as
  auth + transport).
- Dismissed 8 `rustsec`/`gix`-family advisories as tolerable risk: blocked upstream
  (no `rustsec` release pulls `gix ≥ 0.83`) and reached only via the deps capturer
  parsing the trusted `rustsec/advisory-db` repo, not attacker-supplied input.

## [0.3.0] — 2026-05-25

### Added

- Internal risk register: an authoritative, append-only, hash-chained event log
  per risk under `risks/`, with a regenerable `risks/index.json` cache. Each risk
  binds to the finding that produced it by content hash (`manifest_sha256` +
  `finding_id`). Full design in `docs/risks.md`.
- `secunit risks` command family — `open` (promote a sealed run's draft finding
  into the register), `assign`, `score`, `status`, `relink`, `link`, `observe`,
  `note`, `remediate`, `reopen`, `except`, plus read-only `list`, `show`,
  `rebuild`.
- `secunit verify` now also walks each risk's event chain and confirms every
  `finding_ref` resolves to a sealed manifest whose recomputed sha matches.
- GUI: a read-only Risks register view with SLA countdown, a risk detail view
  that renders the event log as a timeline, and an open-risks overview tile.

### Changed

- The risk register is now maintained inside `secunit` as authoritative state and
  synced *out* to external trackers, rather than living externally and referenced
  by URL (amends the prior non-goal in `spec.md`).
- `capture-sweep` now specifies the structured `result.json` `draft_risks` shape,
  and `secunit risks open --from` tolerates legacy drafts — deriving
  impact/likelihood from severity and matching `--finding` against
  id / body_path anchor / `ghsa[]` / subject — so existing findings promote
  without a re-run.

## [0.2.0] — 2026-05-25

### Added

- Project docs and policy baseline: `README`, `LICENSE`, `SECURITY.md` (CM-2).
- New application icon for the desktop app (SecUnit shield + visor).

### Changed

- Bumped `gix` to 0.83 (core) and refreshed frontend advisory dependencies;
  migrated the GUI build to Vite 8 + Vitest 4 (rolldown).

### Security

- Pinned all GitHub Actions to commit SHAs (AC-6/SR-3); restored
  dependency-review gating once the Dependency Graph was enabled.

## [0.1.3] — 2026-05-25

### Added

- macOS desktop app now ships in GitHub Releases: the `secunit-gui` Tauri
  viewer is built for Apple Silicon and Intel and attached as a `.dmg`
  alongside the CLI archives on every tagged release. The bundle is
  unsigned — see `getting-started.md` for the one-time Gatekeeper step.
- Bundled skill standard library + `secunit skills` subcommand. The six
  reusable runbooks (`capture-sweep`, `attestation-review`,
  `policy-annual-review`, `report`, `bootstrap`, `inventory-seed`) now
  ship embedded in the binary, so an org needs no install/copy step to
  use them. `secunit skills list|show|path` inspect and resolve skills;
  `show` is the call the agent front door uses to load a runbook by name.
- One uniform skill resolver (`secunit_core::skills`): every skill
  reference — a control's `skill:`, a runbook's `skill_args.extend:`,
  `validate`, `run prepare`, and `skills show/path` — resolves
  local `<root>/skills/<name>.md` first, then bundled. An org overrides
  any runbook (or adds a bespoke one) by dropping a same-named local file.

### Changed

- `run prepare` now embeds the resolved skill in the prepare context as
  `skill: { name, source, sha256 }` (and `prepare.schema.json` with it),
  so the agent loads the runbook without knowing whether it is bundled or
  local. `prepare` fails fast if the skill resolves to nothing.
- `validate` resolves `control.skill` and its `requires_features:` through
  the bundled∪local resolver instead of requiring a file under `skills/`.
- `docs/examples/skills/` no longer duplicates the bundled runbooks; it
  keeps one bespoke per-control example (`sca-weekly-dependency-scan.md`)
  plus a README. `getting-started.md` drops the skill-copy step.

## [0.1.0] — 2026-05-01

First tagged release. Covers Phases 0–4 of `PLAN.md`: a working
registry, run lifecycle, bootstrap, and the first source-side
captures (deps + GitHub). Sufficient to drive a real
`sca-weekly-dependency-scan` end-to-end.

### Added

- Cargo workspace (`secunit-core`, `secunit-capture`, `secunit-cli`),
  pinned `rust-toolchain.toml`, and CI running `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `cargo test`, and `cargo build
  --release` with `sccache`. (Phase 0)
- JSON Schemas for `control`, `manifest`, `inventory`, `state`,
  `result`, `prepare`, and `_config`, with example registries
  validated in CI. (Phase 0)
- Registry model and loader: `Control`, `Inventory`, `Schedule`,
  `State`, `Config` types with serde + schema validation; per-file
  error reporting. (Phase 1a)
- Cadence and scope resolvers: `next_due`, `is_overdue` with grace
  periods, and `resolve_scope` honoring `in_scope_since`, `excludes`,
  `retired`, and aliases. (Phase 1a)
- Read-only CLI subcommands: `due`, `calendar`, `status`, `show`,
  `scope`, `history`, `features`, `validate`. Human tables by default;
  `--json` output validated against published schemas. (Phase 1b)
- Run lifecycle: `prepare`, `finalize`, `verify`, `abort`, `resume`.
  Empty-scope detection, singleton flat-scope default, exit-2 mapping
  for prepare runtime errors, exit-4 for pending conflicts, and
  per-run `Unreadable` failures that don't abort the whole chain
  walk. (Phase 2)
- Bootstrap skills, registry import, and inventory CLI for seeding a
  registry from a target WISP. (Phase 3)
- Source-side captures (Phase 4):
  - `secunit capture deps` — `pip-audit`, `pnpm audit`, `cargo audit`,
    OSV query.
  - `secunit capture github` — `dependabot-alerts`, `codeql-alerts`,
    `branch-protection`, `org-members`, `audit-log`.
  - Canonicalized envelopes that round-trip byte-identically across
    runs and validate against the published schemas.
- `git` is required: dropped sha-zero fallbacks, switched HEAD lookup
  to `gix`, fail loudly outside a repository.
- Pre-commit config running `cargo fmt`, `cargo clippy`, and schema
  validation on the example registries.

### Fixed

- `secunit capture github dependabot-alerts` no longer fails with HTTP
  422 *"Pagination using the `page` parameter is not supported."* The
  shared `paginate_array` helper drove pagination via `?page=N`, but
  the Dependabot alerts endpoint only honors cursor pagination via the
  response `Link` header. Added a sibling `paginate_array_cursor` and
  switched `dependabot_alerts::capture` to use it; the page-based
  paginator is kept for the other capturers (org-members,
  branch-protection, codeql, audit-log) that still accept it.
  ([#3](https://github.com/Battle-Creek-LLC/secunit/issues/3),
  [#4](https://github.com/Battle-Creek-LLC/secunit/pull/4))
- `secunit capture github <subcommand>` no longer panics at startup
  with *"there is no reactor running, must be called from the context
  of a Tokio 1.x runtime."* Octocrab's transport stack constructs a
  `tower::buffer::Service` at builder time whose worker is spawned via
  `tokio::spawn`, so it requires an active reactor in the current
  thread. The CLI now holds an `rt.enter()` guard across `GhClient`
  construction.
  ([#2](https://github.com/Battle-Creek-LLC/secunit/pull/2))

[Unreleased]: https://github.com/Battle-Creek-LLC/secunit/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Battle-Creek-LLC/secunit/releases/tag/v0.1.0
