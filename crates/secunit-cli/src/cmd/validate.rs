use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use secunit_core::model::LoadedRegistry;
use secunit_core::registry::loader::{Diagnostic, LoadReport};
use secunit_core::skills;

use super::Ctx;

pub fn run(ctx: &Ctx, _strict: bool) -> Result<ExitCode> {
    let (reg, mut report) = ctx.load()?;

    check_skills(&reg, &mut report);

    if ctx.json {
        let payload = serde_json::json!({
            "errors": report.errors.iter().map(|d| serde_json::json!({
                "path": d.path,
                "message": d.message,
            })).collect::<Vec<_>>(),
            "warnings": report.warnings.iter().map(|d| serde_json::json!({
                "path": d.path,
                "message": d.message,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(if report.is_clean() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    if report.is_clean() && report.warnings.is_empty() {
        println!("✓ registry is valid");
        return Ok(ExitCode::SUCCESS);
    }

    for w in &report.warnings {
        println!("warn  {}: {}", w.path.display(), w.message);
    }
    for e in &report.errors {
        println!("error {}: {}", e.path.display(), e.message);
    }
    if report.is_clean() {
        println!("✓ registry is valid ({} warning(s))", report.warnings.len());
        Ok(ExitCode::SUCCESS)
    } else {
        println!(
            "✗ registry has {} error(s), {} warning(s)",
            report.errors.len(),
            report.warnings.len()
        );
        Ok(ExitCode::from(1))
    }
}

/// Skill-level checks: (a) every local skill's frontmatter parses as
/// YAML, and (b) every skill a control names — local or bundled — has
/// the `requires_features:` it declares compiled into this binary.
fn check_skills(reg: &LoadedRegistry, report: &mut LoadReport) {
    check_local_frontmatter(&reg.root, report);
    check_requires_features(reg, report);
}

/// Flag any local skill whose frontmatter is not valid YAML.
fn check_local_frontmatter(root: &Path, report: &mut LoadReport) {
    let skills_dir = root.join("skills");
    let Ok(entries) = fs::read_dir(&skills_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Ok(body) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(fm) = skills::frontmatter(&body) else {
            continue;
        };
        if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(fm) {
            report.errors.push(Diagnostic {
                path: path.clone(),
                message: format!("frontmatter not valid YAML: {e}"),
            });
        }
    }
}

/// For every distinct skill a control names, resolve it (local-first,
/// then bundled) and flag any `requires_features:` the binary lacks.
/// A skill that resolves to nothing is reported by the loader's
/// cross-check, not here.
fn check_requires_features(reg: &LoadedRegistry, report: &mut LoadReport) {
    let enabled: HashSet<&str> = secunit_capture::enabled_features()
        .iter()
        .copied()
        .collect();
    let mut checked: HashSet<&str> = HashSet::new();
    for ctrl in reg.controls.values() {
        if !checked.insert(ctrl.skill.as_str()) {
            continue;
        }
        let Some(resolved) = skills::resolve(&reg.root, &ctrl.skill) else {
            continue;
        };
        for feat in skills::requires_features(&resolved.body) {
            if !enabled.contains(feat.as_str()) {
                let path = resolved
                    .path
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(format!("<bundled>/{}.md", ctrl.skill)));
                report.errors.push(Diagnostic {
                    path,
                    message: format!(
                        "skill `{}` declares `requires_features: [{feat}]` but \
                         this binary was built without the `{feat}` feature",
                        ctrl.skill
                    ),
                });
            }
        }
    }
}
