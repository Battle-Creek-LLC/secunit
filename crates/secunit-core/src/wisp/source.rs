//! WISP source resolution, assembly, provenance, and the `export` entry point.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::evidence::{hasher, runner};
use crate::model::Config;

use super::doc::{WispDoc, WispMeta};
use super::{render, template, typst_emit, ExportOptions, ExportReport, Renderer, Status};

// ---------- config view -----------------------------------------------------

/// The `wisp:` block in `_config.yaml`, read from `Config::extras`.
#[derive(Debug, Default, Deserialize)]
struct WispConfig {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    order: Vec<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    renderer: Option<String>,
    #[serde(default)]
    metadata: WispMetaConfig,
    #[serde(default)]
    toc: TocConfig,
    #[serde(default)]
    output: OutputConfig,
}

#[derive(Debug, Default, Deserialize)]
struct WispMetaConfig {
    #[serde(default)]
    org: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    classification: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TocConfig {
    #[serde(default = "yes")]
    enabled: bool,
}

impl Default for TocConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Deserialize)]
struct OutputConfig {
    #[serde(default = "yes")]
    page_numbers: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self { page_numbers: true }
    }
}

fn yes() -> bool {
    true
}

impl WispConfig {
    fn from_config(config: &Config) -> Self {
        config
            .extras
            .get("wisp")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}

/// Front-matter we recognise on the first source file.
#[derive(Debug, Default, Deserialize)]
struct FrontMatter {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    effective_date: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    classification: Option<String>,
}

// ---------- public entry point ----------------------------------------------

/// Render the latest WISP to PDF (or, until the in-binary Typst compile is
/// wired, to a composed `.typ` document). See [`super`] for the design.
pub fn export(opts: &ExportOptions) -> Result<ExportReport> {
    let root = opts
        .root
        .canonicalize()
        .with_context(|| format!("resolve root {}", opts.root.display()))?;

    let (reg, _report) = crate::registry::loader::load(&root);
    let wisp_cfg = WispConfig::from_config(&reg.config);

    let renderer: Renderer = opts
        .renderer
        .or_else(|| wisp_cfg.renderer.as_deref().and_then(|s| s.parse().ok()))
        .unwrap_or_default();

    // ---- assemble the source ----
    let source_dir = resolve_source(&root, opts.source.as_deref(), &wisp_cfg)?;
    let (body_markdown, hash_src, sections, first_file) = assemble(&source_dir, &wisp_cfg)?;
    if sections.is_empty() {
        bail!(
            "no WISP markdown found under {} — set `wisp.source` in _config.yaml \
             or pass --source",
            source_dir.display()
        );
    }
    let fm = first_file
        .as_deref()
        .map(extract_front_matter)
        .unwrap_or_default();

    // ---- provenance + metadata ----
    // Refuse to stamp a commit pointer onto a tree with uncommitted changes
    // unless the operator opts in; mark such exports as `-dirty` so the
    // provenance line is honest about it.
    let dirty = runner::git_is_dirty(&root).unwrap_or(false);
    if dirty && !opts.allow_dirty {
        bail!(
            "WISP source tree has uncommitted changes — commit them so the \
             provenance commit matches the rendered content, or pass \
             --allow-dirty to export anyway."
        );
    }
    let commit = runner::git_head(&root)
        .ok()
        .map(|sha| sha.get(..12).unwrap_or(sha.as_str()).to_string())
        .map(|sha| if dirty { format!("{sha}-dirty") } else { sha })
        .unwrap_or_else(|| "uncommitted".to_string());
    // Hash the policy content itself (without the internal anchor markers), so
    // the provenance digest stays a pure function of the source text.
    let full_hash = hasher::sha256_bytes(hash_src.as_bytes());
    let content_hash = format!("sha256:{}", &full_hash[..full_hash.len().min(12)]);

    let version = opts_version(&wisp_cfg, &fm).unwrap_or_else(|| "0.0.0".to_string());
    let effective_date = fm
        .effective_date
        .clone()
        .unwrap_or_else(|| opts.today.to_string());
    let status = if opts.draft {
        Status::Draft
    } else {
        Status::Approved
    };

    let org = wisp_cfg
        .metadata
        .org
        .clone()
        .or_else(|| reg.config.org.as_ref().and_then(|o| o.name.clone()))
        .unwrap_or_else(|| "Organization".to_string());
    let title = wisp_cfg
        .metadata
        .title
        .clone()
        .or(fm.title.clone())
        .unwrap_or_else(|| "Written Information Security Program".to_string());
    let classification = wisp_cfg
        .metadata
        .classification
        .clone()
        .or(fm.classification.clone())
        .unwrap_or_else(|| "Confidential".to_string());

    // ---- require the partials ----
    let template_dir = template::resolve_dir(
        &root,
        opts.template
            .as_deref()
            .or(wisp_cfg.template.as_deref().map(Path::new)),
    );
    let format = renderer.format();
    template::require_complete(&template_dir, format)?;
    // The cover/header reference whatever logo the operator scaffolded; a custom
    // `wisp init --logo brand.png` writes `logo.png`, so resolve the real file
    // rather than assuming `logo.svg`.
    let logo = resolve_logo(&template_dir).ok_or_else(|| {
        anyhow!(
            "no logo found in {} — run `secunit wisp init` to scaffold one, or \
             add a logo.<svg|png|jpg|jpeg|webp|gif> file.",
            template_dir.display()
        )
    })?;

    // ---- build doc + emit ----
    let doc = WispDoc {
        meta: WispMeta {
            org,
            title,
            version: version.clone(),
            effective_date: effective_date.clone(),
            classification,
            status,
            logo,
            commit: commit.clone(),
            content_hash: content_hash.clone(),
            generated_at: opts.today.to_string(),
        },
        body_markdown,
        sections: sections.clone(),
    };

    let toc = opts.toc.unwrap_or(wisp_cfg.toc.enabled);
    let page_numbers = opts.page_numbers.unwrap_or(wisp_cfg.output.page_numbers);
    let typst_source = typst_emit::emit(&doc, typst_emit::EmitOptions { toc, page_numbers });

    let output = opts
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("wisp-{version}.pdf")));

    let result = render::render(
        renderer,
        &render::RenderRequest {
            doc: &doc,
            template_dir: &template_dir,
            typst_source: &typst_source,
            output: &output,
        },
    )?;

    Ok(ExportReport {
        output,
        version,
        effective_date,
        commit,
        content_hash,
        status,
        renderer,
        sections,
        pages: result.pages,
    })
}

