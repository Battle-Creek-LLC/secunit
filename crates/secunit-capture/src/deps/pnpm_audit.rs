//! `pnpm audit` capturer. Shells out to pnpm and canonicalizes its
//! JSON advisory report.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::canonical::{canonicalize_value, sort_array_by_key, strip_keys, Envelope};

use super::cmd::{CmdRunner, RealRunner};

pub const CAPTURER: &str = "deps.pnpm-audit";
pub const VERSION: &str = "1";

/// Capture `pnpm audit` for a Node project rooted at `path`.
pub fn capture(path: &Path) -> Result<Envelope> {
    capture_with(path, &RealRunner)
}

pub fn capture_with(path: &Path, runner: &dyn CmdRunner) -> Result<Envelope> {
    let out = runner
        .run("pnpm", &["audit", "--json"], path)
        .with_context(|| format!("pnpm not found or failed in {}", path.display()))?;

    let raw: Value = if out.stdout.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&out.stdout).with_context(|| {
            format!(
                "parse pnpm audit stdout (exit={}): {}",
                out.exit_code,
                truncate(&out.stdout, 200)
            )
        })?
    };

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({ "path": path.display().to_string() }),
        canonicalize_pnpm_audit(raw),
    ))
}

fn canonicalize_pnpm_audit(mut v: Value) -> Value {
    // pnpm audit shape: `{ advisories: { "<id>": {...} }, metadata: {...} }`.
    // The metadata block is volatile (contains scan duration). Strip it.
    strip_keys(&mut v, &["metadata", "vulnerabilities"]);

    // Convert the keyed `advisories` map into a sorted array so output
    // is positional and stable.
    if let Some(Value::Object(map)) = v.get("advisories").cloned() {
        let mut arr: Vec<Value> = map.into_values().collect();
        for entry in arr.iter_mut() {
            if let Some(findings) = entry.get_mut("findings").and_then(|f| f.as_array_mut()) {
                for finding in findings.iter_mut() {
                    if let Some(paths) = finding.get_mut("paths").and_then(|p| p.as_array_mut()) {
                        paths
                            .sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
                    }
                }
                sort_array_by_key(findings, "version");
            }
        }
        sort_array_by_key(&mut arr, "id");
        if let Value::Object(m) = &mut v {
            m.remove("advisories");
            m.insert("advisories".into(), Value::Array(arr));
        }
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

    fn runner() -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "pnpm",
            &["audit", "--json"],
            CmdOutput {
                stdout: r#"{
                    "advisories": {
                        "1234": {"id":1234,"module_name":"lodash","severity":"high","findings":[{"version":"4.17.20","paths":["b","a"]}]},
                        "5": {"id":5,"module_name":"axios","severity":"low","findings":[]}
                    },
                    "metadata": {"vulnerabilities": {"high": 1, "low": 1}, "totalDependencies": 999}
                }"#.to_string(),
                stderr: String::new(),
                exit_code: 1,
            },
        );
        r
    }

    #[test]
    fn pnpm_audit_canonicalizes() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let env = capture_with(std::path::Path::new("/tmp"), &runner()).unwrap();
        let body = env.to_canonical_json().unwrap();
        assert!(!body.contains("metadata"));
        assert!(!body.contains("totalDependencies"));
        // id 5 sorts before id 1234 numerically.
        let a_pos = body.find("\"axios\"").unwrap();
        let b_pos = body.find("\"lodash\"").unwrap();
        assert!(
            a_pos < b_pos,
            "axios (id 5) must come before lodash (id 1234)"
        );
        // path "a" sorted before "b".
        let pa = body.find("\"a\"").unwrap();
        let pb = body.find("\"b\"").unwrap();
        assert!(pa < pb);
    }

    #[test]
    fn pnpm_audit_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let a = capture_with(std::path::Path::new("/tmp"), &runner())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let b = capture_with(std::path::Path::new("/tmp"), &runner())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(a, b);
    }
}
