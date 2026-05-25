//! On-disk risk register: the append protocol, id allocation, log loading,
//! and the derived `index.json` build/rebuild.
//!
//! Writes mirror the evidence runner: every mutation takes the root lock
//! ([`crate::evidence::lock::RootLock`]), reads the tail for `seq` +
//! `prev_sha256`, validates the event, appends exactly one line with
//! `O_APPEND` semantics, and refreshes that risk's index entry. Lines are
//! never rewritten or deleted. SHA-256 reuses
//! [`crate::evidence::hasher::sha256_bytes`].

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::evidence::hasher::{atomic_write, sha256_bytes, sha256_file};
use crate::evidence::lock::RootLock;
use crate::risks::fold::{self, validate_transition};
use crate::risks::model::{
    Agent, EventData, EventEnvelope, ExternalLink, FindingRef, RiskEvent, RiskState, Severity,
    Status,
};
use crate::schemas::Schema;
use crate::SCHEMA_VERSION;

const RISKS_DIR: &str = "risks";
const EVENTS_FILE: &str = "events.jsonl";
const INDEX_FILE: &str = "index.json";

// ---------- index types -----------------------------------------------------

/// `risks/index.json` — the derived register cache, same role as
/// `state.json`. Regenerable from the logs with [`rebuild`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskIndex {
    pub schema_version: u32,
    #[serde(default)]
    pub risks: BTreeMap<String, RiskIndexEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl Default for RiskIndex {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            risks: BTreeMap::new(),
            updated_at: None,
        }
    }
}

/// One projected risk in the index — its fold flattened for fast
/// list/dashboard reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskIndexEntry {
    pub title: String,
    pub fingerprint: String,
    pub severity: Severity,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<chrono::NaiveDate>,
    pub source_control: String,
    pub first_run_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external: Vec<IndexExternal>,
    /// SHA-256 of the latest event line this entry was built from, so
    /// readers can detect staleness without re-folding.
    pub log_head_sha256: String,
}

/// External tracker mirror as projected into the index (`{system, id, url}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexExternal {
    pub system: String,
    pub id: String,
    pub url: String,
}

impl From<&ExternalLink> for IndexExternal {
    fn from(e: &ExternalLink) -> Self {
        Self {
            system: e.system.clone(),
            id: e.external_id.clone(),
            url: e.url.clone(),
        }
    }
}

// ---------- outcomes --------------------------------------------------------

/// Result of [`append`]: the event written and the new chain head sha.
#[derive(Debug, Clone)]
pub struct AppendOutcome {
    pub risk_id: String,
    pub event: RiskEvent,
    /// SHA-256 of the line just written — the new chain head.
    pub log_head_sha256: String,
}

/// Result of [`open`]: the allocated id plus the `opened` event written.
#[derive(Debug, Clone)]
pub struct OpenOutcome {
    pub risk_id: String,
    pub event: RiskEvent,
    pub log_head_sha256: String,
}

// ---------- paths -----------------------------------------------------------

fn risks_root(root: &Path) -> PathBuf {
    root.join(RISKS_DIR)
}

fn risk_dir(root: &Path, risk_id: &str) -> PathBuf {
    risks_root(root).join(risk_id)
}

fn events_path(root: &Path, risk_id: &str) -> PathBuf {
    risk_dir(root, risk_id).join(EVENTS_FILE)
}

fn index_path(root: &Path) -> PathBuf {
    risks_root(root).join(INDEX_FILE)
}

// ---------- loading ---------------------------------------------------------

/// Read and parse a risk's `events.jsonl` in `seq` order.
///
/// Validates structural integrity: monotonic 1-based `seq`, a leading
/// `opened` event, and a `prev_sha256` chain where each line's `prev_sha256`
/// equals the SHA-256 of the previous line's bytes. Returns the parsed
/// events; fold them with [`crate::risks::fold::fold`].
pub fn load_events(root: &Path, risk_id: &str) -> Result<Vec<RiskEvent>> {
    let path = events_path(root, risk_id);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    parse_events(&text, &path)
}

fn parse_events(text: &str, path: &Path) -> Result<Vec<RiskEvent>> {
    let mut events: Vec<RiskEvent> = Vec::new();
    let mut prev_line_sha: Option<String> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            continue;
        }
        let ev: RiskEvent = serde_json::from_str(line)
            .with_context(|| format!("{}: line {} is not a valid risk event", path.display(), i + 1))?;

        let expected_seq = (events.len() as u64) + 1;
        if ev.seq != expected_seq {
            bail!(
                "{}: line {} has seq {} but expected {}",
                path.display(),
                i + 1,
                ev.seq,
                expected_seq
            );
        }
        if events.is_empty() && !matches!(ev.data, EventData::Opened { .. }) {
            bail!(
                "{}: first event is `{}`, must be `opened`",
                path.display(),
                ev.data.type_str()
            );
        }
        if ev.prev_sha256 != prev_line_sha {
            bail!(
                "{}: line {} broken hash chain (prev_sha256={:?}, expected {:?})",
                path.display(),
                i + 1,
                ev.prev_sha256,
                prev_line_sha
            );
        }
        prev_line_sha = Some(sha256_bytes(line.as_bytes()));
        events.push(ev);
    }
    if events.is_empty() {
        bail!("{}: empty risk log", path.display());
    }
    Ok(events)
}

