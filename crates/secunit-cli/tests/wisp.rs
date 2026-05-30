//! Integration tests for `secunit wisp init` and `secunit wisp export`.
//!
//! These exercise the M1 pipeline that needs no external toolchain: scaffolding
//! the required partials, enforcing their presence, and emitting the composed
//! Typst document. The in-binary Typst → PDF compile is a separate milestone
//! (see FIXES.md / docs/wisp-pdf-export.md), so these assert on the emitted
//! `.typ`, not a PDF.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

/// `wisp init` writes the full required Typst partial set plus the logo.
#[test]
fn wisp_init_scaffolds_partials() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("secunit")
        .unwrap()
        .arg("-C")
        .arg(dir.path())
        .args(["wisp", "init"])
        .assert()
        .success();

    let tpl = dir.path().join("templates/wisp");
    for name in [
        "theme.typ",
        "header.typ",
        "footer.typ",
        "cover.typ",
        "toc.typ",
        "logo.svg",
    ] {
        assert!(
            tpl.join(name).exists(),
            "missing scaffolded partial: {name}"
        );
    }
}

/// A second `init` without `--force` skips existing files.
#[test]
fn wisp_init_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let run = || {
        Command::cargo_bin("secunit")
            .unwrap()
            .arg("-C")
            .arg(dir.path())
            .args(["wisp", "init"])
            .assert()
            .success()
    };
    run();
    run().stdout(predicate::str::contains("skipped"));
}

/// `export` refuses to run when the partials are missing, pointing at `init`.
#[test]
fn wisp_export_requires_partials() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("security")).unwrap();
    fs::write(
        dir.path().join("security/policy.md"),
        "# Access Control\n\nUsers shall authenticate.\n",
    )
    .unwrap();

    Command::cargo_bin("secunit")
        .unwrap()
        .arg("-C")
        .arg(dir.path())
        .args(["wisp", "export", "-o"])
        .arg(dir.path().join("out.pdf"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("wisp init"));
}

/// End to end (sans PDF): scaffold, then export emits a Typst document that
/// carries the converted headings and the cover/ToC wiring.
#[test]
fn wisp_export_emits_typst_document() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("security")).unwrap();
    fs::write(
        dir.path().join("security/policy.md"),
        "# Access Control Policy\n\nUsers shall authenticate.\n\n## Scope\n\nAll systems.\n",
    )
    .unwrap();

    Command::cargo_bin("secunit")
        .unwrap()
        .arg("-C")
        .arg(dir.path())
        .args(["wisp", "init"])
        .assert()
        .success();

    let out = dir.path().join("out.pdf");
    Command::cargo_bin("secunit")
        .unwrap()
        .arg("-C")
        .arg(dir.path())
        .args(["wisp", "export", "-o"])
        .arg(&out)
        .assert()
        .success();

    let typ = fs::read_to_string(dir.path().join("out.typ")).expect("emitted main.typ");
    assert!(typ.contains("= Access Control Policy"), "heading converted");
    assert!(typ.contains("== Scope"), "subheading converted");
    assert!(typ.contains("#wisp-cover(ctx)"), "cover wired");
    assert!(typ.contains("#wisp-toc(ctx)"), "toc wired");
    assert!(typ.contains("#import \"theme.typ\""), "imports partials");
}
