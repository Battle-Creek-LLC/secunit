use std::process::ExitCode;

use anyhow::{anyhow, Result};

use super::Ctx;

pub fn run(ctx: &Ctx, control_id: &str) -> Result<ExitCode> {
    let (reg, _) = ctx.load()?;
    let ctrl = reg
        .controls
        .get(control_id)
        .ok_or_else(|| anyhow!("control `{control_id}` not found"))?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(ctrl)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!("id:         {}", ctrl.id);
    println!("title:      {}", ctrl.title);
    println!("policy:     {}", ctrl.policy);
    if !ctrl.nist.is_empty() {
        println!("nist:       {}", ctrl.nist.join(", "));
    }
    println!("owner:      {}", ctrl.owner);
    println!("cadence:    {:?}", ctrl.cadence);
    if let Some(w) = ctrl.weekday {
        println!("weekday:    {:?}", w);
    }
    if let Some(due_by) = &ctrl.due_by {
        println!("due_by:     {due_by}");
    }
    println!("skill:      {}", ctrl.skill);
    if let Some(scope) = &ctrl.scope {
        println!("scope:      {}", serde_json::to_string(scope)?);
    } else {
        println!("scope:      (org-wide)");
    }
    if !ctrl.evidence_required.is_empty() {
        println!("evidence:");
        for e in &ctrl.evidence_required {
            let extra = e
                .description
                .as_deref()
                .or(e.prompt.as_deref())
                .unwrap_or("");
            println!("  - {} — {}", e.kind, extra);
        }
    }
    Ok(ExitCode::SUCCESS)
}
