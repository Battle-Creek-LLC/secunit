//! Walk an org root and produce a `LoadedRegistry` plus per-file diagnostics.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::{Config, Control, Inventory, LoadedRegistry, Schedule, State};
use crate::schemas::Schema;

/// Aggregated load + validation report. `errors` is fatal; `warnings` is
/// informational (e.g. a control YAML parsed but a referenced policy file
/// is missing).
#[derive(Debug, Default)]
pub struct LoadReport {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub message: String,
}

impl LoadReport {
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }

    fn err(&mut self, path: impl Into<PathBuf>, message: impl Into<String>) {
        self.errors.push(Diagnostic {
            path: path.into(),
            message: message.into(),
        });
    }

    fn warn(&mut self, path: impl Into<PathBuf>, message: impl Into<String>) {
        self.warnings.push(Diagnostic {
            path: path.into(),
            message: message.into(),
        });
    }
}

/// Load the org tree at `root`. Returns the registry even when soft-validation
/// surfaces issues; check `report.is_clean()` before relying on it.
pub fn load(root: &Path) -> (LoadedRegistry, LoadReport) {
    let mut report = LoadReport::default();
    let controls = load_controls(root, &mut report);
    let inventory = load_inventory(root, &mut report);
    let schedule = load_schedule(root, &mut report);
    let state = load_state(root, &mut report);
    let config = load_config(root, &mut report);
    cross_check(root, &controls, &inventory, &schedule, &mut report);
    let registry = LoadedRegistry {
        root: root.to_path_buf(),
        controls,
        inventory,
        schedule,
        state,
        config,
    };
    (registry, report)
}

fn load_controls(root: &Path, report: &mut LoadReport) -> BTreeMap<String, Control> {
    let dir = root.join("controls");
    let mut out = BTreeMap::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            report.err(&dir, format!("read controls/: {e}"));
            return out;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        let value: Value = match read_yaml(&path) {
            Ok(v) => v,
            Err(e) => {
                report.err(&path, e);
                continue;
            }
        };
        for msg in Schema::Control.validate(&value) {
            report.err(&path, format!("schema: {msg}"));
        }
        let control: Control = match serde_json::from_value(value) {
            Ok(c) => c,
            Err(e) => {
                report.err(&path, format!("deserialise: {e}"));
                continue;
            }
        };
        let expected = format!(
            "{}.yaml",
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
        );
        if expected != format!("{}.yaml", control.id) {
            report.warn(
                &path,
                format!(
                    "filename `{}` does not match control id `{}`",
                    expected, control.id
                ),
            );
        }
        if let Some(existing) = out.insert(control.id.clone(), control.clone()) {
            report.err(
                &path,
                format!(
                    "duplicate control id `{}` (also in earlier file)",
                    existing.id
                ),
            );
        }
    }
    out
}

fn load_inventory(root: &Path, report: &mut LoadReport) -> Inventory {
    let path = root.join("inventory.yaml");
    if !path.exists() {
        return Inventory::default();
    }
    let value: Value = match read_yaml(&path) {
        Ok(v) => v,
        Err(e) => {
            report.err(&path, e);
            return Inventory::default();
        }
    };
    for msg in Schema::Inventory.validate(&value) {
        report.err(&path, format!("schema: {msg}"));
    }
    match serde_json::from_value::<Inventory>(value) {
        Ok(inv) => {
            for (kind, entries) in &inv.kinds {
                let mut seen = std::collections::HashSet::new();
                for e in entries {
                    if !seen.insert(e.name.clone()) {
                        report.err(
                            &path,
                            format!(
                                "duplicate name `{}` within inventory kind `{}`",
                                e.name, kind
                            ),
                        );
                    }
                }
            }
            inv
        }
        Err(e) => {
            report.err(&path, format!("deserialise: {e}"));
            Inventory::default()
        }
    }
}

fn load_schedule(root: &Path, report: &mut LoadReport) -> Schedule {
    let path = root.join("schedule.yaml");
    if !path.exists() {
        return Schedule::default();
    }
    let value: Value = match read_yaml(&path) {
        Ok(v) => v,
        Err(e) => {
            report.err(&path, e);
            return Schedule::default();
        }
    };
    for msg in Schema::Schedule.validate(&value) {
        report.err(&path, format!("schema: {msg}"));
    }
    serde_json::from_value(value).unwrap_or_else(|e| {
        report.err(&path, format!("deserialise: {e}"));
        Schedule::default()
    })
}

fn load_state(root: &Path, report: &mut LoadReport) -> State {
    let path = root.join("state.json");
    if !path.exists() {
        return State::default();
    }
    let value: Value = match read_json(&path) {
        Ok(v) => v,
        Err(e) => {
            report.err(&path, e);
            return State::default();
        }
    };
    for msg in Schema::State.validate(&value) {
        report.err(&path, format!("schema: {msg}"));
    }
    serde_json::from_value(value).unwrap_or_else(|e| {
        report.err(&path, format!("deserialise: {e}"));
        State::default()
    })
}

fn load_config(root: &Path, report: &mut LoadReport) -> Config {
    let path = root.join("_config.yaml");
    if !path.exists() {
        return Config::default();
    }
    let value: Value = match read_yaml(&path) {
        Ok(v) => v,
        Err(e) => {
            report.err(&path, e);
            return Config::default();
        }
    };
    for msg in Schema::Config.validate(&value) {
        report.err(&path, format!("schema: {msg}"));
    }
    serde_json::from_value(value).unwrap_or_else(|e| {
        report.err(&path, format!("deserialise: {e}"));
        Config::default()
    })
}

/// Cross-file checks the per-file loaders cannot do on their own.
fn cross_check(
    root: &Path,
    controls: &BTreeMap<String, Control>,
    inventory: &Inventory,
    schedule: &Schedule,
    report: &mut LoadReport,
) {
    let skills_dir = root.join("skills");
    for (id, ctrl) in controls {
        let skill_path = skills_dir.join(format!("{}.md", ctrl.skill));
        if !skill_path.exists() {
            report.warn(
                root.join("controls").join(format!("{id}.yaml")),
                format!(
                    "skill `{}` not found at {}",
                    ctrl.skill,
                    skill_path.display()
                ),
            );
        }
        if let Some(crate::model::Scope::Inventory(scope)) = &ctrl.scope {
            if inventory.section_for(&scope.kind).is_none() && !inventory.kinds.is_empty() {
                report.err(
                    root.join("controls").join(format!("{id}.yaml")),
                    format!(
                        "scope.kind `{}` matches no inventory section (have: {:?})",
                        scope.kind,
                        inventory.kinds.keys().collect::<Vec<_>>()
                    ),
                );
            }
        }
        let policy_path = root.join(&ctrl.policy);
        if !policy_path.exists() && !ctrl.policy.starts_with("http") {
            report.warn(
                root.join("controls").join(format!("{id}.yaml")),
                format!("policy file not found at {}", policy_path.display()),
            );
        }
    }
    for ov in &schedule.overrides {
        if !controls.contains_key(&ov.control_id) {
            report.err(
                root.join("schedule.yaml"),
                format!("override references unknown control `{}`", ov.control_id),
            );
        }
    }
}

fn read_yaml(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    serde_yaml::from_str(&text).map_err(|e| format!("yaml: {e}"))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("json: {e}"))
}
