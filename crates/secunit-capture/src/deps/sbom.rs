//! CycloneDX SBOM capturer.
//!
//! Emits a software bill of materials for a source tree by shelling out
//! to the CycloneDX project's own per-ecosystem generator — the same
//! subprocess pattern `pip-audit` and `pnpm audit` already use. Each
//! ecosystem gets the generator its own toolchain implies rather than
//! one generic resolver:
//!
//! | ecosystem | generator         | spec | transport            |
//! |-----------|-------------------|------|----------------------|
//! | `rust`    | `cargo-cyclonedx` | 1.5  | files, see below     |
//! | `python`  | `cyclonedx-py`    | 1.6  | stdout               |
//! | `node`    | `cyclonedx-npm`   | 1.6  | stdout               |
//!
//! Rust caps at CycloneDX 1.5 because `cargo cyclonedx --spec-version`
//! accepts only `1.3`, `1.4`, or `1.5`. The artifact is validated
//! against whatever version it declares — see [`crate::cyclonedx`].
//!
//! ## The Rust arm writes into the audited tree
//!
//! `cargo-cyclonedx` has no stdout mode and no output-directory flag:
//! it drops a BOM next to every `Cargo.toml` in the workspace. The
//! capturer therefore writes with `--override-filename` under a
//! process-unique stem, reads the result, and deletes it — including
//! on the error path, via [`EmittedFiles`]. It refuses to start if a
//! file under that stem already exists, so it can never clobber
//! something the project keeps. This is the one place a `deps` capture
//! is not strictly read-only against the target tree; a crash between
//! write and cleanup leaves a `secunit-sbom-<pid>.json` behind.
//!
//! ## Output
//!
//! [`capture_with`] returns the bare, schema-valid BOM *and* a
//! canonical envelope describing it. The CLI writes the BOM to `--out`
//! so an assessor's CycloneDX tooling can read it unchanged, and the
//! envelope to `<out>.envelope.json` carrying the generator provenance
//! and the BOM's sha256.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use secunit_core::evidence::hasher::sha256_bytes;

use crate::canonical::Envelope;
use crate::cyclonedx;

use super::cmd::{ensure_installed, tool, CmdRunner, RealRunner};
use super::truncate;

pub const CAPTURER: &str = "deps.sbom";
pub const VERSION: &str = "1";

/// Filename the CLI writes the bare BOM to, recorded in the envelope so
/// the sidecar names its subject.
pub const DEFAULT_BOM_FILENAME: &str = "sbom.cdx.json";

/// Which toolchain's dependency graph is being described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    Rust,
    Python,
    Node,
}

impl Ecosystem {
    pub const ALL: &'static [Ecosystem] = &[Ecosystem::Rust, Ecosystem::Python, Ecosystem::Node];

    pub fn as_str(self) -> &'static str {
        match self {
            Ecosystem::Rust => "rust",
            Ecosystem::Python => "python",
            Ecosystem::Node => "node",
        }
    }

    /// The binary probed before dispatch. For Rust this is the cargo
    /// subcommand binary, not `cargo` itself.
    pub fn generator(self) -> &'static str {
        match self {
            Ecosystem::Rust => "cargo-cyclonedx",
            Ecosystem::Python => "cyclonedx-py",
            Ecosystem::Node => "cyclonedx-npm",
        }
    }

    /// Highest CycloneDX version this ecosystem's generator supports.
    pub fn spec_version(self) -> &'static str {
        match self {
            Ecosystem::Rust => "1.5",
            Ecosystem::Python | Ecosystem::Node => "1.6",
        }
    }

    /// Files whose presence implies this ecosystem, most specific first.
    ///
    /// Python includes `pyproject.toml` and `uv.lock` so detection
    /// agrees with [`crate::deps::pip_audit`]: a sweep must not get a
    /// clean `pip-audit` capture and a "manifest unsupported" failure
    /// from `sbom` for the same tree.
    fn markers(self) -> &'static [&'static str] {
        match self {
            Ecosystem::Rust => &["Cargo.toml"],
            Ecosystem::Python => &[
                "requirements.txt",
                "poetry.lock",
                "uv.lock",
                "pyproject.toml",
            ],
            Ecosystem::Node => &["package.json"],
        }
    }
}

