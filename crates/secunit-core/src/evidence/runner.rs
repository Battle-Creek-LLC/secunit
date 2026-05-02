//! prepare → (skill executes) → finalize, plus abort and resume.
//!
//! All filesystem state changes happen here; the agent only writes data
//! files into the slots `prepare` carved out. Hash chaining and atomic
//! writes are handled in `hasher`; concurrency in `lock`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};

use super::hasher::{self, atomic_write, sha256_bytes, sha256_file};
use super::lock::RootLock;
use super::manifest::{
    AgentInfo, Artifact, BySystemBlock, Manifest, PrepareContext, PriorRun, RunOutcome, RunResult,
    ScopeLayout, SystemOutcome,
};
use crate::model::{Cadence, LoadedRegistry, RunStatus, StateEntry};
use crate::registry::{period, resolver};
use crate::SCHEMA_VERSION;

const PENDING_SENTINEL: &str = ".run-pending";
const PREPARE_FILE: &str = "prepare.json";
const RESULT_FILE: &str = "result.json";
const MANIFEST_FILE: &str = "manifest.json";
const STATE_FILE: &str = "state.json";

/// Options for `prepare`. Only `today` is required; the rest are
/// agent-supplied metadata.
#[derive(Debug, Clone, Default)]
pub struct PrepareOpts {
    pub today: Option<NaiveDate>,
    pub operator: Option<String>,
    pub note: Option<String>,
    pub now: Option<DateTime<Utc>>,
    /// Operator-supplied period claim. `None` means derive from `today` —
    /// `period_id` records the calendar period the work was performed in.
    pub period_id: Option<String>,
}

/// Allocate a run directory, snapshot scope, write `prepare.json`, drop
/// the `.run-pending` sentinel, and return the prepare context. Holds
/// the root lock for the duration so concurrent prepares serialise.
pub fn prepare(
    reg: &LoadedRegistry,
    control_id: &str,
    opts: &PrepareOpts,
) -> Result<PrepareContext> {
    let _lock = RootLock::acquire(&reg.root).context("acquire root lock")?;
    let ctrl = reg
        .controls
        .get(control_id)
        .ok_or_else(|| anyhow!("control `{control_id}` not found"))?;

    let now = opts.now.unwrap_or_else(Utc::now);
    let today = opts.today.unwrap_or_else(|| now.date_naive());

    // Refuse to allocate a second pending run for the same control.
    let existing = list_pending(&reg.root)?;
    if let Some(p) = existing.iter().find(|r| r.control_id == control_id) {
        bail!(
            "pending run already exists for `{}` at {}",
            control_id,
            p.run_dir.display()
        );
    }

    let resolved = resolver::resolve_scope(ctrl, &reg.inventory, today);

    // Empty scope against a non-org-wide control is the silent-failure
    // case: allocating a run dir would seal a "successful" manifest with
    // zero artifacts, advancing the chain as if work happened. Fail
    // early instead — operator either updates inventory.yaml or retires
    // the control.
    if ctrl.scope.is_some() && resolved.is_empty() {
        let scope_desc = match &ctrl.scope {
            Some(crate::model::Scope::Inventory(s)) => {
                format!("kind: {}, has_tags: {:?}", s.kind, s.has_tags)
            }
            Some(crate::model::Scope::Inline(_)) => "inline".to_string(),
            None => unreachable!(),
        };
        bail!(
            "control `{control_id}` has scope ({scope_desc}) but no inventory entries match on {today}. Either update inventory.yaml or retire the control."
        );
    }

    // Per storage.md: flat is legal when scope is empty (org-wide) or
    // resolves to exactly one entry. Default to flat in both cases —
    // by-system would just nest a lone system under an extra dir.
    let scope_layout = if ctrl.scope.is_none() || resolved.len() == 1 {
        ScopeLayout::Flat
    } else {
        ScopeLayout::BySystem
    };

    // Resolve period_id before allocating any disk state so an invalid
    // --period rejects without leaving a half-formed run dir behind.
    let period_id = match &opts.period_id {
        Some(supplied) => {
            if matches!(ctrl.cadence, Cadence::Continuous) {
                bail!(
                    "control `{control_id}` has continuous cadence and does not have schedule periods; --period is not allowed"
                );
            }
            if period::bounds(ctrl.cadence, supplied).is_none() {
                bail!(
                    "`{supplied}` is not a valid period id for cadence {:?}",
                    ctrl.cadence
                );
            }
            Some(supplied.clone())
        }
        None => {
            if matches!(ctrl.cadence, Cadence::Continuous) {
                None
            } else {
                // period_id records the calendar period the work was
                // done in, so anchor on `today`. Using `next_due` here
                // would attribute a run on Sat 2026-05-02 (W18) to the
                // upcoming Mon 2026-05-04 (W19), leaving the current
                // week stuck Open in coverage.
                period::derive(ctrl.cadence, today)
            }
        }
    };

    let run_dir = allocate_run_dir(&reg.root, control_id, today)?;
    if matches!(scope_layout, ScopeLayout::BySystem) {
        for sys in &resolved {
            fs::create_dir_all(run_dir.join("by-system").join(&sys.name).join("raw"))?;
        }
    } else {
        fs::create_dir_all(run_dir.join("raw"))?;
    }

    let registry_git_sha = git_head(&reg.root).with_context(|| {
        format!(
            "{} is not a git repository — `cd` into a checked-out registry, or `git init && git commit` if starting fresh",
            reg.root.display()
        )
    })?;

    let ctx = PrepareContext {
        schema_version: SCHEMA_VERSION,
        control_id: control_id.to_string(),
        run_id: run_id_from_dir(&run_dir)?,
        run_dir: run_dir.clone(),
        started_at: now,
        operator: opts.operator.clone(),
        note: opts.note.clone(),
        scope_layout,
        resolved_scope: resolved,
        registry_git_sha,
        period_id,
    };

    let json = serde_json::to_vec_pretty(&ctx)?;
    atomic_write(&run_dir.join(PREPARE_FILE), &json)?;
    // Sentinel last — once it exists, the run is officially pending.
    atomic_write(&run_dir.join(PENDING_SENTINEL), b"")?;
    Ok(ctx)
}

