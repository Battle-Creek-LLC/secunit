//! CLI subcommand implementations. Each module owns one subcommand and
//! returns `anyhow::Result<ExitCode>` so the binary can map domain errors
//! to the exit-code convention from `docs/cli.md`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use secunit_core::model::LoadedRegistry;
use secunit_core::registry::loader::{self, LoadReport};

pub mod capture;
pub mod coverage;
pub mod due;
pub mod features;
pub mod inventory;
pub mod registry;
pub mod run;
pub mod scope;
pub mod show;
pub mod status;
pub mod validate;
pub mod verify;

/// Per-invocation context shared across subcommands.
pub struct Ctx {
    pub root: PathBuf,
    pub json: bool,
    pub today: NaiveDate,
}

impl Ctx {
    pub fn load(&self) -> Result<(LoadedRegistry, LoadReport)> {
        let root = self
            .root
            .canonicalize()
            .with_context(|| format!("resolve --root {}", self.root.display()))?;
        Ok(loader::load(&root))
    }
}
