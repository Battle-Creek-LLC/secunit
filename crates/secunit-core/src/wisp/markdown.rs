//! Markdown → Typst conversion, driven by `pulldown-cmark`.
//!
//! Policy documents are arbitrary prose: they contain `$`, `@`, `#`, tables,
//! and other characters that are syntactically meaningful in Typst markup. So
//! we don't pass markdown through as Typst — we parse it into events and emit
//! Typst, escaping every text run. Headings become `=` so Typst's native
//! `outline()` builds the table of contents and PDF bookmarks; emphasis,
//! lists, code, blockquotes, links, rules, and tables map to their Typst forms.
//!
//! Block separation is explicit: every block element (heading, list, table,
//! blockquote, code block) is preceded by a blank line so Typst never folds a
//! list into the paragraph above it. The WISP also uses `**Bold**` lines as
//! ad-hoc sub-headings; a paragraph that is nothing but one bold run is
//! promoted to a real (level-4) heading so it gains weight and spacing instead
//! of bleeding into the following list.

use std::collections::{HashMap, HashSet};

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Convert a markdown document to Typst markup.
///
/// `sections` carries the relative names of the assembled source files. A link
/// whose target is one of those files (e.g. the policy index's
/// `[Access Control Policy](access-control-policy.md)`) is rewired into an
/// internal cross-reference to that section's heading, so it jumps within the
/// PDF instead of trying to open a sibling `.md` file. Section anchors are
/// placed by the `<!--wisp:anchor …-->` markers that assembly injects ahead of
/// each file.
pub fn to_typst(markdown: &str, sections: &[String]) -> String {
    // Per-section unique labels (collision-resolved, deterministic by order)
    // and a basename→label map for resolving `.md` links to the right anchor.
    let slugs = section_slugs(sections);
    let known_slugs: HashSet<&str> = slugs.iter().map(String::as_str).collect();
    let link_map: HashMap<&str, &str> = sections
        .iter()
        .map(|s| basename(s))
        .zip(slugs.iter().map(String::as_str))
        .collect();
    // Anchors already emitted, so a duplicate/forged marker can't produce a
    // second label with the same name (which Typst rejects).
    let mut emitted_anchors: HashSet<String> = HashSet::new();

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);

    let mut out = String::with_capacity(markdown.len() * 2);
    let mut list_stack: Vec<Option<u64>> = Vec::new();

    // Table accumulation.
    let mut row_cells: Vec<String> = Vec::new();
    let mut cell_buf: Option<String> = None;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    // Top-level paragraph buffering, used to detect "bold-only" pseudo-headings.
    let mut para_buf: Option<String> = None;
    let mut strong_depth: u32 = 0;
    let mut para_strong_runs: u32 = 0;
    let mut para_has_plain: bool = false;

    // Inside a fenced code block, text is emitted verbatim (never escaped).
    let mut in_code = false;

    for event in Parser::new_ext(markdown, opts) {
        match event {
            // ---- tables: capture cells, emit on table end ------------------
            Event::Start(Tag::Table(_)) => table_rows.clear(),
            Event::End(TagEnd::Table) => {
                ensure_blank(&mut out);
                emit_table(&mut out, &table_rows);
                table_rows.clear();
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => row_cells.clear(),
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                table_rows.push(std::mem::take(&mut row_cells))
            }
            Event::Start(Tag::TableCell) => cell_buf = Some(String::new()),
            Event::End(TagEnd::TableCell) => row_cells.push(cell_buf.take().unwrap_or_default()),

            // ---- headings --------------------------------------------------
            Event::Start(Tag::Heading { level, .. }) => {
                ensure_blank(&mut out);
                for _ in 0..heading_depth(level) {
                    out.push('=');
                }
                out.push(' ');
            }
            Event::End(TagEnd::Heading(_)) => out.push_str("\n\n"),

            // ---- paragraphs (buffered only at the top level) ---------------
            Event::Start(Tag::Paragraph) if list_stack.is_empty() && cell_buf.is_none() => {
                para_buf = Some(String::new());
                para_strong_runs = 0;
                para_has_plain = false;
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if let Some(buf) = para_buf.take() {
                    let trimmed = buf.trim();
                    ensure_blank(&mut out);
                    if para_strong_runs == 1 && !para_has_plain && is_wrapped_bold(trimmed) {
                        // `**Sub-heading**` on its own line → real sub-heading.
                        out.push_str("==== ");
                        out.push_str(trimmed[1..trimmed.len() - 1].trim());
                    } else {
                        out.push_str(trimmed);
                    }
                    out.push_str("\n\n");
                }
                // Paragraphs inside list items emit nothing here; the item's
                // inline text already went straight to `out`.
            }

            // ---- lists -----------------------------------------------------
            Event::Start(Tag::List(start)) => {
                if list_stack.is_empty() {
                    ensure_blank(&mut out);
                } else {
                    ensure_newline(&mut out);
                }
                list_stack.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                if list_stack.is_empty() {
                    ensure_blank(&mut out);
                }
            }
            Event::Start(Tag::Item) => {
                ensure_newline(&mut out);
                for _ in 0..list_stack.len().saturating_sub(1) {
                    out.push_str("  ");
                }
                match list_stack.last() {
                    // Typst auto-numbers `+`, so repeated `1.` in the source
                    // still renders 1, 2, 3 — and nested enums get a, b / i, ii.
                    Some(Some(_)) => out.push_str("+ "),
                    _ => out.push_str("- "),
                }
            }
            Event::End(TagEnd::Item) => ensure_newline(&mut out),

            // ---- blockquote / code / rule ---------------------------------
            Event::Start(Tag::BlockQuote(_)) => {
                ensure_blank(&mut out);
                out.push_str("#quote(block: true)[\n");
            }
            Event::End(TagEnd::BlockQuote(_)) => out.push_str("]\n\n"),
            Event::Start(Tag::CodeBlock(kind)) => {
                ensure_blank(&mut out);
                let lang = match kind {
                    CodeBlockKind::Fenced(l) if !l.is_empty() => l.to_string(),
                    _ => String::new(),
                };
                out.push_str("```");
                out.push_str(&lang);
                out.push('\n');
                in_code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                out.push_str("```\n\n");
            }
            Event::Rule => {
                ensure_blank(&mut out);
                out.push_str("#line(length: 100%)\n\n");
            }

            // ---- inline ----------------------------------------------------
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => {
                sink(&mut out, &mut cell_buf, &mut para_buf).push('_')
            }
            Event::Start(Tag::Strong) => {
                strong_depth += 1;
                if para_buf.is_some() && strong_depth == 1 {
                    para_strong_runs += 1;
                }
                sink(&mut out, &mut cell_buf, &mut para_buf).push('*');
            }
            Event::End(TagEnd::Strong) => {
                strong_depth = strong_depth.saturating_sub(1);
                sink(&mut out, &mut cell_buf, &mut para_buf).push('*');
            }
            Event::Start(Tag::Strikethrough) => {
                sink(&mut out, &mut cell_buf, &mut para_buf).push_str("#strike[")
            }
            Event::End(TagEnd::Strikethrough) => {
                sink(&mut out, &mut cell_buf, &mut para_buf).push(']')
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let internal = link_target_slug(&dest_url, &link_map);
                let s = sink(&mut out, &mut cell_buf, &mut para_buf);
                match internal {
                    // Internal cross-reference: link by label, no quotes.
                    Some(slug) => {
                        s.push_str("#link(<");
                        s.push_str(slug);
                        s.push_str(">)[");
                    }
                    None => {
                        s.push_str("#link(\"");
                        s.push_str(&escape_typst_string(&dest_url));
                        s.push_str("\")[");
                    }
                }
            }
            Event::End(TagEnd::Link) => sink(&mut out, &mut cell_buf, &mut para_buf).push(']'),
            Event::Code(text) => {
                // Emit via `#raw("…")` rather than backtick markup: the string
                // form carries any character faithfully, including interior
                // backticks (which backtick-delimited raw cannot contain).
                let s = sink(&mut out, &mut cell_buf, &mut para_buf);
                s.push_str("#raw(\"");
                s.push_str(&escape_typst_string(&text));
                s.push_str("\")");
            }
            Event::Text(text) => {
                if in_code {
                    // Verbatim inside a raw block — Typst does not interpret
                    // markup between the ``` fences.
                    out.push_str(&text);
                } else {
                    if para_buf.is_some() && strong_depth == 0 && !text.trim().is_empty() {
                        para_has_plain = true;
                    }
                    sink(&mut out, &mut cell_buf, &mut para_buf).push_str(&escape_markup(&text));
                }
            }
            Event::SoftBreak => sink(&mut out, &mut cell_buf, &mut para_buf).push(' '),
            Event::HardBreak => sink(&mut out, &mut cell_buf, &mut para_buf).push_str(" \\\n"),

            Event::Html(text) | Event::InlineHtml(text) => {
                if let Some(slug) = parse_anchor_marker(&text) {
                    // Emit a zero-size labelled anchor at the section start, so
                    // the label always exists even when the section does not
                    // open with a heading. Unknown (forged) or already-emitted
                    // slugs are ignored to avoid duplicate-label compile errors.
                    if known_slugs.contains(slug.as_str()) && emitted_anchors.insert(slug.clone()) {
                        ensure_blank(&mut out);
                        out.push_str("#metadata(none) <");
                        out.push_str(&slug);
                        out.push_str(">\n\n");
                    }
                }
            }
            _ => {}
        }
    }

    // Collapse any run of 3+ newlines to a single blank line.
    normalize_blanks(&out)
}

