use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod cmd;

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

    /// Pin "today" to a specific ISO date. Useful for deterministic
    /// testing and replaying historical state.
    #[arg(long, value_name = "DATE", global = true)]
    today: Option<chrono::NaiveDate>,

    /// Increase verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show controls coming due.
    Due {
        /// Window in days from today.
        #[arg(long, default_value_t = 7)]
        within: i64,
        /// Only list controls past their grace window.
        #[arg(long)]
        overdue_only: bool,
        /// Filter by `owner` field on the control.
        #[arg(long)]
        owner: Option<String>,
    },
    /// Show one control's full configuration.
    Show { control_id: String },
    /// Preview resolved scope for a control.
    Scope {
        control_id: String,
        /// Run date used for lifecycle filtering.
        #[arg(long, value_name = "DATE")]
        at: Option<chrono::NaiveDate>,
    },
    /// Show registry-wide or per-control status.
    Status { control_id: Option<String> },
    /// Validate the registry (schema + cross-refs).
    Validate {
        /// Adds opinionated checks (NIST id format, etc).
        #[arg(long)]
        strict: bool,
    },
    /// Show which integrations are compiled in.
    Features,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let ctx = cmd::Ctx {
        root: cli.root.clone().unwrap_or_else(|| PathBuf::from(".")),
        json: cli.json,
        today: cli.today.unwrap_or_else(today_local),
    };

    let result = match cli.command {
        Command::Due {
            within,
            overdue_only,
            owner,
        } => cmd::due::run(&ctx, within, overdue_only, owner.as_deref()),
        Command::Show { control_id } => cmd::show::run(&ctx, &control_id),
        Command::Scope { control_id, at } => cmd::scope::run(&ctx, &control_id, at),
        Command::Status { control_id } => cmd::status::run(&ctx, control_id.as_deref()),
        Command::Validate { strict } => cmd::validate::run(&ctx, strict),
        Command::Features => cmd::features::run(&ctx),
    };

    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn today_local() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
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
