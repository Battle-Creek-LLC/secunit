//! `cargo-audit` capturer using the `rustsec` library directly. No
//! subprocess; no `cargo-audit` binary required.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};

use crate::canonical::{canonicalize_value, sort_array_by_key, strip_keys, Envelope};

pub const CAPTURER: &str = "deps.cargo-audit";
pub const VERSION: &str = "1";

/// Capture cargo-audit findings for `Cargo.lock` at `lockfile_path`.
///
/// The advisory database is fetched from RustSec's GitHub repo unless
/// `db_path` is supplied (test seam — point at a local advisory-db
/// clone).
pub fn capture(lockfile_path: &Path, db_path: Option<&Path>) -> Result<Envelope> {
    let lockfile = rustsec::Lockfile::load(lockfile_path)
        .with_context(|| format!("load Cargo.lock at {}", lockfile_path.display()))?;

    let db = match resolve_db_path(db_path) {
        Some(p) => rustsec::Database::open(&p)
            .map_err(|e| anyhow!("open advisory db at {}: {e}", p.display()))?,
        None => {
            rustsec::Database::fetch().map_err(|e| anyhow!("fetch RustSec advisory db: {e}"))?
        }
    };

    let settings = rustsec::report::Settings::default();
    let report = rustsec::report::Report::generate(&db, &lockfile, &settings);

    let mut value = serde_json::to_value(&report).context("serialize rustsec report")?;
    let result = canonicalize_cargo_audit(&mut value);

    Ok(Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "lockfile": lockfile_path.display().to_string(),
            "db_path": db_path.map(|p| p.display().to_string()),
        }),
        result,
    ))
}

fn resolve_db_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Ok(s) = std::env::var("SECUNIT_RUSTSEC_DB") {
        return Some(PathBuf::from(s));
    }
    None
}

/// Strip volatile fields from the rustsec report and sort lists.
///
/// `database.last-commit` and `database.last-updated` change every time
/// the advisory db is refreshed, so they would phantom-diff every
/// capture even when nothing about the project changed. The
/// vulnerability list is sorted by advisory id for stable output.
/// Canonicalize a raw `rustsec::Report` (as a `serde_json::Value`)
/// into the deterministic shape written into the envelope's
/// `result` field. Exposed for integration tests; production code
/// reaches it through [`capture`].
pub fn canonicalize_report(value: &mut Value) -> Value {
    canonicalize_cargo_audit(value)
}

fn canonicalize_cargo_audit(value: &mut Value) -> Value {
    strip_keys(value, &["last-commit", "last-updated", "advisory-count"]);

    if let Some(vulns) = value
        .pointer_mut("/vulnerabilities/list")
        .and_then(|v| v.as_array_mut())
    {
        // Hoist `advisory.id` to the top level for sorting purposes,
        // then drop it back after sorting (the field itself stays
        // inside `advisory`).
        for entry in vulns.iter_mut() {
            if let Some(id) = entry
                .pointer("/advisory/id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                if let Some(obj) = entry.as_object_mut() {
                    obj.insert("__sort_id".into(), Value::String(id));
                }
            }
        }
        sort_array_by_key(vulns, "__sort_id");
        for entry in vulns.iter_mut() {
            if let Some(obj) = entry.as_object_mut() {
                obj.remove("__sort_id");
            }
        }
    }

    // The `warnings` map is keyed by warning kind (`unmaintained`,
    // `notice`, etc). Convert to a sorted array of `{ kind, items }`
    // for stable, schema-friendly output.
    if let Some(Value::Object(map)) = value.get("warnings").cloned() {
        let mut arr: Vec<Value> = map
            .into_iter()
            .map(|(kind, items)| json!({ "kind": kind, "items": items }))
            .collect();
        for w in arr.iter_mut() {
            if let Some(items) = w.get_mut("items").and_then(|i| i.as_array_mut()) {
                sort_array_by_key(items, "package");
            }
        }
        sort_array_by_key(&mut arr, "kind");
        if let Value::Object(m) = value {
            m.insert("warnings".into(), Value::Array(arr));
        }
    }

    // `settings` carries empty defaults; drop it to keep the result
    // tightly scoped to findings.
    if let Value::Object(m) = value {
        m.remove("settings");
    }

    canonicalize_value(std::mem::replace(value, Value::Object(Map::new())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_report() -> Value {
        json!({
            "database": {
                "advisory-count": 999,
                "last-commit": "abcdef1234",
                "last-updated": "2026-04-30T12:00:00Z"
            },
            "lockfile": { "dependency-count": 14 },
            "settings": { "ignore": [], "informational_warnings": [] },
            "vulnerabilities": {
                "found": true,
                "count": 2,
                "list": [
                    {
                        "advisory": {"id": "RUSTSEC-2024-0010", "title": "Z thing"},
                        "package": {"name": "zlib", "version": "1.0.0"},
                        "versions": {"patched": []},
                        "affected": null
                    },
                    {
                        "advisory": {"id": "RUSTSEC-2024-0001", "title": "A thing"},
                        "package": {"name": "alpha", "version": "0.1.0"},
                        "versions": {"patched": []},
                        "affected": null
                    }
                ]
            },
            "warnings": {
                "unmaintained": [
                    {"package": {"name": "old-crate"}, "kind": "unmaintained"}
                ]
            }
        })
    }

    #[test]
    fn cargo_audit_canonicalize_sorts_and_strips() {
        let mut v = synthetic_report();
        let out = canonicalize_cargo_audit(&mut v);
        let body = serde_json::to_string(&out).unwrap();
        assert!(!body.contains("last-commit"));
        assert!(!body.contains("last-updated"));
        assert!(!body.contains("settings"));
        let alpha = body.find("RUSTSEC-2024-0001").unwrap();
        let zlib = body.find("RUSTSEC-2024-0010").unwrap();
        assert!(alpha < zlib, "list must sort by advisory id");
        assert!(body.contains("\"warnings\":[{"));
        assert!(body.contains("\"kind\":\"unmaintained\""));
    }

    #[test]
    fn cargo_audit_envelope_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let mut v1 = synthetic_report();
        let r1 = canonicalize_cargo_audit(&mut v1);
        let mut v2 = synthetic_report();
        let r2 = canonicalize_cargo_audit(&mut v2);
        let e1 = Envelope::new(CAPTURER, VERSION, json!({"lockfile": "a"}), r1);
        let e2 = Envelope::new(CAPTURER, VERSION, json!({"lockfile": "a"}), r2);
        assert_eq!(
            e1.to_canonical_json().unwrap(),
            e2.to_canonical_json().unwrap()
        );
    }
}
