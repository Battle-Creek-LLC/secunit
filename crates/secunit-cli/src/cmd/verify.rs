use std::process::ExitCode;

use anyhow::Result;
use secunit_core::evidence::verifier;

use super::Ctx;

pub fn run(ctx: &Ctx, control_id: Option<&str>) -> Result<ExitCode> {
    let (reg, _) = ctx.load()?;
    let report = verifier::verify(&reg.root, control_id)?;

    if ctx.json {
        let payload = serde_json::json!({
            "verified": report.verified.iter().map(|v| serde_json::json!({
                "control_id": v.control_id,
                "run_id": v.run_id,
                "run_dir": v.run_dir,
            })).collect::<Vec<_>>(),
            "failures": report.failures.iter().map(|f| serde_json::json!({
                "control_id": f.control_id,
                "run_id": f.run_id,
                "run_dir": f.run_dir,
                "kind": format!("{:?}", f.kind),
                "detail": f.detail,
            })).collect::<Vec<_>>(),
            "verified_risks": report.verified_risks.iter().map(|r| serde_json::json!({
                "risk_id": r.risk_id,
                "finding_refs": r.finding_refs,
            })).collect::<Vec<_>>(),
            "risk_failures": report.risk_failures.iter().map(|f| serde_json::json!({
                "risk_id": f.risk_id,
                "kind": format!("{:?}", f.kind),
                "detail": f.detail,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(if report.is_clean() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    let nothing_to_verify = report.verified.is_empty()
        && report.failures.is_empty()
        && report.verified_risks.is_empty()
        && report.risk_failures.is_empty();
    if nothing_to_verify {
        println!("No runs to verify.");
        return Ok(ExitCode::SUCCESS);
    }

    for f in &report.failures {
        println!(
            "✗ {} / {}: {:?} — {}",
            f.control_id, f.run_id, f.kind, f.detail
        );
    }
    for f in &report.risk_failures {
        println!("✗ risk {}: {:?} — {}", f.risk_id, f.kind, f.detail);
    }

    // Report the run summary line whenever there is run evidence to speak to,
    // even if the only failures came from the register (and vice versa).
    let has_runs = !report.verified.is_empty() || !report.failures.is_empty();
    let has_risks = !report.verified_risks.is_empty() || !report.risk_failures.is_empty();

    // Runs and the register each report their own verdict independently, so a
    // clean register is still acknowledged when some runs fail (and vice versa).
    if has_runs {
        if report.failures.is_empty() {
            println!(
                "✓ {} run(s) verified, hash chain intact",
                report.verified.len()
            );
        } else {
            println!(
                "✗ {} of {} run(s) failed verification",
                report.failures.len(),
                report.verified.len() + report.failures.len()
            );
        }
    }
    if has_risks {
        if report.risk_failures.is_empty() {
            println!(
                "✓ {} risk log(s) verified, chains intact",
                report.verified_risks.len()
            );
        } else {
            println!(
                "✗ {} of {} risk log(s) failed verification",
                report.risk_failures.len(),
                report.verified_risks.len() + report.risk_failures.len()
            );
        }
    }

    Ok(if report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
