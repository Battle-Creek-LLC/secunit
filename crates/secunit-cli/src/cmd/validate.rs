use std::process::ExitCode;

use anyhow::Result;

use super::Ctx;

pub fn run(ctx: &Ctx, _strict: bool) -> Result<ExitCode> {
    let (_reg, report) = ctx.load()?;

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
