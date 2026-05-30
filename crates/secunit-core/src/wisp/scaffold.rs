//! `wisp init` — scaffold the generic, operator-owned partials.

use std::fs;

use anyhow::{Context, Result};

use super::{template, Format, InitOptions, InitReport};

/// Write the bundled generic partials into `opts.dir`. Idempotent: existing
/// files are skipped unless `opts.force` is set. Also seeds `logo.svg` (from
/// `opts.logo` if given, else the bundled shield).
pub fn init(opts: &InitOptions) -> Result<InitReport> {
    let partials = template::bundled(opts.format)?;

    fs::create_dir_all(&opts.dir)
        .with_context(|| format!("create template dir {}", opts.dir.display()))?;

    let mut written = Vec::new();
    let mut skipped = Vec::new();

    for &(name, body) in partials {
        let dest = opts.dir.join(name);
        if dest.exists() && !opts.force {
            skipped.push(name.to_string());
            continue;
        }
        fs::write(&dest, body).with_context(|| format!("write {}", dest.display()))?;
        written.push(name.to_string());
    }

    // Seed the logo. A custom `--logo` is copied verbatim under its own
    // extension; otherwise the bundled shield is written as `logo.svg`.
    match &opts.logo {
        Some(src) => {
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("svg")
                .to_ascii_lowercase();
            let dest = opts.dir.join(format!("logo.{ext}"));
            if dest.exists() && !opts.force {
                skipped.push(format!("logo.{ext}"));
            } else {
                fs::copy(src, &dest).with_context(|| {
                    format!("copy logo {} -> {}", src.display(), dest.display())
                })?;
                written.push(format!("logo.{ext}"));
            }
        }
        None => {
            let dest = opts.dir.join("logo.svg");
            if dest.exists() && !opts.force {
                skipped.push("logo.svg".to_string());
            } else {
                fs::write(&dest, template::LOGO_SVG)
                    .with_context(|| format!("write {}", dest.display()))?;
                written.push("logo.svg".to_string());
            }
        }
    }

    Ok(InitReport {
        dir: opts.dir.clone(),
        format: opts.format,
        written,
        skipped,
    })
}

/// Convenience for the common case: scaffold the default Typst template dir.
pub fn init_default(dir: impl Into<std::path::PathBuf>) -> Result<InitReport> {
    init(&InitOptions {
        dir: dir.into(),
        format: Format::Typst,
        logo: None,
        force: false,
    })
}
