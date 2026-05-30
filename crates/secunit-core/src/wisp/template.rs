//! The bundled generic partials, plus locating/validating an operator's
//! template directory.
//!
//! Partials are required, operator-owned files. `init` materialises the
//! bundled defaults below; `export` checks that the required set is present
//! before rendering and otherwise fails with a pointer to `wisp init`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use super::Format;

/// Default template directory, relative to the registry root.
pub const DEFAULT_DIR: &str = "templates/wisp";

/// The bundled generic Typst partials, embedded into the binary.
pub const TYPST_PARTIALS: &[(&str, &str)] = &[
    ("theme.typ", include_str!("assets/typst/theme.typ")),
    ("header.typ", include_str!("assets/typst/header.typ")),
    ("footer.typ", include_str!("assets/typst/footer.typ")),
    ("cover.typ", include_str!("assets/typst/cover.typ")),
    ("toc.typ", include_str!("assets/typst/toc.typ")),
];

/// The bundled default logo (the secunit shield).
pub const LOGO_SVG: &str = include_str!("assets/logo.svg");

/// The bundled partials for a format. HTML partials are not bundled yet (the
/// HTML backends are opt-in / not wired), so requesting them is an error.
pub fn bundled(format: Format) -> Result<&'static [(&'static str, &'static str)]> {
    match format {
        Format::Typst => Ok(TYPST_PARTIALS),
        Format::Html => bail!(
            "HTML partials are not available yet — the HTML render backends \
             (WeasyPrint/Chromium) are opt-in and not wired. Use `--format typst`."
        ),
    }
}

/// Detect which partial format a template directory holds, by probing for the
/// format-defining theme file.
pub fn detect_format(dir: &Path) -> Option<Format> {
    if dir.join("theme.typ").exists() {
        Some(Format::Typst)
    } else if dir.join("theme.css").exists() {
        Some(Format::Html)
    } else {
        None
    }
}

/// Confirm every required partial for `format` exists in `dir`. Returns the
/// list of missing filenames (empty == complete).
pub fn missing_partials(dir: &Path, format: Format) -> Vec<&'static str> {
    format
        .required_partials()
        .iter()
        .copied()
        .filter(|name| !dir.join(name).exists())
        .collect()
}

/// Fail with an actionable error if the template directory is absent or
/// incomplete. This is the gate that makes the partials *required*.
pub fn require_complete(dir: &Path, format: Format) -> Result<()> {
    if !dir.exists() {
        bail!(
            "WISP template directory `{}` does not exist. Run `secunit wisp init` \
             to scaffold the {} partials, review and commit them, then re-run export.",
            dir.display(),
            format
        );
    }
    let missing = missing_partials(dir, format);
    if !missing.is_empty() {
        bail!(
            "WISP template `{}` is missing required partial(s): {}. Run \
             `secunit wisp init --force` to restore the defaults, or add them by hand.",
            dir.display(),
            missing.join(", ")
        );
    }
    Ok(())
}

/// Resolve the template directory: explicit override, else `<root>/templates/wisp`.
pub fn resolve_dir(root: &Path, override_dir: Option<&Path>) -> PathBuf {
    match override_dir {
        Some(d) if d.is_absolute() => d.to_path_buf(),
        Some(d) => root.join(d),
        None => root.join(DEFAULT_DIR),
    }
}
