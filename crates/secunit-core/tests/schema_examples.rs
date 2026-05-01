//! Validate every YAML/JSON file under `docs/examples/` against the
//! published schemas. If an example drifts from the schema (or vice
//! versa) this test fails loudly.

use std::path::{Path, PathBuf};

use secunit_core::schemas::Schema;
use serde_json::Value;

fn examples_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/secunit-core; repo root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/examples")
        .canonicalize()
        .expect("docs/examples must exist relative to the secunit-core crate")
}

fn read_yaml(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse YAML {path:?}: {e}"))
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse JSON {path:?}: {e}"))
}

fn assert_valid(schema: Schema, path: &Path, value: &Value) {
    let errs = schema.validate(value);
    assert!(
        errs.is_empty(),
        "{path:?} failed {schema:?} schema:\n  {}",
        errs.join("\n  ")
    );
}

#[test]
fn controls_validate() {
    let root = examples_root().join("controls");
    let mut count = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        assert_valid(Schema::Control, &path, &read_yaml(&path));
        count += 1;
    }
    assert!(count > 0, "no control examples found under {root:?}");
}

#[test]
fn inventory_validates() {
    let path = examples_root().join("inventory.yaml");
    assert_valid(Schema::Inventory, &path, &read_yaml(&path));
}

#[test]
fn schedule_validates() {
    let path = examples_root().join("schedule.yaml");
    assert_valid(Schema::Schedule, &path, &read_yaml(&path));
}

#[test]
fn state_validates() {
    let path = examples_root().join("state.json");
    assert_valid(Schema::State, &path, &read_json(&path));
}

#[test]
fn manifests_validate() {
    let root = examples_root().join("evidence");
    let mut count = 0;
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.unwrap();
        if entry.file_name() == "manifest.json" {
            assert_valid(Schema::Manifest, entry.path(), &read_json(entry.path()));
            count += 1;
        }
    }
    assert!(count > 0, "no manifest examples found under {root:?}");
}