/// Re-emit the prepare context for a pending run. No-op idempotent
/// helper for resuming after an interrupted agent session.
pub fn resume(run_dir: &Path) -> Result<PrepareContext> {
    let path = run_dir.join(PREPARE_FILE);
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let ctx: PrepareContext = serde_json::from_slice(&bytes)?;
    Ok(ctx)
}

/// Tear down a pending run by sealing a failed manifest. Records the
/// reason in `manifest.failure_reason` so the audit trail says why.
/// Leaves any partial evidence on disk under the run dir.
pub fn abort(reg: &LoadedRegistry, run_dir: &Path, reason: &str) -> Result<Manifest> {
    let _lock = RootLock::acquire(&reg.root).context("acquire root lock")?;

    let prepare: PrepareContext = read_json(&run_dir.join(PREPARE_FILE))?;
    let ctrl = reg
        .controls
        .get(&prepare.control_id)
        .ok_or_else(|| anyhow!("control `{}` not found", prepare.control_id))?;

    let prior_run = prior_run_link(&reg.root, &prepare.control_id, &prepare.run_id)?;
    let control_sha256 = sha256_for_control(&reg.root, &prepare.control_id)?;
    let skill_sha256 = sha256_for_skill(&reg.root, &ctrl.skill)?;

    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        control_id: prepare.control_id.clone(),
        run_id: prepare.run_id.clone(),
        started_at: prepare.started_at,
        completed_at: Utc::now(),
        operator: prepare.operator.clone(),
        agent: AgentInfo {
            model: std::env::var("SECUNIT_AGENT_MODEL").unwrap_or_else(|_| "unknown".into()),
            skill: ctrl.skill.clone(),
            skill_sha256,
            control_sha256,
        },
        registry_git_sha: prepare.registry_git_sha.clone(),
        scope_layout: prepare.scope_layout,
        resolved_scope: prepare.resolved_scope.clone(),
        prior_run,
        artifacts: Vec::new(),
        by_system: Vec::new(),
        status: RunOutcome::Failed,
        failure_reason: Some(reason.to_string()),
        draft_risks: Vec::new(),
        draft_issues: Vec::new(),
        external_links: Vec::new(),
        period_id: prepare.period_id.clone(),
    };

    let bytes = serde_json::to_vec(&manifest)?;
    atomic_write(&run_dir.join(MANIFEST_FILE), &bytes)?;
    update_state(reg, &manifest)?;

    let pending = run_dir.join(PENDING_SENTINEL);
    if pending.exists() {
        fs::remove_file(&pending)?;
    }
    Ok(manifest)
}

