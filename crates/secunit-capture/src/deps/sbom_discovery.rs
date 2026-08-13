//! Subproject discovery for the CycloneDX SBOM capturer.
//!
//! [`crate::deps::sbom`] describes one project rooted at one directory.
//! A repo whose projects live one or more levels down — `api/` python,
//! `client/` node, no manifest at the root — has nothing there to
//! capture, so the operator has to know the layout, run one capture per
//! project, and name each output by hand. Nothing relates the resulting
//! artifacts to each other.
//!
//! This module walks the tree instead and emits **one artifact per
//! discovered project** plus an index. Merging them is not on offer: a
//! CycloneDX BOM carries a single `metadata.component`, so folding a
//! Python and a Node dependency graph into one document would invent a
//! deployable that does not exist. One BOM per deployable is also what
//! `inventory.yaml` and the `capture-sweep` skill already assume.
//!
//! ## What the walk does and does not enter
//!
//! Descent stops at the first directory carrying a manifest. That is
//! what keeps an npm workspace root captured *once*: `cyclonedx-npm`
//! already aggregates every workspace member and their transitive deps
//! into the root's BOM, so walking on into `packages/*` would emit the
//! same components again under a second artifact. The same holds for a
//! cargo workspace with a root `[package]`.
//!
//! Vendored and installed trees ([`SKIP_DIRS`]) and every dot-directory
//! are never entered — see the constant for why each one matters.
//!
//! ## Failure is per project
//!
//! A generator that is not installed, or a `node_modules` that was never
//! installed, fails *that* project only: it is recorded in the index as
//! `needs-operator` with the exact command to re-run, mirroring how the
//! bundled `capture-sweep` skill handles a system it could not capture.
//! One uninstalled subproject must not cost an operator the SBOMs for
//! the rest of the repo.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::canonical::Envelope;

use super::cmd::{CmdRunner, RealRunner};
use super::sbom::{self, Ecosystem, SbomCapture};
use super::truncate;

pub const CAPTURER: &str = "deps.sbom-discovery";
pub const VERSION: &str = "1";

/// Filename of the index written under `--out-dir`, so a control has one
/// file to diff rather than N artifacts to enumerate.
pub const INDEX_FILENAME: &str = "index.json";

/// Artifact stem used for a project at the discovery root, since its
/// relative path is `.` and `.cdx.json` would be a hidden file.
const ROOT_STEM: &str = "sbom";

pub const STATUS_CAPTURED: &str = "captured";
pub const STATUS_NEEDS_OPERATOR: &str = "needs-operator";

/// Undotted directory names the walk never descends into.
///
/// - `node_modules` and `venv` are *installed* trees: every vendored
///   package carries its own `package.json` / `pyproject.toml`, so
///   descending would emit one artifact per dependency and drown the
///   projects that actually belong to the repo.
/// - `target` is cargo's build directory, which vendors manifests the
///   same way.
///
/// Dot-directories are skipped wholesale rather than named here.
/// `.claude/worktrees/` is the case that matters: those are git
/// worktrees of the same repo, and capturing them would produce a second
/// near-duplicate artifact for every project the repo already has.
/// `.venv` and `.tox` fall out of the same rule.
const SKIP_DIRS: &[&str] = &["node_modules", "target", "venv"];

/// A project the walk found, and what capturing it produced.
#[derive(Debug)]
pub struct DiscoveredProject {
    /// Path relative to the discovery root, `/`-separated, `.` for the
    /// root itself. Slash-separated rather than platform-native so the
    /// index bytes do not depend on which OS captured them.
    pub rel_path: String,
    pub ecosystem: Ecosystem,
    /// Where this project's BOM belongs under `--out-dir`.
    pub bom_file: String,
    pub outcome: Outcome,
}

/// Whether a discovered project yielded evidence or a task.
#[derive(Debug)]
pub enum Outcome {
    Captured(Box<SbomCapture>),
    /// The capture could not complete. Both strings reach the index:
    /// the error so a reviewer sees why, the command so the operator
    /// does not have to re-derive it.
    NeedsOperator {
        error: String,
        remediation: String,
    },
}

impl Outcome {
    pub fn status(&self) -> &'static str {
        match self {
            Outcome::Captured(_) => STATUS_CAPTURED,
            Outcome::NeedsOperator { .. } => STATUS_NEEDS_OPERATOR,
        }
    }
}

