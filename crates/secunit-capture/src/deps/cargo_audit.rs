//! `cargo-audit` capturer using the `rustsec` library directly. No
//! subprocess; no `cargo-audit` binary required.
//!
//! We deliberately build `rustsec` without its `git`/`gix-reqwest`
//! features so no `gix` crate is compiled in (those carry a string of
//! RustSec advisories). The library still exposes `Database::open` and
//! `report::Report::generate`; only the git-backed `Database::fetch`
//! is gated out. When no local advisory-db is supplied we acquire one
//! over plain HTTPS instead — see [`fetch_advisory_db`].

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Map, Value};

use crate::canonical::{canonicalize_value, sort_array_by_key, strip_keys, Envelope};

pub const CAPTURER: &str = "deps.cargo-audit";
pub const VERSION: &str = "1";

/// RustSec advisory-db tarball (the `main` branch snapshot from GitHub).
const ADVISORY_DB_TARBALL: &str =
    "https://github.com/rustsec/advisory-db/archive/refs/heads/main.tar.gz";

/// Capture cargo-audit findings for `Cargo.lock` at `lockfile_path`.
///
/// The advisory database is resolved from `db_path` (or the
/// `SECUNIT_RUSTSEC_DB` env var) when supplied — a test seam pointing
/// at a local advisory-db clone. Otherwise the RustSec advisory-db
/// snapshot is downloaded over HTTPS, extracted into a local cache,
/// and opened from disk. No `gix` is involved either way.
pub fn capture(lockfile_path: &Path, db_path: Option<&Path>) -> Result<Envelope> {
    let lockfile = rustsec::Lockfile::load(lockfile_path)
        .with_context(|| format!("load Cargo.lock at {}", lockfile_path.display()))?;

    let db = match resolve_db_path(db_path) {
        Some(p) => rustsec::Database::open(&p)
            .map_err(|e| anyhow!("open advisory db at {}: {e}", p.display()))?,
        None => {
            let cached = fetch_advisory_db().context("acquire RustSec advisory db over HTTPS")?;
            rustsec::Database::open(&cached)
                .map_err(|e| anyhow!("open advisory db at {}: {e}", cached.display()))?
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

/// Download the RustSec advisory-db snapshot over HTTPS and extract it
/// into a local cache directory, returning the path to the extracted
/// repository root (suitable for `rustsec::Database::open`).
///
/// This replaces `rustsec::Database::fetch`, which is gated behind the
/// `git`/`gix` feature we removed to clear the gix advisory tree. The
/// download is a plain `gzip`-compressed `tar` archive fetched with
/// `reqwest` (blocking) — no git client, no `gix`.
fn fetch_advisory_db() -> Result<PathBuf> {
    let cache_root = advisory_db_cache_dir()?;
    std::fs::create_dir_all(&cache_root)
        .with_context(|| format!("create advisory-db cache dir {}", cache_root.display()))?;

    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("secunit/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build reqwest client")?;
    let resp = client
        .get(ADVISORY_DB_TARBALL)
        .send()
        .with_context(|| format!("GET {ADVISORY_DB_TARBALL}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!(
            "advisory-db download {ADVISORY_DB_TARBALL} returned HTTP {status}"
        ));
    }
    let bytes = resp.bytes().context("read advisory-db tarball body")?;

    // Extract into a fresh staging dir, then atomically swap it into
    // place so a partially-written cache can never be opened.
    let staging = cache_root.join(".staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("clear staging dir {}", staging.display()))?;
    }
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("create staging dir {}", staging.display()))?;

    let decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(&staging)
        .context("unpack advisory-db tarball")?;

    // GitHub source tarballs nest everything under a single top-level
    // directory (e.g. `advisory-db-main/`). Find it.
    let repo_root = first_subdir(&staging)?
        .ok_or_else(|| anyhow!("advisory-db tarball had no top-level directory"))?;

    let dest = cache_root.join("advisory-db");
    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .with_context(|| format!("clear stale cache {}", dest.display()))?;
    }
    std::fs::rename(&repo_root, &dest)
        .with_context(|| format!("install advisory-db into {}", dest.display()))?;
    let _ = std::fs::remove_dir_all(&staging);

    Ok(dest)
}

/// Cache directory for the downloaded advisory db. Honours
/// `SECUNIT_CACHE_DIR` so callers (and tests) can redirect it; falls
/// back to the platform cache dir, then the system temp dir.
fn advisory_db_cache_dir() -> Result<PathBuf> {
    if let Ok(s) = std::env::var("SECUNIT_CACHE_DIR") {
        return Ok(PathBuf::from(s).join("rustsec"));
    }
    let base = dirs_cache_dir().unwrap_or_else(std::env::temp_dir);
    Ok(base.join("secunit").join("rustsec"))
}

/// Minimal, dependency-free platform cache-dir lookup. We avoid pulling
/// in the `dirs` crate for a single path.
fn dirs_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
            return Some(PathBuf::from(x));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))
    }
}

/// Return the first directory entry directly under `dir`, if any.
fn first_subdir(dir: &Path) -> Result<Option<PathBuf>> {
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("read staging dir {}", dir.display()))?
    {
        let entry = entry.context("read staging dir entry")?;
        if entry.file_type().context("stat staging entry")?.is_dir() {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
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
