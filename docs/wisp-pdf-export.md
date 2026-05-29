# Spec: `secunit wisp export` — branded PDF of the latest WISP

| | |
|---|---|
| **Status** | Draft — pending review |
| **Owner** | TBD |
| **Crates touched** | `secunit-cli` (subcommand) + `secunit-core` (render module) |
| **Created** | 2026-05-29 |
| **Tracking** | TBD |

Prerequisite reading: [spec.md](spec.md) (concepts), [storage.md](storage.md)
(provenance / hash chaining), [cli.md](cli.md) (command surface).

## 1. Summary

Add a command that renders the **latest version of the WISP** (Written
Information Security Program) — the org's `security/*.md` policy set — into a
single polished, audit-ready **PDF**. The output must support a **custom header
and logo**, an automatically generated **table of contents**, and **page
numbers**. The branding partials (header, footer, cover, theme) are **required,
committed, operator-owned files**, scaffolded by a companion `wisp init` command
with generic starter versions.

```
secunit wisp init                      # scaffold generic partials into templates/wisp/
secunit wisp export --output wisp.pdf  # render using those partials
```

The default rendering engine is **pure-Rust Typst** (compiles into the binary, no
external runtime), so partials are Typst templates by default; HTML partials are
available via the opt-in HTML backends (§8).

This is a **new capability**. Today secunit only *reads* the WISP (the
`bootstrap` skill walks `security/*-policy.md` to extract cadence-bearing
obligations); it never renders it. There is no existing PDF/HTML output for the
WISP itself.

## 2. Motivation

`secunit` is "the operational layer for an organization's WISP" — but the WISP
document itself lives as a set of markdown policies under `security/` in the
org's repo, and there is no first-class way to produce a distributable artifact
from it. Auditors, insurers, and the board ask for a single paginated PDF with a
cover page, a ToC, page numbers, an effective date, and a version stamp.
Producing that by hand is slow, error-prone, and — most importantly for this
project — **not traceable to a committed source**.

Generating it from the source of truth means the PDF is always tied to a
specific git commit and content hash, matching the provenance/hash-chain ethos
the rest of secunit is built on (every evidence manifest already pins
`registry_git_sha`). The same WISP, rebuilt, is the same PDF.

## 3. Goals / Non-goals

### Goals
- Render the latest WISP to PDF from its `security/` source with one command.
- Support a **custom header/footer, cover, and logo** via operator-owned partials.
- **Scaffold generic starter partials** with `secunit wisp init`.
- **Require** the partials to exist for `export` (no hidden embedded defaults —
  branding is committed, reviewable source).
- Generate a **table of contents** with section page references.
- Render **page numbers** ("Page X of Y").
- Stamp **version + effective date + provenance** (WISP-repo git SHA + content hash).
- Be **reproducible**: same committed source → stable output. Default to a
  **pure-Rust** engine so `cargo install` yields a working renderer with no
  external runtime.
- Preflight the renderer (and partials) via the existing `secunit doctor` command.

### Non-goals (v1)
- **Authoring or editing the WISP.** secunit operationalizes a WISP; it does not
  author one (see [setup-checklist.md](setup-checklist.md) A0). This command only
  *renders* the existing source.
- Rendering the *registry* (controls/evidence/risks) — that is what the `report`
  skill and the GUI already do. This command is the WISP **document**, not the
  program state.
- A WYSIWYG editor or live preview (the GUI may add this later).
- Digital signing of the PDF (tracked as a follow-up; see §12).

## 4. Background & current state

