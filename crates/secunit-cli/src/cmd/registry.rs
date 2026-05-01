//! `secunit registry import` — promote drafts emitted by the bootstrap or
//! inventory-seed skills into the live registry.
//!
//! The skill writes draft files under `<run-dir>/raw/`; this command finds
//! them, validates each against the relevant JSON schema, and copies into
//! the live registry root. Existing files are preserved (idempotent
//! re-runs); `inventory.yaml` is *merged* — new entries are appended,
//! existing entries are left as the operator curated them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use secunit_core::schemas::Schema;
use serde_json::Value as JsonValue;
use serde_yaml::{Mapping, Value as YamlValue};

use super::Ctx;

#[derive(Debug, Default)]
struct ImportSummary {
    /// New control YAMLs written into `controls/`.
    added_controls: Vec<String>,
    /// Control YAMLs that already exist in the live registry; left untouched.
    kept_controls: Vec<String>,
    /// Top-level files written for the first time (`schedule.yaml`, `_config.yaml`).
    added_files: Vec<String>,
    /// Top-level files that already exist; left untouched.
    kept_files: Vec<String>,
    /// Whether `inventory.yaml` was created from scratch.
    inventory_created: bool,
    /// Inventory entries appended into existing kinds during merge: (kind, name).
    inventory_added: Vec<(String, String)>,
    /// New inventory kinds added wholesale during merge.
    inventory_added_kinds: Vec<String>,
    /// Drafts that failed schema validation; (path, errors).
    invalid: Vec<(PathBuf, Vec<String>)>,
}

impl ImportSummary {
    fn print_human(&self) {
        if self.added_controls.is_empty()
            && self.kept_controls.is_empty()
            && self.added_files.is_empty()
            && self.kept_files.is_empty()
            && !self.inventory_created
            && self.inventory_added.is_empty()
            && self.inventory_added_kinds.is_empty()
            && self.invalid.is_empty()
        {
            println!("No drafts found to import.");
            return;
        }
        if !self.added_controls.is_empty() {
            println!("Added controls ({}):", self.added_controls.len());
            for c in &self.added_controls {
                println!("  + {c}");
            }
        }
        if !self.kept_controls.is_empty() {
            println!("Kept controls ({}):", self.kept_controls.len());
            for c in &self.kept_controls {
                println!("  = {c}");
            }
        }
        if self.inventory_created {
            println!("Created inventory.yaml from draft.");
        }
        if !self.inventory_added_kinds.is_empty() {
            println!(
                "Added inventory kinds ({}): {}",
                self.inventory_added_kinds.len(),
                self.inventory_added_kinds.join(", ")
            );
        }
        if !self.inventory_added.is_empty() {
            println!("Added inventory entries ({}):", self.inventory_added.len());
            for (k, n) in &self.inventory_added {
                println!("  + {k}/{n}");
            }
        }
        for f in &self.added_files {
            println!("Added {f} from draft.");
        }
        for f in &self.kept_files {
            println!("Kept existing {f} (draft ignored).");
        }
        if !self.invalid.is_empty() {
            println!("\nDrafts rejected ({}):", self.invalid.len());
            for (path, errs) in &self.invalid {
                println!("  ✗ {}", path.display());
                for e in errs {
                    println!("      {e}");
                }
            }
        }
    }

    fn to_json(&self) -> JsonValue {
        serde_json::json!({
            "added_controls": self.added_controls,
            "kept_controls": self.kept_controls,
            "added_files": self.added_files,
            "kept_files": self.kept_files,
            "inventory_created": self.inventory_created,
            "inventory_added": self.inventory_added.iter()
                .map(|(k, n)| serde_json::json!({"kind": k, "name": n}))
                .collect::<Vec<_>>(),
            "inventory_added_kinds": self.inventory_added_kinds,
            "invalid": self.invalid.iter().map(|(p, e)| serde_json::json!({
                "path": p,
                "errors": e,
            })).collect::<Vec<_>>(),
        })
    }
}

