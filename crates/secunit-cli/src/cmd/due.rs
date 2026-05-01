use std::process::ExitCode;

use anyhow::Result;
use secunit_core::registry::resolver;

use super::Ctx;

pub fn run(
    ctx: &Ctx,
    within_days: i64,
    overdue_only: bool,
    owner: Option<&str>,
) -> Result<ExitCode> {
    let (reg, report) = ctx.load()?;
    if !report.is_clean() {
        eprintln!(
            "warning: registry has {} load error(s); continuing",
            report.errors.len()
        );
    }

    let mut rows: Vec<_> = resolver::due_within(&reg, ctx.today, within_days)
        .into_iter()
        .filter(|r| {
            if let Some(o) = owner {
                reg.controls
                    .get(&r.control_id)
                    .map(|c| c.owner == o)
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .collect();
    if overdue_only {
        rows.retain(|r| r.overdue);
    }

    if ctx.json {
        let payload: Vec<_> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "control_id": r.control_id,
                    "cadence": r.cadence,
                    "next_due": r.next_due,
                    "overdue": r.overdue,
                    "owner": reg.controls.get(&r.control_id).map(|c| &c.owner),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }

    if rows.is_empty() {
        println!("Nothing due within {within_days} day(s) of {}.", ctx.today);
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "{:<40} {:<12} {:<12} {:<7} {:<10}",
        "ID", "CADENCE", "DUE", "STATUS", "OWNER"
    );
    for r in &rows {
        let due = r
            .next_due
            .map(|d| d.to_string())
            .unwrap_or_else(|| "—".to_string());
        let status = if r.overdue { "overdue" } else { "due" };
        let owner = reg
            .controls
            .get(&r.control_id)
            .map(|c| c.owner.as_str())
            .unwrap_or("");
        println!(
            "{:<40} {:<12} {:<12} {:<7} {}",
            r.control_id,
            cadence_str(r.cadence),
            due,
            status,
            owner
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn cadence_str(c: secunit_core::model::Cadence) -> &'static str {
    use secunit_core::model::Cadence::*;
    match c {
        Continuous => "continuous",
        Weekly => "weekly",
        Monthly => "monthly",
        Quarterly => "quarterly",
        SemiAnnual => "semi-annual",
        Annual => "annual",
        Scheduled => "scheduled",
    }
}
