//! Registry-backed read commands. Every entry point loads or reads from
//! the cached `LoadedProject` in `AppState`; the cache is populated by
//! `load_project` and rebuilt by the watcher on disk events (JOB-04).

use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};
use secunit_core::evidence::manifest::{AbortRecord, Manifest, PrepareContext};
use secunit_core::model::{Cadence, Control, RunStatus};
use secunit_core::registry::{self, loader, resolver};
use tauri::{AppHandle, State};

use crate::api::types::*;
use crate::projects;
use crate::state::{AppState, Diagnostic, LoadedProject};
use crate::watcher::{self, TauriSink};

fn cadence_str(c: Cadence) -> String {
    match c {
        Cadence::Continuous => "continuous",
        Cadence::Weekly => "weekly",
        Cadence::Monthly => "monthly",
        Cadence::Quarterly => "quarterly",
        Cadence::SemiAnnual => "semi-annual",
        Cadence::Annual => "annual",
        Cadence::Scheduled => "scheduled",
    }
    .into()
}

fn require_loaded<'a>(
    project: &'a std::sync::MutexGuard<'a, Option<LoadedProject>>,
) -> Result<&'a LoadedProject, String> {
    project
        .as_ref()
        .ok_or_else(|| "no project loaded — call load_project first".to_string())
}

fn project_root_for(name: &str) -> Result<(String, PathBuf), String> {
    let cfg_path = projects::projects_yaml_path().map_err(|e| e.to_string())?;
    let cfg = projects::load_config(&cfg_path).map_err(|e| e.to_string())?;
    let entry = cfg
        .projects
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("unknown project `{name}`"))?;
    Ok((entry.name.clone(), entry.resolved_path()))
}

