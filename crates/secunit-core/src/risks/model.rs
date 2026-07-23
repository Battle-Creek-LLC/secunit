//! Strongly-typed risk event envelope, per-type payloads, and the folded
//! [`RiskState`].
//!
//! Every type here maps 1:1 to `risk-event.schema.json` /
//! `risk-index.schema.json`. Keep field names aligned with the schemas —
//! `serde` is the load path, and the on-disk line is the chained, hashed
//! source of truth, so the JSON shape is a contract, not an implementation
//! detail.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ---------- shared primitives -----------------------------------------------

/// Risk severity. Ordered most- to least-severe in declaration order so
/// callers can compare and sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Lifecycle status. The legal transitions between these are enforced by
/// [`crate::risks::fold::validate_transition`]; see `docs/risks.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Initial state set by `opened`, and the state after `reopened`.
    Open,
    InProgress,
    Remediated,
    /// Transient lifecycle marker emitted by a `reopened` event; the fold
    /// resolves it back to `Open`.
    Reopened,
    AcceptedException,
    FalsePositive,
}

impl Status {
    /// Still-open lifecycle states — the risk demands ongoing work.
    /// Exhaustive on purpose: a new variant must decide its partition
    /// here, not fall through a consumer's wildcard arm.
    pub fn is_open(self) -> bool {
        match self {
            Status::Open | Status::InProgress | Status::Reopened => true,
            Status::Remediated | Status::AcceptedException | Status::FalsePositive => false,
        }
    }

    /// Terminal states. Note: SLA surfacing is deliberately narrower —
    /// `secunit risks list --past-sla` still shows an accepted-exception
    /// whose date has lapsed (see `cmd/risks.rs::is_past_sla`), so don't
    /// substitute this for that check.
    pub fn is_closed(self) -> bool {
        !self.is_open()
    }

    /// The on-disk kebab-case spelling used in `status-changed` payloads
    /// and the index.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in-progress",
            Status::Remediated => "remediated",
            Status::Reopened => "reopened",
            Status::AcceptedException => "accepted-exception",
            Status::FalsePositive => "false-positive",
        }
    }
}

/// `{ model, skill }` when an agent appended the event; `None` for a direct
/// operator action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub model: String,
    pub skill: String,
}

/// Binds a risk to immutable evidence by content hash. The fingerprint
/// `<control_id>:<finding_id>` is the risk's cross-run identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingRef {
    pub control_id: String,
    pub run_id: String,
    pub manifest_sha256: String,
    pub finding_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_path: Option<String>,
}

impl FindingRef {
    /// `<control_id>:<finding_id>` — the cross-run identity of the risk this
    /// finding produced.
    pub fn fingerprint(&self) -> String {
        format!("{}:{}", self.control_id, self.finding_id)
    }
}

/// A tracker mirror created by sync-out, recorded by `external-linked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLink {
    pub system: String,
    pub external_id: String,
    pub url: String,
}

// ---------- event envelope --------------------------------------------------

/// One line of `events.jsonl`: the immutable envelope plus its hash-chain
/// fields. `data` is flattened so the on-disk shape is
/// `{seq, ts, actor, agent, type, prev_sha256, data}` (the `type`/`data`
/// pair is produced by [`EventData`]'s adjacent tagging).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// 1-based, monotonic within the file.
    pub seq: u64,
    /// ISO-8601 UTC.
    pub ts: DateTime<Utc>,
    /// Operator handle responsible for the change.
    pub actor: String,
    /// `{ model, skill }` when an agent appended this; `null` otherwise.
    #[serde(default)]
    pub agent: Option<Agent>,
    /// SHA-256 of the previous canonicalised line; `None` on `seq: 1`. This
    /// is the per-risk hash chain.
    pub prev_sha256: Option<String>,
    /// The typed payload (`type` + `data` on the wire).
    #[serde(flatten)]
    pub data: EventData,
}

/// Back-compat alias: the envelope *is* the event. Callers may use either
/// name; `RiskEvent` reads better at use sites that talk about "events".
pub type RiskEvent = EventEnvelope;

