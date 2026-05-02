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
    /// Period-by-period coverage report for one control.
    Coverage {
        control_id: String,
        /// Window start (inclusive). Defaults to start of current quarter.
        #[arg(long, value_name = "DATE")]
        from: Option<chrono::NaiveDate>,
        /// Window end (inclusive). Defaults to end of current quarter.
        #[arg(long, value_name = "DATE")]
        to: Option<chrono::NaiveDate>,
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
    Status {
        control_id: Option<String>,
        /// Print the latest run's findings.md inline. Requires CONTROL_ID.
        #[arg(short = 'e', long, requires = "control_id")]
        evidence: bool,
    },
    /// Validate the registry (schema + cross-refs).
    Validate {
        /// Adds opinionated checks (NIST id format, etc).
        #[arg(long)]
        strict: bool,
    },
    /// Allocate / finalize / abort runs.
    Run {
        #[command(subcommand)]
        sub: RunCmd,
    },
    /// Verify manifest hash chains.
    Verify { control_id: Option<String> },
    /// Show which integrations are compiled in.
    Features,
    /// Manage controls and the schedule.
    Registry {
        #[command(subcommand)]
        sub: RegistryCmd,
    },
    /// Manage the inventory.
    Inventory {
        #[command(subcommand)]
        sub: InventoryCmd,
    },
    /// Capture evidence via native integrations.
    Capture {
        #[command(subcommand)]
        sub: cmd::capture::CaptureCmd,
    },
}

#[derive(Debug, Subcommand)]
enum RegistryCmd {
    /// Promote drafts emitted by a `bootstrap` or `inventory-seed` run
    /// directory into the live registry.
    Import {
        /// The bootstrap/inventory-seed run dir, or any directory with
        /// drafts laid out under `raw/` (or at the top level).
        bootstrap_dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum InventoryCmd {
    /// List inventory entries.
    List {
        /// Restrict to one kind (singular or plural section name).
        #[arg(long)]
        kind: Option<String>,
    },
    /// Append a new entry. `in_scope_since` is set to today.
    Add {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        name: String,
        #[arg(long, num_args = 0..)]
        tags: Vec<String>,
        #[arg(long)]
        url: Option<String>,
    },
    /// Mark an entry retired. History is preserved.
    Retire {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        on: chrono::NaiveDate,
        #[arg(long)]
        reason: String,
    },
    /// Sanity-check inventory.yaml (schema, duplicates, lifecycle dates).
    Check,
}

#[derive(Debug, Subcommand)]
enum RunCmd {
    /// Allocate a run dir and emit the prepare context.
    Prepare {
        control_id: String,
        #[arg(long)]
        note: Option<String>,
        /// Override the period this run claims (e.g. `2026-W18`,
        /// `2026-q2`). Defaults to the current period derived from cadence.
        #[arg(long)]
        period: Option<String>,
        /// Print human-readable summary instead of JSON.
        #[arg(long)]
        human: bool,
    },
    /// Hash artifacts, link the chain, and seal the run.
    Finalize { run_dir: PathBuf },
    /// Discard a pending run; keep the directory for audit.
    Abort {
        run_dir: PathBuf,
        #[arg(long)]
        reason: String,
    },
    /// Re-emit the prepare context for a pending run.
    Resume { run_dir: PathBuf },
    /// List all `.run-pending` runs under the root.
    List {
        #[arg(long)]
        pending: bool,
    },
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
        Command::Coverage {
            control_id,
            from,
            to,
        } => cmd::coverage::run(&ctx, &control_id, from, to),
        Command::Show { control_id } => cmd::show::run(&ctx, &control_id),
        Command::Scope { control_id, at } => cmd::scope::run(&ctx, &control_id, at),
        Command::Status {
            control_id,
            evidence,
        } => cmd::status::run(&ctx, control_id.as_deref(), evidence),
        Command::Validate { strict } => cmd::validate::run(&ctx, strict),
        Command::Run { sub } => match sub {
            RunCmd::Prepare {
                control_id,
                note,
                period,
                human,
            } => cmd::run::prepare(&ctx, &control_id, note.as_deref(), period.as_deref(), human),
            RunCmd::Finalize { run_dir } => cmd::run::finalize(&ctx, &run_dir),
            RunCmd::Abort { run_dir, reason } => cmd::run::abort(&ctx, &run_dir, &reason),
            RunCmd::Resume { run_dir } => cmd::run::resume(&ctx, &run_dir),
            RunCmd::List { pending } => cmd::run::list(&ctx, pending),
        },
        Command::Verify { control_id } => cmd::verify::run(&ctx, control_id.as_deref()),
        Command::Features => cmd::features::run(&ctx),
        Command::Registry { sub } => match sub {
            RegistryCmd::Import { bootstrap_dir } => cmd::registry::import(&ctx, &bootstrap_dir),
        },
        Command::Inventory { sub } => match sub {
            InventoryCmd::List { kind } => cmd::inventory::list(&ctx, kind.as_deref()),
            InventoryCmd::Add {
                kind,
                name,
                tags,
                url,
            } => cmd::inventory::add(&ctx, &kind, &name, &tags, url.as_deref()),
            InventoryCmd::Retire {
                kind,
                name,
                on,
                reason,
            } => cmd::inventory::retire(&ctx, &kind, &name, on, &reason),
            InventoryCmd::Check => cmd::inventory::check(&ctx),
        },
        Command::Capture { sub } => cmd::capture::run(sub),
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