/// Pick the active inline sink: a table cell, else a buffered paragraph, else
/// the main output.
fn sink<'a>(
    out: &'a mut String,
    cell: &'a mut Option<String>,
    para: &'a mut Option<String>,
) -> &'a mut String {
    if let Some(c) = cell.as_mut() {
        c
    } else if let Some(p) = para.as_mut() {
        p
    } else {
        out
    }
}

fn heading_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// True if `s` is `*...*` with no interior unescaped `*` — i.e. a single bold
/// run wrapping the whole string.
fn is_wrapped_bold(s: &str) -> bool {
    s.len() > 2 && s.starts_with('*') && s.ends_with('*') && !s[1..s.len() - 1].contains('*')
}

/// Ensure `s` ends with a blank line (`\n\n`) unless it is empty.
fn ensure_blank(s: &mut String) {
    if s.is_empty() {
        return;
    }
    while s.ends_with([' ', '\t', '\n']) {
        s.pop();
    }
    if !s.is_empty() {
        s.push_str("\n\n");
    }
}

/// Ensure `s` ends with at least one newline (no trailing spaces) unless empty.
fn ensure_newline(s: &mut String) {
    while s.ends_with([' ', '\t']) {
        s.pop();
    }
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
}

/// Collapse 3+ consecutive newlines into exactly two, and trim leading blanks.
fn normalize_blanks(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0u32;
    for ch in s.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else {
            newlines = 0;
            out.push(ch);
        }
    }
    out.trim_start_matches('\n').to_string()
}

