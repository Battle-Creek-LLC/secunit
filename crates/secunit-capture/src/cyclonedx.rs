//! CycloneDX BOM normalization and schema validation.
//!
//! The SBOM capturer shells out to a per-ecosystem generator and each
//! one emits the highest CycloneDX version it supports — 1.6 for
//! `cyclonedx-py` and `cyclonedx-npm`, 1.5 for `cargo-cyclonedx`, whose
//! `--spec-version` accepts only `1.3`, `1.4`, or `1.5`. Rather than
//! pin every ecosystem to the weakest tool, the artifact is validated
//! against whatever `specVersion` it declares; both schemas are
//! vendored under `schemas/cyclonedx/`.
//!
//! ## Why the schemas are vendored
//!
//! `bom-1.x.schema.json` `$ref`s two sibling documents
//! (`spdx.schema.json`, `jsf-0.82.schema.json`). `jsonschema` is
//! compiled here with `default-features = false`, so neither the HTTP
//! nor the file resolver is available — the siblings are registered as
//! in-memory documents under the URLs the relative refs resolve to.
//! Vendoring also keeps validation working offline, which an evidence
//! capture run must not depend on.
//!
//! ## Determinism
//!
//! CycloneDX emits two fields that change on every generation:
//! `serialNumber` (a random `urn:uuid`) and `metadata.timestamp`.
//! Left in place they make every run's SBOM hash differently, so the
//! prior-run diff in a sweep reports change even when no dependency
//! moved. [`normalize`] strips both and sorts the component and
//! dependency arrays. `cyclonedx-py` and `cyclonedx-npm` are also
//! invoked with `--output-reproducible`, which suppresses the two
//! fields upstream; `cargo-cyclonedx` has no such flag, so the strip
//! is what makes the Rust arm reproducible.

use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use jsonschema::{Draft, JSONSchema};
use serde_json::Value;

use crate::canonical::canonicalize_value;

/// CycloneDX spec versions this crate can validate, lowest first.
pub const SUPPORTED_SPEC_VERSIONS: &[&str] = &["1.5", "1.6"];

/// Base URL the vendored schemas' relative `$ref`s resolve against.
const SCHEMA_BASE: &str = "http://cyclonedx.org/schema/";

/// Compile a vendored CycloneDX schema with its two sibling documents
/// pre-registered.
///
/// The CycloneDX schemas declare draft-07, not the 2020-12 draft the
/// registry schemas in [`crate::schema`] use, so the draft is set
/// explicitly rather than inherited.
fn compile(raw: &str, name: &str) -> JSONSchema {
    let json: Value = serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("cyclonedx schema {name}: invalid JSON: {e}"));

    let spdx: Value = serde_json::from_str(include_str!("../schemas/cyclonedx/spdx.schema.json"))
        .expect("vendored spdx.schema.json is valid JSON");
    let jsf: Value =
        serde_json::from_str(include_str!("../schemas/cyclonedx/jsf-0.82.schema.json"))
            .expect("vendored jsf-0.82.schema.json is valid JSON");

    JSONSchema::options()
        .with_draft(Draft::Draft7)
        .with_document(format!("{SCHEMA_BASE}spdx.schema.json"), spdx)
        .with_document(format!("{SCHEMA_BASE}jsf-0.82.schema.json"), jsf)
        .compile(&json)
        .unwrap_or_else(|e| panic!("cyclonedx schema {name}: compile failed: {e}"))
}

fn schema_for(spec_version: &str) -> Option<&'static JSONSchema> {
    match spec_version {
        "1.5" => {
            static CELL: OnceLock<JSONSchema> = OnceLock::new();
            Some(CELL.get_or_init(|| {
                compile(
                    include_str!("../schemas/cyclonedx/bom-1.5.schema.json"),
                    "bom-1.5",
                )
            }))
        }
        "1.6" => {
            static CELL: OnceLock<JSONSchema> = OnceLock::new();
            Some(CELL.get_or_init(|| {
                compile(
                    include_str!("../schemas/cyclonedx/bom-1.6.schema.json"),
                    "bom-1.6",
                )
            }))
        }
        _ => None,
    }
}

