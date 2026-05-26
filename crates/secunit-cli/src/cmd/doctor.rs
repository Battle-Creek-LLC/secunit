//! `secunit doctor` — a read-only health check over the operator's
//! environment and the registry on disk. It automates the Part B audit in
//! `docs/setup-checklist.md`: a preflight before relying on the registry.
//!
//! It never mutates anything; like `validate`/`verify` it exits 1 when a
//! check fails (the "data wrong" convention) and 0 otherwise — warnings and
//! informational notes do not fail the run. Each section composes existing
//! building blocks (`loader`, `validate::check_skills`, `verifier::verify`,
//! the risk-register fold) so doctor stays a thin aggregator.
//!
//! Every `⚠`/`✗` line carries a `fix:` — a concrete next action — so an agent
//! reading the output (especially `--json`, where `fix` is a field) can
//! remediate itself. The fixes deliberately distinguish two kinds of trouble:
//! things that are *safe to auto-repair* (`git init`, `risks rebuild`, editing
//! `_config.yaml`) and *integrity failures the agent must NOT auto-repair* —
//! a broken manifest or risk-log hash chain means evidence was altered, and
//! the correct response is to investigate, never to re-finalize or hand-edit
//! the append-only logs to paper over it.

use std::collections::HashSet;
use std::fs;
use std::process::ExitCode;

use anyhow::Result;
use secunit_core::evidence::verifier;
use secunit_core::risks::{build_index, RiskIndex};

use super::Ctx;

/// Severity of a single check line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Ok,
    Info,
    Warn,
    Fail,
}

impl Level {
    fn glyph(self) -> char {
        match self {
            Level::Ok => '✓',
            Level::Info => 'ℹ',
            Level::Warn => '⚠',
            Level::Fail => '✗',
        }
    }

    fn json(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Fail => "fail",
        }
    }
}

struct Check {
    name: String,
    level: Level,
    detail: String,
    /// A concrete remediation for `warn`/`fail` lines. `None` for `ok`/`info`.
    /// This is what lets an agent act on the report instead of just reading it.
    fix: Option<String>,
}

struct Section {
    title: &'static str,
    checks: Vec<Check>,
}

impl Section {
    fn new(title: &'static str) -> Self {
        Self {
            title,
            checks: Vec::new(),
        }
    }

    fn push(&mut self, level: Level, name: impl Into<String>, detail: impl Into<String>, fix: Option<String>) {
        self.checks.push(Check {
            name: name.into(),
            level,
            detail: detail.into(),
            fix,
        });
    }

    fn ok(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        self.push(Level::Ok, name, detail, None);
    }
    fn info(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        self.push(Level::Info, name, detail, None);
    }
    fn warn(&mut self, name: impl Into<String>, detail: impl Into<String>, fix: impl Into<String>) {
        self.push(Level::Warn, name, detail, Some(fix.into()));
    }
    fn fail(&mut self, name: impl Into<String>, detail: impl Into<String>, fix: impl Into<String>) {
        self.push(Level::Fail, name, detail, Some(fix.into()));
    }
}