/// The result of one discovery run: every project's outcome, plus the
/// index envelope describing all of them.
///
/// Nothing is written here — the caller writes each BOM, its sidecar,
/// and the index, so a schema failure can still leave the out-dir
/// untouched.
#[derive(Debug)]
pub struct DiscoveryRun {
    pub projects: Vec<DiscoveredProject>,
    pub index: Envelope,
}

impl DiscoveryRun {
    pub fn captured_count(&self) -> usize {
        self.projects
            .iter()
            .filter(|p| matches!(p.outcome, Outcome::Captured(_)))
            .count()
    }

    pub fn needs_operator_count(&self) -> usize {
        self.projects.len() - self.captured_count()
    }
}

/// Discover every project under `root` and capture an SBOM for each.
///
/// `out_dir` is not written to; it is recorded in the index's args and
/// used to spell out the re-run command for a project that failed.
pub fn capture_tree(root: &Path, out_dir: &Path) -> Result<DiscoveryRun> {
    capture_tree_with(root, out_dir, &RealRunner)
}

/// Same as [`capture_tree`] but with an injectable subprocess runner.
pub fn capture_tree_with(
    root: &Path,
    out_dir: &Path,
    runner: &dyn CmdRunner,
) -> Result<DiscoveryRun> {
    let found = discover(root)?;
    if found.is_empty() {
        return Err(anyhow!(
            "no supported dependency manifest anywhere under {} — expected one of: {}. \
             {} and dot-directories (`.claude/worktrees/`, `.venv`) are not descended into",
            root.display(),
            sbom::all_markers().join(", "),
            SKIP_DIRS
                .iter()
                .map(|d| format!("{d}/"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let projects: Vec<DiscoveredProject> = found
        .into_iter()
        .map(|p| capture_one(&p, out_dir, runner))
        .collect();

    let entries: Vec<Value> = projects.iter().map(index_entry).collect();
    let captured = entries
        .iter()
        .filter(|e| e["status"] == STATUS_CAPTURED)
        .count();

    let index = Envelope::new(
        CAPTURER,
        VERSION,
        json!({
            "path": root.display().to_string(),
            "out_dir": out_dir.display().to_string(),
        }),
        json!({
            "captured_count": captured,
            "needs_operator_count": entries.len() - captured,
            "project_count": entries.len(),
            "projects": entries,
        }),
    );

    Ok(DiscoveryRun { projects, index })
}

fn capture_one(found: &Found, out_dir: &Path, runner: &dyn CmdRunner) -> DiscoveredProject {
    let bom_file = bom_file(&found.rel_path, found.ecosystem, found.ambiguous);
    let outcome = match sbom::capture_with(&found.dir, Some(found.ecosystem), runner) {
        Ok(mut cap) => {
            cap.set_bom_filename(&bom_file);
            Outcome::Captured(Box::new(cap))
        }
        Err(e) => Outcome::NeedsOperator {
            // `{:#}` flattens anyhow's context chain onto one line: the
            // index is read as JSON, and the generator's own remediation
            // ("install it with …") lives in the innermost cause.
            error: truncate(&format!("{e:#}"), 2000),
            remediation: format!(
                "secunit capture deps sbom --path {} --ecosystem {} --out {}",
                found.dir.display(),
                found.ecosystem.as_str(),
                out_dir.join(&bom_file).display()
            ),
        },
    };
    DiscoveredProject {
        rel_path: found.rel_path.clone(),
        ecosystem: found.ecosystem,
        bom_file,
        outcome,
    }
}

fn index_entry(p: &DiscoveredProject) -> Value {
    let mut entry = json!({
        "bom_file": p.bom_file,
        "ecosystem": p.ecosystem.as_str(),
        "path": p.rel_path,
        "status": p.outcome.status(),
    });
    match &p.outcome {
        Outcome::Captured(cap) => {
            // Copied from the sidecar rather than recomputed, so the
            // index and the envelope can never disagree about the hash.
            for k in ["bom_sha256", "component_count", "spec_version"] {
                entry[k] = cap.envelope.result[k].clone();
            }
        }
        Outcome::NeedsOperator { error, remediation } => {
            entry["error"] = json!(error);
            entry["remediation"] = json!(remediation);
        }
    }
    entry
}

/// Artifact name for a project.
///
/// The ecosystem is spliced in only when the directory carries manifests
/// for more than one — two BOMs of the same tree cannot share a
/// filename. Keeping it out of the single-ecosystem case means the
/// common artifact name is just the project's path, so an operator can
/// map artifact to subdirectory without reading the index.
fn bom_file(rel_path: &str, ecosystem: Ecosystem, ambiguous: bool) -> String {
    let stem = if rel_path == "." { ROOT_STEM } else { rel_path };
    if ambiguous {
        format!("{stem}.{}.cdx.json", ecosystem.as_str())
    } else {
        format!("{stem}.cdx.json")
    }
}

/// A project root the walk identified, before any capture is attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    dir: PathBuf,
    rel_path: String,
    ecosystem: Ecosystem,
    /// The directory carries manifests for more than one ecosystem, so
    /// this is one of several projects sharing a path.
    ambiguous: bool,
}

/// Every project root under `root`, sorted, at most one entry per
/// (directory, ecosystem).
fn discover(root: &Path) -> Result<Vec<Found>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let ecosystems = ecosystems_at(&dir);
        if !ecosystems.is_empty() {
            // Stop descending: a project root owns its subtree. Every
            // generator here already aggregates the members below it,
            // so continuing would re-emit the same components under a
            // second artifact.
            //
            // A directory carrying markers for several toolchains
            // becomes several projects rather than the error
            // `sbom::detect_ecosystem` raises for a single capture:
            // there is nothing to disambiguate when both are wanted,
            // and a python service that builds its own static assets is
            // an ordinary shape.
            let rel_path = rel_path(root, &dir);
            let ambiguous = ecosystems.len() > 1;
            for ecosystem in ecosystems {
                found.push(Found {
                    dir: dir.clone(),
                    rel_path: rel_path.clone(),
                    ecosystem,
                    ambiguous,
                });
            }
            continue;
        }
        stack.extend(child_dirs(&dir)?);
    }

    // The stack pops in reverse read_dir order, which is filesystem
    // dependent; the index has to be byte-identical across runs and
    // machines, so ordering is imposed here rather than inherited.
    found.sort_by(|a, b| {
        a.rel_path
            .cmp(&b.rel_path)
            .then_with(|| a.ecosystem.as_str().cmp(b.ecosystem.as_str()))
    });
    Ok(found)
}

/// Which toolchains' manifests sit directly in `dir`.
fn ecosystems_at(dir: &Path) -> Vec<Ecosystem> {
    Ecosystem::ALL
        .iter()
        .copied()
        .filter(|e| e.markers().iter().any(|m| dir.join(m).is_file()))
        .collect()
}

fn child_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("read directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("read directory {}", dir.display()))?;
        // `DirEntry::file_type` does not follow symlinks, so a symlinked
        // directory is never descended into. That keeps the walk from
        // looping on a self-referential link, and stops a link pointing
        // outside the tree from pulling in a project that is not part of
        // the repo being captured.
        if !entry
            .file_type()
            .with_context(|| format!("stat {}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
            continue;
        }
        out.push(entry.path());
    }
    Ok(out)
}

/// `dir` relative to `root`, `/`-separated, `.` for `root` itself.
///
/// The root is exempt from the dot-directory rule the walk applies to
/// everything below it: a checkout can legitimately live under a dotted
/// path (`~/.local/src/repo`, or a temp dir), and pruning the root would
/// silently discover nothing.
fn rel_path(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let joined = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::cmd::testing::{CannedRunner, MissingToolRunner};
    use crate::deps::cmd::CmdOutput;

    fn fixture_projects() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/projects")
    }

    fn fixture_capture(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/captures/deps/sbom")
            .join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
    }

    /// Copy one of the committed single-project fixtures to `dest`, so a
    /// multi-project tree is composed from the same manifests the
    /// single-project tests use rather than from invented ones.
    fn plant(fixture: &str, dest: &Path) {
        copy_dir(&fixture_projects().join(fixture), dest);
    }

    fn copy_dir(src: &Path, dest: &Path) {
        std::fs::create_dir_all(dest).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dest.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), &to).unwrap();
            }
        }
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "{}\n").unwrap();
    }

    /// A runner with a canned response for every generator, keyed on
    /// program and args only — so one registration serves every project
    /// of that ecosystem in the tree.
    fn all_generators() -> CannedRunner {
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
                stdout: fixture_capture("python.json"),
                stderr: String::new(),
                exit_code: 0,
            },
        );
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
                stdout: fixture_capture("node.json"),
                stderr: String::new(),
                exit_code: 0,
            },
        );
        r
    }

    /// `api/` python + `client/` node, nothing at the root: the shape
    /// that produces no capture at all without discovery.
    fn polyglot_tree() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        plant("python", &d.path().join("api"));
        plant("node", &d.path().join("client"));
        d
    }

    fn out_dir() -> PathBuf {
        PathBuf::from("/evidence/sbom")
    }

    fn paths(run: &DiscoveryRun) -> Vec<(&str, &str, &str)> {
        run.projects
            .iter()
            .map(|p| {
                (
                    p.rel_path.as_str(),
                    p.ecosystem.as_str(),
                    p.outcome.status(),
                )
            })
            .collect()
    }

    #[test]
    fn discovery_finds_a_subproject_per_ecosystem_when_the_root_has_no_manifest() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = polyglot_tree();
        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();

        assert_eq!(
            paths(&run),
            vec![
                ("api", "python", STATUS_CAPTURED),
                ("client", "node", STATUS_CAPTURED),
            ]
        );
        assert_eq!(run.index.result["project_count"], 2);
        assert_eq!(run.index.result["needs_operator_count"], 0);
    }

    #[test]
    fn each_project_gets_an_artifact_named_after_its_path() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = polyglot_tree();
        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();

        let files: Vec<&str> = run.projects.iter().map(|p| p.bom_file.as_str()).collect();
        assert_eq!(files, vec!["api.cdx.json", "client.cdx.json"]);

        // The sidecar has to name the file the BOM actually lands in,
        // not the single-capture default.
        for p in &run.projects {
            let Outcome::Captured(cap) = &p.outcome else {
                panic!("expected a capture")
            };
            assert_eq!(cap.envelope.result["bom_file"], p.bom_file);
        }
    }

    #[test]
    fn a_workspace_root_is_captured_once_not_once_per_member() {
        // cyclonedx-npm aggregates every workspace member into the
        // root's BOM. Descending into `packages/*` would emit the same
        // components a second and third time.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = tempfile::tempdir().unwrap();
        plant("node", d.path());
        touch(&d.path().join("packages/a/package.json"));
        touch(&d.path().join("packages/b/package.json"));

        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        assert_eq!(paths(&run), vec![(".", "node", STATUS_CAPTURED)]);
        assert_eq!(run.projects[0].bom_file, "sbom.cdx.json");
    }

    #[test]
    fn vendored_installed_and_worktree_directories_are_never_entered() {
        let d = polyglot_tree();
        // Every one of these carries a real manifest; none is a project
        // of this repo.
        touch(&d.path().join("client/node_modules/ms/package.json"));
        touch(&d.path().join("engine/target/debug/build/x/Cargo.toml"));
        touch(&d.path().join("api/.venv/lib/pkg/pyproject.toml"));
        touch(&d.path().join("venv/lib/pkg/pyproject.toml"));
        // A git worktree of the same repo: near-duplicate artifacts.
        touch(
            &d.path()
                .join(".claude/worktrees/wt-71/api/requirements.txt"),
        );

        let found = discover(d.path()).unwrap();
        let rels: Vec<&str> = found.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(rels, vec!["api", "client"]);
    }

    #[test]
    fn a_symlinked_directory_is_not_followed() {
        // A link back into the tree would otherwise loop the walk, and a
        // link out of it would capture something the repo does not own.
        #[cfg(unix)]
        {
            let d = polyglot_tree();
            std::os::unix::fs::symlink(d.path(), d.path().join("loop")).unwrap();
            let found = discover(d.path()).unwrap();
            let rels: Vec<&str> = found.iter().map(|f| f.rel_path.as_str()).collect();
            assert_eq!(rels, vec!["api", "client"]);
        }
    }

    #[test]
    fn a_failing_project_becomes_needs_operator_without_failing_the_rest() {
        // The uv fixture has no `.venv` and no lockfile cyclonedx-py can
        // read — the real shape of a repo cloned but never installed.
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = polyglot_tree();
        plant("python-uv", &d.path().join("tooling"));

        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        assert_eq!(
            paths(&run),
            vec![
                ("api", "python", STATUS_CAPTURED),
                ("client", "node", STATUS_CAPTURED),
                ("tooling", "python", STATUS_NEEDS_OPERATOR),
            ]
        );
        assert_eq!(run.index.result["captured_count"], 2);
        assert_eq!(run.index.result["needs_operator_count"], 1);

        let entry = &run.index.result["projects"][2];
        assert!(
            entry["error"].as_str().unwrap().contains("uv export"),
            "the generator's own remediation must survive into the index: {entry}"
        );
        assert_eq!(
            entry["remediation"],
            json!(format!(
                "secunit capture deps sbom --path {} --ecosystem python \
                 --out /evidence/sbom/tooling.cdx.json",
                d.path().join("tooling").display()
            )),
            "the index carries the exact command that re-runs this one project"
        );
    }

    #[test]
    fn a_missing_generator_costs_only_its_own_project() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = polyglot_tree();
        let run = capture_tree_with(d.path(), &out_dir(), &MissingToolRunner).unwrap();

        assert_eq!(run.captured_count(), 0);
        assert_eq!(run.needs_operator_count(), 2);
        let err = run.index.result["projects"][1]["error"].as_str().unwrap();
        assert!(
            err.contains("npm install -g @cyclonedx/cyclonedx-npm"),
            "the install command must reach the operator: {err}"
        );
    }

    #[test]
    fn a_polyglot_directory_yields_one_artifact_per_ecosystem() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = tempfile::tempdir().unwrap();
        plant("python", &d.path().join("service"));
        plant("node", &d.path().join("service"));

        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        let files: Vec<&str> = run.projects.iter().map(|p| p.bom_file.as_str()).collect();
        assert_eq!(
            files,
            vec!["service.node.cdx.json", "service.python.cdx.json"],
            "two BOMs of one directory cannot share a filename"
        );
    }

    #[test]
    fn two_runs_over_an_unchanged_tree_produce_an_identical_index() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = polyglot_tree();
        plant("python-uv", &d.path().join("tooling"));

        let a = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        let b = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        assert_eq!(
            a.index.to_canonical_json().unwrap(),
            b.index.to_canonical_json().unwrap()
        );
    }

    #[test]
    fn the_index_validates_against_its_registered_schema() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = polyglot_tree();
        plant("python-uv", &d.path().join("tooling"));

        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        let errs = crate::schema::validate(&run.index).expect("schema lookup");
        assert!(errs.is_empty(), "index schema mismatch: {errs:?}");
    }

    #[test]
    fn a_tree_with_no_manifests_names_what_was_looked_for_and_skipped() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("docs/README.md"));
        let err = capture_tree_with(d.path(), &out_dir(), &all_generators())
            .expect_err("nothing to capture");
        let msg = format!("{err}");
        assert!(
            msg.contains("Cargo.toml") && msg.contains("package.json"),
            "{msg}"
        );
        assert!(msg.contains("node_modules/"), "names the skips: {msg}");
    }

    #[test]
    fn a_project_at_the_discovery_root_is_still_discovered() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = tempfile::tempdir().unwrap();
        plant("python", d.path());
        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        assert_eq!(paths(&run), vec![(".", "python", STATUS_CAPTURED)]);
    }

    #[test]
    fn a_root_under_a_dotted_path_is_not_pruned_by_its_own_name() {
        // The dot rule applies to what the walk descends into, never to
        // the root itself — a checkout under `~/.local/src` or a dotted
        // temp dir would otherwise discover nothing.
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join(".checkout");
        plant("python", &root.join("api"));
        let found = discover(&root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rel_path, "api");
    }

    #[test]
    fn nested_projects_keep_their_path_in_the_artifact_name() {
        let _g = crate::time::set_fixed_time_for_tests("2026-05-01T00:00:00Z");
        let d = tempfile::tempdir().unwrap();
        plant("node", &d.path().join("services/web/frontend"));
        let run = capture_tree_with(d.path(), &out_dir(), &all_generators()).unwrap();
        assert_eq!(
            run.projects[0].bom_file, "services/web/frontend.cdx.json",
            "the artifact mirrors the subdirectory it describes"
        );
    }
}