/// Hash every artifact, link the manifest to the prior run, atomically
/// write `manifest.json`, update `state.json`, and remove the pending
/// sentinel. Returns the sealed manifest.
pub fn finalize(reg: &LoadedRegistry, run_dir: &Path) -> Result<Manifest> {
    let _lock = RootLock::acquire(&reg.root).context("acquire root lock")?;

    let prepare: PrepareContext = read_json(&run_dir.join(PREPARE_FILE))?;
    let result: RunResult = read_json(&run_dir.join(RESULT_FILE))?;
    if prepare.control_id != result.control_id || prepare.run_id != result.run_id {
        bail!(
            "result.json mismatches prepare.json (prepare={}/{}, result={}/{})",
            prepare.control_id,
            prepare.run_id,
            result.control_id,
            result.run_id,
        );
    }

    // Skip the per-run metadata files when hashing.
    let exclude = [PREPARE_FILE, RESULT_FILE, MANIFEST_FILE, PENDING_SENTINEL];
    let hashed = hasher::hash_tree(run_dir, &exclude)?;

    let mut artifacts: Vec<Artifact> = Vec::new();
    let mut by_system_artifacts: BTreeMap<String, Vec<Artifact>> = BTreeMap::new();

    for h in &hashed {
        let art = Artifact {
            path: h.path.clone(),
            sha256: h.sha256.clone(),
            bytes: h.bytes,
        };
        if let Some(sys) = h.path.strip_prefix("by-system/") {
            // by-system/<name>/raw/<file>...
            let name = sys.split('/').next().unwrap_or("").to_string();
            by_system_artifacts.entry(name).or_default().push(art);
        } else {
            artifacts.push(art);
        }
    }

    let by_system_blocks: Vec<BySystemBlock> = result
        .by_system
        .iter()
        .map(|sr| BySystemBlock {
            name: sr.name.clone(),
            status: sr.status,
            summary: None,
            artifacts: by_system_artifacts.remove(&sr.name).unwrap_or_default(),
        })
        .collect();

    // Anything captured under by-system that wasn't reflected in result.json
    // becomes its own block too — we never silently drop hashed artifacts.
    let mut extra_blocks: Vec<BySystemBlock> = by_system_artifacts
        .into_iter()
        .map(|(name, arts)| BySystemBlock {
            name,
            status: SystemOutcome::Complete,
            summary: None,
            artifacts: arts,
        })
        .collect();
    let mut by_system_blocks = by_system_blocks;
    by_system_blocks.append(&mut extra_blocks);
    by_system_blocks.sort_by(|a, b| a.name.cmp(&b.name));

    // Find the prior run for this control and link via its manifest sha.
    let prior_run = prior_run_link(&reg.root, &prepare.control_id, &prepare.run_id)?;

    let control_sha256 = sha256_for_control(&reg.root, &prepare.control_id)?;
    let skill_sha256 = sha256_for_skill(&reg.root, &reg.controls[&prepare.control_id].skill)?;

    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        control_id: prepare.control_id.clone(),
        run_id: prepare.run_id.clone(),
        started_at: prepare.started_at,
        completed_at: Utc::now(),
        operator: prepare.operator.clone(),
        agent: AgentInfo {
            model: std::env::var("SECUNIT_AGENT_MODEL").unwrap_or_else(|_| "unknown".into()),
            skill: reg.controls[&prepare.control_id].skill.clone(),
            skill_sha256,
            control_sha256,
        },
        registry_git_sha: prepare.registry_git_sha.clone(),
        scope_layout: prepare.scope_layout,
        resolved_scope: prepare.resolved_scope.clone(),
        prior_run,
        artifacts,
        by_system: by_system_blocks,
        status: result.status,
        failure_reason: None,
        draft_risks: result.draft_risks.clone(),
        draft_issues: result.draft_issues.clone(),
        external_links: result.external_links.clone(),
        period_id: prepare.period_id.clone(),
    };

    // Compact canonical JSON (no pretty-printing) so `jq`-style
    // reformatting cannot silently break the chain hash. Operators who
    // want to read a manifest pipe it through `jq .` on demand.
    let bytes = serde_json::to_vec(&manifest)?;
    atomic_write(&run_dir.join(MANIFEST_FILE), &bytes)?;

    update_state(reg, &manifest)?;

    let pending = run_dir.join(PENDING_SENTINEL);
    if pending.exists() {
        fs::remove_file(&pending)?;
    }
    Ok(manifest)
}

