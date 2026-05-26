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
    /// Health-check the environment and registry (read-only preflight).
    Doctor,
    /// Show which integrations are compiled in.
    Features,
    /// List, show, or locate runbook skills (bundled + local overrides).
    Skills {
        #[command(subcommand)]
        sub: SkillsCmd,
    },
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
    /// Manage the risk register.
    Risks {
        #[command(subcommand)]
        sub: RisksCmd,
    },
}

#[derive(Debug, Subcommand)]
enum RisksCmd {
    /// Open a risk from a sealed run's draft finding.
    Open {
        /// The source control id (must match the run's manifest).
        control_id: String,
        /// The sealed run dir holding the manifest + draft risk.
        #[arg(long, value_name = "RUN_DIR")]
        from: PathBuf,
        /// The draft-risk finding id to promote.
        #[arg(long, value_name = "ID")]
        finding: String,
        /// Optional initial owner (role).
        #[arg(long)]
        owner: Option<String>,
        /// Override the SLA window; defaults from the control's
        /// remediation_thresholds for the risk's severity.
        #[arg(long, value_name = "N")]
        sla_days: Option<u32>,
    },
    /// Assign an owner.
    Assign {
        risk_id: String,
        #[arg(long)]
        owner: String,
    },
    /// Supersede the score (and severity).
    Score {
        risk_id: String,
        #[arg(long)]
        impact: u8,
        #[arg(long)]
        likelihood: u8,
        /// New severity (critical|high|medium|low|info).
        #[arg(long, default_value = "high")]
        severity: String,
        #[arg(long)]
        reason: String,
    },
    /// Move through the status machine.
    Status {
        risk_id: String,
        /// Target status (open|in-progress|remediated|accepted-exception|false-positive).
        #[arg(long)]
        to: String,
        #[arg(long)]
        reason: String,
    },
    /// Link another finding ref (re-observed in a later run).
    Relink {
        risk_id: String,
        #[arg(long, value_name = "RUN_DIR")]
        from: PathBuf,
        #[arg(long, value_name = "ID")]
        finding: String,
    },
    /// Record an external tracker mirror created by sync-out.
    Link {
        risk_id: String,
        #[arg(long)]
        system: String,
        #[arg(long, value_name = "EXT_ID")]
        id: String,
        #[arg(long)]
        url: String,
    },
    /// Record an advisory inbound tracker status (never authoritative).
    Observe {
        risk_id: String,
        #[arg(long)]
        system: String,
        #[arg(long)]
        status: String,
    },
    /// Append a free-text note.
    Note {
        risk_id: String,
        #[arg(long)]
        text: String,
    },
    /// Mark remediated, optionally binding resolution evidence.
    Remediate {
        risk_id: String,
        /// Sealed run dir proving the fix.
        #[arg(long, value_name = "RUN_DIR")]
        evidence: Option<PathBuf>,
        #[arg(long)]
        note: String,
    },
    /// Reopen a remediated risk.
    Reopen {
        risk_id: String,
        #[arg(long)]
        reason: String,
    },
    /// Document an accepted exception.
    Except {
        risk_id: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_name = "WHO")]
        approved_by: String,
        #[arg(long, value_name = "DATE")]
        expires: chrono::NaiveDate,
    },
    /// List risks (human table; --json for the structured index).
    List {
        #[arg(long)]
        status: Option<String>,
        /// Comma-separated severities (e.g. `critical,high`).
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        /// Only risks past their SLA as of today.
        #[arg(long)]
        past_sla: bool,
    },
    /// Show one risk: current fold + event timeline.
    Show { risk_id: String },
    /// Regenerate risks/index.json from the logs.
    Rebuild,
}

#[derive(Debug, Subcommand)]
enum SkillsCmd {
    /// List available skills (bundled standard library + local overrides).
    List,
    /// Print a resolved skill's markdown to stdout (local wins over bundled).
    Show { name: String },
    /// Print a filesystem path to a skill (materialises bundled skills to a cache).
    Path { name: String },
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
        Command::Doctor => cmd::doctor::run(&ctx),
        Command::Features => cmd::features::run(&ctx),
        Command::Skills { sub } => match sub {
            SkillsCmd::List => cmd::skills::list(&ctx),
            SkillsCmd::Show { name } => cmd::skills::show(&ctx, &name),
            SkillsCmd::Path { name } => cmd::skills::path(&ctx, &name),
        },
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
        Command::Risks { sub } => match sub {
            RisksCmd::Open {
                control_id,
                from,
                finding,
                owner,
                sla_days,
            } => cmd::risks::open(
                &ctx,
                &control_id,
                &from,
                &finding,
                owner.as_deref(),
                sla_days,
            ),
            RisksCmd::Assign { risk_id, owner } => cmd::risks::assign(&ctx, &risk_id, &owner),
            RisksCmd::Score {
                risk_id,
                impact,
                likelihood,
                severity,
                reason,
            } => cmd::risks::score(&ctx, &risk_id, impact, likelihood, &severity, &reason),
            RisksCmd::Status {
                risk_id,
                to,
                reason,
            } => cmd::risks::status(&ctx, &risk_id, &to, &reason),
            RisksCmd::Relink {
                risk_id,
                from,
                finding,
            } => cmd::risks::relink(&ctx, &risk_id, &from, &finding),
            RisksCmd::Link {
                risk_id,
                system,
                id,
                url,
            } => cmd::risks::link(&ctx, &risk_id, &system, &id, &url),
            RisksCmd::Observe {
                risk_id,
                system,
                status,
            } => cmd::risks::observe(&ctx, &risk_id, &system, &status),
            RisksCmd::Note { risk_id, text } => cmd::risks::note(&ctx, &risk_id, &text),
            RisksCmd::Remediate {
                risk_id,
                evidence,
                note,
            } => cmd::risks::remediate(&ctx, &risk_id, evidence.as_deref(), &note),
            RisksCmd::Reopen { risk_id, reason } => cmd::risks::reopen(&ctx, &risk_id, &reason),
            RisksCmd::Except {
                risk_id,
                rationale,
                approved_by,
                expires,
            } => cmd::risks::except(&ctx, &risk_id, &rationale, &approved_by, expires),
            RisksCmd::List {
                status,
                severity,
                owner,
                past_sla,
            } => cmd::risks::list(
                &ctx,
                status.as_deref(),
                severity.as_deref(),
                owner.as_deref(),
                past_sla,
            ),
            RisksCmd::Show { risk_id } => cmd::risks::show(&ctx, &risk_id),
            RisksCmd::Rebuild => cmd::risks::rebuild(&ctx),
        },
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
