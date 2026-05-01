# Weekly software composition analysis — 2026-05-04

## Summary
Weekly SCA across 2 source repos with the `has-sca` tag. Overall posture is healthy: zero Critical findings across both repos, one new High in `app-api` (Werkzeug request-smuggling CVE), no missing configuration. Nothing exceeds the remediation SLA. The new High is drafted as a risk register entry pending operator review.

## Scope
2 source repos in scope (tag: has-sca). Inventory git sha: e8d6b3a1c2f4...truncated.

- app-api
- app-ui

## Per-system results

### app-api
- Stack: python-django
- Critical: 0 | High: 1 | Medium: 4 | Low: 11
- New this week: 1 High (Werkzeug request smuggling) | Resolved since prior: 2 Medium | Persisting: 13
- Dependabot config: present
- Open Dependabot alerts: 3 (all Medium/Low)
- Notes: Werkzeug pinned at 2.3.x; fix is in 3.0.4. Upgrade path requires Django ≥ 4.2 (already met).
- Evidence: by-system/app-api/raw/pip-audit.json, dependabot-config.yml, dependabot-alerts.json, diff-pip-audit.txt

### app-ui
- Stack: typescript-react
- Critical: 0 | High: 0 | Medium: 2 | Low: 7
- New this week: 0 | Resolved since prior: 1 Low | Persisting: 9
- Dependabot config: present
- Open Dependabot alerts: 2 (all Low)
- Notes: No actionable changes vs prior run.
- Evidence: by-system/app-ui/raw/pnpm-audit.json, dependabot-config.yml, dependabot-alerts.json, diff-pnpm-audit.txt

## Cross-system anomalies
- None. Both repos have Dependabot configured with weekly cadence; both block CI on Critical/High audit findings.

## Recommended actions
- Upgrade Werkzeug to 3.0.4 in `app-api`. Add a tracking risk register entry (drafted below) until the upgrade lands.

## Draft risk entries

### Risk 1: Werkzeug 2.3.x — CVE-2024-XXXXX (request smuggling)
- Affected systems: app-api
- Severity: High
- Impact: 3
- Likelihood: 3
- Score: 9
- Body:
  **What:** Werkzeug versions before 3.0.4 contain a request-smuggling vulnerability (CVE-2024-XXXXX). `app-api` currently pins `Werkzeug==2.3.7` via Django's transitive dependency. Vulnerable on requests routed through misconfigured intermediaries.
  **Why it matters:** `app-api` serves customer-data endpoints behind a load balancer. Smuggled requests could bypass authentication checks at the edge and reach unauthenticated handlers.
  **Suggested remediation:** Bump Werkzeug to 3.0.4 in `app-api/pyproject.toml`. Compatibility check: Django 4.2+ supports Werkzeug 3.x. SLA: 90 days from today (2026-08-02).
  **Evidence:** by-system/app-api/raw/pip-audit.json, by-system/app-api/raw/dependabot-alerts.json