/// Emit a Typst `table(...)` from collected rows (first row treated as header).
fn emit_table(out: &mut String, rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    use std::fmt::Write as _;
    out.push_str("#table(\n");
    let _ = writeln!(out, "  columns: {cols},");
    out.push_str("  inset: 6pt,\n  align: left + top,\n  stroke: 0.5pt + luma(180),\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str("  ");
        for c in 0..cols {
            let cell = row.get(c).map(String::as_str).unwrap_or("").trim();
            if i == 0 {
                let _ = write!(out, "[*{cell}*], ");
            } else {
                let _ = write!(out, "[{cell}], ");
            }
        }
        out.push('\n');
    }
    out.push_str(")\n\n");
}

/// Escape a text run for Typst *markup* mode: prefix every character that
/// could start markup/code with a backslash. Conservative but safe for prose.
fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '=' | '[' | ']' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Escape for a Typst double-quoted string literal (e.g. link URLs, `#raw`).
/// Shared with `typst_emit` so both escape values identically.
pub(crate) fn escape_typst_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The final path segment of `name` (its basename), used to match a link's
/// target file against the assembled sections.
fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// Per-section unique Typst labels, one per entry in `sections`, in order.
/// Derived from [`section_slug`] with collisions resolved by a `-2`, `-3`, …
/// suffix so two files that normalise to the same base slug still get distinct,
/// non-conflicting labels. Deterministic for a given ordered `sections`, so the
/// anchor side (assembly) and the link side (conversion) agree.
pub fn section_slugs(sections: &[String]) -> Vec<String> {
    let mut used: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(sections.len());
    for name in sections {
        let base = section_slug(name);
        let mut slug = base.clone();
        let mut n = 2u32;
        while !used.insert(slug.clone()) {
            slug = format!("{base}-{n}");
            n += 1;
        }
        out.push(slug);
    }
    out
}

