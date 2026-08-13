//! Subprocess runner abstraction.
//!
//! `pip-audit`, `pnpm audit`, and the three CycloneDX SBOM generators
//! ship as external binaries. Capturers invoke them through this trait
//! so tests can substitute a deterministic canned-output runner without
//! touching real installs.
//!
//! Every external binary this crate shells out to is declared in
//! [`REQUIRED_TOOLS`]. Capturers call [`ensure_installed`] *before*
//! invoking one, so a missing tool produces a named remediation
//! ("install `cyclonedx-py`: `pipx install cyclonedx-bom`") instead of
//! a bare `No such file or directory (os error 2)`. `secunit doctor`
//! and `secunit capture deps --help` read the same table, so the list
//! an operator is shown and the list the capturers enforce cannot
//! drift apart.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// An external binary a capturer shells out to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalTool {
    /// The executable looked up on `PATH`. For `cargo cyclonedx` this
    /// is the `cargo-cyclonedx` subcommand binary, not `cargo`.
    pub program: &'static str,
    /// Which `capture deps` action needs it.
    pub action: &'static str,
    /// Copy-pasteable install command shown when the probe fails.
    pub install_hint: &'static str,
}

/// Every external binary the `deps` capturers require, in the order
/// `--help` and `doctor` list them.
pub const REQUIRED_TOOLS: &[ExternalTool] = &[
    ExternalTool {
        program: "pip-audit",
        action: "deps pip-audit",
        install_hint: "pip install pip-audit",
    },
    ExternalTool {
        program: "pnpm",
        action: "deps pnpm-audit",
        install_hint: "npm install -g pnpm",
    },
    ExternalTool {
        program: "cargo-cyclonedx",
        action: "deps sbom --ecosystem rust",
        install_hint: "cargo install cargo-cyclonedx",
    },
    ExternalTool {
        program: "cyclonedx-py",
        action: "deps sbom --ecosystem python",
        install_hint: "pipx install cyclonedx-bom",
    },
    ExternalTool {
        program: "cyclonedx-npm",
        action: "deps sbom --ecosystem node",
        install_hint: "npm install -g @cyclonedx/cyclonedx-npm",
    },
];

/// Look up `program` in [`REQUIRED_TOOLS`].
pub fn tool(program: &str) -> Option<&'static ExternalTool> {
    REQUIRED_TOOLS.iter().find(|t| t.program == program)
}

/// Outcome of a subprocess invocation that the capturer cares about.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub stdout: String,
    pub stderr: String,
    /// Process exit code (0 success). pip-audit and pnpm both exit
    /// non-zero when findings are present, so capturers do not treat
    /// non-zero as a hard error.
    pub exit_code: i32,
}

/// Runs a binary with arguments in a working directory.
pub trait CmdRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Result<CmdOutput>;

    /// Whether `program` resolves to an executable on `PATH`.
    ///
    /// Defaults to a real `PATH` scan; the canned test runner overrides
    /// it so unit tests never depend on what is installed.
    fn is_installed(&self, program: &str) -> bool {
        which_on_path(program).is_some()
    }
}

/// Fail with a named remediation unless `tool` is on `PATH`.
///
/// Capturers call this before the first `run`, so the operator is told
/// which binary is missing and how to install it rather than reading a
/// spawn errno.
pub fn ensure_installed(runner: &dyn CmdRunner, tool: &ExternalTool) -> Result<()> {
    if runner.is_installed(tool.program) {
        return Ok(());
    }
    Err(anyhow!(
        "`{}` is not installed or not on PATH — `secunit capture {}` shells out to it.\n\
         Install it with: {}\n\
         Run `secunit doctor` to see every external tool the compiled-in capturers expect.",
        tool.program,
        tool.action,
        tool.install_hint
    ))
}

/// First executable named `program` on `PATH`, or `None`.
///
/// Kept dependency-free rather than pulling the `which` crate: the
/// lookup is a `PATH` split plus an executable-bit check, and this
/// crate's dependency surface is audited evidence-collection code.
pub fn which_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for candidate in executable_names(program) {
            let full = dir.join(&candidate);
            if is_executable_file(&full) {
                return Some(full);
            }
        }
    }
    None
}

/// On Windows a bare name resolves through `PATHEXT`; elsewhere the
/// name is used verbatim.
#[cfg(windows)]
fn executable_names(program: &str) -> Vec<String> {
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string());
    let mut names = vec![program.to_string()];
    names.extend(
        exts.split(';')
            .filter(|e| !e.is_empty())
            .map(|e| format!("{program}{e}")),
    );
    names
}

