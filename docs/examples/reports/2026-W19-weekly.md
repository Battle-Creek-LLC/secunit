# Example Org security status — 2026-W19

**Period:** 2026-05-04 through 2026-05-10
**Owner:** CTO
**Published:** https://gitlab.com/example-org/secops/-/issues/57

## Headline

All weekly controls fired on schedule. The dependency scan surfaced one new
High advisory in the `api` repo, assigned and inside its 90-day SLA. No
incidents were declared.

## Control activity

| Control | Cadence | Status | Notes |
|---|---|---|---|
| aa-weekly-audit-review | weekly | satisfied | no anomalies |
| sca-weekly-dependency-scan | weekly | satisfied | 1 new High (see below) |
| ca-quarterly-vuln-scan | quarterly | open (2026-q2) | scan scheduled 2026-06-15 |

## New findings

- `RUSTSEC-2026-0031` in `api` — High, transitive via `hyper`. Owner: platform.
  Evidence: `evidence/2026/q2/sca-weekly-dependency-scan/2026-05-08-run-001/`.

## Risk register

- Open: 2 (0 past SLA). Opened this period: 1. Closed: 0.

| Risk | Severity | Title | Owner | Due |
|---|---|---|---|---|
| R-0007 | High | Vulnerable transitive dependency in api | platform | 2026-08-06 |
| R-0005 | Medium | Single-region deployment exposes BCP RTO assumptions | cto | 2026-09-30 |

## Upcoming

- 2026-05-11: weekly audit review, weekly dependency scan (W20)
- 2026-06-15: quarterly vulnerability scan (2026-q2)

## Report provenance

- Generator skill: `report` (`kind: weekly`)
- Data: `secunit report data --week 2026-W19` → `raw/report-data.json`
- Report run: `evidence/2026/q2/rp-weekly-status/2026-05-11-run-001/`