/// Read the tail of an existing log without full validation: the next `seq`
/// and the chain head sha (SHA-256 of the last line). Returns `None` if the
/// log does not exist yet.
fn read_tail(root: &Path, risk_id: &str) -> Result<Option<(u64, String, EventEnvelope)>> {
    let path = events_path(root, risk_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let last = text
        .lines()
        .map(|l| l.trim_end_matches(['\r', '\n']))
        .rfind(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow!("{}: log exists but is empty", path.display()))?;
    let ev: EventEnvelope = serde_json::from_str(last)
        .with_context(|| format!("{}: tail line is not a valid risk event", path.display()))?;
    let head_sha = sha256_bytes(last.as_bytes());
    Ok(Some((ev.seq + 1, head_sha, ev)))
}

// ---------- canonical line --------------------------------------------------

/// Serialise an envelope to its canonical, compact JSON line (no trailing
/// newline). This is the exact byte string written to disk and the input to
/// the next event's `prev_sha256`, so it must be deterministic.
fn canonical_line(ev: &EventEnvelope) -> Result<String> {
    Ok(serde_json::to_string(ev)?)
}

// ---------- append ----------------------------------------------------------

/// Append exactly one event to `risk_id`'s log under the root lock, then
/// refresh its index entry.
///
/// Protocol (mirrors the manifest chain + state rebuild):
/// 1. take the root lock,
/// 2. read the tail for `seq` and the chain head sha,
/// 3. validate the status transition (for lifecycle events) against the
///    status machine and the event against `risk-event.schema.json`,
/// 4. set `prev_sha256` to the head sha and `seq` to tail+1,
/// 5. append ONE line with `O_APPEND` — existing lines are never touched,
/// 6. refresh this risk's `index.json` entry from the freshly-folded state.
///
/// `actor` is the operator handle; `agent` is `Some` when an agent (not a
/// direct operator action) appended the event. `ts` lets tests pin the clock;
/// production callers pass `None` for wall-clock.
pub fn append(
    root: &Path,
    risk_id: &str,
    data: EventData,
    actor: &str,
    agent: Option<Agent>,
    ts: Option<DateTime<Utc>>,
) -> Result<AppendOutcome> {
    let _lock = RootLock::acquire(root).context("acquire root lock")?;
    append_locked(root, risk_id, data, actor, agent, ts)
}

/// The append body, assuming the caller already holds the root lock. Used by
/// [`open`], which must allocate the id and write under one lock hold.
fn append_locked(
    root: &Path,
    risk_id: &str,
    data: EventData,
    actor: &str,
    agent: Option<Agent>,
    ts: Option<DateTime<Utc>>,
) -> Result<AppendOutcome> {
    let tail = read_tail(root, risk_id)?;
    let (seq, prev_sha256, prior_events) = match &tail {
        None => {
            // First event of a brand-new log must be `opened`.
            if !matches!(data, EventData::Opened { .. }) {
                bail!(
                    "cannot append `{}` to {risk_id}: log does not exist (first event must be `opened`)",
                    data.type_str()
                );
            }
            (1u64, None, Vec::new())
        }
        Some((next_seq, head_sha, _tail_ev)) => {
            if matches!(data, EventData::Opened { .. }) {
                bail!("cannot append a second `opened` event to {risk_id}");
            }
            // Load + validate the full prior chain so we fold against a sane
            // state and can validate transitions from the real current
            // status.
            let prior = load_events(root, risk_id)?;
            (*next_seq, Some(head_sha.clone()), prior)
        }
    };

    let ev = EventEnvelope {
        seq,
        ts: ts.unwrap_or_else(Utc::now),
        actor: actor.to_string(),
        agent,
        prev_sha256,
        data,
    };

    // Validate the status transition for lifecycle events against the
    // current folded status. Rejected transitions never reach the log.
    if !prior_events.is_empty() {
        let current = fold::fold(&prior_events).status;
        if let Some((from, to)) = lifecycle_transition(&prior_events, &ev.data) {
            // `from` declared on status-changed must match reality; for the
            // shorthand events we derive `from` from the fold.
            if let EventData::StatusChanged { from: declared, .. } = &ev.data {
                if *declared != current {
                    bail!(
                        "status-changed `from` is {} but the risk is currently {}",
                        declared.as_str(),
                        current.as_str()
                    );
                }
            }
            validate_transition(from, to).map_err(|e| anyhow!(e))?;
        }
    }

    // Schema-validate the on-disk shape before writing.
    let value = serde_json::to_value(&ev)?;
    let errs = Schema::RiskEvent.validate(&value);
    if !errs.is_empty() {
        bail!(
            "risk event fails risk-event.schema.json: {}",
            errs.join("; ")
        );
    }

    let line = canonical_line(&ev)?;
    let head_sha = sha256_bytes(line.as_bytes());

    // Ensure the risk dir exists, then append one line with O_APPEND.
    fs::create_dir_all(risk_dir(root, risk_id))?;
    let path = events_path(root, risk_id);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {} for append", path.display()))?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;

    // Refresh this risk's index entry from the now-current fold.
    let mut all = prior_events;
    all.push(ev.clone());
    refresh_index_entry(root, risk_id, &all, &head_sha)?;

    Ok(AppendOutcome {
        risk_id: risk_id.to_string(),
        event: ev,
        log_head_sha256: head_sha,
    })
}

/// For an event about to be applied to a non-empty log, return the
/// `(from, to)` lifecycle transition it represents, or `None` if it isn't a
/// lifecycle-changing event.
fn lifecycle_transition(prior: &[RiskEvent], data: &EventData) -> Option<(Status, Status)> {
    let current = fold::fold(prior).status;
    match data {
        EventData::StatusChanged { from, to, .. } => Some((*from, *to)),
        EventData::Remediated { .. } => Some((current, Status::Remediated)),
        EventData::Reopened { .. } => Some((current, Status::Reopened)),
        EventData::ExceptionDocumented { .. } => Some((current, Status::AcceptedException)),
        _ => None,
    }
}

// ---------- open ------------------------------------------------------------

/// Allocate the next `R-NNNN` id under the root lock and write the `opened`
/// event. The fingerprint is `<control_id>:<finding_id>` carried in
/// `finding_ref`.
///
/// Callers that promote a sealed `draft_risk` from a run should first call
/// [`verify_finding_ref`] so the risk cannot be bound to absent or fabricated
/// evidence.
#[allow(clippy::too_many_arguments)]
pub fn open(
    root: &Path,
    finding_ref: FindingRef,
    title: impl Into<String>,
    severity: Severity,
    impact: u8,
    likelihood: u8,
    affected_systems: Vec<String>,
    sla_days: u32,
    due_at: chrono::NaiveDate,
    actor: &str,
    agent: Option<Agent>,
    ts: Option<DateTime<Utc>>,
) -> Result<OpenOutcome> {
    let _lock = RootLock::acquire(root).context("acquire root lock")?;
    let risk_id = allocate_risk_id(root)?;
    let data = EventData::Opened {
        finding_ref,
        title: title.into(),
        severity,
        impact,
        likelihood,
        affected_systems,
        sla_days,
        due_at,
    };
    let out = append_locked(root, &risk_id, data, actor, agent, ts)?;
    Ok(OpenOutcome {
        risk_id: out.risk_id,
        event: out.event,
        log_head_sha256: out.log_head_sha256,
    })
}

/// Scan `risks/` for the highest `R-NNNN` and return the next id. Globally
/// sequential, zero-padded to four digits, allocated under the lock.
fn allocate_risk_id(root: &Path) -> Result<String> {
    let dir = risks_root(root);
    fs::create_dir_all(&dir)?;
    let mut max = 0u32;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            if let Some(n) = parse_risk_id(name) {
                max = max.max(n);
            }
        }
    }
    Ok(format!("R-{:04}", max + 1))
}

