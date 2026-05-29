//! `secunit wisp` — scaffold the PDF partials (`init`) and render the latest
//! WISP to a branded PDF (`export`). See `docs/wisp-pdf-export.md`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Result};
use secunit_core::wisp::{self, ExportOptions, Format, InitOptions, Renderer};

use super::Ctx;

/// Resolve a possibly-relative path against the registry root.
fn under_root(root: &Path, p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        root.join(p)
    }
}

pub fn init(
    ctx: &Ctx,
    dir: Option<PathBuf>,
    format: &str,
    logo: Option<PathBuf>,
    force: bool,
) -> Result<ExitCode> {
    let format: Format = format.parse().map_err(|e: String| anyhow!(e))?;
    let dir = match dir {
        Some(d) => under_root(&ctx.root, d),
        None => ctx.root.join(wisp::template::DEFAULT_DIR),
    };
    let logo = logo.map(|l| under_root(&ctx.root, l));

    let report = wisp::init(&InitOptions {
        dir,
        format,
        logo,
        force,
    })?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "scaffolded {} partials in {}",
        report.format,
        report.dir.display()
    );
    if !report.written.is_empty() {
        println!("  wrote:   {}", report.written.join(", "));
    }
    if !report.skipped.is_empty() {
        println!(
            "  skipped: {} (already present; use --force to overwrite)",
            report.skipped.join(", ")
        );
    }
    println!("\nReview and commit the partials, then run `secunit wisp export`.");
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
pub fn export(
    ctx: &Ctx,
    output: Option<PathBuf>,
    source: Option<PathBuf>,
    template: Option<PathBuf>,
    toc: bool,
    no_toc: bool,
    page_numbers: bool,
    no_page_numbers: bool,
    draft: bool,
    allow_dirty: bool,
    renderer: Option<String>,
) -> Result<ExitCode> {
    let renderer: Option<Renderer> = match renderer {
        Some(s) => Some(s.parse().map_err(|e: String| anyhow!(e))?),
        None => None,
    };
    let toc = tri_state(toc, no_toc);
    let page_numbers = tri_state(page_numbers, no_page_numbers);

    let report = wisp::export(&ExportOptions {
        root: ctx.root.clone(),
        today: ctx.today,
        source: source.map(|s| under_root(&ctx.root, s)),
        template: template.map(|t| under_root(&ctx.root, t)),
        output,
        toc,
        page_numbers,
        draft,
        allow_dirty,
        renderer,
    })?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!("WISP export — {} v{}", report.status, report.version);
    println!("  output:    {}", report.output.display());
    println!("  renderer:  {}", report.renderer);
    println!("  effective: {}", report.effective_date);
    println!("  provenance: {} · {}", report.commit, report.content_hash);
    println!("  sections:  {}", report.sections.join(", "));
    match report.pages {
        Some(n) => println!("  pages:     {n}"),
        None => println!(
            "  note:      Typst document generated; in-binary PDF compilation is not \
             wired yet (see FIXES.md)."
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// `--flag` / `--no-flag` → `Some(true)` / `Some(false)` / `None`.
fn tri_state(on: bool, off: bool) -> Option<bool> {
    if off {
        Some(false)
    } else if on {
        Some(true)
    } else {
        None
    }
}
