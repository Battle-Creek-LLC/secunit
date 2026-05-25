//! Walk every run for a control (or all controls) in chronological order,
//! recompute artifact hashes, and check each `prior_run.manifest_sha256`
//! against the recomputed sha of the prior manifest.
//!
//! This is the single point of integrity for an assessor; the test
//! surface here matters more than perf.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{Datelike, NaiveDate};

use super::hasher::{hash_tree, sha256_file};
use super::manifest::Manifest;
use crate::risks::fold;
use crate::risks::model::FindingRef;
use crate::risks::store;

const MANIFEST_FILE: &str = "manifest.json";
const PREPARE_FILE: &str = "prepare.json";
const RESULT_FILE: &str = "result.json";
const PENDING_SENTINEL: &str = ".run-pending";
const RISKS_DIR: &str = "risks";
const EVENTS_FILE: &str = "events.jsonl";

/// One verified run.
#[derive(Debug, Clone)]
pub struct VerifiedRun {
    pub control_id: String,
    pub run_id: String,
    pub run_dir: PathBuf,
}

/// Aggregate report over a verification pass.
#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub verified: Vec<VerifiedRun>,
    pub failures: Vec<VerifyFailure>,
    /// Risk logs whose chain and finding refs all verified.
    pub verified_risks: Vec<VerifiedRisk>,
    /// Risk logs that failed verification (broken chain or unresolvable
    /// finding ref).
    pub risk_failures: Vec<RiskFailure>,
}

/// One verified risk log: its chain walked clean and every `finding_ref`
/// resolved to a sealed manifest whose recomputed sha matched.
#[derive(Debug, Clone)]
pub struct VerifiedRisk {
    pub risk_id: String,
    /// Number of `finding_ref`s resolved and hash-checked.
    pub finding_refs: usize,
}

/// A risk log that failed verification.
#[derive(Debug, Clone)]
pub struct RiskFailure {
    pub risk_id: String,
    pub kind: RiskFailureKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskFailureKind {
    /// The `events.jsonl` `prev_sha256` chain is broken, a `seq` is
    /// non-monotonic, the leading event is not `opened`, or a line failed to
    /// parse — i.e. the log was edited or tampered with.
    BrokenChain,
    /// A `finding_ref` did not resolve to a sealed manifest whose recomputed
    /// sha matched (manifest absent, mutated, or control/run id mismatch).
    BadFindingRef,
}

#[derive(Debug, Clone)]
pub struct VerifyFailure {
    pub control_id: String,
    pub run_id: String,
    pub run_dir: PathBuf,
    pub kind: FailureKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureKind {
    /// Manifest could not be parsed.
    BadManifest,
    /// One or more artifact hashes did not match the manifest.
    ArtifactMismatch,
    /// An artifact under the run dir, or the manifest file itself, could
    /// not be read (broken symlink, permission denied, vanished mid-walk,
    /// disk error). Distinct from ArtifactMismatch so an operator chases
    /// the I/O problem, not a tampering false alarm.
    Unreadable,
    /// `prior_run.manifest_sha256` did not match the recomputed sha of
    /// the immediately-preceding sealed manifest for that control.
    BrokenChain,
    /// Manifest claims a prior run but no prior manifest exists in the
    /// evidence tree.
    MissingPrior,
    /// Manifest is missing a prior_run link but a prior manifest exists.
    MissingLink,
}

impl VerifyReport {
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty() && self.risk_failures.is_empty()
    }
}

/// Verify every run for `control_id`, or every run if `None`. Walks
/// runs in chronological order (by `run_id`, which is ISO-date-prefixed).
pub fn verify(root: &Path, control_id: Option<&str>) -> Result<VerifyReport> {
    let mut report = VerifyReport::default();

    // Risk-register verification is independent of which control (if any)
    // the caller scoped evidence verification to; the register binds across
    // controls, so always walk it when present.
    verify_risks(root, &mut report);

    let evidence = root.join("evidence");
    if !evidence.exists() {
        return Ok(report);
    }

    // Group manifests by control id.
    let mut grouped: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    for entry in walkdir::WalkDir::new(&evidence) {
        let entry = entry?;
        if entry.file_name() != MANIFEST_FILE {
            continue;
        }
        let dir = entry.path().parent().unwrap().to_path_buf();
        let run_id = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let cid = dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if let Some(want) = control_id {
            if cid != want {
                continue;
            }
        }
        grouped
            .entry(cid)
            .or_default()
            .push((run_id, entry.path().to_path_buf()));
    }

    for (cid, mut runs) in grouped {
        runs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut prior_sha: Option<String> = None;
        let mut prior_run_id: Option<String> = None;
        for (run_id, manifest_path) in &runs {
            let run_dir = manifest_path.parent().unwrap().to_path_buf();

            // Parse manifest.
            let bytes = match fs::read(manifest_path) {
                Ok(b) => b,
                Err(e) => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::BadManifest,
                        detail: format!("read: {e}"),
                    });
                    continue;
                }
            };
            let manifest: Manifest = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(e) => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::BadManifest,
                        detail: format!("parse: {e}"),
                    });
                    continue;
                }
            };

            // Check artifact hashes match the on-disk files. An I/O error
            // walking the run dir (one chmod-000'd file is enough to
            // trigger this) becomes a per-run Unreadable failure rather
            // than aborting the entire verify pass — otherwise a single
            // unreadable file in run N silently masks every run after it.
            match recompute_and_compare(&run_dir, &manifest) {
                Ok(mismatches) if !mismatches.is_empty() => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::ArtifactMismatch,
                        detail: mismatches.join("; "),
                    });
                }
                Ok(_) => {}
                Err(io_detail) => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::Unreadable,
                        detail: io_detail,
                    });
                }
            }

            // Check chain link.
            match (&manifest.prior_run, &prior_sha, &prior_run_id) {
                (None, None, _) => {}
                (None, Some(sha), Some(pid)) => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::MissingLink,
                        detail: format!(
                            "prior run `{pid}` (sha {sha:.12}…) exists but manifest has no prior_run link"
                        ),
                    });
                }
                (Some(link), None, _) => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::MissingPrior,
                        detail: format!(
                            "manifest claims prior `{}` but no prior manifest exists",
                            link.run_id
                        ),
                    });
                }
                (Some(link), Some(sha), Some(pid))
                    if &link.manifest_sha256 != sha || &link.run_id != pid =>
                {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::BrokenChain,
                        detail: format!(
                            "expected prior {pid} sha {sha}; got {} sha {}",
                            link.run_id, link.manifest_sha256
                        ),
                    });
                }
                _ => {}
            }

            // Also tolerate the manifest file itself becoming unreadable
            // between the directory walk and now (race or transient I/O).
            match sha256_file(manifest_path) {
                Ok(sha) => {
                    prior_sha = Some(sha);
                    prior_run_id = Some(run_id.clone());
                    report.verified.push(VerifiedRun {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir,
                    });
                }
                Err(e) => {
                    report.failures.push(VerifyFailure {
                        control_id: cid.clone(),
                        run_id: run_id.clone(),
                        run_dir: run_dir.clone(),
                        kind: FailureKind::Unreadable,
                        detail: format!("hash manifest: {e}"),
                    });
                    // Don't advance prior_sha — keep checking subsequent
                    // runs against the last-known-good chain anchor.
                }
            }
        }
    }
    Ok(report)
}

