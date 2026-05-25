// Included from store.rs into a nested `tests` module via `include!`. Pull in
// the parent module's items (private helpers like `events_path`,
// `index_path`, `log_head_sha`, plus the re-exported public types) with
// `super::*`, then add the test-only deps.

use super::*;

use chrono::{NaiveDate, TimeZone};
use tempfile::tempdir;

fn ts(day: u32, secs: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, day, 14, 0, secs).unwrap()
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn finding(control: &str, run: &str, finding_id: &str) -> FindingRef {
    FindingRef {
        control_id: control.to_string(),
        run_id: run.to_string(),
        // 64 hex chars — must satisfy the schema's sha256 pattern.
        manifest_sha256: "a".repeat(64),
        finding_id: finding_id.to_string(),
        body_path: Some("findings.md#risk-1".to_string()),
    }
}

fn open_a_risk(root: &Path) -> String {
    let out = open(
        root,
        finding("ra-vuln-audit", "2026-05-25-run-001", "S032"),
        "S032 — pickle deserialization RCE (CWE-502)",
        Severity::Critical,
        3,
        3,
        vec!["app-api".to_string()],
        30,
        date(2026, 6, 24),
        "jstockdi",
        None,
        Some(ts(25, 0)),
    )
    .unwrap();
    out.risk_id
}

// ---------- status machine --------------------------------------------------

#[test]
fn status_machine_accepts_legal_transitions() {
    assert!(validate_transition(Status::Open, Status::InProgress).is_ok());
    assert!(validate_transition(Status::InProgress, Status::Remediated).is_ok());
    assert!(validate_transition(Status::Remediated, Status::Reopened).is_ok());
    assert!(validate_transition(Status::Reopened, Status::Open).is_ok());
    assert!(validate_transition(Status::Open, Status::AcceptedException).is_ok());
    assert!(validate_transition(Status::Open, Status::FalsePositive).is_ok());
    assert!(validate_transition(Status::InProgress, Status::FalsePositive).is_ok());
}

#[test]
fn status_machine_rejects_illegal_transitions() {
    // Can't jump straight from open to remediated.
    assert!(validate_transition(Status::Open, Status::Remediated).is_err());
    // Terminal false-positive can't move anywhere.
    assert!(validate_transition(Status::FalsePositive, Status::Open).is_err());
    assert!(validate_transition(Status::FalsePositive, Status::InProgress).is_err());
    // No-op rejected.
    assert!(validate_transition(Status::Open, Status::Open).is_err());
    // Can't remediate something already remediated via direct status change.
    assert!(validate_transition(Status::Remediated, Status::Remediated).is_err());
}

// ---------- fold ------------------------------------------------------------

#[test]
fn fold_is_deterministic_and_correct() {
    let events = vec![
        EventEnvelope {
            seq: 1,
            ts: ts(25, 0),
            actor: "jstockdi".into(),
            agent: None,
            prev_sha256: None,
            data: EventData::Opened {
                finding_ref: finding("ra-vuln-audit", "2026-05-25-run-001", "S032"),
                title: "pickle RCE".into(),
                severity: Severity::Critical,
                impact: 3,
                likelihood: 3,
                affected_systems: vec!["app-api".into()],
                sla_days: 30,
                due_at: date(2026, 6, 24),
            },
        },
        EventEnvelope {
            seq: 2,
            ts: ts(25, 1),
            actor: "jstockdi".into(),
            agent: None,
            prev_sha256: Some("b".repeat(64)),
            data: EventData::OwnerAssigned { owner: "cto".into() },
        },
        EventEnvelope {
            seq: 3,
            ts: ts(25, 2),
            actor: "jstockdi".into(),
            agent: None,
            prev_sha256: Some("c".repeat(64)),
            data: EventData::ExternalLinked {
                system: "linear".into(),
                external_id: "SEC-412".into(),
                url: "https://linear.app/x/SEC-412".into(),
            },
        },
        EventEnvelope {
            seq: 4,
            ts: ts(25, 3),
            actor: "jstockdi".into(),
            agent: None,
            prev_sha256: Some("d".repeat(64)),
            data: EventData::EvidenceLinked {
                finding_ref: finding("ra-vuln-audit", "2026-06-01-run-001", "S032"),
            },
        },
        EventEnvelope {
            seq: 5,
            ts: ts(26, 0),
            actor: "jstockdi".into(),
            agent: None,
            prev_sha256: Some("e".repeat(64)),
            data: EventData::StatusChanged {
                from: Status::Open,
                to: Status::InProgress,
                reason: "work started".into(),
            },
        },
        EventEnvelope {
            seq: 6,
            ts: ts(27, 0),
            actor: "jstockdi".into(),
            agent: None,
            prev_sha256: Some("f".repeat(64)),
            data: EventData::Remediated {
                resolved_run_ref: None,
                note: "pickle replaced".into(),
            },
        },
    ];

    let state = fold::fold(&events);
    // Determinism: folding twice yields the same state.
    assert_eq!(state, fold::fold(&events));

    assert_eq!(state.status, Status::Remediated);
    assert_eq!(state.severity, Severity::Critical);
    assert_eq!(state.owner.as_deref(), Some("cto"));
    assert_eq!(state.finding_refs.len(), 2, "evidence-linked accumulates");
    assert_eq!(state.external.len(), 1);
    assert_eq!(state.external[0].external_id, "SEC-412");
    assert!(state.resolved_at.is_some());
    assert_eq!(state.fingerprint().as_deref(), Some("ra-vuln-audit:S032"));
    assert_eq!(state.source_control(), Some("ra-vuln-audit"));
}

