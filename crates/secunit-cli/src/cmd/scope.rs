use std::process::ExitCode;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use secunit_core::registry::resolver;

use super::Ctx;

pub fn run(ctx: &Ctx, control_id: &str, at: Option<NaiveDate>) -> Result<ExitCode> {
    let (reg, _) = ctx.load()?;
    let ctrl = reg
        .controls
        .get(control_id)
        .ok_or_else(|| anyhow!("control `{control_id}` not found"))?;
    let date = at.unwrap_or(ctx.today);
    let resolved = resolver::resolve_scope(ctrl, &reg.inventory, date);

    if ctx.json {
        let payload = serde_json::json!({
            "control_id": control_id,
            "at": date,
            "resolved": resolved,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }

    if resolved.is_empty() {
        if ctrl.scope.is_none() {
            println!("`{control_id}` is org-wide; no scope to resolve.");
        } else {
            println!("`{control_id}` resolves to no systems on {date}.");
        }
        return Ok(ExitCode::SUCCESS);
    }
    println!("{:<40} {:<16} {:<40}", "NAME", "KIND", "TAGS");
    for r in &resolved {
        println!("{:<40} {:<16} {}", r.name, r.kind, r.tags.join(","));
    }
    Ok(ExitCode::SUCCESS)
}
