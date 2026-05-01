use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "secunit", version, about = "WISP control registry helper", long_about = None)]
struct Cli {
    /// Treat DIR as the secunit root.
    #[arg(short = 'C', long, value_name = "DIR", global = true)]
    root: Option<PathBuf>,

    /// Alternate _config.yaml path.
    #[arg(long, value_name = "FILE", global = true)]
    config: Option<PathBuf>,

    /// Machine-readable output where applicable.
    #[arg(long, global = true)]
    json: bool,

    /// Increase verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show which integrations are compiled in.
    Features,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match cli.command {
        Command::Features => cmd_features(cli.json),
    }
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .with_writer(std::io::stderr)
        .try_init();
}

fn cmd_features(json: bool) -> ExitCode {
    let features = secunit_capture::enabled_features();
    if json {
        let payload = serde_json::json!({ "features": features });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else if features.is_empty() {
        println!("No optional capture features compiled in.");
    } else {
        println!("Compiled-in capture features:");
        for f in features {
            println!("  {f}");
        }
    }
    ExitCode::SUCCESS
}