#[test]
fn score_changed_recomputes_due_at_from_sla_days() {
    let events = vec![
        EventEnvelope {
            seq: 1,
            ts: ts(25, 0),
            actor: "op".into(),
            agent: None,
            prev_sha256: None,
            data: EventData::Opened {
                finding_ref: finding("ra-vuln-audit", "2026-05-25-run-001", "S1"),
                title: "t".into(),
                severity: Severity::High,
                impact: 2,
                likelihood: 2,
                affected_systems: vec![],
                sla_days: 30,
                due_at: date(2026, 6, 24),
            },
        },
        EventEnvelope {
            seq: 2,
            ts: ts(26, 0),
            actor: "op".into(),
            agent: None,
            prev_sha256: Some("0".repeat(64)),
            data: EventData::ScoreChanged {
                impact: 3,
                likelihood: 3,
                severity: Severity::Critical,
                reason: "reassessed".into(),
            },
        },
    ];
    let state = fold::fold(&events);
    // open day 2026-05-25 + 30 days = 2026-06-24.
    assert_eq!(state.due_at, Some(date(2026, 6, 24)));
    assert_eq!(state.severity, Severity::Critical);
}

// ---------- append + chaining -----------------------------------------------

#[test]
fn open_allocates_sequential_ids() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let r1 = open_a_risk(root);
    assert_eq!(r1, "R-0001");

    let out2 = open(
        root,
        finding("sast-scan", "2026-05-25-run-001", "X1"),
        "second",
        Severity::High,
        2,
        2,
        vec![],
        30,
        date(2026, 6, 24),
        "op",
        None,
        Some(ts(25, 5)),
    )
    .unwrap();
    assert_eq!(out2.risk_id, "R-0002");
}

#[test]
fn append_chains_prev_sha256_correctly() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = open_a_risk(root);

    append(
        root,
        &id,
        EventData::OwnerAssigned { owner: "cto".into() },
        "jstockdi",
        None,
        Some(ts(25, 10)),
    )
    .unwrap();
    append(
        root,
        &id,
        EventData::StatusChanged {
            from: Status::Open,
            to: Status::InProgress,
            reason: "starting".into(),
        },
        "jstockdi",
        None,
        Some(ts(26, 0)),
    )
    .unwrap();

    // Re-read raw lines and verify each prev_sha256 equals SHA-256 of the
    // previous line's bytes.
    let text = std::fs::read_to_string(events_path(root, &id)).unwrap();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3);

    let ev0: EventEnvelope = serde_json::from_str(lines[0]).unwrap();
    assert!(ev0.prev_sha256.is_none(), "seq 1 has null prev_sha256");

    for i in 1..lines.len() {
        let ev: EventEnvelope = serde_json::from_str(lines[i]).unwrap();
        let expected = sha256_bytes(lines[i - 1].as_bytes());
        assert_eq!(
            ev.prev_sha256.as_deref(),
            Some(expected.as_str()),
            "line {i} prev_sha256 must hash the previous line"
        );
    }

    // load_events validates the chain end-to-end.
    let loaded = load_events(root, &id).unwrap();
    assert_eq!(loaded.len(), 3);
}

#[test]
fn append_rejects_illegal_transition() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = open_a_risk(root);

    // open → remediated directly is illegal.
    let err = append(
        root,
        &id,
        EventData::StatusChanged {
            from: Status::Open,
            to: Status::Remediated,
            reason: "skip ahead".into(),
        },
        "op",
        None,
        Some(ts(26, 0)),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("illegal status transition"),
        "got: {err}"
    );

    // The log must be untouched — still just the opened event.
    let loaded = load_events(root, &id).unwrap();
    assert_eq!(loaded.len(), 1);
}

#[test]
fn append_rejects_mismatched_from_status() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = open_a_risk(root);

    // Current status is open; claim from in-progress.
    let err = append(
        root,
        &id,
        EventData::StatusChanged {
            from: Status::InProgress,
            to: Status::Remediated,
            reason: "wrong from".into(),
        },
        "op",
        None,
        Some(ts(26, 0)),
    )
    .unwrap_err();
    assert!(err.to_string().contains("currently open"), "got: {err}");
}

