//! `pip-audit` capturer. No usable Rust library exists for the PyPA
//! advisory database, so we shell out to `pip-audit` (which the
//! operator must have installed) and canonicalize its JSON output.
//!
//! ## Audit target
//!
//! `pip-audit` with no project flags audits the *active Python
//! interpreter's site-packages*. That is almost never what we want:
//! whichever Python is on `$PATH` when secunit runs has nothing to do
//! with the project's declared dependencies. To produce evidence that
//! actually corresponds to the project, the capturer auto-detects a
//! dependency manifest under `path` and passes it to pip-audit
//! explicitly:
//!
//! - `requirements.txt` (preferred) → `pip-audit -r requirements.txt`
//! - `pyproject.toml`               → `pip-audit -P pyproject.toml`
//!
//! If neither file exists, the capturer errors out rather than silently
//! auditing the ambient interpreter — a capture artifact whose contents
//! don't match the project is worse than no artifact.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::canonical::{canonicalize_value, sort_array_by_key, strip_keys, Envelope};

use super::cmd::{CmdRunner, RealRunner};

pub const CAPTURER: &str = "deps.pip-audit";
pub const VERSION: &str = "1";

/// Which manifest file the capturer pointed pip-audit at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditTarget {
    /// `requirements.txt` resolved via `pip-audit -r`.
    Requirements,
    /// `pyproject.toml` resolved via `pip-audit -P`.
    Pyproject,
}

impl AuditTarget {
    fn filename(self) -> &'static str {
        match self {
            AuditTarget::Requirements => "requirements.txt",
            AuditTarget::Pyproject => "pyproject.toml",
        }
    }

    fn flag(self) -> &'static str {
        match self {
            AuditTarget::Requirements => "-r",
            AuditTarget::Pyproject => "-P",
        }
    }
}

/// Pick the most specific manifest available, preferring `requirements.txt`
/// over `pyproject.toml`. Returns `None` if neither exists.
fn detect_target(path: &Path) -> Option<(AuditTarget, PathBuf)> {
    for t in [AuditTarget::Requirements, AuditTarget::Pyproject] {
        let p = path.join(t.filename());
        if p.is_file() {
            return Some((t, p));
        }
    }
    None
}

/// Capture pip-audit output for a Python project rooted at `path`.
pub fn capture(path: &Path) -> Result<Envelope> {
    capture_with(path, &RealRunner)
}

/// Same as [`capture`] but with an injectable subprocess runner.
pub fn capture_with(path: &Path, runner: &dyn CmdRunner) -> Result<Envelope> {
    let (target, _abs_path) = detect_target(path).ok_or_else(|| {
        anyhow!(
            "no requirements.txt or pyproject.toml found at {} — pip-audit needs an explicit \
             dependency manifest; auditing the ambient Python interpreter would produce \
             evidence unrelated to the project",
            path.display()
        )
    })?;

    // Pass the manifest as a path relative to `path` (the runner's cwd)
    // so the captured `args.target` field is portable across machines —
    // i.e. the canonical artifact says `"requirements.txt"`, not
    // `/Users/.../plotzy-api/requirements.txt`.
    let target_arg = target.filename();
    let args = ["--format=json", "--strict", target.flag(), target_arg];

    let out = runner.run("pip-audit", &args, path).with_context(|| {
        format!(
            "pip-audit not found or failed in {} (install `pip install pip-audit`)",
            path.display()
        )
    })?;

    let raw: Value = if out.stdout.trim().is_empty() {
        // Empty stdout with a non-zero exit means pip-audit failed *before*
        // it could emit JSON — typically a venv resolution conflict, missing
        // pin, or argument validation error. Surface stderr so the operator
        // sees what went wrong instead of getting a phantom "successful"
        // capture with `result: {}`. Empty stdout with exit 0 is genuinely
        // a clean audit (no findings), which is rare but valid.
        if out.exit_code != 0 {
            return Err(anyhow!(
                "pip-audit exited {} with no JSON output (target: {target_arg}); stderr: {}",
                out.exit_code,
                truncate(&out.stderr, 500)
            ));
        }
        json!({})
    } else {
        serde_json::from_str(&out.stdout).with_context(|| {
            format!(
                "parse pip-audit stdout (exit={}): {}",
                out.exit_code,
                truncate(&out.stdout, 200)
            )
        })?
    };

    let result = canonicalize_pip_audit(raw);

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "path": path.display().to_string(),
            "target": target_arg,
        }),
        result,
    ))
}