/// Returns `Ok(mismatches)` on a successful tree walk (empty Vec means
/// hashes all matched). Returns `Err(io_detail)` when the walk itself
/// failed — caller turns that into a `FailureKind::Unreadable` for this
/// run rather than aborting the whole verify pass.
fn recompute_and_compare(run_dir: &Path, manifest: &Manifest) -> Result<Vec<String>, String> {
    let exclude = [PREPARE_FILE, RESULT_FILE, MANIFEST_FILE, PENDING_SENTINEL];
    let on_disk = hash_tree(run_dir, &exclude).map_err(|e| format!("hash_tree: {e}"))?;
    let mut by_path: BTreeMap<&str, &super::hasher::HashedArtifact> = BTreeMap::new();
    for h in &on_disk {
        by_path.insert(h.path.as_str(), h);
    }

    let mut mismatches: Vec<String> = Vec::new();
    let claimed: Vec<&super::manifest::Artifact> = manifest
        .artifacts
        .iter()
        .chain(manifest.by_system.iter().flat_map(|b| b.artifacts.iter()))
        .collect();

    for art in &claimed {
        match by_path.remove(art.path.as_str()) {
            None => mismatches.push(format!("{}: file missing", art.path)),
            Some(h) => {
                if h.sha256 != art.sha256 || h.bytes != art.bytes {
                    mismatches.push(format!(
                        "{}: hash/size mismatch (manifest={} {}b, disk={} {}b)",
                        art.path, art.sha256, art.bytes, h.sha256, h.bytes
                    ));
                }
            }
        }
    }
    for leftover in by_path.keys() {
        mismatches.push(format!("{leftover}: artifact on disk not in manifest"));
    }
    Ok(mismatches)
}