- **Stack:** Rust workspace (v0.4.2). Relevant crates:
  - `secunit-core` — registry, evidence, hashing, verification primitives; bundles
    skills under `skills/`. Library code for a renderer belongs here.
  - `secunit-cli` — the `secunit` binary (`bcl-secunit`). Owns all subcommands;
    each is a module under `src/cmd/<name>.rs` dispatched from the `Command` enum
    in `src/main.rs`. The recently-added `doctor` (`src/cmd/doctor.rs`, #47) is
    the pattern to follow for a new subcommand + health check.
  - `secunit-capture` — native upstream capturers (feature-gated).
  - `secunit-gui` — read-only Tauri viewer.
- **Markdown→HTML already exists**: `pulldown-cmark` (`0.13`, `html` feature) +
  `ammonia` are workspace deps, used in `crates/secunit-gui/src-tauri/src/sanitiser.rs`
  (`render_findings`) to render findings markdown to sanitized HTML. **Reuse
  `pulldown-cmark`** rather than adding a second markdown engine.
- **No PDF or templating stack exists** — no weasyprint/chromium/typst/wkhtmltopdf,
  no tera/askama/handlebars/minijinja. This is greenfield.
- **Branding asset exists**: `crates/secunit-gui/src-tauri/icons/icon.svg` (shield
  + teal `#2dd4bf` visor) — the default logo. GUI palette/fonts (Inter, JetBrains
  Mono) in `crates/secunit-gui/src/styles.css` can seed the default theme.

## 5. WISP source & "latest version" resolution

The renderer must not guess. Resolution is explicit, configurable, and the
resolved identity is recorded in the output — consistent with how the rest of
secunit pins provenance.

### 5.1 Source
The WISP source is the org's policy document set, conventionally the markdown
files under `security/` (as `bootstrap` already walks). Resolution order:
1. `--source <path>` (file or directory), else
2. `wisp.source` in `_config.yaml`, else
3. The `security/` directory under the WISP repo (`_config.org.wisp_repo`), else
4. The set of paths referenced by control `policy:` / `references[].path` fields.

**Section order** (a WISP is many files → one ordered document) is determined by:
1. An explicit `wisp.order` list in `_config.yaml` (filenames or globs), else
2. `wisp.toc.json` / front-matter `order:` keys if present, else
3. Lexical filename order, with any `*-overview`/`*-introduction` floated first.

Order resolution is reported to stderr so the operator can see how the document
was assembled (and pin it via `wisp.order` if the default is wrong).

### 5.2 "Latest version"
Resolved deterministically and stamped on the cover/footer, reusing secunit's
existing provenance primitives (the same `git_head()` logic in
`secunit-core/src/evidence/runner.rs` that pins `registry_git_sha`):
- **Version**: a `version:` field in the WISP front-matter / a `wisp.version` in
  `_config.yaml`; fallback to the most recent git tag matching `wisp-v*`.
- **Effective date**: explicit `effective_date:`, else the commit date of the
  source's last change.
- **Provenance**: short git SHA of the WISP source tree **and** a SHA-256 content
  hash of the assembled markdown (reuse `secunit-core/src/evidence/hasher.rs`).
  Both are stamped in the footer and printed to stdout on success.

If any WISP source file is dirty in the working tree, mark the document `DRAFT`
(watermark + footer) unless `--allow-dirty` is passed — mirroring the repo's
"the git history *is* the audit trail" stance.

## 6. User-facing design (CLI)

New subcommand `wisp` with `init` and `export` actions (room for `wisp validate`,
`wisp preview` later), added as `Command::Wisp` in `main.rs` and handled in
`crates/secunit-cli/src/cmd/wisp.rs` — same shape as `cmd/doctor.rs`. Honors the
global `-C/--root`, `--config`, `--json`, and `--today` flags.

### `wisp init` — scaffold the partials

Writes generic starter partials into the template directory so the operator can
brand and commit them. Idempotent and non-destructive by default.

```
secunit wisp init [OPTIONS]

Options:
      --dir <PATH>           Template directory [default: templates/wisp/ or config]
      --format <FMT>         typst | html  [default: matches the configured renderer]
      --logo <PATH>          Seed the cover/header with this logo [default: bundled shield]
      --force                Overwrite existing partials (otherwise skip + report)
```

Scaffolds `cover`, `header`, `footer`, `theme`, and a `toc` override stub (in the
chosen format — see §9), plus the logo, and prints what it wrote/skipped. On
completion it nudges the operator to review and `git add` the files.

### `wisp export` — render the PDF

```
secunit wisp export [OPTIONS]

Options:
  -o, --output <PATH>        Output PDF [default: wisp-<version>.pdf]
      --source <PATH>        WISP source file or directory (overrides config)
      --template <PATH>      Template directory holding the partials (overrides config)
      --toc / --no-toc       Include a table of contents [default: on]
      --page-numbers / --no-page-numbers   [default: on]
      --draft                Force the DRAFT watermark
      --allow-dirty          Allow rendering from an uncommitted working tree
      --renderer <BACKEND>   typst | weasyprint | chromium  (overrides config)
      --metadata <K=V>...    Override/extend PDF metadata (title, author, ...)
```

**The partials are required.** If the template directory is missing or a required
partial is absent, `export` fails with exit `1` and a message pointing at
`secunit wisp init`. This keeps branding as committed, reviewable source rather
than hidden binary defaults — consistent with the rest of the registry. (`--logo`
lives on `init`, not `export`: the logo is part of the committed partials.)

Exit codes follow the existing convention ([cli.md](cli.md)): `1` data wrong
(unresolvable source / missing partials / bad front-matter), `2` runtime
(renderer failure), `3` usage. On success `export` prints: output path, version,
effective date, git SHA, content hash, page count, renderer used (and the
assembled section order). `--json` emits the same as a structured object.

## 7. Rendering pipeline (architecture)

```
WISP source (security/*.md + front-matter / _config order)
        │  resolve order, version, date, provenance     (secunit-core::wisp)
        ▼
   Assembled markdown + heading tree (anchors)          (pulldown-cmark, reused)
        │
        ▼
   Build a backend-neutral WispDoc:
     { meta, body blocks, heading tree → ToC, partials, logo }
        │
        ├─► Typst backend (default): emit .typ from WispDoc + .typ partials,
        │     compile in-process  →  PDF bytes   (typst + typst-pdf crates)
        │
        └─► HTML backends (opt-in): render body → HTML, compose with .html
              partials + CSS, hand to WeasyPrint/Chromium subprocess → PDF bytes
        ▼
   Post-process: PDF metadata, outline, optional PDF/A   ──►  wisp.pdf
```

Markdown parsing reuses **`pulldown-cmark`** (already a workspace dep, with the
same GFM extensions enabled in `sanitiser.rs`). Unlike the GUI path this is our
own trusted source, so `ammonia` sanitization is unnecessary — but we do need
stable heading anchors/slugs for the ToC, which `render_findings` does not emit,
so anchor extraction is added in the new code path. The parsed result is held as
a backend-neutral document model (not pre-rendered HTML) so the Typst backend can
emit native Typst rather than round-tripping through HTML.

Rendering is abstracted behind a backend trait so the engine is swappable and
`doctor` can health-check it (the default Typst backend has nothing external to
check):

```rust
// secunit-core::wisp::render
pub trait RenderBackend {
    fn id(&self) -> &'static str;
    /// Verify the backend is usable (e.g. external tool present). Typst: always Ok.
    fn healthcheck(&self) -> Result<(), RenderError>;
    /// Render the assembled doc (with its resolved partials) to PDF bytes.
    fn render(&self, doc: &WispDoc, opts: &RenderOptions) -> Result<Vec<u8>, RenderError>;
}
```

All pipeline logic lives in `secunit-core` so the CLI (and later the GUI) call
one entry point; `secunit-cli` only parses flags and prints the report.

## 8. Renderer backend — decided: Typst default

`secunit` is delivered as a Rust binary (crates.io / `cargo install`, the Homebrew
tap, and the Tauri app). The renderer default must not break that: it has to work
out of the box from a plain `cargo install` with no second toolchain. That rules
out making an external engine the default.

**Decision: default to Typst** (the `typst` + `typst-pdf` crates), compiled into
the binary. WeasyPrint and Chromium remain available as **opt-in, feature-gated**
HTML backends for orgs that specifically want HTML/CSS partials and already have
the toolchain.

| Backend | Delivery | Header/footer/cover + logo | ToC w/ page numbers | Page numbers | Reproducible |
|---|---|---|---|---|---|
| **Typst** *(default, pure Rust)* | In-binary; `cargo install` just works | ✅ Typst template partials, embedded logo | ✅ native `outline()` | ✅ native | ✅ strong |
| **WeasyPrint** *(opt-in, feature `render-weasyprint`)* | External Python tool; `doctor`-detected | ✅ HTML partials, full CSS Paged Media | ✅ `target-counter()` + leader dots | ✅ counters | ✅ |
| **Chromium** *(opt-in, feature `render-chromium`)* | External browser binary; `doctor`-detected | ⚠️ sandboxed header/footer template, base64 images | ❌ needs 2-pass | ✅ classes | ⚠️ |

### Delivery model

- **Typst**: no runtime dependency. It is a Rust dependency that links into the
  binary, so every delivery channel ships a working renderer identically. Native
  ToC, page numbers, headers/footers, embedded images, and deterministic output —
  a strong fit for a reproducible compliance artifact.
- **WeasyPrint / Chromium**: **not bundled and not linkable** — they are external
  system tools invoked as a subprocess. This follows the existing precedent in
  `secunit-capture`, which feature-gates and shells out to `cargo-audit` /
  `pip-audit` / `pnpm-audit`. They are compiled in only when their cargo feature
  is enabled, are absent from a default `cargo install`, and are detected at
  runtime by `doctor` with a clear remediation message. Selecting
  `--renderer weasyprint` without the feature/tool present fails with exit `1`.

### Cost of the Typst default

The only thing given up versus an HTML-first default is that the **default**
partials are authored in Typst markup, not HTML. Since header/footer/cover are a
logo plus a few fields, the Typst templates are small and `wisp init` scaffolds
them. Operators who require HTML partials enable an HTML backend and
`wisp init --format html`. ToC, page numbers, logo, and reproducibility are fully
covered natively by Typst.

## 9. Templating: partials (required, scaffolded by `wisp init`)

Partials are **operator-owned files committed to the registry**, not hidden binary
defaults. They live in the template directory (`wisp.template`, default
`templates/wisp/`). `wisp init` scaffolds generic starter versions; `export`
**requires** them and fails (exit `1`, pointing at `wisp init`) if any are
missing. This makes branding reviewable, diffable, and part of the audit trail —
the same stance the project takes toward controls and inventory.

Format depends on the backend. The default Typst backend uses Typst partials; the
opt-in HTML backends use HTML + CSS. `wisp init --format <typst|html>` writes the
right set (default matches the configured renderer):

```
templates/wisp/                 templates/wisp/         (--format html)
  cover.typ                       cover.html
  header.typ                      header.html
  footer.typ                      footer.html
  theme.typ   # page setup,       theme.css   # @page rules, fonts, colors
              # fonts, colors
  toc.typ     # optional          toc.html    # optional ToC layout override
  logo.svg    # seeded from --logo / bundled shield
```

A required-partial set is defined per format; `init` writes all of them and
`export` checks the same list. The bundled generic versions are embedded in the
binary (e.g. `include_str!`) and seeded from the GUI palette and `icon.svg`, so a
freshly-`init`ed project renders a clean, neutral document immediately.

**Fonts.** The default theme uses **Inter** for body/headings (and JetBrains Mono
for monospace), matching the GUI. For deterministic, self-contained output the
Inter (and JetBrains Mono) font files are **bundled in the binary** and loaded
into the renderer rather than resolved from system fonts — so the PDF looks
identical on any machine and in CI. Both are OFL-licensed, so redistribution is
fine; the license files ship alongside them. The scaffolded `theme.typ` sets
`#set text(font: "Inter")`; the HTML `theme.css` sets `font-family: "Inter", …`
and `@font-face`-embeds the same files.

Context available to every partial (same fields regardless of format):

```jsonc
{
  "org":            "Battle Creek",
  "title":          "Written Information Security Program",
  "version":        "1.4.0",
  "effective_date": "2026-05-29",
  "classification": "Confidential",
  "status":         "APPROVED",        // or "DRAFT"
  "logo":           "logo.svg",        // path within the template dir
  "commit":         "e8bde1f",
  "content_hash":   "sha256:…",
  "generated_at":   "2026-05-29"
}
```

The logo is embedded into the PDF (Typst `image()`, or an HTML data URI) so the
output is self-contained and reproducible regardless of working directory.

Example scaffolded `header.typ`:

```typ
#let wisp-header(ctx) = [
  #grid(columns: (auto, 1fr),
    image(ctx.logo, height: 6mm),
    align(right)[#text(size: 8pt, fill: gray)[#ctx.classification]])
]
```

Example scaffolded `header.html` (`--format html`):

```html
<header class="wisp-header">
  <img class="wisp-logo" src="{{ logo }}" alt="{{ org }} logo">
  <span class="wisp-classification">{{ classification }}</span>
</header>
```

For the HTML backends, partials are filled with a small pure-Rust templating layer
(**minijinja**, or plain keyed substitution given how simple they are). Typst
partials receive `ctx` directly as Typst data — no separate templating engine.

## 10. Table of contents

- Built by walking the heading tree (H1–H3 by default; `wisp.toc.depth`
  configurable) from the parsed markdown.
- Each heading gets a stable, slugified anchor/label.
- Entries link to those anchors with page references and leader dots: Typst uses
  native `outline()`; the HTML backends use CSS `target-counter()` (WeasyPrint) or
  a two-pass measure (Chromium). The ToC also drives the PDF **outline/bookmarks**
  so the document is navigable in any viewer.
- The ToC page is excluded from body numbering or uses roman numerals
  (configurable); arabic numbering starts at the first content page.

## 11. Page numbers

- Rendered in the footer partial as **"Page X of Y"** — Typst via
  `counter(page)` / `here().page()` against the page total; HTML backends via CSS
  `counter(page)`/`counter(pages)` (WeasyPrint) or `pageNumber`/`totalPages`
  classes (Chromium).
- Cover page unnumbered; front matter (ToC) optionally roman.
- `--no-page-numbers` disables them.

## 12. Additional recommendations ("anything else?")

For an audit/insurance-grade WISP PDF, strongly recommended:

1. **Cover page** — title, org, version, effective date, classification banner,
   prepared-by / approved-by (pull from `_config.owners`), and the content hash.
2. **Provenance footer / tamper-evidence** — stamp WISP git SHA + SHA-256 content
   hash on every page (reuse `evidence/hasher.rs`). Ties the artifact to a
   committed source, matching secunit's hash-chain ethos. Echo the same to stdout.
3. **DRAFT vs APPROVED state** — diagonal `DRAFT` watermark when rendering from a
   dirty tree or with `--draft`; `APPROVED` only from a clean, tagged source.
4. **Embedded PDF metadata** — Title, Author (`_config.org.name`), Subject,
   Keywords, Producer (`secunit <version>`), CreationDate. Set deterministically.
5. **Reproducible builds** — derive `generated_at` from the source commit date
   (not wall-clock; the codebase already avoids ambient `now`), embed fonts, and
   avoid nondeterministic IDs so the same source yields a stable PDF.
6. **PDF outline / bookmarks** — from the heading tree (viewer-side nav).
7. **Section numbering** — optional auto-numbered headings (1, 1.1, 1.1.1) via CSS
   counters, common for policy documents.
8. **Accessibility / archival** — optional **tagged PDF** and **PDF/A**
   (`--archival`) for retention; supported natively by Typst (`pdf.standard`) and
   by WeasyPrint.
9. **`secunit wisp validate`** (follow-up) — lint the source before export: broken
   anchors, missing required sections, stale effective date, unresolved order
   entries, undefined front-matter. Reuse in `doctor` / CI / the `bootstrap` flow.
10. **Coverage cross-check** (follow-up) — since `bootstrap` already maps WISP
    obligations to controls, optionally annotate or append a "controls coverage"
    appendix so the PDF shows each policy's operational backing.
11. **Optional evidence integration** (follow-up) — a `publish-wisp` control/skill
    could run `wisp export` and capture the PDF + hash as an auditable artifact in
    a run dir, so "the board-approved WISP for Q2" is itself hash-chained.
12. **GUI hook** (follow-up) — all logic in `secunit-core`, so the Tauri viewer can
    add "Export WISP PDF" with a preview.
13. **Digital signature** (follow-up) — sign the PDF or emit a detached signature
    over the content hash for a stronger chain of custody.

## 13. Configuration (`_config.yaml`, schema `_config.schema.json`)

```yaml
wisp:
  source: security/            # file or directory (default: security/ in wisp_repo)
  order:                       # optional explicit section order (globs allowed)
    - "*-overview.md"
    - access-control-policy.md
    - "*.md"
  version: "1.4.0"             # optional; else front-matter / wisp-v* tag
  template: templates/wisp     # REQUIRED partials dir (scaffold via `wisp init`)
  renderer: typst              # typst (default) | weasyprint | chromium
  metadata:
    org: Battle Creek
    title: Written Information Security Program
    classification: Confidential
  toc:
    enabled: true
    depth: 3                   # H1..H3
  output:
    page_numbers: true
    section_numbers: false
    archival: false            # PDF/A
```

All keys are overridable by the CLI flags in §6. Add a `wisp` block to
`crates/secunit-core/schemas/_config.schema.json`. The `template` dir must exist
with its partials before `export` (run `wisp init` once to create it).

## 14. Crate layout

```
crates/secunit-cli/src/
  cmd/wisp.rs             # clap subcommand: `wisp init` + `wisp export` (mirrors cmd/doctor.rs)
  main.rs                 # add Command::Wisp + dispatch

crates/secunit-core/src/wisp/
  mod.rs                  # public API: init(opts), export(opts) -> Result<…>
  source.rs              # resolve order + assemble markdown; version; provenance
  toc.rs                 # heading tree → ToC + anchors + bookmarks
  doc.rs                 # backend-neutral WispDoc model
  template.rs            # locate/load partials; REQUIRED-set check; context
  scaffold.rs            # `init`: write embedded generic partials to template dir
  assets/                # embedded partials + fonts (include_str!/include_bytes!)
    typst/  { cover.typ, header.typ, footer.typ, theme.typ, toc.typ }
    html/   { cover.html, header.html, footer.html, theme.css, toc.html }
    fonts/  { Inter-*.ttf, JetBrainsMono-*.ttf, OFL.txt }   # bundled, deterministic
    logo.svg
  render/
    mod.rs               # RenderBackend trait + RenderOptions + selection
    typst.rs             # DEFAULT backend (in-binary; typst + typst-pdf)
    weasyprint.rs        # opt-in, feature `render-weasyprint` (subprocess)
    chromium.rs          # opt-in, feature `render-chromium`
```

Reuse existing primitives: `evidence/hasher.rs` (content hash), `git_head()` in
`evidence/runner.rs` (commit SHA), `pulldown-cmark` (markdown parse). New deps:
`typst` + `typst-pdf` (default, pure Rust, in-binary); behind cargo features,
`minijinja` (or none) for HTML partial filling and a Chromium driver
(`chromiumoxide`) when `render-chromium` is on. WeasyPrint/Chromium binaries are
external runtime tools, not crates, and are absent from a default build.

## 15. `secunit doctor` integration

Extend `crates/secunit-cli/src/cmd/doctor.rs` (the env/registry preflight from
#47) with a WISP-export check so problems surface before an export:
- **Renderer**: for the default Typst backend, always OK (in-binary). For an
  opt-in backend, confirm the cargo feature is compiled in **and** the external
  tool is present (`weasyprint --version`, Chrome binary); report version + path
  with actionable remediation if missing.
- **Partials**: confirm the template dir exists and the required-partial set is
  present; if not, point at `secunit wisp init`.
- **Source**: validate the WISP source resolves, assembles in a defined order,
  and parses (front-matter present, headings well-formed).

## 16. Testing & CI

- **Unit** (`secunit-core`): source/order resolution, version & provenance, ToC
  tree, slug stability, template context, required-partial detection.
- **Golden intermediate** (insta, matching existing `tests/snapshots/`): snapshot
  the emitted Typst (and composed HTML for the opt-in backends) — fast, no PDF
  engine required.
- **Scaffold**: `wisp init` writes the full required set; a second run without
  `--force` skips and reports; `export` errors cleanly when a partial is removed.
- **PDF smoke**: with the default Typst backend (no external tool needed), render
  `testdata/orgs/multi-system/security/` and assert via `pdftotext` that the ToC,
  page numbers, version, and content hash appear, and page count > cover + toc.
- **Reproducibility**: render the same fixture twice; assert identical output
  (Typst is deterministic given fixed inputs).
- **doctor**: extend `tests/doctor.rs` to cover missing-partials and (for opt-in
  backends) missing-tool remediation paths.
- CI runs the Typst path by default; opt-in backends are exercised in a job that
  installs WeasyPrint (pip) / Chrome behind their feature flags.

## 17. Open questions / decisions needed

1. ~~Renderer backend default~~ — **decided: Typst default**, HTML backends opt-in
   (§8).
2. **Section ordering** — is there a canonical WISP order to encode as the default,
   or is `_config.wisp.order` always required for a real org? (secops is the
   reference instance to check.)
3. **Version source of truth** — front-matter `version:` vs `_config.wisp.version`
   vs `wisp-v*` tag. Recommend front-matter primary, tag fallback.
4. **`init` defaults** — does `wisp init` write into the registry root
   (`templates/wisp/`) and should `_config.wisp.template` be auto-added on init?
   Recommend yes to both, so `init` then `export` works with zero config.
5. **Scope of v1** — standalone commands only, or also wire the optional
   `publish-wisp` evidence integration (§12.11) now?

## 18. Milestones

1. **M1 — Skeleton + scaffold**: `wisp init` and `wisp export` wired in
   `secunit-cli`; embedded generic partials; source/order resolution + markdown
   parse (pulldown-cmark) + version/provenance + WispDoc model in `secunit-core`;
   required-partial enforcement; golden intermediate + scaffold tests. No PDF yet.
2. **M2 — Typst renderer (default)**: in-binary Typst backend, Typst partials,
   logo embedding, page numbers, native ToC + bookmarks; PDF smoke + reproducibility
   tests. End-to-end `init` → `export` works from a plain `cargo install`.
3. **M3 — doctor + polish**: renderer/partials/source healthcheck, DRAFT/APPROVED,
   PDF metadata, deterministic `generated_at`.
4. **M4 — Opt-in HTML backends + follow-ups**: feature-gated WeasyPrint (then
   Chromium) with `init --format html`, PDF/A, `wisp validate`, coverage appendix,
   GUI hook, signing.

## Implementation status

Landed on branch `feat/wisp-pdf-export` (M1 slice — pure Rust, no new deps):

- `secunit wisp init` — scaffolds the required, operator-owned Typst partials
  (`theme/header/footer/cover/toc.typ` + `logo.svg`) from bundled generics;
  idempotent, `--force` to overwrite. (`--format html` is reserved until the
  HTML backends land.)
- `secunit wisp export` — resolves the source (`security/` or `wisp.source`),
  assembles the markdown in order, computes provenance (WISP-repo git SHA +
  SHA-256 content hash, reusing `evidence::{runner,hasher}`), **requires** the
  partials (errors to `wisp init` if absent), builds the `WispDoc`, and emits the
  composed `main.typ`.
- CLI wired (`Command::Wisp` → `cmd/wisp.rs`); core pipeline in
  `secunit-core::wisp::*`; unit tests for the glob/front-matter/markdown/emit
  helpers.

Verified locally: `cargo clippy -p secunit-core -p bcl-secunit --all-targets --
-D warnings` is clean and all tests pass (6 core unit + 4 CLI integration). The
`wisp:` block has been added to `_config.schema.json`.

Remaining (each needs a crates.io fetch, which isn't available offline here):

1. **In-binary Typst compile** — add `typst` + `typst-pdf`, implement
   `typst::World` in `render/typst.rs` (template dir as import root), and load the
   bundled fonts. This is what turns the emitted `main.typ` into a PDF and lets
   `export` report `pages`. Until then the backend writes `main.typ` beside the
   output and reports the compile as pending. (Deferred only because pulling the
   `typst` crates needs network; the rest builds and tests clean.)
2. **Vendor the fonts** — drop the Inter / JetBrains Mono TTFs + `OFL.txt` into
   `crates/secunit-core/src/wisp/assets/fonts/` (see the README there) and
   `include_bytes!` them into the Typst font book. (Binary blobs — fetch from the
   upstream OFL releases rather than fabricating.)
3. **Upgrade `markdown.rs`** to a `pulldown-cmark`-driven converter (inline
   emphasis/links, Typst escaping) with golden snapshots — `pulldown-cmark` is
   already a workspace dep, so this one needs no network.

## References

- [spec.md](spec.md) — concepts; "Coverage from a WISP".
- [storage.md](storage.md) — provenance, manifest hash chaining.
- [cli.md](cli.md) — command surface and exit codes.
- [setup-checklist.md](setup-checklist.md) — WISP-as-source-of-truth assumptions.
- `crates/secunit-gui/src-tauri/src/sanitiser.rs` — existing pulldown-cmark usage.
- `crates/secunit-cli/src/cmd/doctor.rs` — subcommand + healthcheck pattern.