/// Parse the numeric component of an `R-NNNN` id.
fn parse_risk_id(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("R-")?;
    if digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()) {
        digits.parse().ok()
    } else {
        None
    }
}

// ---------- finding-ref verification ----------------------------------------

/// Verify a `finding_ref` resolves to a real sealed manifest whose recomputed
/// sha matches `manifest_sha256`. Used by `risks open --from <run-dir>` so a
/// risk cannot be bound to absent or fabricated evidence.
///
/// `run_dir` is the sealed run directory holding `manifest.json`. Errors if
/// the manifest is missing, its sha mismatches, or its control/run id differ
/// from the finding ref.
pub fn verify_finding_ref(run_dir: &Path, finding_ref: &FindingRef) -> Result<()> {
    let manifest_path = run_dir.join("manifest.json");
    if !manifest_path.exists() {
        bail!(
            "finding_ref points at {} but no manifest.json exists there",
            run_dir.display()
        );
    }
    let actual = sha256_file(&manifest_path)
        .with_context(|| format!("hash {}", manifest_path.display()))?;
    if actual != finding_ref.manifest_sha256 {
        bail!(
            "manifest sha mismatch for {}: finding_ref says {}, recomputed {}",
            run_dir.display(),
            finding_ref.manifest_sha256,
            actual
        );
    }
    // Cross-check the manifest identifies the same control/run.
    let bytes = fs::read(&manifest_path)?;
    let manifest: crate::evidence::manifest::Manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    if manifest.control_id != finding_ref.control_id {
        bail!(
            "finding_ref control_id `{}` != manifest control_id `{}`",
            finding_ref.control_id,
            manifest.control_id
        );
    }
    if manifest.run_id != finding_ref.run_id {
        bail!(
            "finding_ref run_id `{}` != manifest run_id `{}`",
            finding_ref.run_id,
            manifest.run_id
        );
    }
    Ok(())
}