#[cfg(not(windows))]
fn executable_names(program: &str) -> Vec<String> {
    vec![program.to_string()]
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

/// Real runner that invokes `std::process::Command`.
pub struct RealRunner;

impl CmdRunner for RealRunner {
    fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Result<CmdOutput> {
        let out = std::process::Command::new(program)
            .args(args)
            .current_dir(cwd)
            .output()
            .with_context(|| format!("invoke `{program}`"))?;
        let stdout = String::from_utf8(out.stdout)
            .map_err(|e| anyhow!("`{program}` stdout was not UTF-8: {e}"))?;
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        Ok(CmdOutput {
            stdout,
            stderr,
            exit_code: out.status.code().unwrap_or(-1),
        })
    }
}

pub mod testing {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    /// Test runner that returns a canned output for a `(program, args)`
    /// key. Panics if asked for an unregistered command — we want tests
    /// to fail loudly when capturer call sites change.
    pub struct CannedRunner {
        responses: Mutex<HashMap<String, CmdOutput>>,
        missing: Mutex<HashSet<String>>,
        /// `key -> [(path, contents)]` written relative to the runner's
        /// cwd when that command runs.
        side_effects: Mutex<HashMap<String, Vec<(String, String)>>>,
    }

    impl Default for CannedRunner {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CannedRunner {
        pub fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
                missing: Mutex::new(HashSet::new()),
                side_effects: Mutex::new(HashMap::new()),
            }
        }

        pub fn register(&self, program: &str, args: &[&str], out: CmdOutput) {
            let key = format!("{program} {}", args.join(" "));
            self.responses.lock().unwrap().insert(key, out);
        }

        /// Register a command that writes a file into its working
        /// directory, the way `cargo-cyclonedx` emits a BOM next to
        /// `Cargo.toml` instead of writing to stdout.
        pub fn register_writing(
            &self,
            program: &str,
            args: &[&str],
            filename: &str,
            contents: &str,
        ) {
            self.register_writing_many(program, args, &[(filename, contents)]);
        }

        /// Same, for the workspace case: one invocation drops a BOM
        /// beside *every* member manifest. Paths are relative to the
        /// runner's cwd and their parent directories must already exist,
        /// as a member's directory does.
        pub fn register_writing_many(&self, program: &str, args: &[&str], files: &[(&str, &str)]) {
            let key = format!("{program} {}", args.join(" "));
            self.side_effects.lock().unwrap().insert(
                key.clone(),
                files
                    .iter()
                    .map(|(p, c)| (p.to_string(), c.to_string()))
                    .collect(),
            );
            self.responses.lock().unwrap().insert(
                key,
                CmdOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                },
            );
        }

        /// Make [`CmdRunner::is_installed`] report `program` as absent.
        pub fn mark_missing(&self, program: &str) {
            self.missing.lock().unwrap().insert(program.to_string());
        }

        /// Override the exit code of an already-registered command —
        /// for exercising a generator that writes output *and* fails.
        pub fn set_exit_code(&self, program: &str, args: &[&str], exit_code: i32) {
            let key = format!("{program} {}", args.join(" "));
            let mut responses = self.responses.lock().unwrap();
            let out = responses
                .get_mut(&key)
                .unwrap_or_else(|| panic!("CannedRunner: `{key}` is not registered"));
            out.exit_code = exit_code;
        }
    }

    impl CmdRunner for CannedRunner {
        fn run(&self, program: &str, args: &[&str], cwd: &Path) -> Result<CmdOutput> {
            let key = format!("{program} {}", args.join(" "));
            if let Some(files) = self.side_effects.lock().unwrap().get(&key) {
                for (path, contents) in files {
                    std::fs::write(cwd.join(path), contents)
                        .with_context(|| format!("CannedRunner: write {path} into cwd"))?;
                }
            }
            self.responses
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .ok_or_else(|| anyhow!("CannedRunner: no response registered for `{key}`"))
        }

        /// Every tool is "installed" unless a test calls
        /// [`CannedRunner::mark_missing`], so unit tests never depend on
        /// what happens to be on the developer's or CI runner's PATH.
        fn is_installed(&self, program: &str) -> bool {
            !self.missing.lock().unwrap().contains(program)
        }
    }

    /// Runner whose every tool is absent — for exercising the
    /// missing-tool remediation path.
    pub struct MissingToolRunner;

    impl CmdRunner for MissingToolRunner {
        fn run(&self, program: &str, _args: &[&str], _cwd: &Path) -> Result<CmdOutput> {
            panic!("MissingToolRunner: `{program}` should never be invoked — the probe must fail first")
        }

        fn is_installed(&self, _program: &str) -> bool {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{CannedRunner, MissingToolRunner};
    use super::*;

    #[test]
    fn every_required_tool_names_an_action_and_install_hint() {
        for t in REQUIRED_TOOLS {
            assert!(!t.action.is_empty(), "{} has no action", t.program);
            assert!(!t.install_hint.is_empty(), "{} has no hint", t.program);
        }
    }

    #[test]
    fn missing_tool_error_names_binary_action_and_install_command() {
        let t = tool("cyclonedx-py").expect("cyclonedx-py is a required tool");
        let err = ensure_installed(&MissingToolRunner, t).expect_err("probe should fail");
        let msg = format!("{err}");
        assert!(msg.contains("cyclonedx-py"), "names the binary: {msg}");
        assert!(msg.contains("deps sbom"), "names the action: {msg}");
        assert!(
            msg.contains("pipx install cyclonedx-bom"),
            "gives the install command: {msg}"
        );
        assert!(msg.contains("secunit doctor"), "points at doctor: {msg}");
    }

    #[test]
    fn present_tool_passes_the_probe() {
        let t = tool("pip-audit").unwrap();
        assert!(ensure_installed(&CannedRunner::new(), t).is_ok());
    }

    #[test]
    fn canned_runner_can_simulate_one_missing_tool() {
        let r = CannedRunner::new();
        r.mark_missing("pnpm");
        assert!(!r.is_installed("pnpm"));
        assert!(r.is_installed("pip-audit"));
    }

    #[test]
    fn which_finds_a_binary_that_exists_and_misses_one_that_does_not() {
        // `sh` is on PATH on every platform this crate targets.
        assert!(which_on_path("sh").is_some() || which_on_path("cmd").is_some());
        assert!(which_on_path("secunit-definitely-not-a-real-binary").is_none());
    }
}