/// Locate the directory that actually holds the drafts. A bootstrap run
/// writes them under `<run-dir>/raw/` (since the run is org-wide and uses
/// the flat layout); a hand-assembled directory may have them at the
/// top level. Look for either, preferring `raw/` when both exist.
fn draft_root(src: &Path) -> PathBuf {
    let raw = src.join("raw");
    if raw.is_dir() {
        raw
    } else {
        src.to_path_buf()
    }
}

pub fn import(ctx: &Ctx, src_dir: &Path) -> Result<ExitCode> {
    let src = src_dir
        .canonicalize()
        .with_context(|| format!("resolve source dir {}", src_dir.display()))?;
    let root = ctx
        .root
        .canonicalize()
        .with_context(|| format!("resolve --root {}", ctx.root.display()))?;
    let drafts = draft_root(&src);
    let mut summary = ImportSummary::default();

    import_controls(&drafts, &root, &mut summary)?;
    import_inventory(&drafts, &root, &mut summary)?;
    for fname in ["schedule.yaml", "_config.yaml"] {
        let schema = if fname == "schedule.yaml" {
            Schema::Schedule
        } else {
            Schema::Config
        };
        import_top_level(&drafts, &root, fname, schema, &mut summary)?;
    }

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&summary.to_json())?);
    } else {
        summary.print_human();
    }
    Ok(if summary.invalid.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn import_controls(drafts: &Path, root: &Path, summary: &mut ImportSummary) -> Result<()> {
    let dir = drafts.join("controls");
    if !dir.is_dir() {
        return Ok(());
    }
    let dest_dir = root.join("controls");
    fs::create_dir_all(&dest_dir)?;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("read draft {}", path.display()))?;
        let value: JsonValue = match yaml_to_json(&text) {
            Ok(v) => v,
            Err(e) => {
                summary
                    .invalid
                    .push((path.clone(), vec![format!("yaml: {e}")]));
                continue;
            }
        };
        let errs = Schema::Control.validate(&value);
        if !errs.is_empty() {
            summary.invalid.push((path.clone(), errs));
            continue;
        }
        let fname = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("draft has no filename: {}", path.display()))?;
        let dest = dest_dir.join(fname);
        let label = fname.to_string_lossy().into_owned();
        if dest.exists() {
            summary.kept_controls.push(label);
            continue;
        }
        fs::write(&dest, &text).with_context(|| format!("write {}", dest.display()))?;
        summary.added_controls.push(label);
    }
    summary.added_controls.sort();
    summary.kept_controls.sort();
    Ok(())
}

fn import_inventory(drafts: &Path, root: &Path, summary: &mut ImportSummary) -> Result<()> {
    let src = drafts.join("inventory.yaml");
    if !src.is_file() {
        return Ok(());
    }
    let dest = root.join("inventory.yaml");
    let draft_text = fs::read_to_string(&src)?;
    let draft_yaml: YamlValue = match serde_yaml::from_str(&draft_text) {
        Ok(v) => v,
        Err(e) => {
            summary
                .invalid
                .push((src.clone(), vec![format!("yaml: {e}")]));
            return Ok(());
        }
    };
    // Validate the draft itself against the inventory schema.
    let draft_json = yaml_to_json_value(&draft_yaml)?;
    let errs = Schema::Inventory.validate(&draft_json);
    if !errs.is_empty() {
        summary.invalid.push((src.clone(), errs));
        return Ok(());
    }

    if !dest.exists() {
        fs::write(&dest, &draft_text)?;
        summary.inventory_created = true;
        return Ok(());
    }

    // Merge: append missing entries by name, leave existing entries alone.
    let live_text = fs::read_to_string(&dest)?;
    let live_yaml: YamlValue =
        serde_yaml::from_str(&live_text).with_context(|| format!("parse {}", dest.display()))?;
    let merged = merge_inventory(&live_yaml, &draft_yaml, summary);
    let merged_json = yaml_to_json_value(&merged)?;
    let errs = Schema::Inventory.validate(&merged_json);
    if !errs.is_empty() {
        summary.invalid.push((dest.clone(), errs));
        return Ok(());
    }
    let out = serde_yaml::to_string(&merged)?;
    fs::write(&dest, out).with_context(|| format!("write {}", dest.display()))?;
    Ok(())
}

