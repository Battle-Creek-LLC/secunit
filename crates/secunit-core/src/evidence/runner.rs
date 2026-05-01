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
    AbortRecord, AgentInfo, Artifact, BySystemBlock, Manifest, PrepareContext, PriorRun,
    RunOutcome, RunResult, ScopeLayout, SystemOutcome,
};
use crate::model::{LoadedRegistry, RunStatus, StateEntry};
use crate::registry::resolver;
use crate::SCHEMA_VERSION;

const PENDING_SENTINEL: &str = ".run-pending";
const PREPARE_FILE: &str = "prepare.json";
const RESULT_FILE: &str = "result.json";
const MANIFEST_FILE: &str = "manifest.json";
const ABORT_FILE: &str = "abort.json";
const STATE_FILE: &str = "state.json";

/// Options for `prepare`. Only `today` is required; the rest are
/// agent-supplied metadata.
#[derive(Debug, Clone, Default)]
pub struct PrepareOpts {
    pub today: Option<NaiveDate>,
    pub operator: Option<String>,
    pub note: Option<String>,
    pub now: Option<DateTime<Utc>>,
    /// If `true`, allow `flat` layout when scope resolves to one entry.
    pub allow_flat_when_singleton: bool,
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
    let scope_layout =
        if ctrl.scope.is_none() || (resolved.len() <= 1 && opts.allow_flat_when_singleton) {
            ScopeLayout::Flat
        } else {
            ScopeLayout::BySystem
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

/// Tear down a pending run: write `abort.json`, remove `.run-pending`,
/// keep everything else for audit.
pub fn abort(run_dir: &Path, reason: &str) -> Result<AbortRecord> {
    let prepare_path = run_dir.join(PREPARE_FILE);
    let prepare_bytes =
        fs::read(&prepare_path).with_context(|| format!("read {}", prepare_path.display()))?;
    let prepare: PrepareContext = serde_json::from_slice(&prepare_bytes)?;
    let record = AbortRecord {
        schema_version: SCHEMA_VERSION,
        control_id: prepare.control_id.clone(),
        run_id: prepare.run_id.clone(),
        aborted_at: Utc::now(),
        reason: reason.to_string(),
    };
    let json = serde_json::to_vec_pretty(&record)?;
    atomic_write(&run_dir.join(ABORT_FILE), &json)?;
    let pending = run_dir.join(PENDING_SENTINEL);
    if pending.exists() {
        fs::remove_file(&pending)?;
    }
    Ok(record)
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
    let exclude = [
        PREPARE_FILE,
        RESULT_FILE,
        MANIFEST_FILE,
        ABORT_FILE,
        PENDING_SENTINEL,
    ];
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
        draft_risks: result.draft_risks.clone(),
        draft_issues: result.draft_issues.clone(),
        external_links: result.external_links.clone(),
    };

    // Compact canonical JSON (no pretty-printing) so `jq`-style
    // reformatting cannot silently break the chain hash. Operators who
    // want to read a manifest pipe it through `jq .` on demand.
    let bytes = serde_json::to_vec(&manifest)?;
    atomic_write(&run_dir.join(MANIFEST_FILE), &bytes)?;

    update_state(&reg.root, &manifest)?;

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

fn update_state(root: &Path, manifest: &Manifest) -> Result<()> {
    let path = root.join(STATE_FILE);
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
    state.controls.insert(
        manifest.control_id.clone(),
        StateEntry {
            last_run_id: Some(manifest.run_id.clone()),
            last_run_path: Some(manifest_relative_path(root, manifest)),
            last_run_at: Some(manifest.completed_at),
            last_status: match manifest.status {
                RunOutcome::Complete => RunStatus::Complete,
                RunOutcome::Partial => RunStatus::InProgress,
                RunOutcome::Failed => RunStatus::Failed,
            },
            next_due: None,
        },
    );
    state.updated_at = Some(Utc::now());
    let bytes = serde_json::to_vec_pretty(&state)?;
    let tmp_dest = path;
    atomic_write(&tmp_dest, &bytes)?;
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
