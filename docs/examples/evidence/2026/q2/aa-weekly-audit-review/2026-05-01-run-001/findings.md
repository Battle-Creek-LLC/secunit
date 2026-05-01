# Weekly audit and accountability review — 2026-05-01

## Summary
Weekly review covering 2026-04-26 through 2026-05-01. Cloud audit surfaces are healthy overall: no high-severity firewall alerts, configuration compliance steady at 47/49 rules. One new external principal observed in object-store access logs; flagged as a draft risk pending owner confirmation.

## Evidence captured
- raw/access-analyzer-findings.json — 3 active findings (unchanged from prior run)
- raw/network-firewall-alerts.json — 142 alerts, 0 high / 11 medium / 131 low
- raw/config-compliance.json — 47/49 rules COMPLIANT, 2 NON_COMPLIANT (unchanged)
- raw/object-store-access-sample.txt — 14 distinct principals across 8 buckets
- raw/diff-access-analyzer-findings.txt — no structural change
- raw/diff-network-firewall-alerts.txt — alert volume within 1σ of trailing 4-week mean
- raw/diff-object-store-access-sample.txt — 1 new principal vs prior run

## Anomalies
- New external principal observed accessing the `audit-logs` bucket. Principal was not present in any of the prior four weekly samples.

## Recommended actions
- Confirm with the cloud account owner whether the new principal is expected (e.g. a newly onboarded log-shipping integration). If expected, document the integration in the access-control entitlement export. If unexpected, revoke and investigate.

## Draft risk entries

### Risk 1: New external principal accessing audit-logs bucket
- Impact: 3
- Likelihood: 2
- Score: 6
- Body:
  **What:** Object-store access logs for the `audit-logs` bucket on 2026-04-29 show a previously unseen external principal performing `GetObject` and `ListBucket` calls. Principal not present in prior four weekly samples.
  **Why it matters:** The `audit-logs` bucket holds compliance evidence; unauthorized access could permit log tampering or exfiltration of monitoring data. Even authorized but undocumented access defeats the access-review control.
  **Suggested remediation:** Verify with cloud account owner. If sanctioned, add to the entitlement export and access-review scope. If not, revoke the principal's access, rotate any credentials it held, and investigate scope of access via CloudTrail.
  **Evidence:** raw/object-store-access-sample.txt, raw/diff-object-store-access-sample.txt
