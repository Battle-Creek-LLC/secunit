//! Read-only risk register commands. The register lives at
//! `risks/<id>/events.jsonl` (append-only, hash-chained source of truth)
//! with a derived `risks/index.json` cache. These commands fold the logs
//! in memory and render — they never write, matching the read-only viewer
//! contract (`docs/risks.md` §Read-only viewer).
//!
//! `list_risks` projects `secunit_core::risks::build_index`; `get_risk`
//! loads + folds one risk's log for the detail view and recomputes each
//! bound manifest's sha for the verified ✓/✗ badge. Hashing a file to
//! verify it is still read-only.

use std::path::{Path, PathBuf};

use secunit_core::risks::{self, FindingRef};
use tauri::State;

use crate::api::types::*;
use crate::state::{AppState, LoadedProject};

fn require_loaded<'a>(
    project: &'a std::sync::MutexGuard<'a, Option<LoadedProject>>,
) -> Result<&'a LoadedProject, String> {
    project
        .as_ref()
        .ok_or_else(|| "no project loaded — call load_project first".to_string())
}

fn external_views(external: &[risks::store::IndexExternal]) -> Vec<RiskExternalView> {
    external
        .iter()
        .map(|e| RiskExternalView {
            system: e.system.clone(),
            id: e.id.clone(),
            url: e.url.clone(),
        })
        .collect()
}

/// The register table. Built fresh from the logs each call (cheap; one
/// fold per risk) so the index is never trusted blindly and a stale or
/// missing `index.json` self-heals in the view.
#[tauri::command]
pub fn list_risks(state: State<'_, AppState>) -> Result<Vec<RiskRow>, String> {
    let root = {
        let project = state.project.lock().expect("AppState.project poisoned");
        let project = require_loaded(&project)?;
        project.root.clone()
    };

    // Lenient: a broken log must not blank the whole register table —
    // readable risks still render; the broken ones are logged.
    let (index, register_errors) =
        risks::build_index_lenient(&root).map_err(|e| format!("build risk index: {e:#}"))?;
    for (id, error) in &register_errors {
        tracing::warn!(risk_id = %id, %error, "risk log unreadable; register view is incomplete");
    }

    let mut rows: Vec<RiskRow> = index
        .risks
        .into_iter()
        .map(|(id, entry)| RiskRow {
            id,
            title: entry.title,
            fingerprint: entry.fingerprint,
            severity: severity_str(entry.severity).to_string(),
            status: entry.status.as_str().to_string(),
            owner: entry.owner,
            due_at: entry.due_at,
            source_control: entry.source_control,
            first_run_id: entry.first_run_id,
            external: external_views(&entry.external),
            log_head_sha256: entry.log_head_sha256,
        })
        .collect();
    // Stable order: by id, the way the index BTreeMap already keys them.
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

/// Full detail for one risk: the folded current state, the raw event log
/// rendered chronologically as a timeline, and the bound finding refs with
/// their pinned manifest sha recomputed for the verified badge.
#[tauri::command]
pub fn get_risk(id: String, state: State<'_, AppState>) -> Result<RiskDetail, String> {
    let root = {
        let project = state.project.lock().expect("AppState.project poisoned");
        let project = require_loaded(&project)?;
        project.root.clone()
    };

    let events =
        risks::load_events(&root, &id).map_err(|e| format!("load events for {id}: {e:#}"))?;
    let folded = risks::fold(&events);

    let finding_refs = folded
        .finding_refs
        .iter()
        .map(|fr| finding_ref_view(&root, fr))
        .collect();

    let event_views = events
        .iter()
        .map(|ev| {
            // Serialise the payload-only `data` so the webview gets exactly
            // the type-specific fields documented in docs/risks.md.
            let data = serde_json::to_value(&ev.data)
                .map(|v| {
                    // `EventData` is adjacently tagged as {type, data}; pull
                    // the inner `data` object out so the view has just the
                    // payload (and we carry `type` separately).
                    v.get("data").cloned().unwrap_or(serde_json::Value::Null)
                })
                .unwrap_or(serde_json::Value::Null);
            RiskEventView {
                seq: ev.seq,
                ts: ev.ts,
                actor: ev.actor.clone(),
                agent: ev.agent.as_ref().map(|a| RiskAgentView {
                    model: a.model.clone(),
                    skill: a.skill.clone(),
                }),
                event_type: ev.data.type_str().to_string(),
                data,
            }
        })
        .collect();

    Ok(RiskDetail {
        id,
        title: folded.title.clone(),
        severity: severity_str(folded.severity).to_string(),
        status: folded.status.as_str().to_string(),
        impact: folded.impact,
        likelihood: folded.likelihood,
        owner: folded.owner.clone(),
        due_at: folded.due_at,
        sla_days: folded.sla_days,
        affected_systems: folded.affected_systems.clone(),
        source_control: folded.source_control().map(|s| s.to_string()),
        first_run_id: folded.first_run_id().map(|s| s.to_string()),
        fingerprint: folded.fingerprint(),
        resolved_at: folded.resolved_at,
        exception_expires_at: folded.exception_expires_at,
        external: folded
            .external
            .iter()
            .map(|e| RiskExternalView {
                system: e.system.clone(),
                id: e.external_id.clone(),
                url: e.url.clone(),
            })
            .collect(),
        external_status: folded.external_status.clone(),
        finding_refs,
        events: event_views,
    })
}

fn severity_str(s: risks::Severity) -> &'static str {
    use risks::Severity::*;
    match s {
        Critical => "critical",
        High => "high",
        Medium => "medium",
        Low => "low",
        Info => "info",
    }
}