fn import_top_level(
    drafts: &Path,
    root: &Path,
    fname: &str,
    schema: Schema,
    summary: &mut ImportSummary,
) -> Result<()> {
    let src = drafts.join(fname);
    if !src.is_file() {
        return Ok(());
    }
    let dest = root.join(fname);
    if dest.exists() {
        summary.kept_files.push(fname.to_string());
        return Ok(());
    }
    let text = fs::read_to_string(&src)?;
    let value: JsonValue = match yaml_to_json(&text) {
        Ok(v) => v,
        Err(e) => {
            summary.invalid.push((src, vec![format!("yaml: {e}")]));
            return Ok(());
        }
    };
    let errs = schema.validate(&value);
    if !errs.is_empty() {
        summary.invalid.push((src, errs));
        return Ok(());
    }
    fs::write(&dest, &text).with_context(|| format!("write {}", dest.display()))?;
    summary.added_files.push(fname.to_string());
    Ok(())
}

fn merge_inventory(live: &YamlValue, draft: &YamlValue, summary: &mut ImportSummary) -> YamlValue {
    let live_map = match live.as_mapping() {
        Some(m) => m.clone(),
        None => Mapping::new(),
    };
    let draft_map = match draft.as_mapping() {
        Some(m) => m,
        None => return YamlValue::Mapping(live_map),
    };

    let mut out = live_map.clone();
    for (kind_key, draft_entries) in draft_map {
        let kind_name = kind_key.as_str().unwrap_or_default().to_string();
        let draft_seq = match draft_entries.as_sequence() {
            Some(s) => s,
            None => continue,
        };
        match out.get(kind_key).cloned() {
            Some(YamlValue::Sequence(mut live_seq)) => {
                let live_names: std::collections::HashSet<String> = live_seq
                    .iter()
                    .filter_map(|e| e.as_mapping())
                    .filter_map(|m| m.get(YamlValue::String("name".into())))
                    .filter_map(|n| n.as_str())
                    .map(str::to_string)
                    .collect();
                for d in draft_seq {
                    let dname = d
                        .as_mapping()
                        .and_then(|m| m.get(YamlValue::String("name".into())))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if dname.is_empty() || live_names.contains(dname) {
                        continue;
                    }
                    live_seq.push(d.clone());
                    summary
                        .inventory_added
                        .push((kind_name.clone(), dname.to_string()));
                }
                out.insert(kind_key.clone(), YamlValue::Sequence(live_seq));
            }
            _ => {
                // Kind missing in live registry — copy wholesale.
                out.insert(kind_key.clone(), draft_entries.clone());
                summary.inventory_added_kinds.push(kind_name.clone());
                for d in draft_seq {
                    if let Some(name) = d
                        .as_mapping()
                        .and_then(|m| m.get(YamlValue::String("name".into())))
                        .and_then(|v| v.as_str())
                    {
                        summary
                            .inventory_added
                            .push((kind_name.clone(), name.to_string()));
                    }
                }
            }
        }
    }
    YamlValue::Mapping(out)
}

fn yaml_to_json(text: &str) -> Result<JsonValue> {
    let yaml: YamlValue = serde_yaml::from_str(text)?;
    yaml_to_json_value(&yaml)
}

fn yaml_to_json_value(yaml: &YamlValue) -> Result<JsonValue> {
    let s = serde_json::to_string(yaml)?;
    Ok(serde_json::from_str(&s)?)
}
