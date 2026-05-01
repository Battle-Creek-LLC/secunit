//! `pip-audit` capturer. No usable Rust library exists for the PyPA
//! advisory database, so we shell out to `pip-audit` (which the
//! operator must have installed) and canonicalize its JSON output.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::canonical::{canonicalize_value, sort_array_by_key, strip_keys, Envelope};

use super::cmd::{CmdRunner, RealRunner};

pub const CAPTURER: &str = "deps.pip-audit";
pub const VERSION: &str = "1";

/// Capture pip-audit output for a Python project rooted at `path`.
pub fn capture(path: &Path) -> Result<Envelope> {
    capture_with(path, &RealRunner)
}

/// Same as [`capture`] but with an injectable subprocess runner.
pub fn capture_with(path: &Path, runner: &dyn CmdRunner) -> Result<Envelope> {
    let out = runner
        .run("pip-audit", &["--format=json", "--strict"], path)
        .with_context(|| {
            format!(
                "pip-audit not found or failed in {} (install `pip install pip-audit`)",
                path.display()
            )
        })?;

    let raw: Value = if out.stdout.trim().is_empty() {
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
        json!({ "path": path.display().to_string() }),
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

    fn fixture_runner() -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "pip-audit",
            &["--format=json", "--strict"],
            CmdOutput {
                stdout: r#"{
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
                .to_string(),
                stderr: String::new(),
                exit_code: 1,
            },
        );
        r
    }

    #[test]
    fn pip_audit_canonicalizes_and_sorts() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let env = capture_with(std::path::Path::new("/tmp"), &fixture_runner()).unwrap();
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
        let a = capture_with(std::path::Path::new("/tmp"), &fixture_runner())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture_with(std::path::Path::new("/tmp"), &fixture_runner())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }
}
