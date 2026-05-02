use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Result};
use secunit_core::model::StateEntry;
use secunit_core::registry::resolver;

use super::Ctx;

const FINDINGS_FILE: &str = "findings.md";

pub fn run(ctx: &Ctx, control_id: Option<&str>, evidence: bool) -> Result<ExitCode> {
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

        let evidence_payload = if evidence {
            Some(load_evidence(&reg.root, state))
        } else {
            None
        };

        if ctx.json {
            let evidence_json = evidence_payload.as_ref().map(|e| match e {
                Evidence::Found { rel_path, content } => serde_json::json!({
                    "path": rel_path,
                    "content": content,
                }),
                Evidence::Missing { .. } | Evidence::NoRun => serde_json::Value::Null,
            });
            let payload = serde_json::json!({
                "control_id": id,
                "cadence": ctrl.cadence,
                "next_due": next,
                "state": state,
                "evidence": evidence_json,
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

        if let Some(ev) = evidence_payload {
            match ev {
                Evidence::Found { rel_path, content } => {
                    println!();
                    println!("── evidence: {} ──", rel_path);
                    print!("{}", content);
                    if !content.ends_with('\n') {
                        println!();
                    }
                }
                Evidence::Missing { rel_path } => {
                    println!("  evidence: {} not found at {}", FINDINGS_FILE, rel_path);
                }
                Evidence::NoRun => {
                    println!("  evidence: no run on record");
                }
            }
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

enum Evidence {
    Found { rel_path: String, content: String },
    Missing { rel_path: String },
    NoRun,
}

fn load_evidence(root: &Path, state: Option<&StateEntry>) -> Evidence {
    let Some(run_path) = state.and_then(|s| s.last_run_path.as_deref()) else {
        return Evidence::NoRun;
    };
    let rel_path = join_rel(run_path, FINDINGS_FILE);
    let abs: PathBuf = root.join(run_path).join(FINDINGS_FILE);
    match std::fs::read_to_string(&abs) {
        Ok(content) => Evidence::Found { rel_path, content },
        Err(_) => Evidence::Missing { rel_path },
    }
}

fn join_rel(dir: &str, file: &str) -> String {
    let trimmed = dir.trim_end_matches('/');
    format!("{trimmed}/{file}")
}
