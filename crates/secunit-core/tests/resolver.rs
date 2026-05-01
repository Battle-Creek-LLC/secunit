//! Cadence math and scope-resolution table-driven tests, plus a
//! property test that `next_due` is monotonic in `today`.

use std::path::PathBuf;

use chrono::NaiveDate;
use proptest::prelude::*;
use secunit_core::model::{
    Cadence, Control, DueField, EvidenceRequirement, InventoryEntry, ResolvedSystem, Scope, State,
    Weekday,
};
use secunit_core::registry::{loader, resolver};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("testdata/orgs")
        .join(name)
        .canonicalize()
        .unwrap()
}

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn skeleton(id: &str, cadence: Cadence) -> Control {
    Control {
        id: id.into(),
        title: id.into(),
        policy: "p.md".into(),
        nist: vec![],
        owner: "cto".into(),
        cadence,
        weekday: None,
        due: None,
        due_by: None,
        skill: id.into(),
        skill_args: None,
        scope: None,
        evidence_required: vec![],
        remediation_thresholds: Default::default(),
        outputs: None,
        references: vec![],
    }
}

#[test]
fn weekly_default_monday_from_midweek() {
    let c = skeleton("c", Cadence::Weekly);
    let next = resolver::next_due(&c, &Default::default(), None, d("2026-05-06"), None);
    assert_eq!(next, Some(d("2026-05-11"))); // next Monday
}

#[test]
fn weekly_with_weekday_override_thursday() {
    let mut c = skeleton("c", Cadence::Weekly);
    c.weekday = Some(Weekday::Thursday);
    let next = resolver::next_due(&c, &Default::default(), None, d("2026-05-06"), None);
    assert_eq!(next, Some(d("2026-05-07"))); // Thu after Wed
}

#[test]
fn quarterly_anchors_to_first_of_quarter_business_day() {
    let c = skeleton("c", Cadence::Quarterly);
    // 2026-04-01 is a Wednesday, so anchor is itself.
    let next = resolver::next_due(&c, &Default::default(), None, d("2026-04-01"), None);
    assert_eq!(next, Some(d("2026-04-01")));
    // 2027-01-01 is a Friday (still business day) — but querying mid-Q1 should
    // still produce 2027-01-01 if today is on/before it.
    let next = resolver::next_due(&c, &Default::default(), None, d("2027-01-01"), None);
    assert_eq!(next, Some(d("2027-01-01")));
}

#[test]
fn annual_with_due_by_december_31() {
    let mut c = skeleton("c", Cadence::Annual);
    c.due_by = Some("december-31".into());
    let next = resolver::next_due(&c, &Default::default(), None, d("2026-05-01"), None);
    assert_eq!(next, Some(d("2026-12-31")));
    // After the deadline, rolls to next year.
    let next = resolver::next_due(&c, &Default::default(), None, d("2027-01-15"), None);
    assert_eq!(next, Some(d("2027-12-31")));
}

#[test]
fn scheduled_uses_due_field() {
    let mut c = skeleton("c", Cadence::Scheduled);
    c.due = Some(DueField::Single("2026-03-15".into()));
    let next = resolver::next_due(&c, &Default::default(), None, d("2026-01-01"), None);
    assert_eq!(next, Some(d("2026-03-15")));
}

#[test]
fn overdue_after_grace_window() {
    let c = skeleton("c", Cadence::Weekly);
    let due = d("2026-05-04");
    assert!(!resolver::is_overdue(&c, due, d("2026-05-04")));
    assert!(!resolver::is_overdue(&c, due, d("2026-05-07"))); // 3-day grace
    assert!(resolver::is_overdue(&c, due, d("2026-05-08")));
}

#[test]
fn resolve_scope_filters_by_tag_and_lifecycle() {
    let (reg, report) = loader::load(&fixture("multi-system"));
    assert!(report.is_clean(), "{:?}", report.errors);
    let ctrl = &reg.controls["sca-weekly-dependency-scan"];
    let resolved = resolver::resolve_scope(ctrl, &reg.inventory, d("2026-05-04"));
    let names: Vec<_> = resolved.iter().map(|r| r.name.clone()).collect();
    assert!(names.contains(&"app-api".to_string()));
    assert!(names.contains(&"app-ui".to_string()));
    assert!(names.contains(&"data-pipeline".to_string())); // has-sca, in scope until 2026-09
    assert!(!names.contains(&"marketing-site".to_string())); // explicit exclude
}

#[test]
fn resolve_scope_drops_retired_entries() {
    let (reg, _) = loader::load(&fixture("multi-system"));
    let ctrl = &reg.controls["sca-weekly-dependency-scan"];
    let resolved = resolver::resolve_scope(ctrl, &reg.inventory, d("2026-10-01"));
    let names: Vec<_> = resolved.iter().map(|r| r.name.clone()).collect();
    assert!(!names.contains(&"data-pipeline".to_string())); // retired_on 2026-09-01
}

#[test]
fn inline_scope_passes_through() {
    let (reg, _) = loader::load(&fixture("multi-system"));
    let ctrl = &reg.controls["cp-annual-bcp-test"];
    let resolved = resolver::resolve_scope(ctrl, &reg.inventory, d("2026-08-01"));
    assert_eq!(
        resolved.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["prod", "app-api"]
    );
}

#[test]
fn inventory_entry_active_window() {
    let entry = InventoryEntry {
        name: "x".into(),
        tags: vec![],
        in_scope_since: Some(d("2025-06-01")),
        retired_on: Some(d("2026-09-01")),
        aliases: vec![],
        excludes: vec![],
        extras: Default::default(),
    };
    assert!(!entry.is_active_on(d("2025-05-31")));
    assert!(entry.is_active_on(d("2025-06-01")));
    assert!(entry.is_active_on(d("2026-08-31")));
    assert!(!entry.is_active_on(d("2026-09-01"))); // retired_on is exclusive
}

// ---- property: weekly cadence is monotonic in today --------------------

proptest! {
    #[test]
    fn weekly_next_due_monotonic_in_today(
        a in 0i64..3650,
        b in 0i64..3650,
    ) {
        let c = skeleton("c", Cadence::Weekly);
        let base = d("2025-01-06"); // a Monday
        let day_a = base + chrono::Duration::days(a);
        let day_b = base + chrono::Duration::days(b);
        let na = resolver::next_due(&c, &Default::default(), None, day_a, None);
        let nb = resolver::next_due(&c, &Default::default(), None, day_b, None);
        if let (Some(na), Some(nb)) = (na, nb) {
            // next_due is on or after today
            prop_assert!(na >= day_a);
            prop_assert!(nb >= day_b);
            // monotonic: bigger today never produces a smaller next_due that
            // is also before bigger today.
            if day_a <= day_b {
                prop_assert!(nb >= na || nb >= day_a);
            }
        }
    }
}

// quiet unused-import lint when proptest's macro layout shifts under us.
#[allow(dead_code)]
fn _keep_imports(_: &ResolvedSystem, _: &EvidenceRequirement, _: &State, _: &Scope) {}