/// Stable Typst label for a section, derived from its file name: the stem,
/// lowercased with every non-alphanumeric run collapsed to a single `-`, under
/// a `wisp-` namespace (e.g. `access-control-policy.md` → `wisp-access-control-policy`).
/// May collide for unusual names; [`section_slugs`] disambiguates.
pub fn section_slug(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = base
        .strip_suffix(".md")
        .or_else(|| base.strip_suffix(".markdown"))
        .unwrap_or(base);
    let mut slug = String::from("wisp-");
    let mut prev_dash = false;
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    slug.trim_end_matches('-').to_string()
}

/// If `dest` points at one of the assembled sections (matched by basename),
/// return that section's label. Only same-document `.md` references qualify;
/// absolute URLs, `mailto:`, and bare `#fragment` links stay external.
fn link_target_slug<'a>(dest: &str, link_map: &HashMap<&str, &'a str>) -> Option<&'a str> {
    if dest.contains("://") || dest.starts_with("mailto:") || dest.starts_with('#') {
        return None;
    }
    let path = dest.split('#').next().unwrap_or(dest);
    link_map.get(basename(path)).copied()
}

/// Parse a `<!--wisp:anchor SLUG-->` marker, returning the slug.
fn parse_anchor_marker(html: &str) -> Option<String> {
    let inner = html
        .trim()
        .strip_prefix("<!--wisp:anchor ")?
        .strip_suffix("-->")?;
    let slug = inner.trim();
    (!slug.is_empty()).then(|| slug.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert with no known sections (the common case for these unit tests).
    fn to_typst_default(markdown: &str) -> String {
        to_typst(markdown, &[])
    }

    #[test]
    fn converts_atx_headings() {
        let typ = to_typst_default("# Title\n\nbody\n\n## Section\n");
        assert!(typ.contains("= Title"));
        assert!(typ.contains("== Section"));
        assert!(typ.contains("body"));
    }

    #[test]
    fn escapes_typst_metacharacters_in_prose() {
        let typ = to_typst_default("Email me @ test or pay $5 to #1\n");
        assert!(typ.contains("\\@"));
        assert!(typ.contains("\\$5"));
        assert!(typ.contains("\\#1"));
    }

    #[test]
    fn list_is_separated_from_preceding_paragraph() {
        // The bug that produced inline "wall of text": a list right under a
        // paragraph must be preceded by a blank line.
        let typ = to_typst_default("Some intro.\n- one\n- two\n");
        assert!(
            typ.contains("intro.\n\n- one"),
            "list not blank-separated: {typ:?}"
        );
    }

    #[test]
    fn bold_only_paragraph_becomes_subheading() {
        let typ = to_typst_default("**General Use and Ownership**\n\n- a\n- b\n");
        assert!(
            typ.contains("==== General Use and Ownership"),
            "bold pseudo-heading not promoted: {typ:?}"
        );
    }

    #[test]
    fn inline_emphasis_is_not_promoted() {
        // `*important*` is CommonMark emphasis → Typst `_italic_`, and an inline
        // run must never be promoted to a sub-heading.
        let typ = to_typst_default("This is *important* text.\n");
        assert!(
            !typ.contains("===="),
            "inline emphasis wrongly promoted: {typ:?}"
        );
        assert!(typ.contains("_important_"), "emphasis not mapped: {typ:?}");
    }

    #[test]
    fn inline_strong_is_not_promoted() {
        // `**bold**` mid-sentence has surrounding plain text, so it must stay
        // inline (`*bold*`) and not become a `====` sub-heading.
        let typ = to_typst_default("This is **bold** text.\n");
        assert!(
            !typ.contains("===="),
            "inline strong wrongly promoted: {typ:?}"
        );
        assert!(typ.contains("*bold*"), "strong not mapped: {typ:?}");
    }

    #[test]
    fn renders_a_table() {
        let md = "| Field | Value |\n| --- | --- |\n| Owner | CTO |\n";
        let typ = to_typst_default(md);
        assert!(typ.contains("#table("));
        assert!(typ.contains("columns: 2"));
        assert!(typ.contains("[*Field*]"));
        assert!(typ.contains("[Owner]"));
    }

    #[test]
    fn preserves_code_blocks() {
        let typ = to_typst_default("```rust\nlet x = 1;\n```\n");
        assert!(typ.contains("```rust"));
        assert!(typ.contains("let x = 1;"));
    }

    #[test]
    fn no_triple_newlines() {
        let typ = to_typst_default("# H\n\n\n\npara\n");
        assert!(!typ.contains("\n\n\n"), "blank runs not collapsed: {typ:?}");
    }

    #[test]
    fn section_slug_is_namespaced_and_stable() {
        assert_eq!(
            section_slug("access-control-policy.md"),
            "wisp-access-control-policy"
        );
        assert_eq!(section_slug("security/README.md"), "wisp-readme");
        assert_eq!(section_slug("foo_bar.markdown"), "wisp-foo-bar");
    }

    #[test]
    fn rewrites_md_links_to_internal_anchors() {
        // The policy index links to a sibling .md file that is also an assembled
        // section: the link becomes an internal cross-reference, and the section
        // gets a standalone label anchor from its marker.
        let sections = vec!["access-control-policy.md".to_string()];
        let body = "<!--wisp:anchor wisp-access-control-policy-->\n\n\
                    # Access Control Policy\n\n\
                    See the [ACP](access-control-policy.md#scope) for details.\n";
        let typ = to_typst(body, &sections);
        assert!(
            typ.contains("#metadata(none) <wisp-access-control-policy>"),
            "section anchor not emitted: {typ}"
        );
        assert!(
            typ.contains("#link(<wisp-access-control-policy>)[ACP]"),
            "md link not rewired to internal ref: {typ}"
        );
        assert!(
            !typ.contains("#link(\"access-control-policy"),
            "internal link still emitted as a file URL: {typ}"
        );
    }

    #[test]
    fn anchor_is_emitted_even_without_a_heading() {
        // A section whose first block is not a heading must still get its label,
        // otherwise a link to it would reference a non-existent Typst label.
        let sections = vec!["intro.md".to_string()];
        let body = "<!--wisp:anchor wisp-intro-->\n\nJust a paragraph, no heading.\n";
        let typ = to_typst(body, &sections);
        assert!(
            typ.contains("#metadata(none) <wisp-intro>"),
            "heading-less section lost its anchor: {typ}"
        );
    }

    #[test]
    fn forged_or_unknown_anchor_markers_are_ignored() {
        // A marker whose slug is not a known section (e.g. authored in prose)
        // must not emit a stray label.
        let sections = vec!["intro.md".to_string()];
        let body = "<!--wisp:anchor wisp-intro-->\n\n# Intro\n\n\
                    <!--wisp:anchor wisp-not-a-section-->\n\nBody.\n";
        let typ = to_typst(body, &sections);
        assert!(typ.contains("#metadata(none) <wisp-intro>"));
        assert!(
            !typ.contains("wisp-not-a-section"),
            "forged anchor leaked: {typ}"
        );
    }

    #[test]
    fn section_slugs_disambiguate_collisions() {
        // `-` and `_` both collapse to the same base slug; the second gets a
        // numeric suffix so the two labels never clash.
        let s = section_slugs(&["access-control.md".into(), "access_control.md".into()]);
        assert_eq!(s, vec!["wisp-access-control", "wisp-access-control-2"]);
    }

    #[test]
    fn external_and_unknown_links_stay_literal() {
        let sections = vec!["access-control-policy.md".to_string()];
        // http(s) URLs and .md files that are NOT assembled sections stay as
        // ordinary `#link("…")` targets.
        let typ = to_typst(
            "[site](https://example.com) and [rb](runbooks/restore.md)\n",
            &sections,
        );
        assert!(
            typ.contains("#link(\"https://example.com\")[site]"),
            "{typ}"
        );
        assert!(typ.contains("#link(\"runbooks/restore.md\")[rb]"), "{typ}");
    }
}
