//! `secunit capture …` — native upstream capturers. Each subcommand
//! writes a canonical JSON envelope to `--out`. Per `docs/cli.md`:
//! exit 0 on success, exit 1 on validation failure, exit 2 on runtime
//! / auth / network failure.

use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(any(feature = "deps", feature = "github"))]
use anyhow::Context;
use anyhow::Result;
use clap::Subcommand;

#[cfg(any(feature = "deps", feature = "github"))]
use secunit_capture::canonical::Envelope;

#[derive(Debug, Subcommand)]
pub enum CaptureCmd {
    /// Source-side dependency-audit and SBOM capturers.
    ///
    /// Every action here shells out to an external tool — see the
    /// list under EXTERNAL TOOLS below. Each capturer probes for its
    /// binary before running and names the install command if it is
    /// missing.
    #[command(after_help = external_tools_help())]
    Deps {
        #[command(subcommand)]
        sub: DepsCmd,
    },
    /// GitHub captures via the REST API.
    Github {
        #[command(subcommand)]
        sub: GithubCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum DepsCmd {
    /// Run pip-audit against a Python project.
    PipAudit {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Run pnpm audit against a Node project.
    PnpmAudit {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Audit a Cargo workspace using the rustsec advisory db.
    CargoAudit {
        /// Path to the project's `Cargo.lock`.
        #[arg(long)]
        path: PathBuf,
        /// Optional local advisory-db path (a cloned rustsec/advisory-db).
        /// When omitted, the advisory-db snapshot is downloaded over
        /// HTTPS and cached locally.
        #[arg(long)]
        db_path: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Query OSV.dev for one package@version.
    OsvQuery {
        #[arg(long)]
        ecosystem: String,
        #[arg(long)]
        package: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Generate a CycloneDX SBOM for a project.
    ///
    /// Writes two files: the bare CycloneDX document at `--out`, which
    /// an assessor's tooling reads unchanged, and a canonical envelope
    /// at `<out>.envelope.json` recording the generator, its version,
    /// and the BOM's sha256.
    ///
    /// Requires the CycloneDX generator for the ecosystem:
    /// `cargo-cyclonedx` (rust), `cyclonedx-py` (python), or
    /// `cyclonedx-npm` (node).
    Sbom {
        /// Project root — the directory holding `Cargo.toml`,
        /// `requirements.txt`, or `package.json`.
        #[arg(long)]
        path: PathBuf,
        /// rust | python | node. Inferred from the manifests present
        /// when omitted; required when a tree carries more than one.
        #[arg(long)]
        ecosystem: Option<String>,
        /// Path for the bare CycloneDX BOM.
        #[arg(long)]
        out: PathBuf,
    },
}

/// The `EXTERNAL TOOLS` block appended to `capture deps --help`.
///
/// Built from `REQUIRED_TOOLS` rather than hand-written, so the list an
/// operator reads and the list the capturers probe cannot drift.
#[cfg(feature = "deps")]
fn external_tools_help() -> String {
    use secunit_capture::deps::cmd::REQUIRED_TOOLS;

    let mut s = String::from(
        "EXTERNAL TOOLS:\n  \
         Every `capture deps` action shells out. Install the tool for each action you run;\n  \
         a missing binary fails the capture with its install command (exit 2), never a\n  \
         partial artifact.\n\n",
    );
    let width = REQUIRED_TOOLS
        .iter()
        .map(|t| t.program.len())
        .max()
        .unwrap_or(0);
    for t in REQUIRED_TOOLS {
        s.push_str(&format!(
            "  {:<width$}  {:<30}  {}\n",
            t.program,
            t.action,
            t.install_hint,
            width = width
        ));
    }
    s.push_str("\n  `secunit doctor` reports which of these are on PATH.\n");
    s
}

#[cfg(not(feature = "deps"))]
fn external_tools_help() -> String {
    String::from("EXTERNAL TOOLS:\n  This binary was built without the `deps` feature.\n")
}

#[derive(Debug, Subcommand)]
pub enum GithubCmd {
    /// List Dependabot alerts for a repo.
    DependabotAlerts {
        /// `<owner>/<repo>`
        #[arg(long)]
        repo: String,
        /// open|fixed|dismissed|all (passed verbatim to the API)
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Fetch protection settings for one branch.
    BranchProtection {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        branch: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// List members of an org.
    OrgMembers {
        #[arg(long)]
        org: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Fetch audit log entries since the given absolute ISO-8601 time.
    AuditLog {
        #[arg(long)]
        org: String,
        #[arg(long, value_name = "ISO_8601")]
        since: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// List CodeQL / code-scanning alerts for a repo.
    CodeqlAlerts {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        out: PathBuf,
    },
}

pub fn run(cmd: CaptureCmd) -> Result<ExitCode> {
    match cmd {
        CaptureCmd::Deps { sub } => deps(sub),
        CaptureCmd::Github { sub } => github(sub),
    }
}

#[cfg(feature = "deps")]
fn deps(cmd: DepsCmd) -> Result<ExitCode> {
    // The SBOM action writes two files rather than one envelope, so it
    // returns before the shared `write_envelope` tail.
    if let DepsCmd::Sbom {
        path,
        ecosystem,
        out,
    } = cmd
    {
        return sbom(&path, ecosystem.as_deref(), &out);
    }

    let env = match cmd {
        DepsCmd::PipAudit { path, out } => {
            let env = secunit_capture::deps::pip_audit::capture(&path).map_err(map_runtime)?;
            (env, out)
        }
        DepsCmd::PnpmAudit { path, out } => {
            let env = secunit_capture::deps::pnpm_audit::capture(&path).map_err(map_runtime)?;
            (env, out)
        }
        DepsCmd::CargoAudit { path, db_path, out } => {
            let env = secunit_capture::deps::cargo_audit::capture(&path, db_path.as_deref())
                .map_err(map_runtime)?;
            (env, out)
        }
        DepsCmd::OsvQuery {
            ecosystem,
            package,
            version,
            out,
        } => {
            let rt = build_runtime()?;
            let env = rt
                .block_on(secunit_capture::deps::osv_query::capture(
                    secunit_capture::deps::osv_query::OsvArgs {
                        ecosystem: &ecosystem,
                        package: &package,
                        version: &version,
                    },
                ))
                .map_err(map_runtime)?;
            (env, out)
        }
        // Handled above — it writes a BOM plus a sidecar, not one envelope.
        DepsCmd::Sbom { .. } => unreachable!("sbom returns before this match"),
    };
    write_envelope(&env.0, &env.1)?;
    Ok(ExitCode::SUCCESS)
}

/// Capture an SBOM: bare CycloneDX document at `out`, sidecar envelope
/// at `<out>.envelope.json`.
///
/// The two-file split is deliberate. Every other capturer writes one
/// canonical envelope, but a CycloneDX BOM has to stay a standalone,
/// schema-valid document for an assessor's tooling to read it, so the
/// envelope sits beside it and pins its hash instead of wrapping it.
#[cfg(feature = "deps")]
fn sbom(
    path: &std::path::Path,
    ecosystem: Option<&str>,
    out: &std::path::Path,
) -> Result<ExitCode> {
    use secunit_capture::deps::sbom::{self, Ecosystem};

    let eco = ecosystem
        .map(|e| e.parse::<Ecosystem>())
        .transpose()
        .map_err(map_runtime)?;

    let mut capture = sbom::capture(path, eco).map_err(map_runtime)?;

    let bom_name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| sbom::DEFAULT_BOM_FILENAME.to_string());
    capture.set_bom_filename(&bom_name);

    let envelope_path = sidecar_path(out);
    // Validate the sidecar before either file lands, so a schema
    // failure never leaves a BOM without its provenance record.
    write_envelope(&capture.envelope, &envelope_path)?;
    capture
        .write_bom(out)
        .with_context(|| format!("write BOM to {}", out.display()))?;

    eprintln!(
        "✓ wrote {} ({} components, CycloneDX {})",
        out.display(),
        capture.envelope.result["component_count"],
        capture.envelope.result["spec_version"]
            .as_str()
            .unwrap_or("?")
    );
    Ok(ExitCode::SUCCESS)
}

/// `foo/sbom.cdx.json` -> `foo/sbom.cdx.json.envelope.json`.
///
/// Appended rather than extension-replaced so the sidecar sorts next to
/// its BOM and never collides with another capture's output.
#[cfg(feature = "deps")]
fn sidecar_path(out: &std::path::Path) -> std::path::PathBuf {
    let mut s = out.as_os_str().to_os_string();
    s.push(".envelope.json");
    std::path::PathBuf::from(s)
}

#[cfg(not(feature = "deps"))]
fn deps(_cmd: DepsCmd) -> Result<ExitCode> {
    eprintln!("error: this binary was built without the `deps` feature");
    Ok(ExitCode::from(2))
}

#[cfg(feature = "github")]
fn github(cmd: GithubCmd) -> Result<ExitCode> {
    use secunit_capture::github;
    let rt = build_runtime()?;
    // Octocrab's HTTP stack constructs a `tower::buffer::Service` at build time,
    // which spawns a worker task and therefore requires an active Tokio reactor
    // in the current thread context. Without this guard, `GhClient::from_env`
    // panics with "there is no reactor running" because it's called between
    // `Runtime::new` and the first `block_on`.
    let _rt_guard = rt.enter();
    let client = github::GhClient::from_env().map_err(map_runtime)?;
    let env_and_out = match cmd {
        GithubCmd::DependabotAlerts { repo, state, out } => {
            let (owner, repo_name) = split_repo(&repo)?;
            let env = rt
                .block_on(github::dependabot_alerts::capture(
                    &client,
                    &owner,
                    &repo_name,
                    state.as_deref(),
                ))
                .map_err(map_runtime)?;
            (env, out)
        }
        GithubCmd::BranchProtection { repo, branch, out } => {
            let (owner, repo_name) = split_repo(&repo)?;
            let env = rt
                .block_on(github::branch_protection::capture(
                    &client, &owner, &repo_name, &branch,
                ))
                .map_err(map_runtime)?;
            (env, out)
        }
        GithubCmd::OrgMembers { org, out } => {
            let env = rt
                .block_on(github::org_members::capture(&client, &org))
                .map_err(map_runtime)?;
            (env, out)
        }
        GithubCmd::AuditLog { org, since, out } => {
            let env = rt
                .block_on(github::audit_log::capture(&client, &org, &since))
                .map_err(map_runtime)?;
            (env, out)
        }
        GithubCmd::CodeqlAlerts { repo, out } => {
            let (owner, repo_name) = split_repo(&repo)?;
            let env = rt
                .block_on(github::codeql_alerts::capture(&client, &owner, &repo_name))
                .map_err(map_runtime)?;
            (env, out)
        }
    };
    write_envelope(&env_and_out.0, &env_and_out.1)?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(not(feature = "github"))]
fn github(_cmd: GithubCmd) -> Result<ExitCode> {
    eprintln!("error: this binary was built without the `github` feature");
    Ok(ExitCode::from(2))
}

#[cfg(any(feature = "deps", feature = "github"))]
fn write_envelope(env: &Envelope, out: &std::path::Path) -> Result<()> {
    let errors = secunit_capture::schema::validate(env).context("schema lookup")?;
    if !errors.is_empty() {
        eprintln!("schema validation failed for `{}`:", env.capturer);
        for e in &errors {
            eprintln!("  {e}");
        }
        return Err(anyhow::anyhow!(
            "capture output for `{}` failed schema validation ({} error(s))",
            env.capturer,
            errors.len()
        ));
    }
    env.write_to(out)
        .with_context(|| format!("write envelope to {}", out.display()))?;
    eprintln!("✓ wrote {}", out.display());
    Ok(())
}

#[cfg(any(feature = "deps", feature = "github"))]
fn build_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")
}

#[cfg(feature = "github")]
fn split_repo(s: &str) -> Result<(String, String)> {
    let (a, b) = s
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("--repo must be in `owner/name` form: got `{s}`"))?;
    Ok((a.to_string(), b.to_string()))
}

#[cfg(any(feature = "deps", feature = "github"))]
fn map_runtime(e: anyhow::Error) -> anyhow::Error {
    // Tag runtime errors so the binary's top-level can map them to
    // exit code 2. We don't have a typed error layer yet — just
    // re-throw with a marker the main loop picks up.
    e.context("capture runtime failure")
}

#[cfg(all(test, feature = "github"))]
mod tests {
    use super::*;

    /// Regression: GhClient construction goes through Octocrab::builder().build(),
    /// whose transport (tower::buffer::Service) spawns a worker task at build
    /// time. Without entering the runtime context first, this panics with
    /// "there is no reactor running". The CLI's github subcommand must hold
    /// an `rt.enter()` guard across the client construction. Tests using
    /// `#[tokio::test]` mask this because they run inside an already-entered
    /// runtime — the bug only surfaces in the synchronous CLI path that builds
    /// the runtime, then constructs the client outside its context. We use
    /// `with_base_uri` instead of `from_env` here to keep the test free of
    /// global env mutation; both go through the same Octocrab::build path.
    #[test]
    fn ghclient_build_does_not_panic_without_runtime_guard() {
        let rt = build_runtime().expect("build runtime");
        let _guard = rt.enter();
        let client =
            secunit_capture::github::GhClient::with_base_uri("https://example.invalid", Some("t"));
        assert!(
            client.is_ok(),
            "client build should succeed under rt.enter(): {}",
            client.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }
}
