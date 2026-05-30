// toc.typ — table of contents. Uses Typst's native outline so entries carry
// page numbers (with leader dots) and drive the PDF bookmarks/outline.
// `ctx` carries the document metadata (see docs §9).
//
// Scaffolded by `secunit wisp init`; edit freely to restyle.

#let wisp-toc(ctx) = {
  heading(level: 1, outlined: false, numbering: none)[Contents]
  v(2mm)
  outline(title: none, depth: 3, indent: auto)
  pagebreak()
}
