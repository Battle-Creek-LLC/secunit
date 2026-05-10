//! Compile-time embedded JSON Schemas. The schemas live in
//! `crates/secunit-core/schemas/*.schema.json` and are baked into
//! the binary so validation does not depend on the install location.

use std::sync::OnceLock;

use jsonschema::{Draft, JSONSchema};
use serde_json::Value;

macro_rules! schema {
    ($name:literal, $path:literal) => {{
        static CELL: OnceLock<JSONSchema> = OnceLock::new();
        CELL.get_or_init(|| {
            let raw = include_str!(concat!("../schemas/", $path));
            let json: Value = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("schema {}: invalid JSON: {e}", $name));
            JSONSchema::options()
                .with_draft(Draft::Draft202012)
                .compile(&json)
                .unwrap_or_else(|e| panic!("schema {}: compile failed: {e}", $name))
        })
    }};
}

#[derive(Debug, Clone, Copy)]
pub enum Schema {
    Control,
    Inventory,
    Schedule,
    State,
    Manifest,
    Prepare,
    Result,
    Config,
}

impl Schema {
    pub fn compiled(self) -> &'static JSONSchema {
        match self {
            Schema::Control => schema!("control", "control.schema.json"),
            Schema::Inventory => schema!("inventory", "inventory.schema.json"),
            Schema::Schedule => schema!("schedule", "schedule.schema.json"),
            Schema::State => schema!("state", "state.schema.json"),
            Schema::Manifest => schema!("manifest", "manifest.schema.json"),
            Schema::Prepare => schema!("prepare", "prepare.schema.json"),
            Schema::Result => schema!("result", "result.schema.json"),
            Schema::Config => schema!("_config", "_config.schema.json"),
        }
    }

    /// Validate `value` against this schema. Returns the list of errors as
    /// `path: message` strings; empty when valid.
    pub fn validate(self, value: &Value) -> Vec<String> {
        match self.compiled().validate(value) {
            Ok(()) => Vec::new(),
            Err(errors) => errors
                .map(|e| format!("{}: {}", e.instance_path, e))
                .collect(),
        }
    }
}
