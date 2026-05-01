use std::process::ExitCode;

use anyhow::{anyhow, Result};
use secunit_core::registry::resolver;

use super::Ctx;

pub fn run(ctx: &Ctx, control_id: Option<&str>) -> Result<ExitCode> {
    let (reg, _) = ctx.load()?;

    if let Some(id) = control_id {
        let ctrl = reg
            .controls
            .get(id)
            .ok_or_else(|| anyhow!("control `{id}` not found"))?;
        let state = reg.state.controls.get(id);
        let next = resolver::next_due(
            ctrl,
            &reg.schedule,
            state,
            ctx.today,
            reg.config.weekly_default_weekday,
        );

        if ctx.json {
            let payload = serde_json::json!({
                "control_id": id,
                "cadence": ctrl.cadence,
                "next_due": next,
                "state": state,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(ExitCode::SUCCESS);
        }

        println!("{} ({:?})", id, ctrl.cadence);
        println!(
            "  next due: {}",
            next.map(|d| d.to_string()).unwrap_or_else(|| "—".into())
        );
        if let Some(s) = state {
            println!(
                "  last run: {} ({:?})",
                s.last_run_id.as_deref().unwrap_or("—"),
                s.last_status,
            );
        } else {
            println!("  last run: never recorded in state.json");
        }
        return Ok(ExitCode::SUCCESS);
    }

    let rows = resolver::due_rows(&reg, ctx.today);
    if ctx.json {
        let payload: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "control_id": r.control_id,
                    "cadence": r.cadence,
                    "next_due": r.next_due,
                    "overdue": r.overdue,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "{:<40} {:<12} {:<12} {:<8}",
        "ID", "CADENCE", "NEXT DUE", "STATUS"
    );
    for r in &rows {
        let due = r
            .next_due
            .map(|d| d.to_string())
            .unwrap_or_else(|| "—".into());
        let status = if r.overdue { "overdue" } else { "ok" };
        println!(
            "{:<40} {:?} {:<12} {}",
            r.control_id, r.cadence, due, status
        );
    }
    Ok(ExitCode::SUCCESS)
}
