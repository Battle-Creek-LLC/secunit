//! Typst backend — compiles the composed `main.typ` to PDF in-process using
//! the `typst` + `typst-pdf` crates (the default WISP renderer; pure Rust, no
//! external toolchain).
//!
//! The operator's template directory is the import root: `#import "theme.typ"`
//! and `image("logo.svg")` in the partials resolve to files there. The body
//! fonts (Inter + JetBrains Mono) are bundled into the binary and loaded into
//! the Typst font book so output is identical on every machine.
//!
//! With `--no-default-features` (no `pdf` feature) the heavy typst dependency
//! is dropped and this backend just writes the composed `.typ` beside the
//! intended output.

use std::fs;

use anyhow::{Context, Result};

use super::{RenderRequest, RenderResult};

#[cfg(not(feature = "pdf"))]
pub fn render(req: &RenderRequest) -> Result<RenderResult> {
    let typ_path = req.output.with_extension("typ");
    if let Some(parent) = typ_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output dir {}", parent.display()))?;
    }
    fs::write(&typ_path, req.typst_source)
        .with_context(|| format!("write {}", typ_path.display()))?;
    tracing::warn!(
        "wisp export: built without the `pdf` feature — wrote Typst source to {} \
         (no PDF compiled).",
        typ_path.display()
    );
    Ok(RenderResult {
        pages: None,
        wrote_pdf: false,
        intermediate: Some(typ_path),
    })
}

#[cfg(feature = "pdf")]
pub fn render(req: &RenderRequest) -> Result<RenderResult> {
    use anyhow::{anyhow, bail};

    if let Some(parent) = req.output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output dir {}", parent.display()))?;
    }
    // Keep the composed source beside the PDF for debugging/inspection.
    let typ_path = req.output.with_extension("typ");
    fs::write(&typ_path, req.typst_source)
        .with_context(|| format!("write {}", typ_path.display()))?;

    let world = world::WispWorld::new(req.template_dir, req.typst_source)?;

    let typst::diag::Warned { output, warnings } =
        typst::compile::<typst::layout::PagedDocument>(&world);
    for w in &warnings {
        tracing::warn!("typst: {}", w.message);
    }
    let document = output.map_err(|errs| {
        let detail = errs
            .iter()
            .map(|e| e.message.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        anyhow!("typst compile failed: {detail}")
    })?;

    let pages = document.pages.len() as u32;

    let pdf = typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errs| anyhow!("typst PDF export failed: {} diagnostic(s)", errs.len()))?;
    if pdf.is_empty() {
        bail!("typst produced an empty PDF");
    }
    fs::write(req.output, &pdf).with_context(|| format!("write {}", req.output.display()))?;

    Ok(RenderResult {
        pages: Some(pages),
        wrote_pdf: true,
        intermediate: Some(typ_path),
    })
}

#[cfg(feature = "pdf")]
mod world {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use anyhow::{Context, Result};
    use typst::diag::{FileError, FileResult};
    use typst::foundations::{Bytes, Datetime};
    use typst::syntax::{FileId, Source, VirtualPath};
    use typst::text::{Font, FontBook};
    use typst::utils::LazyHash;
    use typst::{Library, World};

    /// Bundled body + mono fonts (see assets/fonts/README.md). Loaded into the
    /// Typst font book so output never depends on system fonts.
    const FONTS: &[&[u8]] = &[
        include_bytes!("../assets/fonts/Inter-Regular.ttf"),
        include_bytes!("../assets/fonts/Inter-Medium.ttf"),
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
        include_bytes!("../assets/fonts/Inter-Bold.ttf"),
        include_bytes!("../assets/fonts/Inter-Italic.ttf"),
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
        include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    ];

    pub struct WispWorld {
        library: LazyHash<Library>,
        book: LazyHash<FontBook>,
        fonts: Vec<Font>,
        root: PathBuf,
        main: FileId,
        main_source: Source,
        /// Cache of source/byte reads keyed by file id.
        slots: Mutex<HashMap<FileId, FileResult<Bytes>>>,
    }

    impl WispWorld {
        pub fn new(template_dir: &Path, main_src: &str) -> Result<Self> {
            let root = template_dir
                .canonicalize()
                .with_context(|| format!("resolve template dir {}", template_dir.display()))?;

            let mut fonts = Vec::new();
            let mut book = FontBook::new();
            for raw in FONTS {
                let bytes = Bytes::new(*raw);
                // A TTF holds one face; index 0.
                if let Some(font) = Font::new(bytes, 0) {
                    book.push(font.info().clone());
                    fonts.push(font);
                }
            }

            let main = FileId::new(None, VirtualPath::new("main.typ"));
            let main_source = Source::new(main, main_src.to_string());

            Ok(Self {
                library: LazyHash::new(Library::default()),
                book: LazyHash::new(book),
                fonts,
                root,
                main,
                main_source,
                slots: Mutex::new(HashMap::new()),
            })
        }

        /// Resolve a non-main file id to an on-disk path under the template dir.
        fn path_of(&self, id: FileId) -> FileResult<PathBuf> {
            // Only package-less ids (our template) are supported.
            if id.package().is_some() {
                return Err(FileError::NotFound(id.vpath().as_rootless_path().into()));
            }
            id.vpath()
                .resolve(&self.root)
                .ok_or_else(|| FileError::AccessDenied)
        }

        fn read_bytes(&self, id: FileId) -> FileResult<Bytes> {
            if let Some(hit) = self.slots.lock().unwrap().get(&id) {
                return hit.clone();
            }
            let result = (|| {
                let path = self.path_of(id)?;
                let data = std::fs::read(&path).map_err(|e| FileError::from_io(e, &path))?;
                Ok(Bytes::new(data))
            })();
            self.slots.lock().unwrap().insert(id, result.clone());
            result
        }
    }

    impl World for WispWorld {
        fn library(&self) -> &LazyHash<Library> {
            &self.library
        }

        fn book(&self) -> &LazyHash<FontBook> {
            &self.book
        }

        fn main(&self) -> FileId {
            self.main
        }

        fn source(&self, id: FileId) -> FileResult<Source> {
            if id == self.main {
                return Ok(self.main_source.clone());
            }
            // Read partials as text straight from disk (Bytes→str conversion
            // differs across typst patch versions; this avoids it).
            let path = self.path_of(id)?;
            let text = std::fs::read_to_string(&path).map_err(|e| FileError::from_io(e, &path))?;
            Ok(Source::new(id, text))
        }

        fn file(&self, id: FileId) -> FileResult<Bytes> {
            self.read_bytes(id)
        }

        fn font(&self, index: usize) -> Option<Font> {
            self.fonts.get(index).cloned()
        }

        fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
            // The partials don't call datetime.today(); keep output deterministic.
            None
        }
    }
}
