// footer.typ — running page footer: provenance on the sides, "Page X of Y" in
// the centre. `ctx` carries the document metadata (see docs §9).
//
// Scaffolded by `secunit wisp init`; edit freely to brand.

#let wisp-footer(ctx) = {
  set text(size: 7pt, fill: gray)
  line(length: 100%, stroke: 0.5pt + gray.lighten(40%))
  v(1mm)
  grid(
    columns: (1fr, auto, 1fr),
    align: (left + horizon, center + horizon, right + horizon),
    [v#ctx.version · #ctx.status],
    context [Page #counter(page).display() of #counter(page).final().first()],
    [#ctx.commit · #ctx.content_hash],
  )
}