#[tauri::command]
pub fn load_project(
    name: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<LoadSummary, String> {
    let (project_name, root) = project_root_for(&name)?;
    if !root.exists() {
        return Err(format!("project root does not exist: {}", root.display()));
    }

    let (registry, report) = loader::load(&root);
    let errors: Vec<String> = report
        .errors
        .iter()
        .map(|d| format!("{}: {}", d.path.display(), d.message))
        .collect();
    let warnings: Vec<String> = report
        .warnings
        .iter()
        .map(|d| format!("{}: {}", d.path.display(), d.message))
        .collect();

    let summary = LoadSummary {
        name: project_name.clone(),
        root: root.display().to_string(),
        controls_count: registry.controls.len(),
        inventory_count: registry.inventory.iter().count(),
        has_state: !registry.state.controls.is_empty()
            || registry.state.updated_at.is_some(),
        has_config: registry.config.org.is_some()
            || !registry.config.owners.is_empty()
            || !registry.config.integrations.is_empty(),
        errors,
        warnings,
    };

    let diagnostics: Vec<Diagnostic> = report
        .errors
        .into_iter()
        .map(|d| Diagnostic {
            level: "error",
            path: d.path.display().to_string(),
            message: d.message,
        })
        .chain(report.warnings.into_iter().map(|d| Diagnostic {
            level: "warning",
            path: d.path.display().to_string(),
            message: d.message,
        }))
        .collect();

    {
        let mut slot = state.project.lock().expect("AppState.project poisoned");
        *slot = Some(LoadedProject {
            name: project_name,
            root: root.clone(),
            registry,
            diagnostics,
        });
    }

    // Swap the watcher: drop the previous one (which stops the thread)
    // before starting a new one against the new root. Single-instance
    // contract preserved.
    let debounce = std::env::var("SECUNIT_GUI_WATCH_DEBOUNCE_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let new_handle = watcher::start(&root, TauriSink { handle: app }, debounce)
        .map_err(|e| format!("start watcher: {e}"))?;
    {
        let mut slot = state.watcher.lock().expect("AppState.watcher poisoned");
        *slot = Some(new_handle);
    }

    Ok(summary)
}

fn today_or(today: Option<NaiveDate>) -> NaiveDate {
    today.unwrap_or_else(|| Local::now().date_naive())
}

#[tauri::command]
pub fn list_controls(
    today: Option<NaiveDate>,
    state: State<'_, AppState>,
) -> Result<Vec<ControlSummary>, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;
    let today = today_or(today);
    let mut rows: Vec<ControlSummary> = project
        .registry
        .controls
        .values()
        .map(|c| summarise_control(&project.registry, c, today))
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

fn summarise_control(
    reg: &secunit_core::model::LoadedRegistry,
    c: &Control,
    today: NaiveDate,
) -> ControlSummary {
    let state_entry = reg.state.controls.get(&c.id);
    let next_due = resolver::next_due(
        c,
        &reg.schedule,
        state_entry,
        today,
        reg.config.weekly_default_weekday,
    );
    let overdue = next_due
        .map(|d| resolver::is_overdue(c, d, today))
        .unwrap_or(false);
    let status = derive_status(c, state_entry, next_due, overdue, today);

    ControlSummary {
        id: c.id.clone(),
        title: c.title.clone(),
        cadence: cadence_str(c.cadence),
        owner: c.owner.clone(),
        status,
        next_due,
        overdue,
        last_run_id: state_entry.and_then(|s| s.last_run_id.clone()),
        last_run_at: state_entry.and_then(|s| s.last_run_at),
        last_status: state_entry.map(|s| match s.last_status {
            RunStatus::Complete => "complete".into(),
            RunStatus::InProgress => "in-progress".into(),
            RunStatus::Aborted => "aborted".into(),
            RunStatus::Failed => "failed".into(),
            RunStatus::NeverRun => "never-run".into(),
        }),
    }
}

fn derive_status(
    c: &Control,
    state: Option<&secunit_core::model::StateEntry>,
    next_due: Option<NaiveDate>,
    overdue: bool,
    today: NaiveDate,
) -> ControlStatus {
    if overdue {
        return ControlStatus::Overdue;
    }
    match state {
        Some(s) => match s.last_status {
            RunStatus::InProgress => ControlStatus::InProgress,
            RunStatus::Aborted | RunStatus::Failed => ControlStatus::Aborted,
            RunStatus::NeverRun => ControlStatus::NeverRun,
            RunStatus::Complete => match next_due {
                Some(d) if d <= today + chrono::Duration::days(7) => ControlStatus::DueSoon,
                Some(_) => ControlStatus::Sealed,
                None => match c.cadence {
                    Cadence::Continuous => ControlStatus::Idle,
                    _ => ControlStatus::Sealed,
                },
            },
        },
        None => ControlStatus::NeverRun,
    }
}

#[tauri::command]
pub fn get_control(
    id: String,
    today: Option<NaiveDate>,
    state: State<'_, AppState>,
) -> Result<ControlDetail, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;
    let today = today_or(today);
    let control = project
        .registry
        .controls
        .get(&id)
        .ok_or_else(|| format!("unknown control `{id}`"))?;
    let summary = summarise_control(&project.registry, control, today);

    let resolved = resolver::resolve_scope(control, &project.registry.inventory, today);
    let resolved_scope_today = resolved
        .into_iter()
        .map(|r| ResolvedSystemView {
            name: r.name,
            kind: r.kind,
            tags: r.tags,
        })
        .collect();

    let recent_runs = walk_runs(&project.root)
        .into_iter()
        .filter(|r| r.control_id == id)
        .take(10)
        .collect();

    Ok(ControlDetail {
        summary,
        policy: control.policy.clone(),
        nist: control.nist.clone(),
        skill: control.skill.clone(),
        references: control
            .references
            .iter()
            .map(|r| ReferenceView {
                title: r.title.clone(),
                path: r.path.clone(),
                url: r.url.clone(),
            })
            .collect(),
        recent_runs,
        resolved_scope_today,
    })
}

#[tauri::command]
pub fn due_rows(
    today: Option<NaiveDate>,
    state: State<'_, AppState>,
) -> Result<Vec<DueRowView>, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;
    let today = today_or(today);
    let rows = registry::resolver::due_rows(&project.registry, today);
    Ok(rows
        .into_iter()
        .map(|r| DueRowView {
            control_id: r.control_id,
            cadence: cadence_str(r.cadence),
            next_due: r.next_due,
            overdue: r.overdue,
        })
        .collect())
}

#[tauri::command]
pub fn get_inventory(
    today: Option<NaiveDate>,
    state: State<'_, AppState>,
) -> Result<InventoryView, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;
    let today = today_or(today);
    let mut kinds = Vec::new();
    for (kind, entries) in &project.registry.inventory.kinds {
        let view_entries = entries
            .iter()
            .map(|e| InventoryEntryView {
                name: e.name.clone(),
                tags: e.tags.clone(),
                in_scope_since: e.in_scope_since,
                retired_on: e.retired_on,
                aliases: e.aliases.clone(),
                active_today: e.is_active_on(today),
                extras: e.extras.clone(),
            })
            .collect();
        kinds.push(InventoryKindView {
            kind: kind.clone(),
            entries: view_entries,
        });
    }
    Ok(InventoryView { kinds })
}

