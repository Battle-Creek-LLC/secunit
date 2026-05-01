//! Render `findings.md` to safe HTML.
//!
//! Pipeline: `pulldown-cmark` → string → `ammonia` allow-list. The allow
//! list is intentionally narrow — these markdown files come from skill
//! output, but the same render path is used for any operator-authored
//! note inside an evidence dir, so XSS hardening is non-negotiable.

use ammonia::Builder;
use pulldown_cmark::{html as md_html, Options, Parser};

/// Allowed tags. No `<script>`, `<iframe>`, `<object>`, no inline event
/// handlers, no `javascript:` URLs, no nested forms. `<img>` is dropped
/// outright — operators open artifacts via the editor IPC, not via
/// inline images, so the attribute surface is one less thing to police.
fn allowed_tags() -> std::collections::HashSet<&'static str> {
    [
        "a", "abbr", "b", "blockquote", "br", "code", "del", "details", "em", "h1", "h2", "h3",
        "h4", "h5", "h6", "hr", "i", "ins", "kbd", "li", "ol", "p", "pre", "s",
        "samp", "strong", "sub", "summary", "sup", "table", "tbody", "td", "tfoot", "th",
        "thead", "tr", "ul", "var",
    ]
    .into_iter()
    .collect()
}

/// Render and sanitise a markdown body. Pure function so the Rust unit
/// tests can drive every dangerous case from the OWASP cheat sheet.
pub fn render_findings(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(markdown, options);
    let mut raw = String::with_capacity(markdown.len() * 2);
    md_html::push_html(&mut raw, parser);

    Builder::new()
        .tags(allowed_tags())
        .url_schemes(["http", "https", "mailto"].into_iter().collect())
        .add_generic_attributes(["class"])
        .add_tag_attributes("a", ["href", "title"])
        .add_tag_attributes("td", ["align"])
        .add_tag_attributes("th", ["align"])
        .clean(&raw)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = render_findings("# Hello\n\nA **bold** line and a [link](https://example.com).\n");
        assert!(html.contains("<h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("href=\"https://example.com\""));
    }

    #[test]
    fn renders_tables() {
        let html = render_findings(
            "| a | b |\n|---|---|\n| 1 | 2 |\n",
        );
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>a</th>"));
        assert!(html.contains("<td>2</td>"));
    }

    #[test]
    fn strips_scripts() {
        let html = render_findings("<script>alert(1)</script>");
        assert!(!html.contains("<script"));
        assert!(!html.contains("alert"));
    }

    #[test]
    fn strips_iframes() {
        let html = render_findings("<iframe src=\"https://evil.example/\"></iframe>");
        assert!(!html.contains("<iframe"));
    }

    #[test]
    fn strips_event_handlers() {
        let html = render_findings("<a href=\"https://x\" onclick=\"alert(1)\">x</a>");
        assert!(!html.contains("onclick"));
    }

    #[test]
    fn rejects_javascript_urls() {
        let html = render_findings("[click](javascript:alert(1))");
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn drops_images_entirely() {
        // We do not allow <img> at all — operators open artifacts via the
        // editor IPC, not via the webview. Confirm the tag is gone.
        let html = render_findings("![alt](https://example.com/x.png)");
        assert!(!html.contains("<img"));
        assert!(!html.contains("src="));
    }

    #[test]
    fn preserves_code_blocks() {
        let html = render_findings("```\nrm -rf /tmp\n```\n");
        assert!(html.contains("<pre>"));
        assert!(html.contains("<code>"));
        assert!(html.contains("rm -rf /tmp"));
    }
}
