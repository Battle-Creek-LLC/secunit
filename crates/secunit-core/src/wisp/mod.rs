//! WISP → PDF export.
//!
//! Renders an organisation's Written Information Security Program — the
//! markdown policy set under `security/` — into a single branded PDF with a
//! cover page, table of contents, page numbers, and a provenance stamp.
//!
//! Design and decisions live in `docs/wisp-pdf-export.md`. The short version:
//!
//! - The default renderer is **Typst** (pure Rust, compiled into the binary),
//!   so a plain `cargo install` produces a working renderer with no external
//!   toolchain. WeasyPrint / Chromium are opt-in HTML backends (not yet wired).
//! - The branding **partials are required, operator-owned files** (Typst
//!   templates by default), scaffolded by [`init`] and checked by [`export`].
//! - Provenance reuses the same primitives as the evidence chain: the WISP
//!   repo's `git_head` commit and a SHA-256 over the assembled markdown.
//!
//! This module owns the format-neutral pipeline:
//! source resolution → markdown assembly → [`doc::WispDoc`] → Typst emission.
//! The final Typst→PDF compile (the `typst` crate) is wired separately; see
//! [`render`].

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub mod doc;
pub mod markdown;
pub mod render;
pub mod scaffold;
pub mod source;
pub mod template;
pub mod typst_emit;

pub use doc::WispDoc;
pub use scaffold::init;
pub use source::export;

/// Partial flavour. Determined by the chosen renderer: the in-binary Typst
/// backend uses `.typ` partials; the opt-in HTML backends use HTML + CSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Typst,
    Html,
}

impl Format {
    /// The required partial filenames for this format. `init` writes all of
    /// these and `export` checks for the same set.
    pub fn required_partials(self) -> &'static [&'static str] {
        match self {
            Format::Typst => &["theme.typ", "header.typ", "footer.typ", "cover.typ", "toc.typ"],
            Format::Html => &["theme.css", "header.html", "footer.html", "cover.html", "toc.html"],
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Typst => "typst",
            Format::Html => "html",
        })
    }
}

impl FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "typst" | "typ" => Ok(Format::Typst),
            "html" => Ok(Format::Html),
            other => Err(format!("unknown partial format `{other}` (expected `typst` or `html`)")),
        }
    }
}

/// Rendering backend. Only `Typst` is implemented today; the others are
/// reserved for the opt-in HTML backends documented in the spec (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Renderer {
    #[default]
    Typst,
    Weasyprint,
    Chromium,
}

impl Renderer {
    /// The partial format this backend consumes.
    pub fn format(self) -> Format {
        match self {
            Renderer::Typst => Format::Typst,
            Renderer::Weasyprint | Renderer::Chromium => Format::Html,
        }
    }
}

impl fmt::Display for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Renderer::Typst => "typst",
            Renderer::Weasyprint => "weasyprint",
            Renderer::Chromium => "chromium",
        })
    }
}

impl FromStr for Renderer {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "typst" => Ok(Renderer::Typst),
            "weasyprint" => Ok(Renderer::Weasyprint),
            "chromium" | "chrome" => Ok(Renderer::Chromium),
            other => Err(format!(
                "unknown renderer `{other}` (expected `typst`, `weasyprint`, or `chromium`)"
            )),
        }
    }
}

/// Document status, surfaced on the cover/footer and as a watermark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Status {
    Approved,
    Draft,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Approved => "APPROVED",
            Status::Draft => "DRAFT",
        })
    }
}

/// Inputs to [`init`]. `dir` is the template directory to scaffold into.
#[derive(Debug, Clone)]
pub struct InitOptions {
    pub dir: PathBuf,
    pub format: Format,
    /// Optional logo to seed instead of the bundled shield.
    pub logo: Option<PathBuf>,
    /// Overwrite existing partials instead of skipping them.
    pub force: bool,
}

/// Outcome of [`init`].
#[derive(Debug, Clone, Serialize)]
pub struct InitReport {
    pub dir: PathBuf,
    pub format: Format,
    pub written: Vec<String>,
    pub skipped: Vec<String>,
}

/// Inputs to [`export`]. `None` fields fall back to `_config.yaml`'s `wisp:`
/// block and then to built-in defaults.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Registry root (used for git provenance + config defaults).
    pub root: PathBuf,
    /// Date to stamp as "generated"/effective fallback (the CLI's `--today`).
    pub today: chrono::NaiveDate,
    /// WISP source file or directory (overrides config).
    pub source: Option<PathBuf>,
    /// Template directory holding the partials (overrides config).
    pub template: Option<PathBuf>,
    /// Output PDF path (defaults to `wisp-<version>.pdf`).
    pub output: Option<PathBuf>,
    /// Include a table of contents (overrides config; default on).
    pub toc: Option<bool>,
    /// Render page numbers (overrides config; default on).
    pub page_numbers: Option<bool>,
    /// Force the DRAFT watermark regardless of working-tree state.
    pub draft: bool,
    pub allow_dirty: bool,
    /// Render backend (overrides config; default Typst).
    pub renderer: Option<Renderer>,
}

/// Outcome of [`export`].
#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub output: PathBuf,
    pub version: String,
    pub effective_date: String,
    pub commit: String,
    pub content_hash: String,
    pub status: Status,
    pub renderer: Renderer,
    /// Source files assembled, in order, so the operator can see the layout.
    pub sections: Vec<String>,
    /// Page count, once a backend that reports it is wired.
    pub pages: Option<u32>,
}
