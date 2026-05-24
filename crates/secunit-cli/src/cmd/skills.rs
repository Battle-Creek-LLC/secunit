//! `secunit skills` — inspect and resolve runbook skills.
//!
//! Skills resolve local-first, then bundled (see `secunit_core::skills`).
//! `show` is the resolver the agent / `/ciso` front door uses to load a
//! runbook by name, so a release ships updated skills with no install
//! step. `list` reports the union of the bundled standard library and any
//! local overrides under `<root>/skills/`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use secunit_core::skills;

use super::Ctx;

struct Row {
    name: String,
    source: &'static str,
    description: String,
    requires_features: Vec<String>,
}

/// Resolve every skill name available to this root, local overriding
/// bundled, sorted by name.
fn collect(root: &std::path::Path) -> Vec<Row> {
    // BTreeMap keeps names sorted and de-duplicates local-over-bundled.
    let mut names: BTreeMap<String, ()> = BTreeMap::new();
    for s in skills::BUNDLED {
        names.insert(s.name.to_string(), ());
    }
    if let Ok(entries) = fs::read_dir(root.join("skills")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.insert(stem.to_string(), ());
                }
            }
        }
    }
    names
        .into_keys()
        .filter_map(|name| {
            let r = skills::resolve(root, &name)?;
            Some(Row {
                name: r.name,
                source: r.source.as_str(),
                description: skills::description(&r.body).unwrap_or_default(),
                requires_features: skills::requires_features(&r.body),
            })
        })
        .collect()
}

/// `secunit skills list` — bundled standard library + local overrides.
pub fn list(ctx: &Ctx) -> Result<ExitCode> {
    let root = ctx.root.canonicalize().unwrap_or_else(|_| ctx.root.clone());
    let rows = collect(&root);
    if ctx.json {
        let payload: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "source": r.source,
                    "requires_features": r.requires_features,
                    "description": r.description,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "skills": payload }))?
        );
        return Ok(ExitCode::SUCCESS);
    }
    if rows.is_empty() {
        println!("No skills available.");
        return Ok(ExitCode::SUCCESS);
    }
    println!("{:<22} {:<8} DESCRIPTION", "NAME", "SOURCE");
    for r in &rows {
        println!(
            "{:<22} {:<8} {}",
            r.name,
            r.source,
            first_sentence(&r.description)
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// `secunit skills show <name>` — print the resolved skill to stdout.
pub fn show(ctx: &Ctx, name: &str) -> Result<ExitCode> {
    let root = ctx.root.canonicalize().unwrap_or_else(|_| ctx.root.clone());
    let resolved = skills::resolve(&root, name).ok_or_else(|| {
        anyhow!(
            "unknown skill `{name}` (no local skills/{name}.md and not bundled). \
             Run `secunit skills list` to see what's available."
        )
    })?;
    print!("{}", resolved.body);
    if !resolved.body.ends_with('\n') {
        println!();
    }
    Ok(ExitCode::SUCCESS)
}

/// `secunit skills path <name>` — print a filesystem path to the skill.
/// Local skills resolve to their file; bundled skills are materialised
/// into a cache dir so tooling that needs a real path can read them.
pub fn path(ctx: &Ctx, name: &str) -> Result<ExitCode> {
    let root = ctx.root.canonicalize().unwrap_or_else(|_| ctx.root.clone());
    let resolved = skills::resolve(&root, name).ok_or_else(|| {
        anyhow!("unknown skill `{name}` (no local skills/{name}.md and not bundled)")
    })?;
    let p = match resolved.path {
        Some(p) => p,
        None => materialize(name, &resolved.body)?,
    };
    println!("{}", p.display());
    Ok(ExitCode::SUCCESS)
}

/// Write a bundled skill into a per-version cache dir and return its path.
/// Only rewrites when the cached copy differs, so the path is stable.
fn materialize(name: &str, body: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir()
        .join("secunit-skills")
        .join(env!("CARGO_PKG_VERSION"));
    fs::create_dir_all(&dir).with_context(|| format!("create cache dir {}", dir.display()))?;
    let path = dir.join(format!("{name}.md"));
    let needs_write = fs::read_to_string(&path).map(|c| c != body).unwrap_or(true);
    if needs_write {
        fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(path)
}

/// First sentence (or first ~80 chars) of a description, for the table.
fn first_sentence(desc: &str) -> String {
    let trimmed = desc.trim();
    if let Some(idx) = trimmed.find(". ") {
        return trimmed[..idx + 1].to_string();
    }
    if trimmed.chars().count() > 80 {
        let cut: String = trimmed.chars().take(79).collect();
        return format!("{cut}…");
    }
    trimmed.to_string()
}
