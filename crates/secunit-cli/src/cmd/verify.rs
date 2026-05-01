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
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(if report.is_clean() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    if report.verified.is_empty() && report.failures.is_empty() {
        println!("No runs to verify.");
        return Ok(ExitCode::SUCCESS);
    }

    for f in &report.failures {
        println!(
            "✗ {} / {}: {:?} — {}",
            f.control_id, f.run_id, f.kind, f.detail
        );
    }
    if report.is_clean() {
        println!(
            "✓ {} run(s) verified, hash chain intact",
            report.verified.len()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        println!(
            "✗ {} of {} run(s) failed verification",
            report.failures.len(),
            report.verified.len() + report.failures.len()
        );
        Ok(ExitCode::from(1))
    }
}