// ---------- source resolution + assembly ------------------------------------

fn resolve_source(root: &Path, override_src: Option<&Path>, cfg: &WispConfig) -> Result<PathBuf> {
    let candidate = match override_src {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => root.join(p),
        None => match &cfg.source {
            Some(s) => root.join(s),
            None => root.join("security"),
        },
    };
    if !candidate.exists() {
        bail!(
            "WISP source {} does not exist — set `wisp.source` in _config.yaml or \
             pass --source <file|dir>",
            candidate.display()
        );
    }
    Ok(candidate)
}

/// Returns `(render_markdown, hash_source, ordered_section_names, first_file)`.
///
/// `render_markdown` carries the `<!--wisp:anchor …-->` markers the converter
/// turns into per-section anchors; `hash_source` is the same content *without*
/// the markers, so the provenance digest stays a pure function of the policy
/// text regardless of the internal-link machinery.
fn assemble(
    source: &Path,
    cfg: &WispConfig,
) -> Result<(String, String, Vec<String>, Option<PathBuf>)> {
    let files = if source.is_file() {
        vec![source.to_path_buf()]
    } else {
        ordered_markdown(source, &cfg.order)?
    };

    let sections: Vec<String> = files.iter().map(|p| rel_name(source, p)).collect();
    // Unique, collision-resolved label per section; the converter recomputes
    // the same slugs from `sections`, so anchors and links agree.
    let slugs = super::markdown::section_slugs(&sections);

    let mut body = String::new();
    let mut hash_src = String::new();
    for (i, path) in files.iter().enumerate() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("read WISP source {}", path.display()))?;
        // Strip front-matter from each file before concatenating.
        let trimmed = strip_front_matter(&content).trim_end().to_string();
        if i > 0 {
            body.push_str("\n\n");
            hash_src.push_str("\n\n");
        }
        // Render body: anchor marker so `.md` links to this file resolve to an
        // in-document anchor instead of trying to open a sibling file.
        body.push_str("<!--wisp:anchor ");
        body.push_str(&slugs[i]);
        body.push_str("-->\n\n");
        body.push_str(&trimmed);
        body.push('\n');
        // Hash source: the policy text only.
        hash_src.push_str(&trimmed);
        hash_src.push('\n');
    }

    Ok((body, hash_src, sections, files.first().cloned()))
}

