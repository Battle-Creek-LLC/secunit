//! Property + example tests for the risk-register integrity invariants.
//!
//! The register is an append-only, hash-chained event log per risk, so the
//! bar here is a `proptest`: drive `store::append` with random high-level
//! intents and assert the chain stays valid, the fold is deterministic, and
//! tampering with a line is detected on load. `append` is the oracle — it
//! validates the status transition and the schema *before* writing, so a
//! rejected intent writes nothing and leaves the log valid. We therefore
//! ignore every `append` error and lean on it to gate illegal mutations
//! rather than modelling the status machine in the test.

use chrono::{NaiveDate, TimeZone, Utc};
use proptest::prelude::*;
use tempfile::tempdir;

use secunit_core::risks::{self, store, EventData, FindingRef, Severity, Status};

// ---------- shared construction helpers -------------------------------------

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// A valid finding ref. `manifest_sha256` must match the schema's
/// `^[a-f0-9]{64}$` pattern; `open` does not require the manifest to exist on
/// disk, so arbitrary-but-well-formed fields are fine.
fn finding(finding_id: &str) -> FindingRef {
    FindingRef {
        control_id: "ra-vuln-audit".to_string(),
        run_id: "2026-05-25-run-001".to_string(),
        manifest_sha256: "a".repeat(64),
        finding_id: finding_id.to_string(),
        body_path: Some("findings.md#risk-1".to_string()),
    }
}

/// Open one risk with a fixed valid `opened` event and return its id.
fn open_one(root: &std::path::Path) -> String {
    store::open(
        root,
        finding("S032"),
        "pickle deserialization RCE",
        Severity::Critical,
        3,
        3,
        vec!["app-api".to_string()],
        30,
        date(2026, 6, 24),
        "jstockdi",
        None,
        Some(Utc.with_ymd_and_hms(2026, 5, 25, 14, 0, 0).unwrap()),
    )
    .expect("open a risk")
    .risk_id
}

// ---------- intents ---------------------------------------------------------

/// High-level, low-fidelity mutation intents. Deliberately small: the test
/// does not model the status machine — it converts each intent to the
/// matching `EventData` and lets `append` accept or reject it.
#[derive(Debug, Clone)]
enum Intent {
    AssignOwner(String),
    Note(String),
    ExternalLink,
    /// Advisory inbound status from a tracker — never touches the status
    /// machine, so always schema-valid.
    Observe(String),
    Score {
        impact: u8,
        likelihood: u8,
    },
    /// A direct `status-changed` to `target`. `from` is filled from the
    /// current fold in the test body; illegal transitions are rejected by
    /// `append` and ignored.
    SetStatus(Status),
    Remediate,
    Reopen,
    Except,
    EvidenceLink,
}

fn intent_strategy() -> impl Strategy<Value = Intent> {
    prop_oneof![
        "[a-z]{1,8}".prop_map(Intent::AssignOwner),
        "[a-z ]{0,16}".prop_map(Intent::Note),
        Just(Intent::ExternalLink),
        "[a-z]{1,8}".prop_map(Intent::Observe),
        (1u8..=5, 1u8..=5).prop_map(|(impact, likelihood)| Intent::Score { impact, likelihood }),
        prop_oneof![
            Just(Status::Open),
            Just(Status::InProgress),
            Just(Status::Remediated),
            Just(Status::Reopened),
            Just(Status::AcceptedException),
            Just(Status::FalsePositive),
        ]
        .prop_map(Intent::SetStatus),
        Just(Intent::Remediate),
        Just(Intent::Reopen),
        Just(Intent::Except),
        Just(Intent::EvidenceLink),
    ]
}

/// Pick a severity from the current score so `score-changed` stays plausible
/// (the actual value is irrelevant to the invariants under test).
fn severity_for(impact: u8, likelihood: u8) -> Severity {
    match impact.saturating_mul(likelihood) {
        20.. => Severity::Critical,
        12..=19 => Severity::High,
        6..=11 => Severity::Medium,
        2..=5 => Severity::Low,
        _ => Severity::Info,
    }
}

