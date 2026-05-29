// theme.typ — base look & feel for the WISP PDF.
//
// This file is yours: edit fonts, colours, spacing, and page geometry to match
// your brand. It is scaffolded by `secunit wisp init` and is REQUIRED by
// `secunit wisp export`. The Inter / JetBrains Mono fonts are bundled with the
// `secunit` binary and made available to the renderer, so output is identical
// on every machine.

#let wisp-theme = (
  font: "Inter",
  mono-font: "JetBrains Mono",
  accent: rgb("#0c7c70"),     // deep teal — used sparingly (rules, links)
  ink: rgb("#1a1a1a"),
  muted: rgb("#6b7280"),
  text-size: 10pt,
  paper: "a4",
  margin: (top: 26mm, bottom: 22mm, x: 22mm),
)

// Apply base typography + the heading hierarchy to the document body. The page
// header/footer, cover, ToC, and the optional DRAFT watermark are wired by the
// generated document, not here.
#let apply-theme(body) = {
  set text(font: wisp-theme.font, size: wisp-theme.text-size, fill: wisp-theme.ink)
  set par(justify: true, leading: 0.65em, spacing: 0.95em)

  // Tight, consistent list styling with real indentation and breathing room.
  set list(indent: 0.6em, spacing: 0.55em, marker: ([•], [–], [·]))
  set enum(indent: 0.6em, spacing: 0.55em, numbering: "1.a.i.")

  // Headings: clear size + weight steps so H1 > H2 > H3 > H4 read as a
  // hierarchy, with space above each so sections don't run together.
  set heading(numbering: none)
  show heading: set text(fill: wisp-theme.ink, weight: "bold")
  show heading.where(level: 1): it => {
    set text(size: 17pt)
    block(above: 1.4em, below: 0.7em, it)
  }
  show heading.where(level: 2): it => {
    set text(size: 13pt, fill: wisp-theme.accent)
    block(above: 1.2em, below: 0.5em, it)
  }
  show heading.where(level: 3): it => {
    set text(size: 11pt)
    block(above: 0.9em, below: 0.4em, it)
  }
  show heading.where(level: 4): it => {
    set text(size: 10pt, fill: wisp-theme.muted)
    block(above: 0.8em, below: 0.3em, smallcaps(it))
  }

  show raw: set text(font: wisp-theme.mono-font, size: 9pt)

  // Only colour *external* links (URLs). Internal cross-references — including
  // every table-of-contents entry — stay ink-coloured so the ToC isn't teal.
  show link: it => {
    if type(it.dest) == str {
      text(fill: wisp-theme.accent, it)
    } else {
      it
    }
  }

  body
}