/// Collect `*.md` files under `dir`, ordered by `order` patterns if given,
/// else lexically with any `*overview*`/`*introduction*` floated to the front.
fn ordered_markdown(dir: &Path, order: &[String]) -> Result<Vec<PathBuf>> {
    let mut md: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read WISP dir {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    md.sort();

    if order.is_empty() {
        md.sort_by_key(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let lead = name.contains("overview") || name.contains("introduction");
            (!lead, name.to_string())
        });
        return Ok(md);
    }

    // Apply the explicit order: each pattern pulls matching files (in lexical
    // order) that haven't been placed yet; a trailing catch-all like `*.md`
    // sweeps the remainder.
    let mut placed: Vec<PathBuf> = Vec::new();
    let mut remaining = md;
    for pat in order {
        let (mut hits, rest): (Vec<_>, Vec<_>) = remaining.into_iter().partition(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| glob_match(pat, n))
                .unwrap_or(false)
        });
        hits.sort();
        placed.append(&mut hits);
        remaining = rest;
    }
    placed.append(&mut remaining); // anything unmatched, lexical order
    Ok(placed)
}

/// The logo filename within `template_dir` to reference from the partials —
/// whichever supported image `wisp init` scaffolded. `None` if none is present.
fn resolve_logo(template_dir: &Path) -> Option<String> {
    ["logo.svg", "logo.png", "logo.jpg", "logo.jpeg", "logo.webp", "logo.gif"]
        .into_iter()
        .find(|name| template_dir.join(name).exists())
        .map(str::to_string)
}

fn rel_name(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------- front-matter + helpers ------------------------------------------

fn opts_version(cfg: &WispConfig, fm: &FrontMatter) -> Option<String> {
    cfg.version.clone().or_else(|| fm.version.clone())
}

/// Split leading `---\n ... \n---` YAML front-matter from a markdown string,
/// returning the body. Tolerant: if there's no closing fence, returns input.
fn strip_front_matter(content: &str) -> &str {
    let Some(rest) = content.strip_prefix("---\n") else {
        return content;
    };
    match rest.find("\n---") {
        Some(idx) => {
            let after = &rest[idx + 4..];
            after.strip_prefix('\n').unwrap_or(after)
        }
        None => content,
    }
}

fn extract_front_matter(path: &Path) -> FrontMatter {
    let Ok(content) = fs::read_to_string(path) else {
        return FrontMatter::default();
    };
    // Reuse the shared front-matter fence parser so all call sites agree.
    crate::skills::frontmatter(&content)
        .and_then(|fm| serde_yaml::from_str(fm).ok())
        .unwrap_or_default()
}

/// Minimal `*` glob over a single path segment. Supports any number of `*`
/// wildcards (each matching any run of characters); all other characters match
/// literally. Sufficient for filename patterns like `*-overview.md` or `*.md`.
fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name; // no wildcard
    }
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // Must match at the start.
            if !name[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if i == parts.len() - 1 {
            // Must match at the end.
            return name[pos..].ends_with(part);
        } else {
            match name[pos..].find(part) {
                Some(found) => pos += found + part.len(),
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_suffix_and_catchall() {
        assert!(glob_match("*.md", "access-control-policy.md"));
        assert!(glob_match("*-overview.md", "company-overview.md"));
        assert!(!glob_match("*-overview.md", "access-policy.md"));
        assert!(glob_match("access-control-policy.md", "access-control-policy.md"));
        assert!(glob_match("*", "anything.md"));
    }

    #[test]
    fn strips_front_matter() {
        let md = "---\nversion: 1.2.0\n---\n# Body\n";
        assert_eq!(strip_front_matter(md), "# Body\n");
        let none = "# No front matter\n";
        assert_eq!(strip_front_matter(none), none);
    }
}