#[tauri::command]
pub fn list_runs(
    control_id: Option<String>,
    quarter: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<RunRow>, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;
    let mut rows = walk_runs(&project.root);
    if let Some(cid) = control_id {
        rows.retain(|r| r.control_id == cid);
    }
    if let Some(q) = quarter {
        rows.retain(|r| format!("{:04}-q{}", r.year, r.quarter) == q);
    }
    Ok(rows)
}

#[tauri::command]
pub fn recent_runs(
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<RunRow>, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;
    let mut rows = walk_runs(&project.root);
    rows.truncate(limit);
    Ok(rows)
}

#[tauri::command]
pub fn get_run(
    control_id: String,
    run_id: String,
    state: State<'_, AppState>,
) -> Result<RunDetail, String> {
    let project = state.project.lock().expect("AppState.project poisoned");
    let project = require_loaded(&project)?;

    let mut row = walk_runs(&project.root)
        .into_iter()
        .find(|r| r.control_id == control_id && r.run_id == run_id)
        .ok_or_else(|| format!("run not found: {control_id}/{run_id}"))?;

    // Re-canonicalise run_dir so any read goes through paths we own.
    let run_dir = PathBuf::from(&row.run_dir);
    let canonical = run_dir
        .canonicalize()
        .map_err(|e| format!("canonicalise {}: {e}", run_dir.display()))?;
    let project_canonical = project
        .root
        .canonicalize()
        .map_err(|e| format!("canonicalise {}: {e}", project.root.display()))?;
    if !canonical.starts_with(&project_canonical) {
        return Err("run_dir escapes project root".into());
    }
    row.run_dir = canonical.display().to_string();

    let manifest = read_json_if_present(&canonical.join("manifest.json"))?
        .map(|m: Manifest| serde_json::to_value(m).expect("manifest serialisable"));
    let prepare = read_json_if_present(&canonical.join("prepare.json"))?
        .map(|p: PrepareContext| serde_json::to_value(p).expect("prepare serialisable"));
    let abort = read_json_if_present(&canonical.join("abort.json"))?
        .map(|a: AbortRecord| serde_json::to_value(a).expect("abort serialisable"));

    let tree = build_run_tree(&canonical, &canonical)?;

    Ok(RunDetail {
        row,
        manifest,
        prepare,
        abort,
        tree,
    })
}

fn read_json_if_present<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let v = serde_json::from_str(&text)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(v))
}

