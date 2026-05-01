use std::process::ExitCode;

use anyhow::Result;

use super::Ctx;

pub fn run(ctx: &Ctx) -> Result<ExitCode> {
    let features = secunit_capture::enabled_features();
    if ctx.json {
        let payload = serde_json::json!({ "features": features });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if features.is_empty() {
        println!("No optional capture features compiled in.");
    } else {
        println!("Compiled-in capture features:");
        for f in features {
            println!("  {f}");
        }
    }
    Ok(ExitCode::SUCCESS)
}