impl FromStr for Ecosystem {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "rust" | "cargo" => Ok(Ecosystem::Rust),
            "python" | "py" => Ok(Ecosystem::Python),
            "node" | "npm" | "javascript" | "typescript" => Ok(Ecosystem::Node),
            other => Err(anyhow!(
                "unknown ecosystem `{other}` — expected one of: {}",
                Ecosystem::ALL
                    .iter()
                    .map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

/// A captured SBOM: the bare CycloneDX document plus the sidecar
/// envelope that describes it.
#[derive(Debug)]
pub struct SbomCapture {
    /// Canonical JSON of the schema-valid BOM, exactly as it should be
    /// written to `--out`. The envelope's `bom_sha256` is the hash of
    /// these bytes plus the trailing newline `write_bom` appends.
    pub bom_json: String,
    pub envelope: Envelope,
}

impl SbomCapture {
    /// Record the filename the BOM is actually written under, so the
    /// sidecar names its subject rather than
    /// [`DEFAULT_BOM_FILENAME`].
    pub fn set_bom_filename(&mut self, name: &str) {
        self.envelope.result["bom_file"] = Value::String(name.to_string());
    }

    /// Write the bare BOM to `path` with the trailing newline its
    /// recorded sha256 covers.
    pub fn write_bom(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
        std::fs::write(path, on_disk_bytes(&self.bom_json))
            .with_context(|| format!("write {}", path.display()))
    }
}

/// The exact bytes `write_bom` puts on disk, and therefore the bytes
/// `bom_sha256` covers.
fn on_disk_bytes(bom_json: &str) -> Vec<u8> {
    let mut bytes = bom_json.as_bytes().to_vec();
    bytes.push(b'\n');
    bytes
}

/// Capture an SBOM for the project rooted at `path`.
///
/// `ecosystem` may be `None`, in which case it is inferred from the
/// manifests present; inference fails rather than guesses when a tree
/// carries markers for more than one.
pub fn capture(path: &Path, ecosystem: Option<Ecosystem>) -> Result<SbomCapture> {
    capture_with(path, ecosystem, &RealRunner)
}

/// Same as [`capture`] but with an injectable subprocess runner.
pub fn capture_with(
    path: &Path,
    ecosystem: Option<Ecosystem>,
    runner: &dyn CmdRunner,
) -> Result<SbomCapture> {
    let eco = match ecosystem {
        Some(e) => e,
        None => detect_ecosystem(path)?,
    };

    ensure_installed(
        runner,
        tool(eco.generator()).expect("every generator is a required tool"),
    )?;

    let (raw, describes) = match eco {
        Ecosystem::Rust => run_cargo_cyclonedx(path, runner)?,
        Ecosystem::Python => run_cyclonedx_py(path, runner)?,
        Ecosystem::Node => run_cyclonedx_npm(path, runner)?,
    };

    let mut bom: Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "parse {} output as JSON: {}",
            eco.generator(),
            truncate(&raw, 200)
        )
    })?;

    cyclonedx::normalize(&mut bom);

    let errs = cyclonedx::validate(&bom)?;
    if !errs.is_empty() {
        return Err(anyhow!(
            "{} produced a BOM that fails the CycloneDX {} schema: {}",
            eco.generator(),
            cyclonedx::spec_version(&bom).unwrap_or("?"),
            errs.join("; ")
        ));
    }

    let bom_json = serde_json::to_string_pretty(&bom).context("serialize BOM")?;
    let bom_sha256 = sha256_bytes(&on_disk_bytes(&bom_json));

    let generators: Vec<Value> = cyclonedx::generators(&bom)
        .into_iter()
        .map(|(name, version)| json!({"name": name, "version": version}))
        .collect();

    let component_count = bom
        .get("components")
        .and_then(|c| c.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let envelope = Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "path": path.display().to_string(),
            "ecosystem": eco.as_str(),
            "requested_spec_version": eco.spec_version(),
        }),
        json!({
            "bom_file": DEFAULT_BOM_FILENAME,
            "bom_format": bom.get("bomFormat").and_then(|f| f.as_str()).unwrap_or("CycloneDX"),
            "bom_sha256": bom_sha256,
            "component_count": component_count,
            "describes": describes,
            "ecosystem": eco.as_str(),
            "generators": generators,
            "spec_version": cyclonedx::spec_version(&bom)?,
        }),
    );

    Ok(SbomCapture { bom_json, envelope })
}

