# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