#[test]
fn second_opened_event_is_rejected() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = open_a_risk(root);
    let err = append(
        root,
        &id,
        EventData::Opened {
            finding_ref: finding("c", "2026-05-25-run-001", "F"),
            title: "dup".into(),
            severity: Severity::Low,
            impact: 1,
            likelihood: 1,
            affected_systems: vec![],
            sla_days: 30,
            due_at: date(2026, 6, 24),
        },
        "op",
        None,
        Some(ts(26, 0)),
    )
    .unwrap_err();
    assert!(err.to_string().contains("second `opened`"), "got: {err}");
}

// ---------- round-trip + index ----------------------------------------------

#[test]
fn round_trip_open_append_read_fold() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = open_a_risk(root);

    append(
        root,
        &id,
        EventData::OwnerAssigned { owner: "cto".into() },
        "jstockdi",
        None,
        Some(ts(25, 10)),
    )
    .unwrap();
    append(
        root,
        &id,
        EventData::StatusChanged {
            from: Status::Open,
            to: Status::InProgress,
            reason: "starting".into(),
        },
        "jstockdi",
        None,
        Some(ts(26, 0)),
    )
    .unwrap();
    append(
        root,
        &id,
        EventData::Remediated {
            resolved_run_ref: None,
            note: "fixed".into(),
        },
        "jstockdi",
        None,
        Some(ts(27, 0)),
    )
    .unwrap();

    let events = load_events(root, &id).unwrap();
    let state = fold::fold(&events);
    assert_eq!(state.status, Status::Remediated);
    assert_eq!(state.owner.as_deref(), Some("cto"));
    assert!(state.resolved_at.is_some());

    // The index entry refreshed by append must match the fold + chain head.
    let idx_bytes = std::fs::read(index_path(root)).unwrap();
    let index: RiskIndex = serde_json::from_slice(&idx_bytes).unwrap();
    let entry = index.risks.get(&id).expect("index has the risk");
    assert_eq!(entry.status, Status::Remediated);
    assert_eq!(entry.owner.as_deref(), Some("cto"));
    assert_eq!(entry.fingerprint, "ra-vuln-audit:S032");
    assert_eq!(entry.source_control, "ra-vuln-audit");
    assert_eq!(entry.log_head_sha256, log_head_sha(root, &id).unwrap());
}

#[test]
fn rebuild_reproduces_the_index() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    let id = open_a_risk(root);
    append(
        root,
        &id,
        EventData::OwnerAssigned { owner: "cto".into() },
        "jstockdi",
        None,
        Some(ts(25, 10)),
    )
    .unwrap();

    let before: RiskIndex = serde_json::from_slice(&std::fs::read(index_path(root)).unwrap()).unwrap();

    // Delete the index and rebuild from the logs.
    std::fs::remove_file(index_path(root)).unwrap();
    let rebuilt = rebuild(root).unwrap();

    // Same risks projected to the same entries (ignoring updated_at clock).
    assert_eq!(before.risks, rebuilt.risks);
    assert_eq!(rebuilt.risks.len(), 1);
    assert_eq!(rebuilt.risks[&id].owner.as_deref(), Some("cto"));
}

#[test]
fn verify_finding_ref_matches_real_manifest() {
    let dir = tempdir().unwrap();
    let run_dir = dir.path().join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    // Minimal but schema-shaped manifest is unnecessary for the sha check;
    // we only parse control_id/run_id, so write a small valid JSON manifest.
    let manifest = serde_json::json!({
        "schema_version": 1,
        "control_id": "ra-vuln-audit",
        "run_id": "2026-05-25-run-001",
        "started_at": "2026-05-25T14:00:00Z",
        "completed_at": "2026-05-25T14:30:00Z",
        "agent": {"model":"m","skill":"s","skill_sha256":"a".repeat(64),"control_sha256":"b".repeat(64)},
        "registry_git_sha": "abcdef0",
        "scope_layout": "flat",
        "resolved_scope": [],
        "artifacts": [],
        "status": "complete"
    });
    let bytes = serde_json::to_vec(&manifest).unwrap();
    std::fs::write(run_dir.join("manifest.json"), &bytes).unwrap();
    let real_sha = sha256_bytes(&bytes);

    let good = FindingRef {
        control_id: "ra-vuln-audit".into(),
        run_id: "2026-05-25-run-001".into(),
        manifest_sha256: real_sha.clone(),
        finding_id: "S032".into(),
        body_path: None,
    };
    assert!(verify_finding_ref(&run_dir, &good).is_ok());

    // Wrong sha is rejected.
    let bad = FindingRef {
        manifest_sha256: "0".repeat(64),
        ..good.clone()
    };
    assert!(verify_finding_ref(&run_dir, &bad).is_err());

    // Missing manifest is rejected.
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(verify_finding_ref(&empty, &good).is_err());
}