pub fn run(ctx: &Ctx) -> Result<ExitCode> {
    let (reg, mut report) = ctx.load()?;
    let mut sections: Vec<Section> = Vec::new();

    // ---- Environment -------------------------------------------------------
    let mut env = Section::new("Environment");
    env.info("version", format!("secunit v{}", env!("CARGO_PKG_VERSION")));
    let features = secunit_capture::enabled_features();
    env.info(
        "capture features",
        if features.is_empty() {
            "none compiled in".to_string()
        } else {
            features.join(", ")
        },
    );
    if reg.root.join(".git").exists() {
        env.ok("git repository", "root is a git repository");
    } else {
        env.fail(
            "git repository",
            "root is not a git repository, so manifests cannot pin a commit sha",
            "run `git init` here and commit the registry stub — `run prepare` refuses to \
             allocate a run outside a real repo (docs/setup-checklist.md §A3)",
        );
    }
    // Cross-check the unambiguous external integrations against the features
    // this binary was actually built with.
    let enabled: HashSet<&str> = features.iter().copied().collect();
    for (intg, feat) in [("github", "github"), ("aws", "aws")] {
        if reg.config.integrations.contains_key(intg) && !enabled.contains(feat) {
            env.warn(
                "integration",
                format!("`{intg}` is declared in _config.yaml but this binary lacks the `{feat}` capture feature"),
                format!(
                    "reinstall/rebuild secunit with the `{feat}` feature \
                     (e.g. `cargo install bcl-secunit --features {feat}`), or remove the \
                     `{intg}:` integration block from _config.yaml if the org does not use it"
                ),
            );
        }
    }
    sections.push(env);

    // ---- Repo structure ----------------------------------------------------
    let mut st = Section::new("Repo structure");
    if reg.root.join("_config.yaml").exists() {
        st.ok("_config.yaml", "present");
        match reg.config.org.as_ref().and_then(|o| o.wisp_repo.as_deref()) {
            Some(w) if !w.is_empty() => st.ok("org.wisp_repo", w.to_string()),
            _ => st.warn(
                "org.wisp_repo",
                "not set — the `bootstrap` skill requires it to locate the WISP",
                "add an `org:` block to _config.yaml with `wisp_repo: <git-url-or-path>`",
            ),
        }
    } else {
        st.warn(
            "_config.yaml",
            "missing — this is the only place to declare org identity + integrations",
            "create _config.yaml at the root (schema: \
             crates/secunit-core/schemas/_config.schema.json; see docs/setup-checklist.md §A2)",
        );
    }
    let controls_dir = reg.root.join("controls");
    if !controls_dir.exists() {
        st.fail(
            "controls/",
            "missing — the registry has no controls",
            "bootstrap from the WISP: `secunit run prepare bootstrap`, follow the bootstrap \
             skill, then `secunit registry import <run-dir>` (docs/setup-checklist.md §A4)",
        );
    } else if reg.controls.is_empty() {
        st.fail(
            "controls/",
            "present but no valid control YAMLs loaded",
            "see the Registry section below for the schema/parse errors that stopped them \
             loading, fix the named files, then re-run `secunit doctor`",
        );
    } else {
        st.ok("controls/", format!("{} control(s)", reg.controls.len()));
    }
    for (rel, label) in [
        ("inventory.yaml", "inventory.yaml"),
        ("schedule.yaml", "schedule.yaml"),
        ("state.json", "state.json"),
    ] {
        if reg.root.join(rel).exists() {
            st.ok(label, "present");
        } else {
            st.info(label, "absent (optional until first use)");
        }
    }
    match fs::read_to_string(reg.root.join(".gitignore")) {
        Ok(gi) if gi.lines().any(|l| l.trim() == ".secunit.lock") => {
            st.ok(".gitignore", "ignores .secunit.lock")
        }
        Ok(_) => st.warn(
            ".gitignore",
            "does not ignore .secunit.lock — the runtime lock must never be committed",
            "add a line `.secunit.lock` to .gitignore (alongside `target/` and `.DS_Store`)",
        ),
        Err(_) => st.warn(
            ".gitignore",
            "missing",
            "create .gitignore ignoring `.secunit.lock`, `target/`, and `.DS_Store`",
        ),
    }
    if reg.root.join(".secunit.lock").exists() {
        st.warn(
            ".secunit.lock",
            "present at root — this is a runtime lock, never an artifact",
            "if it is tracked, `git rm --cached .secunit.lock` and gitignore it; otherwise it \
             is a stale lock from an interrupted run and is safe to delete",
        );
    }
    sections.push(st);

    // ---- Registry ----------------------------------------------------------
    // Fold in the same skill-resolution / requires_features checks `validate`
    // runs, on top of the loader's schema + cross-reference diagnostics. The
    // loader's messages already name the offending file and the exact problem,
    // so the fix points the agent at the named file and the matching command.
    let mut rg = Section::new("Registry");
    super::validate::check_skills(&reg, &mut report);
    if report.errors.is_empty() {
        if report.warnings.is_empty() {
            rg.ok("validate", "schema + cross-references clean");
        } else {
            rg.ok(
                "validate",
                format!("schema + cross-references clean ({} warning(s))", report.warnings.len()),
            );
        }
    } else {
        for e in &report.errors {
            rg.fail(
                "validate",
                format!("{}: {}", e.path.display(), e.message),
                "edit the file named in this message to satisfy the constraint, then re-run \
                 `secunit doctor` (this is the same check `secunit validate` runs)",
            );
        }
    }
    for w in &report.warnings {
        rg.warn(
            "validate",
            format!("{}: {}", w.path.display(), w.message),
            "non-fatal, but resolve the named file when convenient (e.g. add the missing \
             policy/skill file, or fix the reference) — `secunit validate` lists these",
        );
    }
    sections.push(rg);

    // ---- Evidence integrity ------------------------------------------------
    // A hash-chain failure here means evidence was altered after sealing. There
    // is no safe auto-fix: re-finalizing or editing would destroy the audit
    // trail. The fix tells the agent to investigate, not to repair.
    let mut ev = Section::new("Evidence integrity");
    match verifier::verify(&reg.root, None) {
        Ok(vr) => {
            let runs = vr.verified.len() + vr.failures.len();
            if runs == 0 {
                ev.info("run manifests", "no sealed runs yet");
            } else if vr.failures.is_empty() {
                ev.ok(
                    "run manifests",
                    format!("{} run(s) verified, hash chains intact", vr.verified.len()),
                );
            } else {
                for f in &vr.failures {
                    ev.fail(
                        "run manifests",
                        format!("{} / {}: {:?} — {}", f.control_id, f.run_id, f.kind, f.detail),
                        "this run's hash chain does not recompute — the evidence or manifest \
                         was altered after sealing. Do NOT re-finalize or edit to clear it; \
                         investigate the run dir and `git log` for the registry. Evidence is \
                         append-only and immutable.",
                    );
                }
            }
            let risks = vr.verified_risks.len() + vr.risk_failures.len();
            if risks > 0 {
                if vr.risk_failures.is_empty() {
                    ev.ok(
                        "risk logs",
                        format!("{} risk log(s) verified, chains intact", vr.verified_risks.len()),
                    );
                } else {
                    for f in &vr.risk_failures {
                        ev.fail(
                            "risk logs",
                            format!("risk {}: {:?} — {}", f.risk_id, f.kind, f.detail),
                            "this risk's events.jsonl chain or finding_ref is broken. Do NOT \
                             hand-edit the log (it is append-only/immutable) and note that \
                             `risks rebuild` only regenerates the index, not the log — \
                             investigate the named risk and the registry git history.",
                        );
                    }
                }
            }
        }
        Err(e) => ev.fail(
            "verify",
            format!("could not run verification: {e:#}"),
            "confirm `-C <root>` points at the registry and the tree is readable; \
             run `secunit verify` directly for the raw error",
        ),
    }
    sections.push(ev);

    // ---- Risk register (append-only event-log format) ----------------------
    // `build_index` folds every risks/<id>/events.jsonl, which validates the
    // on-disk format end to end: monotonic seq, a leading `opened` event, and
    // an intact prev_sha256 chain. We then confirm index.json is a faithful,
    // up-to-date projection of those logs. A stale/missing/corrupt index is
    // safe to rebuild; a fold error means a log itself is broken (investigate).
    let mut rr = Section::new("Risk register");
    match build_index(&reg.root) {
        Ok(built) => {
            let n = built.risks.len();
            rr.info(
                "event logs",
                if n == 0 {
                    "no risks tracked yet".to_string()
                } else {
                    format!("{n} risk(s) in the append-only event log")
                },
            );
            let index_path = reg.root.join("risks").join("index.json");
            if index_path.exists() {
                let on_disk = fs::read(&index_path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<RiskIndex>(&b).ok());
                match on_disk {
                    Some(idx) if idx.risks == built.risks => {
                        rr.ok("risks/index.json", "fresh (matches the folded logs)")
                    }
                    Some(_) => rr.warn(
                        "risks/index.json",
                        "stale — the cached index no longer matches the logs",
                        "run `secunit risks rebuild` to regenerate it from the authoritative logs",
                    ),
                    None => rr.fail(
                        "risks/index.json",
                        "unreadable or corrupt JSON",
                        "run `secunit risks rebuild` — the index is a derived cache and is \
                         always safe to regenerate from the logs",
                    ),
                }
            } else if n > 0 {
                rr.warn(
                    "risks/index.json",
                    "missing, but risk logs exist",
                    "run `secunit risks rebuild` to generate the index from the logs",
                );
            }
        }
        Err(e) => rr.fail(
            "event logs",
            format!("cannot fold the risk logs: {e:#}"),
            "a risks/<id>/events.jsonl is malformed (the error names the file/line). Logs are \
             append-only — do NOT hand-edit; identify which line broke and restore it from \
             `git` history. `risks rebuild` will not fix a broken log.",
        ),
    }
    sections.push(rr);

    // ---- Output ------------------------------------------------------------
    let count = |level: Level| {
        sections
            .iter()
            .flat_map(|s| &s.checks)
            .filter(|c| c.level == level)
            .count()
    };
    let fails = count(Level::Fail);
    let warns = count(Level::Warn);
    let infos = count(Level::Info);

    if ctx.json {
        let payload = serde_json::json!({
            "ok": fails == 0,
            "summary": { "fail": fails, "warn": warns, "info": infos },
            "sections": sections.iter().map(|s| serde_json::json!({
                "title": s.title,
                "checks": s.checks.iter().map(|c| serde_json::json!({
                    "name": c.name,
                    "status": c.level.json(),
                    "detail": c.detail,
                    "fix": c.fix,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(if fails == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }

    println!("secunit doctor — environment & registry health\n");
    for s in &sections {
        println!("{}", s.title);
        for c in &s.checks {
            println!("  {} {}: {}", c.level.glyph(), c.name, c.detail);
            if let Some(fix) = &c.fix {
                println!("      ↳ fix: {fix}");
            }
        }
        println!();
    }
    if fails == 0 {
        println!("✓ doctor: all checks passed ({warns} warning(s), {infos} note(s))");
        Ok(ExitCode::SUCCESS)
    } else {
        println!(
            "✗ doctor: {fails} failure(s), {warns} warning(s) — each ✗/⚠ above has a `fix:` line; \
             apply the safe ones, investigate (do not auto-repair) integrity failures"
        );
        Ok(ExitCode::from(1))
    }
}
