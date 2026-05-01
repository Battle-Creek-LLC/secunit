//! Subprocess runner abstraction.
//!
//! `pip-audit` and `pnpm audit` ship as external binaries. Capturers
//! invoke them through this trait so tests can substitute a deterministic
//! canned-output runner without touching real installs.

use std::path::Path;

use anyhow::{anyhow, Context, Result};

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
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Test runner that returns a canned output for a `(program, args)`
    /// key. Panics if asked for an unregistered command — we want tests
    /// to fail loudly when capturer call sites change.
    pub struct CannedRunner {
        responses: Mutex<HashMap<String, CmdOutput>>,
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
            }
        }

        pub fn register(&self, program: &str, args: &[&str], out: CmdOutput) {
            let key = format!("{program} {}", args.join(" "));
            self.responses.lock().unwrap().insert(key, out);
        }
    }

    impl CmdRunner for CannedRunner {
        fn run(&self, program: &str, args: &[&str], _cwd: &Path) -> Result<CmdOutput> {
            let key = format!("{program} {}", args.join(" "));
            self.responses
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .ok_or_else(|| anyhow!("CannedRunner: no response registered for `{key}`"))
        }
    }
}
