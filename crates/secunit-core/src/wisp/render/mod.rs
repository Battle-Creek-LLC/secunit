//! Render backends. The default `Typst` backend compiles in-binary; the
//! `Weasyprint`/`Chromium` HTML backends are opt-in and not wired yet (see
//! `docs/wisp-pdf-export.md` §8).

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use super::{doc::WispDoc, Renderer};

pub mod typst;

/// Everything a backend needs to produce the PDF.
pub struct RenderRequest<'a> {
    pub doc: &'a WispDoc,
    /// The operator's template directory (import root for Typst partials).
    pub template_dir: &'a Path,
    /// The composed top-level Typst source (`main.typ`).
    pub typst_source: &'a str,
    /// Desired PDF output path.
    pub output: &'a Path,
}

/// Result of a render.
pub struct RenderResult {
    /// Page count, if the backend reports it.
    pub pages: Option<u32>,
    /// True once a PDF was actually written.
    pub wrote_pdf: bool,
    /// Any intermediate artifact written (e.g. the `.typ` source).
    pub intermediate: Option<PathBuf>,
}

/// Dispatch to the selected backend.
pub fn render(renderer: Renderer, req: &RenderRequest) -> Result<RenderResult> {
    match renderer {
        Renderer::Typst => typst::render(req),
        Renderer::Weasyprint | Renderer::Chromium => bail!(
            "the `{renderer}` HTML render backend is not wired yet — use the default \
             `typst` renderer (it needs no external toolchain)"
        ),
    }
}
