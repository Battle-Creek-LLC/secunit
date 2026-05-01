use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use secunit_core::registry::loader::{Diagnostic, LoadReport};

use super::Ctx;

pub fn run(ctx: &Ctx, _strict: bool) -> Result<ExitCode> {
    let (reg, mut report) = ctx.load()?;

    check_requires_features(&reg.root, &mut report);

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

/// Parse `requires_features:` from each skill's YAML frontmatter and
/// flag any feature the running binary does not advertise via
/// `secunit_capture::enabled_features()`.
fn check_requires_features(root: &Path, report: &mut LoadReport) {
    let enabled: HashSet<&str> = secunit_capture::enabled_features()
        .iter()
        .copied()
        .collect();
    let skills_dir = root.join("skills");
    let entries = match fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let body = match fs::read_to_string(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let Some(fm) = extract_frontmatter(&body) else {
            continue;
        };
        let parsed: serde_yaml::Value = match serde_yaml::from_str(fm) {
            Ok(v) => v,
            Err(e) => {
                report.errors.push(Diagnostic {
                    path: path.clone(),
                    message: format!("frontmatter not valid YAML: {e}"),
                });
                continue;
            }
        };
        let Some(seq) = parsed
            .get("requires_features")
            .and_then(|v| v.as_sequence())
        else {
            continue;
        };
        for item in seq {
            let Some(name) = item.as_str() else { continue };
            if !enabled.contains(name) {
                report.errors.push(Diagnostic {
                    path: path.clone(),
                    message: format!(
                        "skill declares `requires_features: [{name}]` but \
                         this binary was built without the `{name}` feature"
                    ),
                });
            }
        }
    }
    let _ = PathBuf::new(); // silence unused-import lints in stripped builds
}

/// Pull the YAML frontmatter block out of a markdown file. Returns
/// `None` if the file does not start with `---\n`.
fn extract_frontmatter(body: &str) -> Option<&str> {
    let rest = body.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_extract_basic() {
        let body = "---\nname: x\nrequires_features: [a, b]\n---\n# body";
        let fm = extract_frontmatter(body).unwrap();
        assert!(fm.contains("requires_features"));
    }

    #[test]
    fn frontmatter_returns_none_when_absent() {
        assert!(extract_frontmatter("# just markdown").is_none());
    }
}