// ---------- index build / rebuild -------------------------------------------

/// Project a folded state into an index entry pinned to `log_head_sha256`.
fn entry_from_state(state: &RiskState, log_head_sha256: &str) -> RiskIndexEntry {
    RiskIndexEntry {
        title: state.title.clone(),
        fingerprint: state.fingerprint().unwrap_or_default(),
        severity: state.severity,
        status: state.status,
        owner: state.owner.clone(),
        due_at: state.due_at,
        source_control: state.source_control().unwrap_or_default().to_string(),
        first_run_id: state.first_run_id().unwrap_or_default().to_string(),
        external: state.external.iter().map(IndexExternal::from).collect(),
        log_head_sha256: log_head_sha256.to_string(),
    }
}

/// Refresh a single risk's index entry in place, leaving every other entry
/// untouched. Assumes the caller holds the root lock.
fn refresh_index_entry(
    root: &Path,
    risk_id: &str,
    events: &[RiskEvent],
    log_head_sha256: &str,
) -> Result<()> {
    let path = index_path(root);
    let mut index: RiskIndex = if path.exists() {
        let bytes = fs::read(&path)?;
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "{} is corrupt; refusing to overwrite — run `risks rebuild` to regenerate",
                path.display()
            )
        })?
    } else {
        RiskIndex::default()
    };
    let state = fold::fold(events);
    index
        .risks
        .insert(risk_id.to_string(), entry_from_state(&state, log_head_sha256));
    index.updated_at = Some(Utc::now());
    write_index(&path, &index)
}

/// Build the full index in memory by folding every risk log under
/// `risks/`. Pure-ish: reads the logs but writes nothing. `rebuild` wraps
/// this with the lock and an atomic write.
pub fn build_index(root: &Path) -> Result<RiskIndex> {
    let dir = risks_root(root);
    let mut index = RiskIndex::default();
    if !dir.exists() {
        index.updated_at = Some(Utc::now());
        return Ok(index);
    }
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = match entry.file_name().to_str() {
            Some(n) if parse_risk_id(n).is_some() => n.to_string(),
            _ => continue,
        };
        if !events_path(root, &name).exists() {
            continue;
        }
        let events = load_events(root, &name)
            .with_context(|| format!("load events for {name}"))?;
        let head_sha = log_head_sha(root, &name)?;
        let state = fold::fold(&events);
        index
            .risks
            .insert(name, entry_from_state(&state, &head_sha));
    }
    index.updated_at = Some(Utc::now());
    Ok(index)
}

/// Regenerate `risks/index.json` from all logs and write it atomically under
/// the root lock — the `state.json` rebuild analogue for the register.
pub fn rebuild(root: &Path) -> Result<RiskIndex> {
    let _lock = RootLock::acquire(root).context("acquire root lock")?;
    let index = build_index(root)?;
    fs::create_dir_all(risks_root(root))?;
    write_index(&index_path(root), &index)?;
    Ok(index)
}

/// SHA-256 of a risk log's last (head) line — the chain head.
fn log_head_sha(root: &Path, risk_id: &str) -> Result<String> {
    let path = events_path(root, risk_id);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let last = text
        .lines()
        .map(|l| l.trim_end_matches(['\r', '\n']))
        .rfind(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow!("{}: empty log", path.display()))?;
    Ok(sha256_bytes(last.as_bytes()))
}

fn write_index(path: &Path, index: &RiskIndex) -> Result<()> {
    // Validate against risk-index.schema.json so a malformed projection
    // never lands on disk.
    let value = serde_json::to_value(index)?;
    let errs = Schema::RiskIndex.validate(&value);
    if !errs.is_empty() {
        bail!("risk index fails risk-index.schema.json: {}", errs.join("; "));
    }
    // Pretty-printed: the index is a derived cache (not chained), so human
    // readability beats compactness, matching state.json.
    let bytes = serde_json::to_vec_pretty(index)?;
    atomic_write(path, &bytes).with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    include!("tests.rs");
}
