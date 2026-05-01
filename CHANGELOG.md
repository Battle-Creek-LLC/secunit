# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `secunit capture github dependabot-alerts` no longer fails with HTTP 422
  *"Pagination using the `page` parameter is not supported."* The shared
  `paginate_array` helper drove pagination via `?page=N`, but the Dependabot
  alerts endpoint only honors cursor pagination via the response `Link`
  header. Added a sibling `paginate_array_cursor` and switched
  `dependabot_alerts::capture` to use it; the page-based paginator is kept
  for the other capturers (org-members, branch-protection, codeql,
  audit-log) that still accept it.
  ([#3](https://github.com/Battle-Creek-LLC/secunit/issues/3),
  [#4](https://github.com/Battle-Creek-LLC/secunit/pull/4))
- `secunit capture github <subcommand>` no longer panics at startup with
  *"there is no reactor running, must be called from the context of a
  Tokio 1.x runtime."* Octocrab's transport stack constructs a
  `tower::buffer::Service` at builder time whose worker is spawned via
  `tokio::spawn`, so it requires an active reactor in the current thread.
  The CLI now holds an `rt.enter()` guard across `GhClient` construction.
  ([#2](https://github.com/Battle-Creek-LLC/secunit/pull/2))