/// Canonicalize pip-audit's JSON output:
/// - keep `dependencies[]`, sort by `name`
/// - within each dependency, sort `vulns[]` by `id`
/// - strip the volatile `fixed_versions` upstream-id ordering by
///   sorting that array too
fn canonicalize_pip_audit(mut v: Value) -> Value {
    // pip-audit shapes its output as `{ "dependencies": [...] }` plus
    // optional `fixes`/`requirements`. Drop volatile fields not part of
    // the audit signal.
    strip_keys(&mut v, &["pip_audit_version"]);

    if let Some(deps) = v.get_mut("dependencies").and_then(|d| d.as_array_mut()) {
        for dep in deps.iter_mut() {
            if let Some(vulns) = dep.get_mut("vulns").and_then(|v| v.as_array_mut()) {
                for vuln in vulns.iter_mut() {
                    if let Some(fv) = vuln.get_mut("fix_versions").and_then(|f| f.as_array_mut()) {
                        fv.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
                    }
                    if let Some(aliases) = vuln.get_mut("aliases").and_then(|a| a.as_array_mut()) {
                        aliases
                            .sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
                    }
                }
                sort_array_by_key(vulns, "id");
            }
        }
        sort_array_by_key(deps, "name");
    }
    canonicalize_value(v)
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::cmd::testing::CannedRunner;
    use crate::deps::cmd::CmdOutput;

    /// Sample pip-audit JSON output used by the canonicalization tests.
    fn sample_stdout() -> String {
        r#"{
            "pip_audit_version": "2.7.3",
            "dependencies": [
                {
                    "name": "requests",
                    "version": "2.30.0",
                    "vulns": [
                        {"id":"GHSA-9wx4-h78v-vm56","fix_versions":["2.32.0","2.31.0"],"aliases":["CVE-2024-35195"],"description":"x"}
                    ]
                },
                {
                    "name": "Django",
                    "version": "4.2.0",
                    "vulns": []
                }
            ]
        }"#
        .to_string()
    }

    fn out(stdout: String) -> CmdOutput {
        CmdOutput {
            stdout,
            stderr: String::new(),
            exit_code: 1,
        }
    }

    /// Registers a canned response for the exact args the capturer should
    /// emit when it detects the given target file in `path`.
    fn runner_for(target: AuditTarget) -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "pip-audit",
            &[
                "--format=json",
                "--strict",
                target.flag(),
                target.filename(),
            ],
            out(sample_stdout()),
        );
        r
    }

    /// A scratch project dir containing the requested manifest file(s).
    /// Returns the dir; tempdir is dropped when the returned guard is dropped.
    fn scratch_with(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            std::fs::write(d.path().join(f), "# placeholder\n").unwrap();
        }
        d
    }

    #[test]
    fn pip_audit_canonicalizes_and_sorts() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);
        let env = capture_with(d.path(), &runner_for(AuditTarget::Requirements)).unwrap();
        let body = env.to_canonical_json().unwrap();
        // Django sorts before requests; fix_versions sorted ascending.
        assert!(body.find("\"Django\"").unwrap() < body.find("\"requests\"").unwrap());
        assert!(body.contains("\"2.31.0\""));
        assert!(body.find("\"2.31.0\"").unwrap() < body.find("\"2.32.0\"").unwrap());
        // pip_audit_version is stripped.
        assert!(!body.contains("pip_audit_version"));
    }

    #[test]
    fn pip_audit_byte_identical_across_runs() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);
        let a = capture_with(d.path(), &runner_for(AuditTarget::Requirements))
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture_with(d.path(), &runner_for(AuditTarget::Requirements))
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn pip_audit_uses_requirements_when_present() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        // Both files present — requirements.txt wins.
        let d = scratch_with(&["requirements.txt", "pyproject.toml"]);
        let env = capture_with(d.path(), &runner_for(AuditTarget::Requirements)).unwrap();
        // The captured args record which manifest was audited.
        assert_eq!(env.args.get("target").and_then(|v| v.as_str()), Some("requirements.txt"));
    }

    #[test]
    fn pip_audit_falls_back_to_pyproject() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["pyproject.toml"]);
        let env = capture_with(d.path(), &runner_for(AuditTarget::Pyproject)).unwrap();
        assert_eq!(env.args.get("target").and_then(|v| v.as_str()), Some("pyproject.toml"));
    }

    #[test]
    fn pip_audit_errors_when_subprocess_fails_with_empty_stdout() {
        // Repro for the silently-empty-success bug: pip-audit aborts before
        // emitting JSON (e.g. on a requirements resolution conflict),
        // emits explanation to stderr, returns non-zero. The capturer must
        // surface that as an error rather than write a phantom envelope
        // with `result: {}`.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);
        let r = CannedRunner::new();
        r.register(
            "pip-audit",
            &[
                "--format=json",
                "--strict",
                AuditTarget::Requirements.flag(),
                AuditTarget::Requirements.filename(),
            ],
            CmdOutput {
                stdout: String::new(),
                stderr: "ERROR: ResolutionImpossible: nltk==3.9.3 conflicts with line 79".into(),
                exit_code: 1,
            },
        );
        let err = capture_with(d.path(), &r).expect_err("expected error from failed subprocess");
        let msg = format!("{err}");
        assert!(msg.contains("exited 1"), "error should record exit code: {msg}");
        assert!(
            msg.contains("ResolutionImpossible"),
            "error should surface stderr: {msg}"
        );
    }

    #[test]
    fn pip_audit_treats_empty_stdout_with_zero_exit_as_clean() {
        // Symmetric with the previous test: zero exit + empty stdout is a
        // genuine clean audit (no findings) and should produce a successful
        // envelope with an empty `result`.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);
        let r = CannedRunner::new();
        r.register(
            "pip-audit",
            &[
                "--format=json",
                "--strict",
                AuditTarget::Requirements.flag(),
                AuditTarget::Requirements.filename(),
            ],
            CmdOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
            },
        );
        let env = capture_with(d.path(), &r).expect("zero-exit empty stdout should be ok");
        assert_eq!(env.result, serde_json::json!({}));
    }

    #[test]
    fn pip_audit_errors_when_no_manifest_present() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&[]); // empty project dir
        let runner = CannedRunner::new(); // intentionally empty: any subprocess call would panic
        let err = capture_with(d.path(), &runner).expect_err("expected an error");
        let msg = format!("{err}");
        assert!(
            msg.contains("requirements.txt") && msg.contains("pyproject.toml"),
            "error should name both manifest types: {msg}"
        );
        assert!(
            msg.contains("ambient"),
            "error should explain why we refuse to fall through to ambient interpreter: {msg}"
        );
    }
}