/// Verify the risk register under `<root>/risks/`: for every
/// `risks/<id>/events.jsonl`, walk its `prev_sha256` chain and resolve every
/// `finding_ref` to a sealed manifest whose recomputed sha matches.
///
/// Pushes results into `report.verified_risks` / `report.risk_failures`. The
/// register is optional: a root with no `risks/` dir simply contributes no
/// risk results.
fn verify_risks(root: &Path, report: &mut VerifyReport) {
    let risks_dir = root.join(RISKS_DIR);
    if !risks_dir.exists() {
        return;
    }

    // Enumerate `risks/<id>/events.jsonl` dirs, in id order for stable output.
    let mut risk_ids: Vec<String> = Vec::new();
    let entries = match fs::read_dir(&risks_dir) {
        Ok(e) => e,
        Err(e) => {
            report.risk_failures.push(RiskFailure {
                risk_id: RISKS_DIR.to_string(),
                kind: RiskFailureKind::BrokenChain,
                detail: format!("read risks dir: {e}"),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if entry.path().join(EVENTS_FILE).exists() {
            risk_ids.push(name);
        }
    }
    risk_ids.sort();

    for risk_id in risk_ids {
        // load_events validates seq monotonicity, a leading `opened`, AND the
        // prev_sha256 chain — any break (including a single edited line) is an
        // error here, which is exactly the tamper signal we want to surface.
        let events = match store::load_events(root, &risk_id) {
            Ok(events) => events,
            Err(e) => {
                report.risk_failures.push(RiskFailure {
                    risk_id: risk_id.clone(),
                    kind: RiskFailureKind::BrokenChain,
                    detail: format!("{e:#}"),
                });
                continue;
            }
        };

        // Resolve every finding_ref accumulated by the fold (the originating
        // `opened` ref plus every `evidence-linked` / remediated ref).
        let state = fold::fold(&events);
        let mut bad: Option<String> = None;
        for fref in &state.finding_refs {
            let run_dir = run_dir_for(root, fref);
            if let Err(e) = store::verify_finding_ref(&run_dir, fref) {
                bad = Some(format!("{} ({:#})", fref.fingerprint(), e));
                break;
            }
        }
        match bad {
            Some(detail) => report.risk_failures.push(RiskFailure {
                risk_id: risk_id.clone(),
                kind: RiskFailureKind::BadFindingRef,
                detail,
            }),
            None => report.verified_risks.push(VerifiedRisk {
                risk_id: risk_id.clone(),
                finding_refs: state.finding_refs.len(),
            }),
        }
    }
}

/// Locate the sealed run dir a `finding_ref` points at, mirroring the runner's
/// layout: `evidence/<year>/<quarter>/<control_id>/<run_id>/`, with year and
/// quarter derived from the run id's `YYYY-MM-DD` prefix.
fn run_dir_for(root: &Path, fref: &FindingRef) -> PathBuf {
    let date = fref
        .run_id
        .get(0..10)
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let year = fref.run_id.get(0..4).unwrap_or("0000");
    let quarter = date
        .map(|d| format!("q{}", (d.month() - 1) / 3 + 1))
        .unwrap_or_else(|| "q0".to_string());
    root.join("evidence")
        .join(year)
        .join(quarter)
        .join(&fref.control_id)
        .join(&fref.run_id)
}

#[cfg(test)]
mod risk_tests {
    use super::*;
    use crate::risks::model::{FindingRef, Severity};
    use crate::risks::store;
    use std::io::Write;

    const CONTROL: &str = "ra-vuln-audit";
    const RUN_ID: &str = "2026-05-25-run-001";

    /// Seal a minimal but parseable `manifest.json` under the layout
    /// `verify` expects, returning its sha256 so a finding_ref can bind to it.
    fn seal_manifest(root: &Path) -> String {
        let run_dir = root
            .join("evidence")
            .join("2026")
            .join("q2")
            .join(CONTROL)
            .join(RUN_ID);
        fs::create_dir_all(&run_dir).unwrap();
        let manifest = serde_json::json!({
            "schema_version": crate::SCHEMA_VERSION,
            "control_id": CONTROL,
            "run_id": RUN_ID,
            "started_at": "2026-05-25T14:00:00Z",
            "completed_at": "2026-05-25T14:05:00Z",
            "agent": {
                "model": "test", "skill": "ra-vuln-audit",
                "skill_sha256": "0", "control_sha256": "0"
            },
            "registry_git_sha": "deadbeef",
            "scope_layout": "flat",
            "resolved_scope": [],
            "artifacts": [],
            "status": "complete"
        });
        let path = run_dir.join("manifest.json");
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        fs::write(&path, &bytes).unwrap();
        // Sanity: the fixture must deserialize as a real Manifest, since
        // verify_finding_ref parses it.
        let _: Manifest = serde_json::from_slice(&bytes).unwrap();
        sha256_file(&path).unwrap()
    }

    fn finding_ref(manifest_sha256: String) -> FindingRef {
        FindingRef {
            control_id: CONTROL.to_string(),
            run_id: RUN_ID.to_string(),
            manifest_sha256,
            finding_id: "S032".to_string(),
            body_path: Some("findings.md#risk-1".to_string()),
        }
    }

    /// Open one risk bound to the sealed manifest via the real store API, so
    /// its log is a genuinely valid hash chain.
    fn open_risk(root: &Path, manifest_sha256: String) -> String {
        let due = NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        let out = store::open(
            root,
            finding_ref(manifest_sha256),
            "S032 — pickle deserialization RCE",
            Severity::Critical,
            5,
            4,
            vec!["api".to_string()],
            30,
            due,
            "jstockdi",
            None,
            None,
        )
        .expect("open risk");
        out.risk_id
    }

    #[test]
    fn clean_register_verifies() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let sha = seal_manifest(root);
        let risk_id = open_risk(root, sha);

        let report = verify(root, None).expect("verify");
        assert!(
            report.is_clean(),
            "expected clean verify, got risk failures: {:?}",
            report.risk_failures
        );
        assert_eq!(report.verified_risks.len(), 1);
        assert_eq!(report.verified_risks[0].risk_id, risk_id);
        assert_eq!(report.verified_risks[0].finding_refs, 1);
    }

    #[test]
    fn no_register_contributes_no_risk_results() {
        let tmp = tempfile::tempdir().unwrap();
        let report = verify(tmp.path(), None).expect("verify");
        assert!(report.verified_risks.is_empty());
        assert!(report.risk_failures.is_empty());
    }

    #[test]
    fn tampered_log_line_breaks_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let sha = seal_manifest(root);
        let risk_id = open_risk(root, sha);
        // Append a second valid event so the chain has a link to break.
        store::append(
            root,
            &risk_id,
            crate::risks::model::EventData::Note {
                text: "investigating".to_string(),
            },
            "jstockdi",
            None,
            None,
        )
        .expect("append note");

        // Edit the FIRST line in place: its title appears only on line 1, so
        // rewriting a byte there changes line 1's sha — which line 2's stored
        // prev_sha256 was computed over — breaking the chain without touching
        // seq order.
        let events_path = root.join("risks").join(&risk_id).join("events.jsonl");
        let text = fs::read_to_string(&events_path).unwrap();
        let tampered = text.replacen("pickle deserialization", "pickle  deserialization", 1);
        assert_ne!(text, tampered, "tamper must change the bytes");
        fs::write(&events_path, tampered).unwrap();

        let report = verify(root, None).expect("verify");
        assert!(!report.is_clean());
        let f = report
            .risk_failures
            .iter()
            .find(|f| f.risk_id == risk_id)
            .expect("expected a risk failure for the tampered log");
        assert_eq!(f.kind, RiskFailureKind::BrokenChain);
        assert!(report.verified_risks.is_empty());
    }

    #[test]
    fn mutated_manifest_breaks_finding_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let sha = seal_manifest(root);
        let risk_id = open_risk(root, sha);

        // Mutate the sealed manifest after the risk bound to it: its
        // recomputed sha no longer matches the finding_ref.
        let manifest_path = root
            .join("evidence/2026/q2")
            .join(CONTROL)
            .join(RUN_ID)
            .join("manifest.json");
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&manifest_path)
            .unwrap();
        f.write_all(b"\n").unwrap();
        drop(f);

        let report = verify(root, None).expect("verify");
        assert!(!report.is_clean());
        let f = report
            .risk_failures
            .iter()
            .find(|f| f.risk_id == risk_id)
            .expect("expected a risk failure for the mutated manifest");
        assert_eq!(f.kind, RiskFailureKind::BadFindingRef);
        assert!(f.detail.contains("ra-vuln-audit:S032"), "detail: {}", f.detail);
    }

    #[test]
    fn absent_manifest_breaks_finding_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let sha = seal_manifest(root);
        let risk_id = open_risk(root, sha);

        // Remove the whole sealed run dir the finding_ref points at.
        let run_dir = root.join("evidence/2026/q2").join(CONTROL).join(RUN_ID);
        fs::remove_dir_all(&run_dir).unwrap();

        let report = verify(root, None).expect("verify");
        assert!(!report.is_clean());
        let f = report
            .risk_failures
            .iter()
            .find(|f| f.risk_id == risk_id)
            .expect("expected a risk failure for the absent manifest");
        assert_eq!(f.kind, RiskFailureKind::BadFindingRef);
    }
}
