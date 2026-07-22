//! `secunit risks` — the risk-register command family.
//!
//! Mutating verbs each append exactly one event to a risk's log via
//! [`secunit_core::risks::store`] (which takes the root lock, validates the
//! transition + schema, chains, and refreshes `index.json`). Read verbs are
//! pure folds: `list` rebuilds the index in memory from the logs (it does
//! not trust the cached `index.json`, and errors if any log is corrupt),
//! `show` folds a single log, `rebuild` regenerates the on-disk index.
//!
//! Output follows `docs/cli.md`: `list`/`show` are human tables by default
//! and flip to structured JSON under `--json`; mutating verbs print a short
//! confirmation line.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, NaiveDate, Utc};
use secunit_core::evidence::hasher::sha256_file;
use secunit_core::evidence::manifest::Manifest;
use secunit_core::risks::model::{
    Agent, EventData, FindingRef, RiskEvent, RiskState, Severity, Status,
};
use secunit_core::risks::{fold, store};

use super::Ctx;

/// Fallback SLA window when the source control declares no
/// `remediation_thresholds` entry for the risk's severity.
const DEFAULT_SLA_DAYS: u32 = 90;

// ---------- shared helpers --------------------------------------------------

/// The operator handle responsible for the change. The CLI is invoked by an
/// operator (or an agent acting as one); we record the handle from the
/// environment, falling back to a generic label so the field is never empty.
fn actor() -> String {
    std::env::var("SECUNIT_OPERATOR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "operator".to_string())
}

/// CLI mutations are direct operator actions, so no agent is attributed.
fn agent() -> Option<Agent> {
    None
}

/// Wall-clock event timestamp. We do not pin events to `ctx.today` because
/// the chain records when the change actually happened; `ctx.today` only
/// governs SLA / past-due *reads*.
fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

/// Parse a `Severity` from its on-disk spelling, case-insensitively, so a
/// legacy draft carrying `"High"` resolves the same as `"high"`.
fn parse_severity(s: &str) -> Result<Severity> {
    serde_json::from_value(serde_json::Value::String(s.to_lowercase()))
        .map_err(|_| anyhow!("invalid severity `{s}` (expected critical|high|medium|low|info)"))
}

/// Default `(impact, likelihood)` for a severity, used to project a legacy
/// draft that omits an explicit score. The mapping PRESERVES severity
/// ordering — a more severe finding never derives a lower combined score —
/// so the auto-opened risk sorts sanely until an operator refines it with
/// `risks score`. Both values stay within the schema's 1..=5 range.
///
/// | severity | impact | likelihood |
/// |----------|--------|------------|
/// | critical |   4    |     4      |
/// | high     |   3    |     3      |
/// | medium   |   2    |     3      |
/// | low      |   1    |     2      |
/// | info     |   1    |     1      |
fn default_score(severity: Severity) -> (u8, u8) {
    match severity {
        Severity::Critical => (4, 4),
        Severity::High => (3, 3),
        Severity::Medium => (2, 3),
        Severity::Low => (1, 2),
        Severity::Info => (1, 1),
    }
}

/// A lowercase, hyphen-joined slug of an arbitrary string, for tolerant
/// `--finding` matching against a draft's `subject`/`title`. Runs of
/// non-alphanumeric characters collapse to a single `-`.
fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Parse a `Status` from its on-disk kebab-case spelling.
fn parse_status(s: &str) -> Result<Status> {
    serde_json::from_value(serde_json::Value::String(s.to_lowercase()))
        .map_err(|_| {
            anyhow!(
                "invalid status `{s}` (expected open|in-progress|remediated|reopened|accepted-exception|false-positive)"
            )
        })
}

/// Load + fold a single risk's log into its current state.
fn load_state(root: &Path, risk_id: &str) -> Result<(Vec<RiskEvent>, RiskState)> {
    let events =
        store::load_events(root, risk_id).with_context(|| format!("load events for {risk_id}"))?;
    let state = fold::fold(&events);
    Ok((events, state))
}

/// A finding ref built from a sealed run's manifest, plus the matched draft
/// risk's projected fields, used by `open` and `relink`.
struct ResolvedFinding {
    finding_ref: FindingRef,
    title: String,
    severity: Severity,
    impact: u8,
    likelihood: u8,
    affected_systems: Vec<String>,
    completed_at: NaiveDate,
}

/// Read a sealed run's `manifest.json`, locate the named draft risk, and
/// build a verified [`FindingRef`] bound to the recomputed manifest sha.
///
/// `--finding` is matched leniently so legacy / hand-written drafts still
/// promote — see [`draft_matches`] for the precedence. Fields absent on older
/// drafts are projected with documented fallbacks: `title` ← `subject`,
/// `affected_systems` ← `affected`, severity defaults to High, and an absent
/// `impact`/`likelihood` is derived from severity via [`default_score`]. A
/// one-line note is printed whenever a defaulted score or single-draft
/// fallback is applied so the operator knows to refine via `risks score`.
fn resolve_finding(run_dir: &Path, finding: &str) -> Result<ResolvedFinding> {
    let manifest_path = run_dir.join("manifest.json");
    if !manifest_path.exists() {
        bail!(
            "no manifest.json under {} — is the run sealed?",
            run_dir.display()
        );
    }
    let bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    // Recompute the sha exactly as `verify_finding_ref` will, so the ref we
    // build is the one the verifier accepts.
    let manifest_sha256 =
        sha256_file(&manifest_path).with_context(|| format!("hash {}", manifest_path.display()))?;

    // Match `--finding` against each draft in precedence order. If nothing
    // matches but the run carries exactly one draft, accept it (the operator
    // clearly means that one) with a clear note.
    let (draft, resolved_id) = match manifest
        .draft_risks
        .iter()
        .find_map(|d| draft_matches(d, finding).map(|id| (d, id)))
    {
        Some(hit) => hit,
        None if manifest.draft_risks.len() == 1 => {
            let only = &manifest.draft_risks[0];
            // Use the draft's own stable id if it has one, else the operator's
            // `--finding` string, so the FindingRef fingerprint is meaningful.
            let id = draft_stable_id(only).unwrap_or_else(|| finding.to_string());
            eprintln!(
                "note: `--finding {finding}` matched no field, but this run has a single \
                 draft_risk — promoting it (finding_id `{id}`)."
            );
            (only, id)
        }
        None => bail!(
            "no draft_risk matching `{finding}` in {} ({} draft(s) present)",
            manifest_path.display(),
            manifest.draft_risks.len()
        ),
    };

    // title ← title, else subject, else the resolved finding id.
    let title = draft
        .get("title")
        .and_then(|v| v.as_str())
        .or_else(|| draft.get("subject").and_then(|v| v.as_str()))
        .unwrap_or(&resolved_id)
        .to_string();
    // Severity may be absent on older drafts; default to High, the usual
    // `draft_risk_at` bar. Parsed case-insensitively so `"High"` resolves.
    let severity = draft
        .get("severity")
        .and_then(|v| v.as_str())
        .map(parse_severity)
        .transpose()?
        .unwrap_or(Severity::High);
    // impact/likelihood are optional on legacy drafts. When either is absent
    // (or out of the schema's 1..=5 range), derive BOTH from severity so the
    // pair stays consistent, and tell the operator.
    let raw_impact = draft.get("impact").and_then(|v| v.as_u64());
    let raw_likelihood = draft.get("likelihood").and_then(|v| v.as_u64());
    let in_range = |v: Option<u64>| matches!(v, Some(n) if (1..=5).contains(&n));
    let (impact, likelihood) = if in_range(raw_impact) && in_range(raw_likelihood) {
        (raw_impact.unwrap() as u8, raw_likelihood.unwrap() as u8)
    } else {
        let (i, l) = default_score(severity);
        eprintln!(
            "note: draft has no usable impact/likelihood — derived ({i}, {l}) from severity \
             `{}`; refine with `risks score` if needed.",
            severity_str(severity)
        );
        (i, l)
    };
    // affected_systems ← affected_systems, else `affected`.
    let affected_systems = draft
        .get("affected_systems")
        .or_else(|| draft.get("affected"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let body_path = draft
        .get("body_path")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let finding_ref = FindingRef {
        control_id: manifest.control_id.clone(),
        run_id: manifest.run_id.clone(),
        manifest_sha256,
        // The recorded finding_id is whatever `--finding` resolved to — a
        // stable handle (the draft's id/ghsa/cve/anchor), not the raw flag —
        // so the fingerprint is reproducible across runs.
        finding_id: resolved_id,
        body_path,
    };

    // Bind the risk to immutable evidence: the manifest must exist and its
    // recomputed sha must match. (Belt-and-braces — we just computed the sha
    // from the same file, but this also cross-checks control/run id.)
    store::verify_finding_ref(run_dir, &finding_ref)
        .with_context(|| format!("verify finding ref against {}", run_dir.display()))?;

    Ok(ResolvedFinding {
        finding_ref,
        title,
        severity,
        impact,
        likelihood,
        affected_systems,
        completed_at: manifest.completed_at.date_naive(),
    })
}

/// The draft's own stable id, if it carries one: `finding_id` / `id`, else
/// the first value of a `ghsa` / `cve` array, else the trailing `#anchor` of
/// `body_path`. Used to record a meaningful fingerprint when promotion falls
/// back to a run's single draft.
fn draft_stable_id(draft: &serde_json::Value) -> Option<String> {
    for key in ["finding_id", "id"] {
        if let Some(s) = draft.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    for key in ["ghsa", "cve"] {
        if let Some(first) = draft
            .get(key)
            .and_then(|v| v.as_array())
            .and_then(|a| a.iter().find_map(|v| v.as_str()))
        {
            return Some(first.to_string());
        }
    }
    draft
        .get("body_path")
        .and_then(|v| v.as_str())
        .and_then(|p| p.split_once('#').map(|(_, anchor)| anchor.to_string()))
}

/// Does a draft-risk JSON value identify the finding `flag`? Returns the
/// stable id to record on the [`FindingRef`] when it does.
///
/// Precedence (first hit wins, so the recorded id is the strongest match):
/// 1. the draft's `finding_id` or `id` equals `flag`;
/// 2. the trailing `#anchor` of `body_path` equals `flag`
///    (e.g. `findings.md#risk-1` → `risk-1`);
/// 3. any value in a `ghsa` / `cve` array equals `flag`;
/// 4. as a last resort, a case-insensitive slug of `subject`/`title` equals
///    the slug of `flag`.
///
/// For 1–3 the recorded id is the matched value (`flag` itself); for the slug
/// fallback we record the draft's own stable id when it has one, falling back
/// to `flag`, so a fuzzy subject match still pins a reproducible fingerprint.
fn draft_matches(draft: &serde_json::Value, flag: &str) -> Option<String> {
    // 1. explicit id fields.
    for key in ["finding_id", "id"] {
        if draft.get(key).and_then(|v| v.as_str()) == Some(flag) {
            return Some(flag.to_string());
        }
    }
    // 2. body_path anchor.
    if draft
        .get("body_path")
        .and_then(|v| v.as_str())
        .and_then(|p| p.rsplit('#').next())
        == Some(flag)
    {
        return Some(flag.to_string());
    }
    // 3. ghsa / cve arrays.
    for key in ["ghsa", "cve"] {
        if let Some(arr) = draft.get(key).and_then(|v| v.as_array()) {
            if arr.iter().any(|v| v.as_str() == Some(flag)) {
                return Some(flag.to_string());
            }
        }
    }
    // 4. fuzzy: slug of subject/title.
    let want = slug(flag);
    if !want.is_empty() {
        for key in ["subject", "title"] {
            if let Some(text) = draft.get(key).and_then(|v| v.as_str()) {
                if slug(text) == want {
                    return Some(draft_stable_id(draft).unwrap_or_else(|| flag.to_string()));
                }
            }
        }
    }
    None
}

/// SLA window (days) for `severity`, from the source control's
/// `remediation_thresholds`, falling back to [`DEFAULT_SLA_DAYS`].
fn sla_days_for(ctx: &Ctx, control_id: &str, severity: Severity) -> Result<u32> {
    let (reg, _report) = ctx.load()?;
    let sev_key = serde_json::to_value(severity)?
        .as_str()
        .unwrap_or("high")
        .to_string();
    let days = reg
        .controls
        .get(control_id)
        .and_then(|c| c.remediation_thresholds.get(&sev_key).copied())
        .unwrap_or(DEFAULT_SLA_DAYS);
    Ok(days)
}

/// One-line confirmation after a mutating verb, naming the event written.
fn report_event(ctx: &Ctx, risk_id: &str, event: &RiskEvent, head_sha: &str) {
    if ctx.json {
        let payload = serde_json::json!({
            "risk_id": risk_id,
            "seq": event.seq,
            "type": event.data.type_str(),
            "log_head_sha256": head_sha,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    } else {
        println!(
            "{risk_id}: appended `{}` (seq {})",
            event.data.type_str(),
            event.seq
        );
    }
}

// ---------- mutating verbs --------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn open(
    ctx: &Ctx,
    control_id: &str,
    from: &Path,
    finding: &str,
    owner: Option<&str>,
    sla_days: Option<u32>,
) -> Result<ExitCode> {
    let resolved = resolve_finding(from, finding)?;
    // The source control id is the one named on the command line; cross-check
    // it against the manifest so a risk's `source_control` is honest.
    if resolved.finding_ref.control_id != control_id {
        bail!(
            "manifest control id `{}` != CONTROL_ID `{control_id}`",
            resolved.finding_ref.control_id
        );
    }

    let sla = match sla_days {
        Some(d) => d,
        None => sla_days_for(ctx, control_id, resolved.severity)?,
    };
    let due_at = resolved.completed_at + Duration::days(sla as i64);

    let outcome = store::open(
        &ctx.root,
        resolved.finding_ref,
        resolved.title,
        resolved.severity,
        resolved.impact,
        resolved.likelihood,
        resolved.affected_systems,
        sla,
        due_at,
        &actor(),
        agent(),
        Some(now()),
    )?;

    // Apply the optional owner as a second event, mirroring `assign`.
    if let Some(owner) = owner {
        store::append(
            &ctx.root,
            &outcome.risk_id,
            EventData::OwnerAssigned {
                owner: owner.to_string(),
            },
            &actor(),
            agent(),
            Some(now()),
        )?;
    }

    if ctx.json {
        let payload = serde_json::json!({
            "risk_id": outcome.risk_id,
            "log_head_sha256": outcome.log_head_sha256,
            "due_at": due_at,
            "sla_days": sla,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("opened {} (due {due_at}, SLA {sla}d)", outcome.risk_id);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn assign(ctx: &Ctx, risk_id: &str, owner: &str) -> Result<ExitCode> {
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::OwnerAssigned {
            owner: owner.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn score(
    ctx: &Ctx,
    risk_id: &str,
    impact: u8,
    likelihood: u8,
    severity: &str,
    reason: &str,
) -> Result<ExitCode> {
    let severity = parse_severity(severity)?;
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::ScoreChanged {
            impact,
            likelihood,
            severity,
            reason: reason.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn status(ctx: &Ctx, risk_id: &str, to: &str, reason: &str) -> Result<ExitCode> {
    // The append protocol validates `from` against the current fold, so we
    // read it here and let the store reject illegal transitions.
    let (_events, state) = load_state(&ctx.root, risk_id)?;
    let to = parse_status(to)?;
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::StatusChanged {
            from: state.status,
            to,
            reason: reason.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn relink(ctx: &Ctx, risk_id: &str, from: &Path, finding: &str) -> Result<ExitCode> {
    let resolved = resolve_finding(from, finding)?;
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::EvidenceLinked {
            finding_ref: resolved.finding_ref,
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn link(ctx: &Ctx, risk_id: &str, system: &str, id: &str, url: &str) -> Result<ExitCode> {
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::ExternalLinked {
            system: system.to_string(),
            external_id: id.to_string(),
            url: url.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn observe(ctx: &Ctx, risk_id: &str, system: &str, status: &str) -> Result<ExitCode> {
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::ExternalStatusObserved {
            system: system.to_string(),
            status: status.to_string(),
            observed_at: now(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn note(ctx: &Ctx, risk_id: &str, text: &str) -> Result<ExitCode> {
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::Note {
            text: text.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn remediate(
    ctx: &Ctx,
    risk_id: &str,
    evidence: Option<&Path>,
    note: &str,
) -> Result<ExitCode> {
    // `--evidence` binds the resolution to a sealed run. We resolve it the
    // same way as `open`, but the finding id is the risk's own originating
    // finding (its fingerprint), so the resolved ref is verifiable.
    let resolved_run_ref = match evidence {
        Some(run_dir) => {
            let (_events, state) = load_state(&ctx.root, risk_id)?;
            let finding_id = state
                .finding_refs
                .first()
                .map(|f| f.finding_id.clone())
                .ok_or_else(|| anyhow!("{risk_id} has no originating finding"))?;
            Some(resolve_finding(run_dir, &finding_id)?.finding_ref)
        }
        None => None,
    };
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::Remediated {
            resolved_run_ref,
            note: note.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn reopen(ctx: &Ctx, risk_id: &str, reason: &str) -> Result<ExitCode> {
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::Reopened {
            reason: reason.to_string(),
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

pub fn except(
    ctx: &Ctx,
    risk_id: &str,
    rationale: &str,
    approved_by: &str,
    expires: NaiveDate,
) -> Result<ExitCode> {
    let out = store::append(
        &ctx.root,
        risk_id,
        EventData::ExceptionDocumented {
            rationale: rationale.to_string(),
            approved_by: approved_by.to_string(),
            expires_at: expires,
        },
        &actor(),
        agent(),
        Some(now()),
    )?;
    report_event(ctx, risk_id, &out.event, &out.log_head_sha256);
    Ok(ExitCode::SUCCESS)
}

// ---------- read verbs ------------------------------------------------------

pub fn list(
    ctx: &Ctx,
    status: Option<&str>,
    severity: Option<&str>,
    owner: Option<&str>,
    past_sla: bool,
) -> Result<ExitCode> {
    let index = store::build_index(&ctx.root)?;

    let want_status = status.map(parse_status).transpose()?;
    // `--severity` accepts a comma-separated list (e.g. `critical,high`).
    let want_sevs: Option<Vec<Severity>> = severity
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(parse_severity)
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;

    let mut rows: Vec<(&String, &store::RiskIndexEntry)> = index
        .risks
        .iter()
        .filter(|(_, e)| want_status.is_none_or(|s| e.status == s))
        .filter(|(_, e)| {
            want_sevs
                .as_ref()
                .is_none_or(|sevs| sevs.contains(&e.severity))
        })
        .filter(|(_, e)| owner.is_none_or(|o| e.owner.as_deref() == Some(o)))
        .filter(|(_, e)| !past_sla || is_past_sla(e, ctx.today))
        .collect();
    // Stable, deterministic order: by id (BTreeMap iteration already is).
    rows.sort_by(|a, b| a.0.cmp(b.0));

    if ctx.json {
        // Emit a filtered structured index (same shape as index.json).
        let filtered: std::collections::BTreeMap<_, _> = rows
            .iter()
            .map(|(id, e)| ((*id).clone(), (*e).clone()))
            .collect();
        let payload = serde_json::json!({
            "schema_version": index.schema_version,
            "risks": filtered,
            "updated_at": index.updated_at,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }

    if rows.is_empty() {
        println!("No risks match.");
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "{:<8} {:<9} {:<18} {:<14} {:<16} SOURCE",
        "ID", "SEVERITY", "STATUS", "OWNER", "DUE (SLA)"
    );
    for (id, e) in &rows {
        let owner = e.owner.as_deref().unwrap_or("—");
        let due = sla_cell(e, ctx.today);
        println!(
            "{:<8} {:<9} {:<18} {:<14} {:<16} {}",
            id,
            severity_str(e.severity),
            e.status.as_str(),
            owner,
            due,
            e.source_control,
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub fn show(ctx: &Ctx, risk_id: &str) -> Result<ExitCode> {
    let (events, state) = load_state(&ctx.root, risk_id)?;

    if ctx.json {
        let payload = serde_json::json!({
            "risk_id": risk_id,
            "state": state,
            "events": events,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!("{risk_id}  {}", state.title);
    println!("  status:    {}", state.status.as_str());
    println!("  severity:  {}", severity_str(state.severity));
    println!(
        "  score:     impact {} × likelihood {}",
        state.impact, state.likelihood
    );
    println!("  owner:     {}", state.owner.as_deref().unwrap_or("—"));
    if let Some(due) = state.due_at {
        let countdown = sla_countdown(due, ctx.today);
        println!("  due:       {due} ({countdown})");
    } else {
        println!("  due:       —");
    }
    if let Some(fp) = state.fingerprint() {
        println!("  source:    {fp}");
    }
    if !state.affected_systems.is_empty() {
        println!("  systems:   {}", state.affected_systems.join(", "));
    }
    if !state.external.is_empty() {
        for ext in &state.external {
            println!(
                "  external:  {} {} ({})",
                ext.system, ext.external_id, ext.url
            );
        }
    }

    println!();
    println!("  timeline:");
    for ev in &events {
        let agent = ev
            .agent
            .as_ref()
            .map(|a| format!(" [{}/{}]", a.model, a.skill))
            .unwrap_or_default();
        println!(
            "    {} seq {:<3} {:<24} {}{agent}",
            ev.ts.format("%Y-%m-%d %H:%M"),
            ev.seq,
            ev.data.type_str(),
            ev.actor,
        );
    }

    if !state.finding_refs.is_empty() {
        println!();
        println!("  evidence:");
        for fr in &state.finding_refs {
            let body = fr.body_path.as_deref().unwrap_or("");
            println!(
                "    {} / {} {} {}",
                fr.control_id, fr.run_id, fr.finding_id, body
            );
        }
    }

    Ok(ExitCode::SUCCESS)
}

pub fn rebuild(ctx: &Ctx) -> Result<ExitCode> {
    let index = store::rebuild(&ctx.root)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&index)?);
    } else {
        println!("rebuilt risks/index.json ({} risk(s))", index.risks.len());
    }
    Ok(ExitCode::SUCCESS)
}

// ---------- presentation helpers --------------------------------------------

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "critical",
        Severity::High => "high",
        Severity::Medium => "medium",
        Severity::Low => "low",
        Severity::Info => "info",
    }
}

/// Is the entry past its SLA as of `today`? An `accepted-exception` whose
/// `expires_at` has passed is also surfaced as overdue via `due_at`.
fn is_past_sla(entry: &store::RiskIndexEntry, today: NaiveDate) -> bool {
    // Resolved / false-positive risks are not "past SLA" — the clock stops.
    if matches!(entry.status, Status::Remediated | Status::FalsePositive) {
        return false;
    }
    entry.due_at.is_some_and(|d| d < today)
}

/// The "DUE (SLA)" cell for the list table: the date plus a countdown.
fn sla_cell(entry: &store::RiskIndexEntry, today: NaiveDate) -> String {
    match entry.due_at {
        Some(d) => format!("{d} {}", sla_countdown(d, today)),
        None => "—".to_string(),
    }
}

/// Human SLA countdown relative to `today`, e.g. `+3d`, `today`, `-2d past`.
fn sla_countdown(due: NaiveDate, today: NaiveDate) -> String {
    let days = (due - today).num_days();
    match days.cmp(&0) {
        std::cmp::Ordering::Greater => format!("+{days}d"),
        std::cmp::Ordering::Equal => "today".to_string(),
        std::cmp::Ordering::Less => format!("{}d past", days),
    }
}
