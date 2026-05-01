//! Canonical envelope shared by every capturer.
//!
//! Every capturer writes JSON shaped:
//!
//! ```text
//! {
//!   "capturer": "github.dependabot-alerts",
//!   "version":  "1",
//!   "captured_at": "2026-05-01T12:00:00Z",
//!   "args":   { ...invocation arguments, sorted... },
//!   "result": { ...subsystem-specific payload... }
//! }
//! ```
//!
//! Determinism requirements (Phase 4 exit criteria):
//!
//! - Map keys are emitted in lexicographic order regardless of whether
//!   `serde_json` was compiled with the `preserve_order` feature.
//! - Arrays of records are sorted by the caller using a stable id field;
//!   non-record arrays preserve upstream order.
//! - Timestamps are ISO-8601 UTC with whole-second precision.
//! - Ephemeral fields (request ids, pagination cursors, `*_url`,
//!   `node_id`, etag-likes) are stripped *before* the value reaches
//!   here. This module does not know which keys are ephemeral; that's
//!   the capturer's job.

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::time::now_iso8601;

/// The canonical wrapper written to `--out` by every capturer.
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub capturer: String,
    pub version: String,
    pub captured_at: String,
    pub args: Value,
    pub result: Value,
}

impl Envelope {
    /// Build an envelope. `captured_at` is filled from
    /// [`crate::time::now_iso8601`] and `args`/`result` are
    /// canonicalized so the final JSON is byte-stable.
    pub fn new(capturer: &str, version: &str, args: Value, result: Value) -> Self {
        Self {
            capturer: capturer.to_string(),
            version: version.to_string(),
            captured_at: now_iso8601(),
            args: canonicalize_value(args),
            result: canonicalize_value(result),
        }
    }

    /// Serialize to a canonical pretty-printed JSON string.
    pub fn to_canonical_json(&self) -> Result<String> {
        let v = serde_json::to_value(self).context("serialize envelope")?;
        let v = canonicalize_value(v);
        serde_json::to_string_pretty(&v).context("pretty-print envelope")
    }

    /// Atomically write this envelope to `path` (write-temp + rename so
    /// readers never observe a partial file).
    pub fn write_to(&self, path: &Path) -> Result<()> {
        let body = self.to_canonical_json()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            let mut f =
                fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
            f.write_all(body.as_bytes())
                .with_context(|| format!("write {}", tmp.display()))?;
            f.write_all(b"\n").ok();
            f.sync_all().ok();
        }
        fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }
}

/// Recursively rebuild every JSON object so its keys are emitted in
/// lexicographic order. This is independent of the `serde_json`
/// `preserve_order` feature, so output is stable even if a downstream
/// crate flips it on through feature unification.
pub fn canonicalize_value(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = Map::with_capacity(entries.len());
            for (k, v) in entries {
                out.insert(k, canonicalize_value(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalize_value).collect()),
        other => other,
    }
}

/// Sort an array of objects in place by the given top-level key.
/// Non-objects, or objects missing the key, sort to the end in stable
/// upstream order. Tie-broken by the JSON serialization of the entry,
/// so equal-key entries are still deterministic.
pub fn sort_array_by_key(arr: &mut [Value], key: &str) {
    arr.sort_by(|a, b| {
        let ak = extract_sort_key(a, key);
        let bk = extract_sort_key(b, key);
        ak.cmp(&bk).then_with(|| {
            let ja = serde_json::to_string(a).unwrap_or_default();
            let jb = serde_json::to_string(b).unwrap_or_default();
            ja.cmp(&jb)
        })
    });
}

fn extract_sort_key(v: &Value, key: &str) -> SortKey {
    match v.get(key) {
        Some(Value::String(s)) => SortKey::Present(s.clone()),
        Some(Value::Number(n)) => {
            // Pad integers so "10" sorts after "2"; for floats fall
            // back to the JSON form.
            if let Some(i) = n.as_i64() {
                SortKey::Present(format!("{:020}", i))
            } else {
                SortKey::Present(n.to_string())
            }
        }
        Some(Value::Bool(b)) => SortKey::Present(b.to_string()),
        Some(Value::Null) | None => SortKey::Missing,
        Some(other) => SortKey::Present(other.to_string()),
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SortKey {
    Present(String),
    Missing,
}

/// Strip a fixed list of keys (recursively) from any object encountered
/// in `value`. Used to drop ephemeral / volatile upstream fields like
/// `*_url`, `node_id`, `etag` before canonicalization.
pub fn strip_keys(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            for k in keys {
                map.remove(*k);
            }
            for (_, v) in map.iter_mut() {
                strip_keys(v, keys);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_keys(v, keys);
            }
        }
        _ => {}
    }
}

/// Strip every key matching `predicate` (recursively).
pub fn strip_keys_matching(value: &mut Value, predicate: impl Fn(&str) -> bool + Copy) {
    match value {
        Value::Object(map) => {
            let to_remove: Vec<String> = map
                .keys()
                .filter(|k| predicate(k.as_str()))
                .cloned()
                .collect();
            for k in to_remove {
                map.remove(&k);
            }
            for (_, v) in map.iter_mut() {
                strip_keys_matching(v, predicate);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_keys_matching(v, predicate);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonicalize_sorts_keys() {
        let v = json!({ "z": 1, "a": 2, "m": { "y": 3, "b": 4 } });
        let s = serde_json::to_string(&canonicalize_value(v)).unwrap();
        assert_eq!(s, r#"{"a":2,"m":{"b":4,"y":3},"z":1}"#);
    }

    #[test]
    fn sort_array_by_key_sorts_by_id() {
        let mut arr = vec![
            json!({ "id": "c", "v": 1 }),
            json!({ "id": "a", "v": 2 }),
            json!({ "id": "b", "v": 3 }),
        ];
        sort_array_by_key(&mut arr, "id");
        let ids: Vec<&str> = arr.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn sort_array_by_numeric_key() {
        let mut arr = vec![
            json!({ "number": 10 }),
            json!({ "number": 2 }),
            json!({ "number": 100 }),
        ];
        sort_array_by_key(&mut arr, "number");
        let nums: Vec<i64> = arr.iter().map(|v| v["number"].as_i64().unwrap()).collect();
        assert_eq!(nums, vec![2, 10, 100]);
    }

    #[test]
    fn strip_keys_removes_ephemeral() {
        let mut v = json!({
            "id": 1,
            "url": "https://x",
            "nested": { "node_id": "abc", "keep": true },
        });
        strip_keys(&mut v, &["url", "node_id"]);
        assert_eq!(v, json!({ "id": 1, "nested": { "keep": true } }));
    }

    #[test]
    fn strip_keys_matching_suffix() {
        let mut v = json!({
            "id": 1,
            "html_url": "x",
            "issues_url": "y",
            "name": "ok",
        });
        strip_keys_matching(&mut v, |k| k.ends_with("_url"));
        assert_eq!(v, json!({ "id": 1, "name": "ok" }));
    }

    #[test]
    fn envelope_is_byte_stable() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let e1 = Envelope::new(
            "test.thing",
            "1",
            json!({ "z": 1, "a": 2 }),
            json!({ "items": [{"id": "b"}, {"id": "a"}] }),
        );
        let e2 = Envelope::new(
            "test.thing",
            "1",
            json!({ "a": 2, "z": 1 }),
            json!({ "items": [{"id": "b"}, {"id": "a"}] }),
        );
        assert_eq!(
            e1.to_canonical_json().unwrap(),
            e2.to_canonical_json().unwrap()
        );
    }
}
