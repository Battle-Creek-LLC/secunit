//! `secunit inventory list / add / retire / check`.
//!
//! Edits go through `serde_yaml::Value` rather than the typed `Inventory`
//! model so that fields the CLI does not know about (kind-specific extras
//! like `provider`, `profile`, `address`, ...) survive a round-trip. We
//! still validate the post-edit document against `inventory.schema.json`
//! before writing.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use secunit_core::schemas::Schema;
use serde_json::Value as JsonValue;
use serde_yaml::{Mapping, Sequence, Value as YamlValue};

use super::Ctx;

pub fn list(ctx: &Ctx, kind: Option<&str>) -> Result<ExitCode> {
    let (reg, _report) = ctx.load()?;
    let mut sections: Vec<(String, Vec<&secunit_core::model::InventoryEntry>)> = Vec::new();
    for (k, entries) in &reg.inventory.kinds {
        if let Some(filter) = kind {
            // Match either the canonical (plural) or singular form.
            if k != filter && k != &format!("{filter}s") {
                continue;
            }
        }
        sections.push((k.clone(), entries.iter().collect()));
    }

    if ctx.json {
        let payload: JsonValue = serde_json::json!(sections
            .iter()
            .map(|(k, es)| serde_json::json!({
                "kind": k,
                "entries": es.iter().map(|e| serde_json::json!({
                    "name": e.name,
                    "tags": e.tags,
                    "in_scope_since": e.in_scope_since,
                    "retired_on": e.retired_on,
                    "aliases": e.aliases,
                    "excludes": e.excludes,
                    "extras": e.extras,
                })).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>());
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }

    if sections.is_empty() {
        match kind {
            Some(k) => println!("No inventory section matches `{k}`."),
            None => println!("Inventory is empty."),
        }
        return Ok(ExitCode::SUCCESS);
    }

    for (k, entries) in &sections {
        println!("[{k}] ({} entries)", entries.len());
        if entries.is_empty() {
            continue;
        }
        println!("  {:<24} {:<14} {:<14} TAGS", "NAME", "IN_SCOPE", "RETIRED");
        for e in entries {
            let in_scope = e
                .in_scope_since
                .map(|d| d.to_string())
                .unwrap_or_else(|| "-".into());
            let retired = e
                .retired_on
                .map(|d| d.to_string())
                .unwrap_or_else(|| "-".into());
            let tags = if e.tags.is_empty() {
                "-".to_string()
            } else {
                e.tags.join(",")
            };
            println!("  {:<24} {:<14} {:<14} {}", e.name, in_scope, retired, tags);
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub fn add(
    ctx: &Ctx,
    kind: &str,
    name: &str,
    tags: &[String],
    url: Option<&str>,
) -> Result<ExitCode> {
    let path = inventory_path(ctx);
    let (mut top, kind_key) = load_or_init(&path, kind)?;

    // Sequence under that kind.
    let mut seq = match top.get(&kind_key).cloned() {
        Some(YamlValue::Sequence(s)) => s,
        Some(_) => bail!(
            "{} has non-sequence value under `{}` — refusing to mutate",
            path.display(),
            kind_key.as_str().unwrap_or("?")
        ),
        None => Sequence::new(),
    };

    // Refuse duplicates within the kind.
    for entry in &seq {
        if let Some(m) = entry.as_mapping() {
            if let Some(YamlValue::String(existing)) = m.get(YamlValue::String("name".into())) {
                if existing == name {
                    bail!(
                        "entry `{}` already exists under kind `{}`",
                        name,
                        kind_key.as_str().unwrap_or("?")
                    );
                }
            }
        }
    }

    let mut entry = Mapping::new();
    entry.insert(
        YamlValue::String("name".into()),
        YamlValue::String(name.into()),
    );
    if let Some(u) = url {
        entry.insert(YamlValue::String("url".into()), YamlValue::String(u.into()));
    }
    if !tags.is_empty() {
        entry.insert(
            YamlValue::String("tags".into()),
            YamlValue::Sequence(
                tags.iter()
                    .map(|t| YamlValue::String(t.clone()))
                    .collect::<Sequence>(),
            ),
        );
    }
    entry.insert(
        YamlValue::String("in_scope_since".into()),
        YamlValue::String(ctx.today.to_string()),
    );

    seq.push(YamlValue::Mapping(entry));
    top.insert(kind_key.clone(), YamlValue::Sequence(seq));
    write_validated(&path, &YamlValue::Mapping(top))?;
    println!(
        "added {}/{} (in_scope_since {})",
        kind_key.as_str().unwrap_or("?"),
        name,
        ctx.today
    );
    Ok(ExitCode::SUCCESS)
}

pub fn retire(ctx: &Ctx, kind: &str, name: &str, on: NaiveDate, reason: &str) -> Result<ExitCode> {
    let path = inventory_path(ctx);
    let (mut top, kind_key) = load_or_init(&path, kind)?;

    let mut seq = match top.get(&kind_key).cloned() {
        Some(YamlValue::Sequence(s)) => s,
        _ => bail!(
            "kind `{}` not present in {}",
            kind_key.as_str().unwrap_or("?"),
            path.display()
        ),
    };

    let mut hit = false;
    for entry in seq.iter_mut() {
        let map = match entry.as_mapping_mut() {
            Some(m) => m,
            None => continue,
        };
        let matches = map
            .get(YamlValue::String("name".into()))
            .and_then(|v| v.as_str())
            .map(|n| n == name)
            .unwrap_or(false);
        if !matches {
            continue;
        }
        if let Some(existing) = map
            .get(YamlValue::String("retired_on".into()))
            .and_then(|v| v.as_str())
        {
            bail!(
                "entry `{}/{}` already has retired_on={}",
                kind_key.as_str().unwrap_or("?"),
                name,
                existing
            );
        }
        map.insert(
            YamlValue::String("retired_on".into()),
            YamlValue::String(on.to_string()),
        );
        // `reason` is preserved in YAML alongside the entry; not in the
        // schema, so store it as a comment-like sibling field that
        // additional-properties allow.
        map.insert(
            YamlValue::String("retired_reason".into()),
            YamlValue::String(reason.to_string()),
        );
        hit = true;
        break;
    }
    if !hit {
        bail!(
            "no entry named `{}` under kind `{}` in {}",
            name,
            kind_key.as_str().unwrap_or("?"),
            path.display()
        );
    }

    top.insert(kind_key.clone(), YamlValue::Sequence(seq));
    write_validated(&path, &YamlValue::Mapping(top))?;
    println!(
        "retired {}/{} on {} ({})",
        kind_key.as_str().unwrap_or("?"),
        name,
        on,
        reason
    );
    Ok(ExitCode::SUCCESS)
}

pub fn check(ctx: &Ctx) -> Result<ExitCode> {
    let path = inventory_path(ctx);
    if !path.exists() {
        if ctx.json {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"errors": [], "warnings": ["inventory.yaml not present"]})
                )?
            );
        } else {
            println!("inventory.yaml not present at {}", path.display());
        }
        return Ok(ExitCode::SUCCESS);
    }
    let text = fs::read_to_string(&path)?;
    let yaml: YamlValue = serde_yaml::from_str(&text)?;
    let json = yaml_to_json_value(&yaml)?;

    let mut errors: Vec<String> = Schema::Inventory.validate(&json);
    let mut warnings: Vec<String> = Vec::new();

    if let Some(map) = yaml.as_mapping() {
        for (k, v) in map {
            let kind = k.as_str().unwrap_or("?");
            let seq = match v.as_sequence() {
                Some(s) => s,
                None => {
                    errors.push(format!("kind `{kind}`: not a sequence"));
                    continue;
                }
            };
            let mut seen = BTreeSet::new();
            for (idx, entry) in seq.iter().enumerate() {
                let m = match entry.as_mapping() {
                    Some(m) => m,
                    None => {
                        errors.push(format!("kind `{kind}` entry #{idx}: not a mapping"));
                        continue;
                    }
                };
                let name = m
                    .get(YamlValue::String("name".into()))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if name.is_empty() {
                    errors.push(format!("kind `{kind}` entry #{idx}: missing `name`"));
                    continue;
                }
                if !seen.insert(name.to_string()) {
                    errors.push(format!("kind `{kind}`: duplicate name `{name}`"));
                }
                let in_scope = m
                    .get(YamlValue::String("in_scope_since".into()))
                    .and_then(|v| v.as_str())
                    .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"));
                let retired = m
                    .get(YamlValue::String("retired_on".into()))
                    .and_then(|v| v.as_str())
                    .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"));
                if let (Some(Ok(s)), Some(Ok(e))) = (in_scope, retired) {
                    if e <= s {
                        errors.push(format!(
                            "kind `{kind}` `{name}`: retired_on ({e}) must be after in_scope_since ({s})"
                        ));
                    }
                }
                if m.get(YamlValue::String("in_scope_since".into())).is_none() {
                    warnings.push(format!("kind `{kind}` `{name}`: missing in_scope_since"));
                }
            }
        }
    }

    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "errors": errors,
                "warnings": warnings,
            }))?
        );
    } else {
        for w in &warnings {
            println!("warn  {w}");
        }
        for e in &errors {
            println!("error {e}");
        }
        if errors.is_empty() {
            println!("✓ inventory ok ({} warning(s))", warnings.len());
        } else {
            println!(
                "✗ inventory has {} error(s), {} warning(s)",
                errors.len(),
                warnings.len()
            );
        }
    }
    Ok(if errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

// ---------- helpers --------------------------------------------------------

fn inventory_path(ctx: &Ctx) -> PathBuf {
    ctx.root.join("inventory.yaml")
}

/// Load `inventory.yaml` (or initialise an empty mapping) and pick the
/// canonical key under which the given user-supplied `kind` lives.
/// Convention: `--kind source_repo` lives at `source_repos:`. If neither
/// the singular nor plural form exists yet, default to plural.
fn load_or_init(path: &PathBuf, kind: &str) -> Result<(Mapping, YamlValue)> {
    let yaml: YamlValue = if path.exists() {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        serde_yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))?
    } else {
        YamlValue::Mapping(Mapping::new())
    };
    let map = yaml
        .as_mapping()
        .cloned()
        .ok_or_else(|| anyhow!("{}: top level is not a mapping", path.display()))?;
    let plural = format!("{kind}s");
    let key = if map.contains_key(YamlValue::String(kind.to_string())) {
        YamlValue::String(kind.to_string())
    } else if map.contains_key(YamlValue::String(plural.clone())) {
        YamlValue::String(plural)
    } else {
        YamlValue::String(format!("{kind}s"))
    };
    Ok((map, key))
}

fn write_validated(path: &PathBuf, doc: &YamlValue) -> Result<()> {
    let json = yaml_to_json_value(doc)?;
    let errs = Schema::Inventory.validate(&json);
    if !errs.is_empty() {
        bail!(
            "post-edit inventory fails schema validation: {}",
            errs.join("; ")
        );
    }
    let text = serde_yaml::to_string(doc)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn yaml_to_json_value(yaml: &YamlValue) -> Result<JsonValue> {
    let s = serde_json::to_string(yaml)?;
    Ok(serde_json::from_str(&s)?)
}
