use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use secunit_core::evidence::runner::{self, PrepareOpts};

use super::Ctx;

pub fn prepare(ctx: &Ctx, control_id: &str, note: Option<&str>, human: bool) -> Result<ExitCode> {
    let (reg, report) = ctx.load()?;
    if !report.is_clean() {
        for e in &report.errors {
            eprintln!("error {}: {}", e.path.display(), e.message);
        }
        return Ok(ExitCode::from(1));
    }
    let opts = PrepareOpts {
        today: Some(ctx.today),
        operator: std::env::var("SECUNIT_OPERATOR").ok(),
        note: note.map(str::to_string),
        now: None,
    };
    let result = runner::prepare(&reg, control_id, &opts);
    let prepare_ctx = match result {
        Ok(c) => c,
        Err(e) => {
            // docs/cli.md exit codes: 4 = pending run prevents action,
            // 2 = runtime failure (everything else: not-a-git-repo,
            // unknown control, empty scope, IO).
            let msg = format!("{e:#}");
            let code = if msg.contains("pending run already exists") {
                4
            } else {
                2
            };
            eprintln!("error: {msg}");
            return Ok(ExitCode::from(code));
        }
    };

    if human {
        println!("control:    {}", prepare_ctx.control_id);
        println!("run id:     {}", prepare_ctx.run_id);
        println!("run dir:    {}", prepare_ctx.run_dir.display());
        println!("scope:      {} system(s)", prepare_ctx.resolved_scope.len());
        for s in &prepare_ctx.resolved_scope {
            println!("  - {} ({})", s.name, s.kind);
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&prepare_ctx)?);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn finalize(ctx: &Ctx, run_dir: &Path) -> Result<ExitCode> {
    let (reg, _) = ctx.load()?;
    let manifest = match runner::finalize(&reg, run_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e:#}");
            return Ok(ExitCode::from(2));
        }
    };
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(ExitCode::SUCCESS);
    }
    let total = manifest.artifacts.len()
        + manifest
            .by_system
            .iter()
            .map(|b| b.artifacts.len())
            .sum::<usize>();
    println!("✓ hashed {total} artifact(s)");
    if let Some(p) = &manifest.prior_run {
        println!("✓ chained to prior {}", p.run_id);
    } else {
        println!("✓ first run for this control (no prior)");
    }
    println!("✓ wrote manifest.json");
    println!("✓ updated state.json");
    Ok(ExitCode::SUCCESS)
}

pub fn abort(ctx: &Ctx, run_dir: &Path, reason: &str) -> Result<ExitCode> {
    let _ = ctx;
    let record = runner::abort(run_dir, reason)?;
    println!("aborted {} (run {})", record.control_id, record.run_id);
    Ok(ExitCode::SUCCESS)
}

pub fn resume(ctx: &Ctx, run_dir: &Path) -> Result<ExitCode> {
    let prepare_ctx = runner::resume(run_dir)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&prepare_ctx)?);
    } else {
        println!("control:    {}", prepare_ctx.control_id);
        println!("run id:     {}", prepare_ctx.run_id);
        println!("run dir:    {}", prepare_ctx.run_dir.display());
    }
    Ok(ExitCode::SUCCESS)
}

pub fn list(ctx: &Ctx, _pending: bool) -> Result<ExitCode> {
    let (reg, _) = ctx.load()?;
    let pending = runner::list_pending(&reg.root)?;
    if ctx.json {
        let payload: Vec<_> = pending
            .iter()
            .map(|p| {
                serde_json::json!({
                    "control_id": p.control_id,
                    "run_id": p.run_id,
                    "run_dir": p.run_dir,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(ExitCode::SUCCESS);
    }
    if pending.is_empty() {
        println!("No pending runs.");
        return Ok(ExitCode::SUCCESS);
    }
    println!("{:<40} {:<24} {:<8}", "CONTROL", "RUN ID", "DIR");
    for p in &pending {
        println!(
            "{:<40} {:<24} {}",
            p.control_id,
            p.run_id,
            p.run_dir.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}