/// Infer the ecosystem from the manifests in `path`.
///
/// A tree carrying markers for more than one toolchain is an error, not
/// a coin flip: the operator names which one they want. An SBOM for the
/// wrong ecosystem is worse than no SBOM, for the same reason
/// `pip-audit` refuses to audit the ambient interpreter.
pub fn detect_ecosystem(path: &Path) -> Result<Ecosystem> {
    let found: Vec<Ecosystem> = Ecosystem::ALL
        .iter()
        .copied()
        .filter(|e| e.markers().iter().any(|m| path.join(m).is_file()))
        .collect();

    match found.as_slice() {
        [one] => Ok(*one),
        [] => Err(anyhow!(
            "no supported dependency manifest under {} — expected one of: {}",
            path.display(),
            all_markers().join(", ")
        )),
        many => Err(anyhow!(
            "{} carries manifests for more than one ecosystem ({}) — pass \
             --ecosystem to say which SBOM to capture",
            path.display(),
            many.iter()
                .map(|e| e.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn all_markers() -> Vec<&'static str> {
    Ecosystem::ALL
        .iter()
        .flat_map(|e| e.markers().iter().copied())
        .collect()
}

// -----------------------------------------------------------------------------
// python
// -----------------------------------------------------------------------------

/// In-project virtualenv directory names, in the order they are tried.
///
/// `.venv` is what `uv`, `pdm`, and `hatch` create by default.
const VENV_DIRS: &[&str] = &[".venv", "venv"];

/// The `cyclonedx-py` subcommand and target chosen for a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PySource {
    subcommand: &'static str,
    /// Passed relative to `path` (the runner's cwd) so the recorded
    /// args stay portable across machines.
    target: &'static str,
    /// What the BOM actually describes — recorded in the envelope,
    /// because "resolved from a lockfile" and "read from an installed
    /// virtualenv" are different evidentiary claims.
    describes: &'static str,
}

/// Pick the `cyclonedx-py` subcommand from what the project provides.
///
/// Declared manifests win over an installed virtualenv: they need no
/// install step and describe what the project *asks* for. A PEP-621
/// project (uv, pdm, hatch) has no manifest cyclonedx-py can read —
/// upstream states the manifest and lockfile "are not explicitly
/// supported" while the virtual environments "are fully supported" — so
/// its own `.venv` is the supported path.
///
/// Pointing at an in-project `.venv` is *not* the ambient-interpreter
/// hazard [`crate::deps::pip_audit`] refuses: the path is explicit and
/// belongs to the project, so the BOM describes this tree and not
/// whichever interpreter happens to be on `$PATH`.
fn detect_python_source(path: &Path) -> Option<PySource> {
    if path.join("poetry.lock").is_file() {
        return Some(PySource {
            subcommand: "poetry",
            target: ".",
            describes: "poetry.lock",
        });
    }
    if path.join("requirements.txt").is_file() {
        return Some(PySource {
            subcommand: "requirements",
            target: "requirements.txt",
            describes: "requirements.txt",
        });
    }
    if path.join("Pipfile.lock").is_file() {
        return Some(PySource {
            subcommand: "pipenv",
            target: ".",
            describes: "Pipfile.lock",
        });
    }
    VENV_DIRS
        .iter()
        .find(|d| path.join(d).is_dir())
        .map(|d| PySource {
            subcommand: "environment",
            target: d,
            describes: d,
        })
}

fn run_cyclonedx_py(path: &Path, runner: &dyn CmdRunner) -> Result<(String, &'static str)> {
    let source = detect_python_source(path).ok_or_else(|| {
        if path.join("pyproject.toml").is_file() || path.join("uv.lock").is_file() {
            // A PEP-621 project that has not been installed. cyclonedx-py
            // reads neither pyproject.toml nor uv.lock directly, so say
            // what to run rather than claiming the tree is unsupported.
            anyhow!(
                "{} is a PEP-621 project with no lockfile cyclonedx-py can read and no \
                 virtualenv to read instead. Create the project's venv — `uv sync`, \
                 `pdm install`, or `hatch env create` — and re-run; the SBOM is then \
                 built from `.venv`. Alternatively export a manifest: `uv export \
                 --format requirements-txt > requirements.txt`.",
                path.display()
            )
        } else {
            anyhow!(
                "no requirements.txt, poetry.lock, Pipfile.lock, or in-project virtualenv \
                 ({}) under {} — cyclonedx-py needs an explicit target; an SBOM of the \
                 ambient Python interpreter would describe something other than the project",
                VENV_DIRS.join(" / "),
                path.display()
            )
        }
    })?;

    let (subcommand, manifest) = (source.subcommand, source.target);

    let out = runner.run(
        "cyclonedx-py",
        &[
            subcommand,
            manifest,
            "--spec-version",
            Ecosystem::Python.spec_version(),
            "--output-format",
            "JSON",
            // Suppresses serialNumber and metadata.timestamp upstream;
            // `cyclonedx::normalize` strips them again regardless.
            "--output-reproducible",
            "--output-file",
            "-",
        ],
        path,
    )?;

    Ok((require_json_stdout("cyclonedx-py", out)?, source.describes))
}

// -----------------------------------------------------------------------------
// node
// -----------------------------------------------------------------------------

fn run_cyclonedx_npm(path: &Path, runner: &dyn CmdRunner) -> Result<(String, &'static str)> {
    if !path.join("package.json").is_file() {
        return Err(anyhow!(
            "no package.json under {} — cyclonedx-npm describes an npm project",
            path.display()
        ));
    }

    let out = runner.run(
        "cyclonedx-npm",
        &[
            "--spec-version",
            Ecosystem::Node.spec_version(),
            "--output-format",
            "JSON",
            "--output-reproducible",
            "--output-file",
            "-",
        ],
        path,
    )?;

    Ok((require_json_stdout("cyclonedx-npm", out)?, "node_modules"))
}

// -----------------------------------------------------------------------------
// rust
// -----------------------------------------------------------------------------

/// Deletes every `filename` under `root` when it drops.
///
/// The guard re-walks at drop time rather than holding a file list
/// captured earlier: it is armed before `cargo cyclonedx` runs, so at
/// construction there is nothing to list, and re-walking also covers
/// files written by a generator run that then failed. Cleanup therefore
/// does not depend on any earlier walk having succeeded.
struct EmittedFiles {
    root: PathBuf,
    filename: String,
}

impl Drop for EmittedFiles {
    fn drop(&mut self) {
        for p in find_emitted(&self.root, &self.filename).unwrap_or_default() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Output stem for the transient BOM files. Process-scoped so two
/// concurrent captures of the same repo cannot delete each other's
/// output; the stem never reaches the artifact.
fn cargo_output_stem() -> String {
    format!("secunit-sbom-{}", std::process::id())
}

fn run_cargo_cyclonedx(path: &Path, runner: &dyn CmdRunner) -> Result<(String, &'static str)> {
    if !path.join("Cargo.toml").is_file() {
        return Err(anyhow!(
            "no Cargo.toml under {} — cargo-cyclonedx describes a cargo workspace",
            path.display()
        ));
    }

    let stem = cargo_output_stem();
    let filename = format!("{stem}.json");

    // Refuse rather than clobber: the generator overwrites silently.
    let preexisting = find_emitted(path, &filename)?;
    if !preexisting.is_empty() {
        return Err(anyhow!(
            "refusing to run cargo-cyclonedx: {} already exists under {} — \
             remove it and retry",
            preexisting[0].display(),
            path.display()
        ));
    }

    // Armed *before* the generator runs, so a failure anywhere below —
    // including inside `runner.run` after it has written some members'
    // BOMs, or in the post-run walk — still cleans the audited tree.
    let _cleanup = EmittedFiles {
        root: path.to_path_buf(),
        filename: filename.clone(),
    };

    let out = runner.run(
        "cargo",
        &[
            "cyclonedx",
            "--format",
            "json",
            "--spec-version",
            Ecosystem::Rust.spec_version(),
            // `-a, --all` is cargo-cyclonedx's dependency-depth flag —
            // the whole graph rather than only top-level deps. It is
            // already the default; passed explicitly so a change of
            // default cannot silently narrow the SBOM. It is *not*
            // `--workspace`: emitting one BOM per member manifest is
            // this tool's unconditional behaviour.
            "--all",
            "--override-filename",
            &stem,
        ],
        path,
    )?;

    // Unlike pip-audit and pnpm, an SBOM generator has no
    // findings-mean-non-zero convention: non-zero always means the run
    // went wrong, and a BOM written alongside it may be missing a
    // subtree. Recording that as clean evidence would understate
    // `component_count` and make the next sweep diff it as
    // "dependencies removed".
    require_success("cargo-cyclonedx", out.exit_code, &out.stderr)?;

    let emitted = find_emitted(path, &filename)?;

    if emitted.is_empty() {
        return Err(anyhow!(
            "cargo-cyclonedx exited 0 without writing {filename} under {}; stderr: {}",
            path.display(),
            truncate(&out.stderr, 500)
        ));
    }

    // A workspace yields one BOM per member. Prefer the root's; merging
    // several BOMs would invent a document no generator produced.
    let root = path.join(&filename);
    let chosen = if root.is_file() {
        root
    } else if emitted.len() == 1 {
        emitted[0].clone()
    } else {
        return Err(anyhow!(
            "cargo-cyclonedx wrote {} BOMs for the workspace at {} and none at its \
             root ({}) — the root manifest is virtual (`[workspace]` with no \
             `[package]`); point --path at a single workspace member",
            emitted.len(),
            path.display(),
            emitted
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };

    let bom =
        std::fs::read_to_string(&chosen).with_context(|| format!("read {}", chosen.display()))?;
    Ok((bom, "Cargo.lock"))
}

/// Every `filename` under `root`, skipping `target/`, `node_modules/`,
/// and dotted dirs.
///
/// Two deliberate choices:
///
/// - **No depth cap.** `cargo-cyclonedx` writes beside *every* member
///   manifest, and a member can sit arbitrarily deep. A cap would hide
///   those files from both the clobber check and the cleanup, so they
///   would accumulate in the operator's repo unnoticed — the pruning
///   below is what keeps the walk cheap.
/// - **The root is exempt from pruning.** `root` itself may sit under a
///   dotted path (a checkout in `~/.local/src`, or a temp dir), and
///   pruning it would silently walk nothing.
fn find_emitted(root: &Path, filename: &str) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let walker = walkdir::WalkDir::new(root).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        let name = e.file_name().to_string_lossy();
        !(name == "target" || name == "node_modules" || name.starts_with('.'))
    });
    for entry in walker {
        let entry = entry.with_context(|| format!("walk {}", root.display()))?;
        if entry.file_type().is_file() && entry.file_name() == filename {
            found.push(entry.into_path());
        }
    }
    found.sort();
    Ok(found)
}

// -----------------------------------------------------------------------------
// shared
// -----------------------------------------------------------------------------

/// Fail unless the generator exited 0.
///
/// `docs/cli.md` promises a capture never yields a partial artifact.
/// An SBOM generator that exits non-zero may still have written a
/// schema-valid BOM that is missing components, so the exit code is the
/// only signal that separates complete evidence from plausible
/// evidence.
fn require_success(program: &str, exit_code: i32, stderr: &str) -> Result<()> {
    if exit_code == 0 {
        return Ok(());
    }
    Err(anyhow!(
        "{program} exited {exit_code}; its BOM may be incomplete, so no artifact was \
         written. stderr: {}",
        truncate(stderr, 500)
    ))
}

/// Require a successful exit *and* non-empty stdout: unlike the audit
/// capturers, "no output" is never a valid SBOM.
fn require_json_stdout(program: &str, out: super::cmd::CmdOutput) -> Result<String> {
    require_success(program, out.exit_code, &out.stderr)?;
    if out.stdout.trim().is_empty() {
        return Err(anyhow!(
            "{program} exited 0 without emitting a BOM; stderr: {}",
            truncate(&out.stderr, 500)
        ));
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::cmd::testing::{CannedRunner, MissingToolRunner};
    use crate::deps::cmd::CmdOutput;

    fn fixture(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/captures/deps/sbom")
            .join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    fn scratch_with(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            std::fs::write(d.path().join(f), "{}\n").unwrap();
        }
        d
    }

    fn py_runner(stdout: String) -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "cyclonedx-py",
            &[
                "requirements",
                "requirements.txt",
                "--spec-version",
                "1.6",
                "--output-format",
                "JSON",
                "--output-reproducible",
                "--output-file",
                "-",
            ],
            CmdOutput {
                stdout,
                stderr: String::new(),
                exit_code: 0,
            },
        );
        r
    }

    fn npm_runner(stdout: String) -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "cyclonedx-npm",
            &[
                "--spec-version",
                "1.6",
                "--output-format",
                "JSON",
                "--output-reproducible",
                "--output-file",
                "-",
            ],
            CmdOutput {
                stdout,
                stderr: String::new(),
                exit_code: 0,
            },
        );
        r
    }

    #[test]
    fn python_capture_is_schema_valid_and_records_provenance() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);
        let cap = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &py_runner(fixture("python.json")),
        )
        .unwrap();

        assert_eq!(cap.envelope.result["spec_version"], "1.6");
        assert_eq!(cap.envelope.result["ecosystem"], "python");
        assert_eq!(cap.envelope.result["component_count"], 3);
        let gens = cap.envelope.result["generators"].as_array().unwrap();
        assert!(
            gens.iter().any(|g| g["name"] == "cyclonedx-py"),
            "generator provenance recorded: {gens:?}"
        );
        assert_eq!(
            cap.envelope.result["bom_sha256"].as_str().unwrap().len(),
            64
        );
    }

    #[test]
    fn node_capture_is_schema_valid() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["package.json"]);
        let cap = capture_with(
            d.path(),
            Some(Ecosystem::Node),
            &npm_runner(fixture("node.json")),
        )
        .unwrap();
        assert_eq!(cap.envelope.result["spec_version"], "1.6");
        assert_eq!(cap.envelope.result["ecosystem"], "node");
    }

    #[test]
    fn captures_of_an_unchanged_tree_are_byte_identical() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);
        let a = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &py_runner(fixture("python.json")),
        )
        .unwrap();
        let b = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &py_runner(fixture("python.json")),
        )
        .unwrap();
        assert_eq!(a.bom_json, b.bom_json, "BOM bytes must be stable");
        assert_eq!(
            a.envelope.to_canonical_json().unwrap(),
            b.envelope.to_canonical_json().unwrap()
        );
    }

    #[test]
    fn a_regenerated_bom_with_a_fresh_serial_hashes_the_same() {
        // The real failure this guards: a generator without
        // --output-reproducible emits a new serialNumber and timestamp
        // every run, so an unchanged tree would diff on every sweep.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["requirements.txt"]);

        let first = fixture("python.json");
        let mut second: Value = serde_json::from_str(&first).unwrap();
        second["serialNumber"] = json!("urn:uuid:deadbeef-cafe-4bad-8f01-feedfacec0de");
        second["metadata"]["timestamp"] = json!("2031-01-01T00:00:00Z");

        let a = capture_with(d.path(), Some(Ecosystem::Python), &py_runner(first)).unwrap();
        let b = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &py_runner(serde_json::to_string(&second).unwrap()),
        )
        .unwrap();

        assert_eq!(
            a.envelope.result["bom_sha256"], b.envelope.result["bom_sha256"],
            "serialNumber and timestamp must not reach the hash"
        );
    }

    #[test]
    fn missing_generator_names_the_install_command() {
        let d = scratch_with(&["requirements.txt"]);
        let err = capture_with(d.path(), Some(Ecosystem::Python), &MissingToolRunner)
            .expect_err("probe should fail");
        let msg = format!("{err}");
        assert!(msg.contains("cyclonedx-py"), "{msg}");
        assert!(msg.contains("pipx install cyclonedx-bom"), "{msg}");
    }

    #[test]
    fn detect_picks_the_single_present_ecosystem() {
        let d = scratch_with(&["Cargo.toml"]);
        assert_eq!(detect_ecosystem(d.path()).unwrap(), Ecosystem::Rust);
    }

    #[test]
    fn detect_refuses_a_polyglot_tree() {
        let d = scratch_with(&["Cargo.toml", "package.json"]);
        let err = detect_ecosystem(d.path()).expect_err("ambiguous tree");
        let msg = format!("{err}");
        assert!(
            msg.contains("--ecosystem"),
            "tells the operator the fix: {msg}"
        );
        assert!(msg.contains("rust") && msg.contains("node"), "{msg}");
    }

    #[test]
    fn detect_names_the_supported_manifests_when_none_match() {
        let d = scratch_with(&[]);
        let err = detect_ecosystem(d.path()).expect_err("empty tree");
        let msg = format!("{err}");
        assert!(
            msg.contains("Cargo.toml") && msg.contains("package.json"),
            "{msg}"
        );
    }

    #[test]
    fn empty_generator_output_is_an_error_not_an_empty_bom() {
        let d = scratch_with(&["package.json"]);
        let r = CannedRunner::new();
        r.register(
            "cyclonedx-npm",
            &[
                "--spec-version",
                "1.6",
                "--output-format",
                "JSON",
                "--output-reproducible",
                "--output-file",
                "-",
            ],
            CmdOutput {
                stdout: String::new(),
                stderr: "npm ERR! code ELSPROBLEMS\nnpm ERR! missing: ms@2.1.3".into(),
                exit_code: 1,
            },
        );
        let err = capture_with(d.path(), Some(Ecosystem::Node), &r).expect_err("expected error");
        let msg = format!("{err}");
        assert!(msg.contains("exited 1"), "{msg}");
        assert!(msg.contains("ELSPROBLEMS"), "surfaces stderr: {msg}");
    }

    #[test]
    fn ecosystem_parses_its_aliases_and_rejects_others() {
        assert_eq!("rust".parse::<Ecosystem>().unwrap(), Ecosystem::Rust);
        assert_eq!("typescript".parse::<Ecosystem>().unwrap(), Ecosystem::Node);
        assert_eq!("py".parse::<Ecosystem>().unwrap(), Ecosystem::Python);
        let err = "go".parse::<Ecosystem>().expect_err("go is unsupported");
        assert!(format!("{err}").contains("rust, python, node"));
    }

    /// cargo-cyclonedx writes a file rather than printing to stdout, so
    /// the canned runner reproduces that side effect.
    fn cargo_runner(bom: &str) -> (CannedRunner, String) {
        let stem = cargo_output_stem();
        let r = CannedRunner::new();
        r.register_writing(
            "cargo",
            &[
                "cyclonedx",
                "--format",
                "json",
                "--spec-version",
                "1.5",
                "--all",
                "--override-filename",
                &stem,
            ],
            &format!("{stem}.json"),
            bom,
        );
        (r, stem)
    }

    #[test]
    fn rust_capture_reads_the_emitted_file_and_validates_against_1_5() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["Cargo.toml"]);
        let (runner, _) = cargo_runner(&fixture("rust.json"));
        let cap = capture_with(d.path(), Some(Ecosystem::Rust), &runner).unwrap();

        assert_eq!(cap.envelope.result["spec_version"], "1.5");
        assert_eq!(cap.envelope.result["ecosystem"], "rust");
        let gens = cap.envelope.result["generators"].as_array().unwrap();
        assert!(
            gens.iter().any(|g| g["name"] == "cargo-cyclonedx"),
            "{gens:?}"
        );
    }

    #[test]
    fn rust_capture_deletes_the_bom_it_wrote_into_the_tree() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["Cargo.toml"]);
        let (runner, stem) = cargo_runner(&fixture("rust.json"));
        capture_with(d.path(), Some(Ecosystem::Rust), &runner).unwrap();

        assert!(
            !d.path().join(format!("{stem}.json")).exists(),
            "the audited tree must be left as it was found"
        );
    }

    #[test]
    fn rust_capture_cleans_up_even_when_the_bom_is_unusable() {
        // The cleanup guard must fire on the error path too, or a bad
        // BOM leaves litter in the operator's repo.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["Cargo.toml"]);
        let (runner, stem) = cargo_runner("{\"bomFormat\":\"CycloneDX\"}");
        let err =
            capture_with(d.path(), Some(Ecosystem::Rust), &runner).expect_err("no specVersion");

        assert!(format!("{err}").contains("specVersion"), "{err}");
        assert!(
            !d.path().join(format!("{stem}.json")).exists(),
            "cleanup must run on the error path"
        );
    }

    #[test]
    fn rust_capture_refuses_to_clobber_a_preexisting_file() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["Cargo.toml"]);
        let (runner, stem) = cargo_runner(&fixture("rust.json"));
        let squatter = d.path().join(format!("{stem}.json"));
        std::fs::write(&squatter, "do not overwrite me").unwrap();

        let err = capture_with(d.path(), Some(Ecosystem::Rust), &runner)
            .expect_err("should refuse to clobber");
        assert!(format!("{err}").contains("already exists"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&squatter).unwrap(),
            "do not overwrite me",
            "the pre-existing file must be untouched"
        );
    }

    #[test]
    fn a_non_zero_exit_fails_even_when_the_bom_parses() {
        // cyclonedx-npm on a partially-installed node_modules warns,
        // emits a BOM missing a subtree, and exits 1. The BOM is
        // schema-valid, so only the exit code separates it from
        // complete evidence — accepting it would understate
        // component_count and diff as "dependencies removed" next sweep.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["package.json"]);
        let r = CannedRunner::new();
        r.register(
            "cyclonedx-npm",
            &[
                "--spec-version",
                "1.6",
                "--output-format",
                "JSON",
                "--output-reproducible",
                "--output-file",
                "-",
            ],
            CmdOutput {
                stdout: fixture("node.json"),
                stderr: "npm warn ELSPROBLEMS missing: ms@2.1.3".into(),
                exit_code: 1,
            },
        );
        let err = capture_with(d.path(), Some(Ecosystem::Node), &r)
            .expect_err("non-zero exit must fail the capture");
        let msg = format!("{err}");
        assert!(msg.contains("exited 1"), "{msg}");
        assert!(msg.contains("may be incomplete"), "{msg}");
        assert!(msg.contains("ELSPROBLEMS"), "surfaces stderr: {msg}");
    }

    #[test]
    fn a_non_zero_cargo_exit_fails_and_leaves_no_bom_behind() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["Cargo.toml"]);
        let stem = cargo_output_stem();
        let r = CannedRunner::new();
        r.register_writing(
            "cargo",
            &[
                "cyclonedx",
                "--format",
                "json",
                "--spec-version",
                "1.5",
                "--all",
                "--override-filename",
                &stem,
            ],
            &format!("{stem}.json"),
            &fixture("rust.json"),
        );
        r.set_exit_code(
            "cargo",
            &[
                "cyclonedx",
                "--format",
                "json",
                "--spec-version",
                "1.5",
                "--all",
                "--override-filename",
                &stem,
            ],
            101,
        );

        let err = capture_with(d.path(), Some(Ecosystem::Rust), &r)
            .expect_err("non-zero exit must fail the capture");
        assert!(format!("{err}").contains("exited 101"), "{err}");
        assert!(
            !d.path().join(format!("{stem}.json")).exists(),
            "the BOM written before the failure must still be cleaned up"
        );
    }

    #[test]
    fn cleanup_reaches_a_workspace_member_deeper_than_four_levels() {
        // cargo-cyclonedx writes beside every member manifest, and a
        // member can sit arbitrarily deep. A depth-capped walk would
        // leave this file in the operator's repo forever, invisible to
        // both the cleanup and the next run's clobber check.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = scratch_with(&["Cargo.toml"]);
        let deep = d.path().join("apps/a/services/b/c");
        std::fs::create_dir_all(&deep).unwrap();

        let stem = cargo_output_stem();
        let filename = format!("{stem}.json");
        {
            let _guard = EmittedFiles {
                root: d.path().to_path_buf(),
                filename: filename.clone(),
            };
            std::fs::write(deep.join(&filename), "{}").unwrap();
            assert!(deep.join(&filename).exists());
        }
        assert!(
            !deep.join(&filename).exists(),
            "cleanup must reach depth 5 and beyond"
        );
    }

    /// A uv / PEP-621 project: no requirements.txt, no poetry.lock, and
    /// a `.venv` the capturer reads instead.
    fn uv_runner(stdout: String) -> CannedRunner {
        let r = CannedRunner::new();
        r.register(
            "cyclonedx-py",
            &[
                "environment",
                ".venv",
                "--spec-version",
                "1.6",
                "--output-format",
                "JSON",
                "--output-reproducible",
                "--output-file",
                "-",
            ],
            CmdOutput {
                stdout,
                stderr: String::new(),
                exit_code: 0,
            },
        );
        r
    }

    fn uv_project_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("pyproject.toml"), "[project]\nname='x'\n").unwrap();
        std::fs::write(d.path().join("uv.lock"), "version = 1\n").unwrap();
        std::fs::create_dir(d.path().join(".venv")).unwrap();
        d
    }

    #[test]
    fn a_uv_project_is_captured_from_its_own_venv() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = uv_project_dir();
        let cap = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &uv_runner(fixture("python-uv.json")),
        )
        .unwrap();

        assert_eq!(cap.envelope.result["ecosystem"], "python");
        assert_eq!(cap.envelope.result["spec_version"], "1.6");
        assert_eq!(cap.envelope.result["component_count"], 2);
        assert_eq!(
            cap.envelope.result["describes"], ".venv",
            "the envelope must record that this came from an install, not a lockfile"
        );
    }

    #[test]
    fn uv_captures_are_byte_identical_across_runs() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = uv_project_dir();
        let a = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &uv_runner(fixture("python-uv.json")),
        )
        .unwrap();
        let b = capture_with(
            d.path(),
            Some(Ecosystem::Python),
            &uv_runner(fixture("python-uv.json")),
        )
        .unwrap();
        assert_eq!(a.bom_json, b.bom_json);
    }

    #[test]
    fn a_declared_manifest_wins_over_an_installed_venv() {
        // requirements.txt describes what the project asks for and needs
        // no install step, so it must be preferred even when a venv is
        // sitting right there.
        let d = uv_project_dir();
        std::fs::write(d.path().join("requirements.txt"), "certifi==2023.7.22\n").unwrap();
        let src = detect_python_source(d.path()).unwrap();
        assert_eq!(src.subcommand, "requirements");
        assert_eq!(src.describes, "requirements.txt");
    }

    #[test]
    fn a_bare_venv_directory_is_found_under_either_conventional_name() {
        for name in [".venv", "venv"] {
            let d = tempfile::tempdir().unwrap();
            std::fs::write(d.path().join("pyproject.toml"), "[project]\n").unwrap();
            std::fs::create_dir(d.path().join(name)).unwrap();
            let src = detect_python_source(d.path()).expect("venv should be found");
            assert_eq!(src.subcommand, "environment");
            assert_eq!(src.target, name);
        }
    }

    #[test]
    fn a_pep621_only_tree_names_the_generator_limit_not_a_missing_manifest() {
        // Regression: `pip-audit` accepts a bare pyproject.toml, so a
        // sweep running both actions must not get "unsupported
        // manifest" from `sbom` for a tree the other action handles.
        let d = scratch_with(&["pyproject.toml"]);
        assert_eq!(
            detect_ecosystem(d.path()).unwrap(),
            Ecosystem::Python,
            "detection must agree with pip_audit"
        );

        let err = capture_with(d.path(), Some(Ecosystem::Python), &CannedRunner::new())
            .expect_err("cyclonedx-py has no bare-pyproject subcommand");
        let msg = format!("{err}");
        assert!(msg.contains("cyclonedx-py"), "names the tool: {msg}");
        assert!(msg.contains("uv export"), "gives a way forward: {msg}");
        assert!(
            !msg.contains("no requirements.txt"),
            "must not claim the manifest is unsupported: {msg}"
        );
    }

    #[test]
    fn python_without_a_manifest_refuses_rather_than_describing_the_interpreter() {
        let d = scratch_with(&[]);
        let err = capture_with(d.path(), Some(Ecosystem::Python), &CannedRunner::new())
            .expect_err("no manifest");
        let msg = format!("{err}");
        assert!(msg.contains("ambient"), "{msg}");
    }
}