/// Build the detail view of a finding ref, recomputing the pinned manifest
/// sha against disk for the verified ✓/✗ badge. Read-only: we only locate
/// the manifest and hash it.
fn finding_ref_view(root: &Path, fr: &FindingRef) -> RiskFindingRefView {
    let verified = locate_manifest(root, &fr.control_id, &fr.run_id)
        .and_then(|path| sha256_of_file(&path))
        .map(|sha| sha == fr.manifest_sha256);

    RiskFindingRefView {
        control_id: fr.control_id.clone(),
        run_id: fr.run_id.clone(),
        manifest_sha256: fr.manifest_sha256.clone(),
        finding_id: fr.finding_id.clone(),
        body_path: fr.body_path.clone(),
        verified,
    }
}

/// Find `manifest.json` for a `control_id`/`run_id` under `evidence/`. The
/// register binds by content hash, not path, so we walk to the run dir the
/// same way the findings reader does.
fn locate_manifest(root: &Path, control_id: &str, run_id: &str) -> Option<PathBuf> {
    let evidence = root.join("evidence");
    if !evidence.exists() {
        return None;
    }
    for entry in walkdir::WalkDir::new(&evidence)
        .max_depth(6)
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() || entry.file_name() != "manifest.json" {
            continue;
        }
        let s = entry.path().to_string_lossy();
        if s.contains(&format!("/{control_id}/")) && s.contains(&format!("/{run_id}/")) {
            return Some(entry.path().to_path_buf());
        }
    }
    None
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

    #[test]
    fn severity_strings_match_on_disk_spelling() {
        assert_eq!(severity_str(risks::Severity::Critical), "critical");
        assert_eq!(severity_str(risks::Severity::High), "high");
        assert_eq!(severity_str(risks::Severity::Medium), "medium");
        assert_eq!(severity_str(risks::Severity::Low), "low");
        assert_eq!(severity_str(risks::Severity::Info), "info");
    }

    #[test]
    fn locate_manifest_returns_none_without_evidence() {
        let dir = tempfile::tempdir().unwrap();
        assert!(locate_manifest(dir.path(), "ctrl", "run").is_none());
    }

    #[test]
    fn locate_manifest_finds_a_sealed_run() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir
            .path()
            .join("evidence/2026/q2/ra-vuln-audit/2026-05-25-run-001");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(run.join("manifest.json"), "{}").unwrap();
        let found = locate_manifest(dir.path(), "ra-vuln-audit", "2026-05-25-run-001");
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("manifest.json"));
    }

    // The frontend timeline's `salientPairs` reads `event.data.<field>`
    // (e.g. `data.severity`, `data.owner`). This pins the wire shape: the
    // `RiskEventView.data` value must be the *payload* object, not the
    // `{type, data}` envelope.
    #[test]
    fn event_view_data_is_the_inner_payload() {
        use chrono::{NaiveDate, TimeZone, Utc};
        use secunit_core::risks::{self, EventData, FindingRef, Severity};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let finding_ref = FindingRef {
            control_id: "ra-vuln-audit".into(),
            run_id: "2026-05-25-run-001".into(),
            manifest_sha256: "a".repeat(64),
            finding_id: "S032".into(),
            body_path: Some("findings.md#risk-1".into()),
        };
        let opened = risks::open(
            root,
            finding_ref,
            "S032 — pickle RCE",
            Severity::Critical,
            5,
            4,
            vec!["api".into()],
            30,
            NaiveDate::from_ymd_opt(2026, 6, 24).unwrap(),
            "jstockdi",
            None,
            Some(Utc.with_ymd_and_hms(2026, 5, 25, 14, 40, 0).unwrap()),
        )
        .unwrap();
        let id = opened.risk_id;

        risks::append(
            root,
            &id,
            EventData::OwnerAssigned {
                owner: "cto".into(),
            },
            "jstockdi",
            None,
            Some(Utc.with_ymd_and_hms(2026, 5, 25, 14, 41, 0).unwrap()),
        )
        .unwrap();

        let events = risks::load_events(root, &id).unwrap();
        // Re-derive the view payloads the way get_risk does.
        let payloads: Vec<(String, serde_json::Value)> = events
            .iter()
            .map(|ev| {
                let data = serde_json::to_value(&ev.data)
                    .unwrap()
                    .get("data")
                    .cloned()
                    .unwrap();
                (ev.data.type_str().to_string(), data)
            })
            .collect();

        assert_eq!(payloads[0].0, "opened");
        assert_eq!(payloads[0].1["severity"], "critical");
        assert_eq!(payloads[0].1["impact"], 5);
        assert_eq!(payloads[0].1["due_at"], "2026-06-24");
        assert_eq!(payloads[1].0, "owner-assigned");
        assert_eq!(payloads[1].1["owner"], "cto");
    }
}
