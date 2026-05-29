// header.typ — running page header. `ctx` carries the document metadata
// described in docs/wisp-pdf-export.md §9 (org, title, version, classification,
// status, logo, commit, content_hash, effective_date, generated_at).
//
// Scaffolded by `secunit wisp init`; edit freely to brand.

#import "theme.typ": wisp-theme

#let wisp-header(ctx) = {
  set text(size: 8pt, fill: gray)
  grid(
    columns: (auto, 1fr),
    align: (left + horizon, right + horizon),
    image(ctx.logo, height: 5mm),
    [#ctx.classification],
  )
  v(1mm)
  line(length: 100%, stroke: 0.5pt + gray.lighten(40%))
}