/// Pending run pointer: control id, run id, and the run dir.
#[derive(Debug, Clone)]
pub struct PendingRun {
    pub control_id: String,
    pub run_id: String,
    pub run_dir: PathBuf,
}

/// Walk `<root>/evidence/` looking for `.run-pending` sentinels. A
/// sentinel sitting next to a sealed `manifest.json` is treated as
/// crash-recovery debris from a finalize that died after the manifest
/// landed but before sentinel removal: it's silently swept rather than
/// surfaced as a pending run (which would block fresh prepares for that
/// control).
pub fn list_pending(root: &Path) -> Result<Vec<PendingRun>> {
    let mut out = Vec::new();
    let evidence = root.join("evidence");
    if !evidence.exists() {
        return Ok(out);
    }
    // Layout depth: evidence/<y>/<q>/<cid>/<rid>/.run-pending = 5 below evidence.
    for entry in walkdir::WalkDir::new(&evidence).max_depth(5) {
        let entry = entry?;
        if entry.file_name() == PENDING_SENTINEL {
            let run_dir = entry.path().parent().unwrap().to_path_buf();
            // Crash recovery: sealed manifest + stale sentinel → sweep.
            if run_dir.join(MANIFEST_FILE).exists() {
                let _ = fs::remove_file(entry.path());
                continue;
            }
            let prepare_path = run_dir.join(PREPARE_FILE);
            if prepare_path.exists() {
                let prepare: PrepareContext = read_json(&prepare_path)?;
                out.push(PendingRun {
                    control_id: prepare.control_id,
                    run_id: prepare.run_id,
                    run_dir,
                });
            }
        }
    }
    Ok(out)
}

// ---------- helpers --------------------------------------------------------

fn allocate_run_dir(root: &Path, control_id: &str, today: NaiveDate) -> Result<PathBuf> {
    let q = quarter_label(today);
    let base = root
        .join("evidence")
        .join(today.year().to_string())
        .join(&q)
        .join(control_id);
    fs::create_dir_all(&base)?;
    let mut n = 1u32;
    loop {
        let id = format!("{}-run-{:03}", today, n);
        let candidate = base.join(&id);
        if !candidate.exists() {
            fs::create_dir_all(&candidate)?;
            return Ok(candidate);
        }
        n += 1;
        if n > 999 {
            bail!("run-id counter overflowed for {control_id} on {today}");
        }
    }
}

fn quarter_label(d: NaiveDate) -> String {
    let q = (d.month() - 1) / 3 + 1;
    format!("q{q}")
}

