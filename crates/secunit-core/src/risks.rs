//! Risk register: an append-only, hash-chained event log per risk, plus a
//! derived `risks/index.json` cache.
//!
//! The register lives inside the store at `risks/<risk-id>/events.jsonl`
//! (one immutable JSON Lines event per line) and is the authoritative
//! source of a risk's state — current state is never stored, it is *folded*
//! from the events. `index.json` is a regenerable projection, the same role
//! `state.json` plays for runs. Full design in `docs/risks.md`; the on-disk
//! contract in `docs/storage.md` (the "Risk register (`risks/`)" section).
//!
//! The integrity model mirrors evidence manifests:
//!
//! - Each event carries `prev_sha256`, the SHA-256 of the previous
//!   canonicalised line (compact JSON), forming a per-risk hash chain —
//!   exactly like `prior_run.manifest_sha256` chains manifests. Lines are
//!   only ever appended; corrections are new events, never edits.
//! - `index.json` is mutable, outside the chain, and rebuilt from the logs
//!   with [`store::rebuild`].
//!
//! Module layout (mirrors `evidence`):
//!
//! - [`model`] — strongly-typed event envelope, payloads, and [`RiskState`].
//! - [`fold`] — the deterministic left-fold and the status machine.
//! - [`store`] — on-disk append protocol, id allocation, loading, and the
//!   index build/rebuild.

pub mod fold;
pub mod model;
pub mod store;

// Re-export the surface most callers (verify, CLI, GUI) want at
// `secunit_core::risks::*` so they don't have to thread the submodule path.
pub use fold::{fold, fold_at, validate_transition, TransitionError};
pub use model::{
    Agent, EventData, EventEnvelope, ExternalLink, FindingRef, RiskEvent, RiskState, Severity,
    Status,
};
pub use store::{
    append, build_index, load_events, open, rebuild, risk_ids, verify_finding_ref, AppendOutcome,
    OpenOutcome, RiskIndex, RiskIndexEntry,
};
