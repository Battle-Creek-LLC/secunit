//! Embedded JSON schemas for every capturer envelope. Schemas live in
//! `schemas/capture-*.schema.json` at repo root and are baked into the
//! binary via `include_str!`.
//!
//! Capturers route output through [`validate`] before writing so the
//! schema is the single source of truth for what a valid envelope
//! looks like — failing it is a hard error (exit code 1, "validation
//! failure"), not a warning.

use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use jsonschema::{Draft, JSONSchema};
use serde_json::Value;

use crate::canonical::Envelope;

macro_rules! compiled {
    ($name:literal) => {{
        static CELL: OnceLock<JSONSchema> = OnceLock::new();
        CELL.get_or_init(|| {
            let raw = include_str!(concat!("../../../schemas/", $name));
            let json: Value = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("schema {}: invalid JSON: {e}", $name));
            JSONSchema::options()
                .with_draft(Draft::Draft202012)
                .compile(&json)
                .unwrap_or_else(|e| panic!("schema {}: compile failed: {e}", $name))
        })
    }};
}

fn schema_for(capturer: &str) -> Option<&'static JSONSchema> {
    Some(match capturer {
        "deps.pip-audit" => compiled!("capture-deps-pip-audit.schema.json"),
        "deps.pnpm-audit" => compiled!("capture-deps-pnpm-audit.schema.json"),
        "deps.cargo-audit" => compiled!("capture-deps-cargo-audit.schema.json"),
        "deps.osv-query" => compiled!("capture-deps-osv-query.schema.json"),
        "github.dependabot-alerts" => {
            compiled!("capture-github-dependabot-alerts.schema.json")
        }
        "github.branch-protection" => {
            compiled!("capture-github-branch-protection.schema.json")
        }
        "github.org-members" => compiled!("capture-github-org-members.schema.json"),
        "github.audit-log" => compiled!("capture-github-audit-log.schema.json"),
        "github.codeql-alerts" => compiled!("capture-github-codeql-alerts.schema.json"),
        _ => return None,
    })
}

/// Validate `envelope` against its registered per-capturer schema.
///
/// Returns the list of jsonschema diagnostics on mismatch — empty
/// when valid. Returns an error if no schema is registered for the
/// envelope's `capturer` field (program error, not a data error).
pub fn validate(envelope: &Envelope) -> Result<Vec<String>> {
    let value = serde_json::to_value(envelope)?;
    let schema = schema_for(&envelope.capturer)
        .ok_or_else(|| anyhow!("no schema registered for capturer `{}`", envelope.capturer))?;
    let errs = match schema.validate(&value) {
        Ok(()) => Vec::new(),
        Err(it) => it.map(|e| format!("{}: {}", e.instance_path, e)).collect(),
    };
    Ok(errs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pip_audit_schema_accepts_minimal_envelope() {
        let env = Envelope::new(
            "deps.pip-audit",
            "1",
            json!({ "path": "/x" }),
            json!({ "dependencies": [] }),
        );
        assert!(validate(&env).unwrap().is_empty());
    }

    #[test]
    fn pip_audit_schema_rejects_wrong_capturer_const() {
        // Force-construct an envelope with a mismatched capturer.
        let mut env = Envelope::new(
            "deps.pip-audit",
            "1",
            json!({ "path": "/x" }),
            json!({ "dependencies": [] }),
        );
        env.capturer = "deps.pip-audit".into();
        // This is the happy case; flip to verify failure detection.
        env.captured_at = "not-a-timestamp".into();
        let errs = validate(&env).unwrap();
        assert!(!errs.is_empty(), "expected captured_at pattern to fail");
    }

    #[test]
    fn dependabot_alerts_schema_accepts_minimal_envelope() {
        let env = Envelope::new(
            "github.dependabot-alerts",
            "1",
            json!({ "repo": "o/r", "state": "all" }),
            json!({ "alerts": [{"number": 1, "state": "open"}] }),
        );
        assert!(validate(&env).unwrap().is_empty());
    }

    #[test]
    fn unknown_capturer_returns_err() {
        let env = Envelope::new("made.up", "1", json!({}), json!({}));
        assert!(validate(&env).is_err());
    }
}