fn run_id_from_dir(dir: &Path) -> Result<String> {
    Ok(dir
        .file_name()
        .ok_or_else(|| anyhow!("run dir has no basename"))?
        .to_string_lossy()
        .into_owned())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn prior_run_link(root: &Path, control_id: &str, current_run_id: &str) -> Result<Option<PriorRun>> {
    let evidence = root.join("evidence");
    if !evidence.exists() {
        return Ok(None);
    }
    let mut all_manifests: Vec<(String, PathBuf)> = Vec::new();
    for entry in walkdir::WalkDir::new(&evidence) {
        let entry = entry?;
        if entry.file_name() != MANIFEST_FILE {
            continue;
        }
        let dir = entry.path().parent().unwrap();
        let id = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        // Restrict to manifests for the same control id.
        let parent_control = dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if parent_control != control_id {
            continue;
        }
        all_manifests.push((id, entry.path().to_path_buf()));
    }
    all_manifests.sort_by(|a, b| a.0.cmp(&b.0));
    let prior = all_manifests
        .into_iter()
        .rfind(|(id, _)| id.as_str() < current_run_id);
    match prior {
        None => Ok(None),
        Some((id, path)) => {
            let sha = sha256_file(&path)?;
            Ok(Some(PriorRun {
                run_id: id,
                manifest_sha256: sha,
            }))
        }
    }
}

fn sha256_for_control(root: &Path, control_id: &str) -> Result<String> {
    let path = root.join("controls").join(format!("{control_id}.yaml"));
    sha256_file(&path).with_context(|| format!("hash control {}", path.display()))
}

fn sha256_for_skill(root: &Path, skill: &str) -> Result<String> {
    let path = root.join("skills").join(format!("{skill}.md"));
    sha256_file(&path).with_context(|| format!("hash skill {}", path.display()))
}

/// Resolve the registry repo's HEAD commit hex via gix. Errors when
/// `root` isn't a git repo or HEAD can't be peeled (no commits yet,
/// detached weirdness, etc) — `prepare` requires a real git sha to
/// pin what the registry said at run time.
fn git_head(root: &Path) -> Result<String> {
    let repo = gix::open(root).context("open repo")?;
    let head = repo.head().context("read HEAD")?;
    let id = head.into_peeled_id().context("peel HEAD to a commit")?;
    Ok(id.to_hex().to_string())
}

fn update_state(reg: &LoadedRegistry, manifest: &Manifest) -> Result<()> {
    let path = reg.root.join(STATE_FILE);
    // Bail loudly on a corrupt state file rather than silently dropping
    // every prior control's entry by replacing with default. Operators
    // who genuinely want to reset state.json can remove it; finalize
    // will then start fresh.
    let mut state: crate::model::State = if path.exists() {
        let bytes = fs::read(&path)?;
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "{} is corrupt; refusing to overwrite — remove it manually to reset state",
                path.display()
            )
        })?
    } else {
        crate::model::State::default()
    };

    // Compute the next firing date as part of finalize so state.json is
    // a useful cache for `secunit due` and downstream report skills,
    // rather than a placeholder readers have to re-derive on every load.
    // Anchor at "the day after the run's target date" (parsed from the
    // run-id's YYYY-MM-DD prefix, not wall-clock completion time) so a
    // weekly control with target Monday returns *next* Monday, not today.
    let run_date = manifest
        .run_id
        .get(0..10)
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let next_due = run_date.and_then(|d| {
        let lookup_from = d + chrono::Duration::days(1);
        reg.controls.get(&manifest.control_id).and_then(|ctrl| {
            crate::registry::resolver::next_due(
                ctrl,
                &reg.schedule,
                None,
                lookup_from,
                reg.config.weekly_default_weekday,
            )
        })
    });

    state.controls.insert(
        manifest.control_id.clone(),
        StateEntry {
            last_run_id: Some(manifest.run_id.clone()),
            last_run_path: Some(manifest_relative_path(&reg.root, manifest)),
            last_run_at: Some(manifest.completed_at),
            last_status: match manifest.status {
                RunOutcome::Complete => RunStatus::Complete,
                RunOutcome::Partial => RunStatus::InProgress,
                RunOutcome::Failed => RunStatus::Failed,
            },
            next_due,
        },
    );
    state.updated_at = Some(Utc::now());
    let bytes = serde_json::to_vec_pretty(&state)?;
    atomic_write(&path, &bytes)?;
    Ok(())
}

fn manifest_relative_path(_root: &Path, manifest: &Manifest) -> String {
    // Keep portability cheap: compute from known shape.
    let q = {
        // Re-derive from the run id (YYYY-MM-DD-run-NNN).
        let date = manifest
            .run_id
            .get(0..10)
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        date.map(quarter_label).unwrap_or_else(|| "q0".into())
    };
    let year = manifest.run_id.get(0..4).unwrap_or("0000");
    format!(
        "evidence/{year}/{q}/{cid}/{rid}/",
        cid = manifest.control_id,
        rid = manifest.run_id,
    )
}

// quiet unused-import lint when sha256_bytes only appears in cfg(test).
#[allow(dead_code)]
fn _silence_unused() -> String {
    sha256_bytes(b"")
}
