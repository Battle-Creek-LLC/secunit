//! Drive the registry-side IPC handlers against the in-tree fixture
//! `testdata/orgs/multi-system/`. We cannot easily call the
//! `#[tauri::command]` functions through Tauri's invocation machinery
//! from a unit test, so instead the test calls the underlying loader
//! and resolver functions to assert the fixture loads cleanly and the
//! same shape the GUI will see.

use std::path::PathBuf;

use chrono::NaiveDate;
use secunit_core::registry::{loader, resolver};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("testdata/orgs/multi-system")
}

#[test]
fn fixture_loads_clean_enough_for_the_gui() {
    let root = fixture_root();
    if !root.exists() {
        eprintln!("skipping: {} not present", root.display());
        return;
    }
    let (registry, report) = loader::load(&root);
    // Hard errors mean we cannot show the project at all.
    assert!(
        report.errors.is_empty(),
        "fixture has loader errors:\n{}",
        report
            .errors
            .iter()
            .map(|d| format!("  {}: {}", d.path.display(), d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(!registry.controls.is_empty());
    assert!(!registry.inventory.kinds.is_empty());
}

#[test]
fn due_rows_against_fixture_today_2026_05_01() {
    let root = fixture_root();
    if !root.exists() {
        return;
    }
    let (registry, _report) = loader::load(&root);
    let today = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
    let rows = resolver::due_rows(&registry, today);
    assert_eq!(
        rows.len(),
        registry.controls.len(),
        "due_rows should return one entry per control"
    );

    // Every weekly control should produce a near-future next_due. (Some
    // scheduled or annual controls may legitimately have None.)
    for r in &rows {
        if matches!(r.cadence, secunit_core::model::Cadence::Weekly) {
            assert!(
                r.next_due.is_some(),
                "weekly control {} missing next_due",
                r.control_id
            );
        }
    }
}

#[test]
fn resolve_scope_inline_and_inventory() {
    let root = fixture_root();
    if !root.exists() {
        return;
    }
    let (registry, _) = loader::load(&root);
    let today = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
    for c in registry.controls.values() {
        let _ = resolver::resolve_scope(c, &registry.inventory, today);
        // No assertion about counts — the contract is "doesn't panic and
        // returns a Vec the GUI can render". Counts vary by control.
    }
}