fn build_run_tree(_root: &Path, dir: &Path) -> Result<Vec<RunTreeNode>, String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let mut out = Vec::new();
    for e in entries {
        let path = e.path();
        let metadata = e
            .metadata()
            .map_err(|err| format!("stat {}: {err}", path.display()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        if metadata.is_dir() {
            out.push(RunTreeNode {
                name,
                path: path.display().to_string(),
                kind: RunTreeKind::Dir,
                size: None,
                children: build_run_tree(_root, &path)?,
            });
        } else {
            out.push(RunTreeNode {
                name,
                path: path.display().to_string(),
                kind: RunTreeKind::File,
                size: Some(metadata.len()),
                children: Vec::new(),
            });
        }
    }
    Ok(out)
}

/// Walk `<root>/evidence/<y>/<q>/<control>/<run>/` and produce one row
/// per run directory we find. Sealed / aborted / pending derived from
/// the presence of `manifest.json`, `abort.json`, `.run-pending`.
fn walk_runs(root: &Path) -> Vec<RunRow> {
    let evidence = root.join("evidence");
    if !evidence.exists() {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let years = match std::fs::read_dir(&evidence) {
        Ok(it) => it,
        Err(_) => return Vec::new(),
    };
    for y in years.flatten() {
        let year_path = y.path();
        if !year_path.is_dir() {
            continue;
        }
        let year: i32 = match y.file_name().to_string_lossy().parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        for q in std::fs::read_dir(&year_path).into_iter().flatten().flatten() {
            let q_path = q.path();
            if !q_path.is_dir() {
                continue;
            }
            let q_name = q.file_name().to_string_lossy().into_owned();
            let quarter: u32 = match q_name.strip_prefix('q').and_then(|s| s.parse().ok()) {
                Some(n) => n,
                None => continue,
            };
            for ctrl in std::fs::read_dir(&q_path).into_iter().flatten().flatten() {
                let ctrl_path = ctrl.path();
                if !ctrl_path.is_dir() {
                    continue;
                }
                let control_id = ctrl.file_name().to_string_lossy().into_owned();
                for run in std::fs::read_dir(&ctrl_path).into_iter().flatten().flatten() {
                    let run_path = run.path();
                    if !run_path.is_dir() {
                        continue;
                    }
                    let run_id = run.file_name().to_string_lossy().into_owned();
                    rows.push(row_from_run_dir(
                        &control_id,
                        &run_id,
                        year,
                        quarter,
                        &run_path,
                    ));
                }
            }
        }
    }
    rows.sort_by(|a, b| match (a.completed_at, b.completed_at) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.run_id.cmp(&a.run_id),
    });
    rows
}

fn row_from_run_dir(
    control_id: &str,
    run_id: &str,
    year: i32,
    quarter: u32,
    run_dir: &Path,
) -> RunRow {
    let manifest_path = run_dir.join("manifest.json");
    let abort_path = run_dir.join("abort.json");
    let pending_path = run_dir.join(".run-pending");

    let (state, started_at, completed_at, manifest_sha) =
        if let Some(m) = read_manifest(&manifest_path) {
            let sha = sha256_of_file(&manifest_path);
            (RunState::Sealed, Some(m.started_at), Some(m.completed_at), sha)
        } else if let Some(a) = read_abort(&abort_path) {
            (RunState::Aborted, Some(a.aborted_at), Some(a.aborted_at), None)
        } else if pending_path.exists() {
            let started = read_prepare(&run_dir.join("prepare.json"))
                .map(|p| p.started_at);
            (RunState::Pending, started, None, None)
        } else {
            (RunState::Pending, None, None, None)
        };

    RunRow {
        control_id: control_id.into(),
        run_id: run_id.into(),
        run_dir: run_dir.display().to_string(),
        state,
        started_at,
        completed_at,
        manifest_sha256: manifest_sha,
        year,
        quarter,
    }
}

fn read_manifest(path: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_prepare(path: &Path) -> Option<PrepareContext> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn read_abort(path: &Path) -> Option<AbortRecord> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn sha256_of_file(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Some(hex::encode(h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("testdata/orgs/multi-system")
    }

    #[test]
    fn walk_runs_handles_missing_evidence_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(walk_runs(dir.path()).is_empty());
    }

    #[test]
    fn walk_runs_finds_a_pending_run() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir
            .path()
            .join("evidence/2026/q2/sca-weekly-dependency-scan/2026-05-04-run-001");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join(".run-pending"), "").unwrap();
        let prepare = serde_json::json!({
            "schema_version": 1,
            "control_id": "sca-weekly-dependency-scan",
            "run_id": "2026-05-04-run-001",
            "run_dir": run.to_str().unwrap(),
            "started_at": Utc::now(),
            "scope_layout": "by-system",
            "resolved_scope": [],
            "registry_git_sha": "deadbeef",
        });
        std::fs::write(run.join("prepare.json"), prepare.to_string()).unwrap();
        let rows = walk_runs(dir.path());
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].state, RunState::Pending));
        assert_eq!(rows[0].control_id, "sca-weekly-dependency-scan");
        assert_eq!(rows[0].year, 2026);
        assert_eq!(rows[0].quarter, 2);
    }

    #[test]
    fn list_controls_against_fixture() {
        let root = fixture_root();
        if !root.exists() {
            eprintln!("skipping: {} not present", root.display());
            return;
        }
        let (registry, _report) = loader::load(&root);
        // Sanity check the fixture loaded.
        assert!(!registry.controls.is_empty(), "fixture has no controls");

        let today = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        for c in registry.controls.values() {
            let s = summarise_control(&registry, c, today);
            assert_eq!(s.id, c.id);
        }
    }

    #[test]
    fn cadence_str_round_trip() {
        for (c, expect) in [
            (Cadence::Continuous, "continuous"),
            (Cadence::Weekly, "weekly"),
            (Cadence::Monthly, "monthly"),
            (Cadence::Quarterly, "quarterly"),
            (Cadence::SemiAnnual, "semi-annual"),
            (Cadence::Annual, "annual"),
            (Cadence::Scheduled, "scheduled"),
        ] {
            assert_eq!(cadence_str(c), expect);
        }
    }

    #[test]
    fn build_run_tree_is_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("z.txt"), "z").unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/m.txt"), "m").unwrap();
        let tree = build_run_tree(dir.path(), dir.path()).unwrap();
        let names: Vec<_> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "sub", "z.txt"]);
    }

}
