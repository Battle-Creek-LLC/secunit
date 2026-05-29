//! The backend-neutral assembled document.

use serde::Serialize;

use super::Status;

/// Document metadata exposed to the partials as `ctx` (see docs §9). Field
/// names match the keys the bundled Typst partials read.
#[derive(Debug, Clone, Serialize)]
pub struct WispMeta {
    pub org: String,
    pub title: String,
    pub version: String,
    pub effective_date: String,
    pub classification: String,
    pub status: Status,
    /// Path to the logo *within the template directory* (e.g. `logo.svg`).
    pub logo: String,
    pub commit: String,
    pub content_hash: String,
    pub generated_at: String,
}

/// The assembled WISP: metadata plus the concatenated markdown body and the
/// list of source files that produced it (in assembly order).
#[derive(Debug, Clone)]
pub struct WispDoc {
    pub meta: WispMeta,
    /// The concatenated markdown for the whole document.
    pub body_markdown: String,
    /// Source files assembled, in order (relative to the source root).
    pub sections: Vec<String>,
}