/// The per-`type` payload. Adjacently tagged so it serialises as
/// `{"type": "...", "data": { ... }}`, matching `risk-event.schema.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "kebab-case")]
pub enum EventData {
    /// Creates the risk; status → `open`.
    Opened {
        finding_ref: FindingRef,
        title: String,
        severity: Severity,
        impact: u8,
        likelihood: u8,
        affected_systems: Vec<String>,
        sla_days: u32,
        due_at: NaiveDate,
    },
    OwnerAssigned {
        owner: String,
    },
    /// Supersedes the score.
    ScoreChanged {
        impact: u8,
        likelihood: u8,
        severity: Severity,
        reason: String,
    },
    /// Overrides the SLA due date.
    SlaSet {
        due_at: NaiveDate,
        basis: String,
    },
    /// Moves through the status machine.
    StatusChanged {
        from: Status,
        to: Status,
        reason: String,
    },
    /// Appends another finding ref — how a risk re-observed in a later run
    /// is recorded as *persisting*.
    EvidenceLinked {
        finding_ref: FindingRef,
    },
    /// Records the tracker mirror created by sync-out.
    ExternalLinked {
        system: String,
        external_id: String,
        url: String,
    },
    /// Advisory inbound status from a tracker. Never authoritative.
    ExternalStatusObserved {
        system: String,
        status: String,
        observed_at: DateTime<Utc>,
    },
    /// Free-text note; no state change.
    Note {
        text: String,
    },
    /// Shorthand for `status-changed → remediated` with evidence.
    Remediated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved_run_ref: Option<FindingRef>,
        note: String,
    },
    /// `remediated → open`.
    Reopened {
        reason: String,
    },
    /// Status → `accepted-exception`.
    ExceptionDocumented {
        rationale: String,
        approved_by: String,
        expires_at: NaiveDate,
    },
}

impl EventData {
    /// The on-disk `type` discriminant string (kebab-case), useful for
    /// timelines and diagnostics without re-serialising.
    pub fn type_str(&self) -> &'static str {
        match self {
            EventData::Opened { .. } => "opened",
            EventData::OwnerAssigned { .. } => "owner-assigned",
            EventData::ScoreChanged { .. } => "score-changed",
            EventData::SlaSet { .. } => "sla-set",
            EventData::StatusChanged { .. } => "status-changed",
            EventData::EvidenceLinked { .. } => "evidence-linked",
            EventData::ExternalLinked { .. } => "external-linked",
            EventData::ExternalStatusObserved { .. } => "external-status-observed",
            EventData::Note { .. } => "note",
            EventData::Remediated { .. } => "remediated",
            EventData::Reopened { .. } => "reopened",
            EventData::ExceptionDocumented { .. } => "exception-documented",
        }
    }
}

// ---------- folded state ----------------------------------------------------

/// The current state of a risk, produced by folding its events in `seq`
/// order. Nothing here is stored on disk — it is always recomputed from the
/// log. See [`crate::risks::fold::fold`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskState {
    /// Current lifecycle status.
    pub status: Status,
    pub title: String,
    pub severity: Severity,
    pub impact: u8,
    pub likelihood: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// SLA due date — defaulted from `opened`, overridable by `sla-set` and
    /// recomputed on `score-changed` when an `sla_days` basis is in effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<NaiveDate>,
    /// SLA window in days, carried from `opened`; used to recompute `due_at`
    /// when the score changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sla_days: Option<u32>,
    pub affected_systems: Vec<String>,
    /// Every finding ref bound to the risk, in append order. The first is
    /// the originating finding; later ones mark re-observation (persisting).
    pub finding_refs: Vec<FindingRef>,
    /// Tracker mirrors created by sync-out.
    pub external: Vec<ExternalLink>,
    /// Latest advisory status per external system. Never authoritative.
    #[serde(default)]
    pub external_status: BTreeMap<String, String>,
    /// When the risk reached `remediated`, if it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    /// When an `accepted-exception` expires, if one is in effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception_expires_at: Option<NaiveDate>,
}

impl RiskState {
    /// `<control_id>:<finding_id>` of the originating finding — the risk's
    /// stable cross-run identity. `None` only for a degenerate empty log.
    pub fn fingerprint(&self) -> Option<String> {
        self.finding_refs.first().map(FindingRef::fingerprint)
    }

    /// The control id that produced the risk (from the first finding ref).
    pub fn source_control(&self) -> Option<&str> {
        self.finding_refs.first().map(|f| f.control_id.as_str())
    }

    /// The run id the risk was first observed in (from the first finding
    /// ref).
    pub fn first_run_id(&self) -> Option<&str> {
        self.finding_refs.first().map(|f| f.run_id.as_str())
    }
}
