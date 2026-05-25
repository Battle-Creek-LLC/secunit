//! The deterministic left-fold over a risk's events and the status machine
//! that governs lifecycle transitions.
//!
//! The fold is a pure function of the event list: last-write-wins per field,
//! status follows the latest lifecycle event, finding refs accumulate. The
//! status machine (see `docs/risks.md`) is enforced at append time so an
//! illegal `status-changed` never lands in the log.

use chrono::Duration;

use super::model::{EventData, RiskEvent, RiskState, Status};

/// An attempted lifecycle transition that the status machine rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionError {
    pub from: Status,
    pub to: Status,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "illegal status transition: {} → {}",
            self.from.as_str(),
            self.to.as_str()
        )
    }
}

impl std::error::Error for TransitionError {}

/// The status machine, as drawn in `docs/risks.md`:
///
/// ```text
///             ┌─────────────► accepted-exception
///             │                       │
/// opened ──► open ──► in-progress ──► remediated ──► reopened ──► open
///             │            │              │
///             └────────────┴──────────────┴──► false-positive
/// ```
///
/// Returns `Ok(())` if moving `from → to` is legal, else a
/// [`TransitionError`]. A no-op (`from == to`) is rejected — a
/// `status-changed` event must actually change the status.
pub fn validate_transition(from: Status, to: Status) -> Result<(), TransitionError> {
    use Status::*;
    let ok = match (from, to) {
        // Any non-terminal state may be ruled a false positive.
        (Open | InProgress | Remediated, FalsePositive) => true,
        // Open or in-progress may be accepted as a documented exception.
        (Open | InProgress, AcceptedException) => true,
        // Forward progress.
        (Open, InProgress) => true,
        (InProgress, Remediated) => true,
        // A remediation can be undone.
        (Remediated, Reopened) => true,
        // The transient Reopened marker resolves back to Open. (The fold
        // also normalises Reopened → Open, but allow it explicitly so a
        // direct status-changed to/from it round-trips.)
        (Reopened, Open) => true,
        // A reopened risk that re-enters work goes straight to in-progress,
        // and an accepted exception that lapses can be reopened to open.
        (Reopened, InProgress) => true,
        (AcceptedException, Open) => true,
        // Everything else (including no-op and all terminal-state exits) is
        // rejected.
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(TransitionError { from, to })
    }
}

/// Fold events in `seq` order into the current [`RiskState`].
///
/// Deterministic and total: the first event MUST be `opened` (callers
/// guarantee this — [`super::store::open`] writes it), and the result is a
/// pure function of the slice. The caller is responsible for passing events
/// already sorted by `seq`; [`super::store::load_events`] does this.
///
/// # Panics
///
/// Panics if the first event is not `opened`, since a log that doesn't start
/// with `opened` is structurally corrupt and cannot produce a coherent
/// state. Loaders validate this before folding.
pub fn fold(events: &[RiskEvent]) -> RiskState {
    let first = events.first().expect("fold: empty event log");
    let mut state = match &first.data {
        EventData::Opened {
            finding_ref,
            title,
            severity,
            impact,
            likelihood,
            affected_systems,
            sla_days,
            due_at,
        } => RiskState {
            status: Status::Open,
            title: title.clone(),
            severity: *severity,
            impact: *impact,
            likelihood: *likelihood,
            owner: None,
            due_at: Some(*due_at),
            sla_days: Some(*sla_days),
            affected_systems: affected_systems.clone(),
            finding_refs: vec![finding_ref.clone()],
            external: Vec::new(),
            external_status: Default::default(),
            resolved_at: None,
            exception_expires_at: None,
        },
        other => panic!(
            "fold: first event must be `opened`, got `{}`",
            other.type_str()
        ),
    };

    for ev in &events[1..] {
        apply(&mut state, ev);
    }
    state
}

fn apply(state: &mut RiskState, ev: &RiskEvent) {
    match &ev.data {
        // Already consumed as the seed; a second `opened` is ignored (the
        // schema/append protocol prevents it landing in the first place).
        EventData::Opened { .. } => {}
        EventData::OwnerAssigned { owner } => {
            state.owner = Some(owner.clone());
        }
        EventData::ScoreChanged {
            impact,
            likelihood,
            severity,
            ..
        } => {
            state.impact = *impact;
            state.likelihood = *likelihood;
            state.severity = *severity;
            // Recompute due_at if the SLA derives from an sla_days window
            // anchored on the open date. We anchor on the originating
            // finding's risk-open day, which is the first event's date.
            if let Some(days) = state.sla_days {
                if let Some(first_ref_day) = open_day(state) {
                    state.due_at = Some(first_ref_day + Duration::days(days as i64));
                }
            }
        }
        EventData::SlaSet { due_at, .. } => {
            state.due_at = Some(*due_at);
            // An explicit override detaches due_at from the sla_days basis.
            state.sla_days = None;
        }
        EventData::StatusChanged { to, .. } => {
            set_status(state, *to, ev);
        }
        EventData::EvidenceLinked { finding_ref } => {
            state.finding_refs.push(finding_ref.clone());
        }
        EventData::ExternalLinked {
            system,
            external_id,
            url,
        } => {
            state.external.push(super::model::ExternalLink {
                system: system.clone(),
                external_id: external_id.clone(),
                url: url.clone(),
            });
        }
        EventData::ExternalStatusObserved { system, status, .. } => {
            // Advisory only — recorded, never authoritative over `status`.
            state
                .external_status
                .insert(system.clone(), status.clone());
        }
        EventData::Note { .. } => {}
        EventData::Remediated { .. } => {
            set_status(state, Status::Remediated, ev);
        }
        EventData::Reopened { .. } => {
            // remediated → open (Reopened is the transient marker; resolve
            // straight to Open and clear the resolution timestamp).
            state.status = Status::Open;
            state.resolved_at = None;
        }
        EventData::ExceptionDocumented { expires_at, .. } => {
            state.status = Status::AcceptedException;
            state.exception_expires_at = Some(*expires_at);
        }
    }
}

/// Apply a status change, normalising the transient `Reopened` marker to
/// `Open` and maintaining `resolved_at`.
fn set_status(state: &mut RiskState, to: Status, ev: &RiskEvent) {
    match to {
        Status::Remediated => {
            state.status = Status::Remediated;
            state.resolved_at = Some(ev.ts);
        }
        Status::Reopened | Status::Open => {
            state.status = Status::Open;
            state.resolved_at = None;
        }
        other => {
            state.status = other;
        }
    }
}

/// The risk's open day, derived from the originating finding's `run_id`
/// (YYYY-MM-DD prefix), used as the anchor for recomputing `due_at`.
fn open_day(state: &RiskState) -> Option<chrono::NaiveDate> {
    let run_id = &state.finding_refs.first()?.run_id;
    run_id
        .get(0..10)
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}
