// cover.typ — title page. Rendered before the table of contents, with no
// running header/footer. `ctx` carries the document metadata (see docs §9).
//
// Scaffolded by `secunit wisp init`; edit freely to brand.

#import "theme.typ": wisp-theme

#let wisp-cover(ctx) = {
  set page(header: none, footer: none, numbering: none)
  v(1fr)
  align(center)[
    #image(ctx.logo, height: 26mm)
    #v(10mm)
    #text(size: 26pt, weight: "bold")[#ctx.title]
    #v(3mm)
    #text(size: 13pt, fill: gray)[#ctx.org]
    #v(14mm)
    #text(size: 11pt)[Version #ctx.version  ·  Effective #ctx.effective_date]
    #v(2mm)
    #box(inset: (x: 8pt, y: 4pt), fill: wisp-theme.accent.lighten(70%), radius: 3pt)[
      #text(size: 9pt, weight: "medium")[#ctx.classification  ·  #ctx.status]
    ]
  ]
  v(1fr)
  align(center)[#text(size: 7pt, fill: gray)[#ctx.commit  ·  #ctx.content_hash]]
  pagebreak()
}