/// Convert an intent into the `EventData` to append. `current` is the folded
/// status, used to fill `status-changed.from`.
fn intent_to_event(intent: &Intent, current: Status, link_n: usize) -> EventData {
    match intent {
        Intent::AssignOwner(owner) => EventData::OwnerAssigned {
            owner: owner.clone(),
        },
        Intent::Note(text) => EventData::Note { text: text.clone() },
        Intent::ExternalLink => EventData::ExternalLinked {
            system: "linear".to_string(),
            external_id: format!("SEC-{link_n}"),
            url: format!("https://linear.app/x/SEC-{link_n}"),
        },
        Intent::Observe(status) => EventData::ExternalStatusObserved {
            system: "linear".to_string(),
            status: status.clone(),
            observed_at: Utc.with_ymd_and_hms(2026, 5, 26, 9, 0, 0).unwrap(),
        },
        Intent::Score { impact, likelihood } => EventData::ScoreChanged {
            impact: *impact,
            likelihood: *likelihood,
            severity: severity_for(*impact, *likelihood),
            reason: "rescored".to_string(),
        },
        Intent::SetStatus(target) => EventData::StatusChanged {
            from: current,
            to: *target,
            reason: "transition".to_string(),
        },
        Intent::Remediate => EventData::Remediated {
            resolved_run_ref: None,
            note: "fixed".to_string(),
        },
        Intent::Reopen => EventData::Reopened {
            reason: "regressed".to_string(),
        },
        Intent::Except => EventData::ExceptionDocumented {
            rationale: "accepted by leadership".to_string(),
            approved_by: "cto".to_string(),
            expires_at: date(2026, 12, 31),
        },
        Intent::EvidenceLink => EventData::EvidenceLinked {
            finding_ref: finding(&format!("S{link_n:03}")),
        },
    }
}

// ---------- example-based sanity checks -------------------------------------

#[test]
fn fresh_log_is_valid_and_seq_one() {
    let dir = tempdir().unwrap();
    let id = open_one(dir.path());
    let events = store::load_events(dir.path(), &id).expect("load");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 1);
    assert!(events[0].prev_sha256.is_none());
}

#[test]
fn flipping_a_byte_breaks_the_chain() {
    let dir = tempdir().unwrap();
    let id = open_one(dir.path());
    // Add a legal second event so there are >= 2 lines to chain.
    store::append(
        dir.path(),
        &id,
        EventData::OwnerAssigned {
            owner: "cto".to_string(),
        },
        "jstockdi",
        None,
        Some(Utc.with_ymd_and_hms(2026, 5, 25, 14, 0, 1).unwrap()),
    )
    .expect("owner-assigned is legal");

    let path = dir.path().join("risks").join(&id).join("events.jsonl");
    assert!(store::load_events(dir.path(), &id).is_ok());
    tamper_first_line(&path);
    assert!(
        store::load_events(dir.path(), &id).is_err(),
        "a flipped byte in line 1 must break the chain or parse"
    );
}

/// Flip one byte inside the content of the first line (never the newline), so
/// either the JSON fails to parse or the next line's `prev_sha256` no longer
/// matches the recomputed hash of the mutated line.
fn tamper_first_line(path: &std::path::Path) {
    let mut bytes = std::fs::read(path).expect("read log");
    let first_nl = bytes
        .iter()
        .position(|&b| b == b'\n')
        .expect("log has at least one newline");
    // Flip a byte well inside the first line's content (not the newline).
    let target = first_nl / 2;
    bytes[target] ^= 0x20;
    std::fs::write(path, &bytes).expect("write tampered log");
}

// ---------- the property ----------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn append_keeps_the_chain_valid_and_fold_deterministic(
        intents in prop::collection::vec(intent_strategy(), 0..30),
    ) {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let id = open_one(root);

        for (i, intent) in intents.iter().enumerate() {
            // Read current state to fill `status-changed.from`. The log is
            // always valid here (append never writes an invalid line), so
            // load + fold must succeed.
            let events = store::load_events(root, &id).expect("log stays loadable");
            let current = risks::fold(&events).status;
            let data = intent_to_event(intent, current, i + 1);
            // Ignore Err: rejected illegal transitions / schema violations
            // are expected and must leave the log intact.
            let _ = store::append(
                root,
                &id,
                data,
                "jstockdi",
                None,
                // Distinct, monotonically increasing timestamps.
                Some(Utc.with_ymd_and_hms(2026, 5, 25, 14, 0, 0).unwrap()
                    + chrono::Duration::seconds((i as i64) + 1)),
            );
        }

        // Property 1 — chain always valid: load_events verifies seq
        // monotonicity, leading `opened`, and the prev_sha256 chain.
        let events = store::load_events(root, &id)
            .expect("chain must verify after all appends");
        prop_assert!(!events.is_empty());
        for (idx, ev) in events.iter().enumerate() {
            prop_assert_eq!(ev.seq, (idx as u64) + 1, "seq must be contiguous from 1");
        }
        prop_assert!(events[0].prev_sha256.is_none(), "first event has no prev");

        // Property 2 — fold determinism: folding the loaded events twice
        // yields equal results.
        let a = risks::fold(&events);
        let b = risks::fold(&events);
        prop_assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap(),
            "fold must be deterministic"
        );

        // Property 3 — tamper detection: with >= 2 events, flipping a byte in
        // line 1 must make load_events fail (broken chain or parse error).
        if events.len() >= 2 {
            let path = root.join("risks").join(&id).join("events.jsonl");
            tamper_first_line(&path);
            prop_assert!(
                store::load_events(root, &id).is_err(),
                "tampering with line 1 must be detected"
            );
        }
    }
}
