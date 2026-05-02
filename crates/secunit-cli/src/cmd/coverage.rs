use std::process::ExitCode;

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use secunit_core::registry::coverage::{self, PeriodStatus};

use super::Ctx;

pub fn run(
    ctx: &Ctx,
    control_id: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<ExitCode> {
    let (reg, report) = ctx.load()?;
    if !report.is_clean() {
        eprintln!(
            "warning: registry has {} load error(s); continuing",
            report.errors.len()
        );
    }

    let (start, end) = window_or_default_quarter(ctx.today, from, to);
    let report = match coverage::coverage(&reg, control_id, start, end, ctx.today) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e:#}");
            return Ok(ExitCode::from(2));
        }
    };

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "control: {} — window {} → {}",
        report.control_id, report.window_start, report.window_end
    );
    if report.periods.is_empty() {
        println!("(no periods in this window — continuous cadence?)");
        return Ok(ExitCode::SUCCESS);
    }

    println!("{:<14} {:<12} {:<24} NOTE", "PERIOD", "STATUS", "EVIDENCE");
    let mut satisfied = 0;
    let mut gaps = 0;
    let mut open = 0;
    for p in &report.periods {
        let status_label = match p.status {
            PeriodStatus::Satisfied => {
                satisfied += 1;
                if p.late {
                    "satisfied*"
                } else {
                    "satisfied"
                }
            }
            PeriodStatus::Gap => {
                gaps += 1;
                "gap"
            }
            PeriodStatus::Open => {
                open += 1;
                "open"
            }
            PeriodStatus::Skipped => "skipped",
            PeriodStatus::Future => "future",
        };
        let evidence = p
            .satisfied_by
            .as_ref()
            .map(|r| r.run_id.clone())
            .unwrap_or_else(|| "—".to_string());
        let note = p.skipped_reason.clone().unwrap_or_else(|| {
            if p.late {
                "completed late".into()
            } else {
                String::new()
            }
        });
        println!(
            "{:<14} {:<12} {:<24} {}",
            p.period_id, status_label, evidence, note
        );
    }
    println!();
    println!(
        "summary: {satisfied} satisfied, {gaps} gap(s), {open} open / {} total period(s)",
        report.periods.len()
    );

    if !report.unclassified_runs.is_empty() {
        println!();
        println!("unclassified runs ({}):", report.unclassified_runs.len());
        for u in &report.unclassified_runs {
            println!(
                "  {} ({:?})  {}",
                u.run_id,
                u.period_id.as_deref().unwrap_or("none"),
                u.reason
            );
        }
    }

    // Non-zero exit when there's an unsatisfied gap — useful for CI gates.
    if gaps > 0 {
        return Ok(ExitCode::from(3));
    }
    Ok(ExitCode::SUCCESS)
}

fn window_or_default_quarter(
    today: NaiveDate,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> (NaiveDate, NaiveDate) {
    let q_first_month = match today.month() {
        1..=3 => 1,
        4..=6 => 4,
        7..=9 => 7,
        _ => 10,
    };
    let default_start = NaiveDate::from_ymd_opt(today.year(), q_first_month, 1).unwrap();
    let default_end = if q_first_month == 10 {
        NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap()
    } else {
        NaiveDate::from_ymd_opt(today.year(), q_first_month + 3, 1)
            .unwrap()
            .pred_opt()
            .unwrap()
    };
    (from.unwrap_or(default_start), to.unwrap_or(default_end))
}