/// The `specVersion` a BOM declares.
///
/// Errors when the field is missing or is not one of
/// [`SUPPORTED_SPEC_VERSIONS`] — an unrecognized version means the
/// generator was upgraded past what this build vendors, which is an
/// operator-visible problem, not something to validate loosely.
pub fn spec_version(bom: &Value) -> Result<&str> {
    let declared = bom
        .get("specVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("BOM has no `specVersion` field — not a CycloneDX document"))?;
    if !SUPPORTED_SPEC_VERSIONS.contains(&declared) {
        return Err(anyhow!(
            "BOM declares CycloneDX {declared}, which this build cannot validate \
             (vendored: {}). Pin the generator to a supported version, or vendor \
             the {declared} schema.",
            SUPPORTED_SPEC_VERSIONS.join(", ")
        ));
    }
    Ok(declared)
}

/// Validate `bom` against the vendored schema for its declared
/// `specVersion`.
///
/// Returns the jsonschema diagnostics on mismatch — empty when valid.
pub fn validate(bom: &Value) -> Result<Vec<String>> {
    let version = spec_version(bom)?;
    let schema =
        schema_for(version).ok_or_else(|| anyhow!("no vendored schema for CycloneDX {version}"))?;
    let errs = match schema.validate(bom) {
        Ok(()) => Vec::new(),
        Err(it) => it.map(|e| format!("{}: {}", e.instance_path, e)).collect(),
    };
    Ok(errs)
}

/// Strip the two per-generation fields and sort every array whose order
/// carries no meaning, so repeated captures of an unchanged tree are
/// byte-identical.
pub fn normalize(bom: &mut Value) {
    let Some(obj) = bom.as_object_mut() else {
        return;
    };

    obj.remove("serialNumber");
    if let Some(meta) = obj.get_mut("metadata").and_then(|m| m.as_object_mut()) {
        meta.remove("timestamp");
    }

    // `purl` is the stable cross-generator identity; fall back to
    // name+version for components that carry none (some generators omit
    // it for the root component's local targets).
    if let Some(components) = obj.get_mut("components").and_then(|c| c.as_array_mut()) {
        components.sort_by_key(component_sort_key);
    }

    if let Some(deps) = obj.get_mut("dependencies").and_then(|d| d.as_array_mut()) {
        for dep in deps.iter_mut() {
            if let Some(on) = dep.get_mut("dependsOn").and_then(|o| o.as_array_mut()) {
                on.sort_by_key(|v| v.as_str().unwrap_or("").to_string());
            }
        }
        deps.sort_by_key(|d| {
            d.get("ref")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string()
        });
    }

    *bom = canonicalize_value(bom.take());
}

fn component_sort_key(c: &Value) -> String {
    if let Some(purl) = c.get("purl").and_then(|p| p.as_str()) {
        return purl.to_string();
    }
    format!(
        "{}@{}",
        c.get("name").and_then(|n| n.as_str()).unwrap_or(""),
        c.get("version").and_then(|v| v.as_str()).unwrap_or("")
    )
}

/// The generators named in `metadata.tools`, as `(name, version)`.
///
/// CycloneDX changed this field's shape: 1.5 and earlier use a bare
/// array of `{vendor, name, version}`, 1.6 nests them under
/// `tools.components`. Both are read so the SBOM's provenance lands in
/// the sidecar envelope regardless of which generator produced it.
pub fn generators(bom: &Value) -> Vec<(String, String)> {
    let Some(tools) = bom.get("metadata").and_then(|m| m.get("tools")) else {
        return Vec::new();
    };
    let list = match tools {
        Value::Array(a) => a.as_slice(),
        Value::Object(o) => match o.get("components").and_then(|c| c.as_array()) {
            Some(a) => a.as_slice(),
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    let mut out: Vec<(String, String)> = list
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?.to_string();
            let version = t
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some((name, version))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_bom(spec: &str) -> Value {
        json!({
            "bomFormat": "CycloneDX",
            "specVersion": spec,
            "version": 1,
            "serialNumber": "urn:uuid:3a02ad18-cacd-4640-995e-eb3b53c86a00",
            "metadata": { "timestamp": "2026-08-13T01:17:12.846344+00:00" },
            "components": [
                {"type": "library", "name": "ryu", "version": "1.0.23", "purl": "pkg:cargo/ryu@1.0.23"},
                {"type": "library", "name": "itoa", "version": "1.0.18", "purl": "pkg:cargo/itoa@1.0.18"}
            ]
        })
    }

    #[test]
    fn both_vendored_schemas_compile_and_accept_a_minimal_bom() {
        for v in SUPPORTED_SPEC_VERSIONS {
            let mut bom = minimal_bom(v);
            normalize(&mut bom);
            let errs = validate(&bom).expect("schema lookup");
            assert!(
                errs.is_empty(),
                "CycloneDX {v} rejected a minimal BOM: {errs:?}"
            );
        }
    }

    #[test]
    fn normalize_strips_serial_number_and_timestamp() {
        let mut bom = minimal_bom("1.6");
        normalize(&mut bom);
        assert!(bom.get("serialNumber").is_none());
        assert!(bom["metadata"].get("timestamp").is_none());
    }

    #[test]
    fn normalize_sorts_components_by_purl() {
        let mut bom = minimal_bom("1.6");
        normalize(&mut bom);
        let names: Vec<&str> = bom["components"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["itoa", "ryu"]);
    }

    #[test]
    fn normalize_is_idempotent_and_order_independent() {
        let mut a = minimal_bom("1.6");
        let mut b = minimal_bom("1.6");
        // Reverse b's components and give it a different serial and
        // timestamp: both must normalize to the same bytes.
        b["components"].as_array_mut().unwrap().reverse();
        b["serialNumber"] = json!("urn:uuid:deadbeef-cafe-4bad-8f01-feedfacec0de");
        b["metadata"]["timestamp"] = json!("2020-01-01T00:00:00Z");
        normalize(&mut a);
        normalize(&mut b);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );

        let once = serde_json::to_string(&a).unwrap();
        normalize(&mut a);
        assert_eq!(
            once,
            serde_json::to_string(&a).unwrap(),
            "normalize must be idempotent"
        );
    }

    #[test]
    fn unsupported_spec_version_is_a_named_error() {
        let bom = minimal_bom("1.7");
        let err = spec_version(&bom).expect_err("1.7 is not vendored");
        let msg = format!("{err}");
        assert!(msg.contains("1.7"), "names the declared version: {msg}");
        assert!(msg.contains("1.5, 1.6"), "names what is vendored: {msg}");
    }

    #[test]
    fn missing_spec_version_is_rejected() {
        let bom = json!({"bomFormat": "CycloneDX"});
        assert!(spec_version(&bom).is_err());
    }

    #[test]
    fn generators_reads_both_the_1_5_and_1_6_tools_shapes() {
        let legacy = json!({"metadata": {"tools": [
            {"vendor": "CycloneDX", "name": "cargo-cyclonedx", "version": "0.5.9"}
        ]}});
        assert_eq!(
            generators(&legacy),
            vec![("cargo-cyclonedx".to_string(), "0.5.9".to_string())]
        );

        let modern = json!({"metadata": {"tools": {"components": [
            {"name": "cyclonedx-py", "version": "7.3.1"},
            {"name": "cyclonedx-python-lib", "version": "11.11.2"}
        ]}}});
        assert_eq!(
            generators(&modern),
            vec![
                ("cyclonedx-py".to_string(), "7.3.1".to_string()),
                ("cyclonedx-python-lib".to_string(), "11.11.2".to_string()),
            ]
        );
    }

    #[test]
    fn schema_rejects_a_bom_with_a_bad_component_type() {
        let mut bom = minimal_bom("1.6");
        bom["components"][0]["type"] = json!("not-a-valid-type");
        normalize(&mut bom);
        let errs = validate(&bom).unwrap();
        assert!(!errs.is_empty(), "expected the enum constraint to fire");
    }
}
