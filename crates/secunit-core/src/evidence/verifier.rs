//! Walk every run for a control (or all controls) in chronological order,
//! recompute artifact hashes, and check each `prior_run.manifest_sha256`
//! against the recomputed sha of the prior manifest.
//!
//! This is the single point of integrity for an assessor; the test
//! surface here matters more than perf.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use super::hasher::{hash_tree, sha256_file};
use super::manifest::Manifest;

const MANIFEST_FILE: &str = "manifest.json";
const PREPARE_FILE: &str = "prepare.json";
const RESULT_FILE: &str = "result.json";
const PENDING_SENTINEL: &str = ".run-pending";
const ABORT_FILE: &str = "abort.json";

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
        self.failures.is_empty()
    }
}

/// Verify every run for `control_id`, or every run if `None`. Walks
/// runs in chronological order (by `run_id`, which is ISO-date-prefixed).
pub fn verify(root: &Path, control_id: Option<&str>) -> Result<VerifyReport> {
    let mut report = VerifyReport::default();
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

            // Check artifact hashes match the on-disk files.
            let mismatches = recompute_and_compare(&run_dir, &manifest)?;
            if !mismatches.is_empty() {
                report.failures.push(VerifyFailure {
                    control_id: cid.clone(),
                    run_id: run_id.clone(),
                    run_dir: run_dir.clone(),
                    kind: FailureKind::ArtifactMismatch,
                    detail: mismatches.join("; "),
                });
                // Don't abandon the chain just because one run had a tampered
                // artifact — we still report subsequent runs.
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

            prior_sha = Some(sha256_file(manifest_path)?);
            prior_run_id = Some(run_id.clone());
            report.verified.push(VerifiedRun {
                control_id: cid.clone(),
                run_id: run_id.clone(),
                run_dir,
            });
        }
    }
    Ok(report)
}

fn recompute_and_compare(run_dir: &Path, manifest: &Manifest) -> Result<Vec<String>> {
    let exclude = [
        PREPARE_FILE,
        RESULT_FILE,
        MANIFEST_FILE,
        ABORT_FILE,
        PENDING_SENTINEL,
    ];
    let on_disk = hash_tree(run_dir, &exclude)?;
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

#[allow(dead_code)]
fn _ensure_anyhow_used() -> anyhow::Error {
    anyhow!("placeholder")
}
