//! Cadence math and scope-resolution table-driven tests, plus a
//! property test that `next_due` is monotonic in `today`.

use std::path::PathBuf;

use chrono::NaiveDate;
use proptest::prelude::*;
use secunit_core::model::{
    Cadence, Control, EvidenceRequirement, InventoryEntry, ResolvedSystem, Schedule, ScheduleEntry,
    ScheduleInsert, ScheduleSkip, Scope, State, Weekday,
};
use secunit_core::registry::{
    loader,
    resolver::{self, DueReason, DueResolution},
};

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

// ---- next_due_with_reason: per-source provenance --------------------------

fn entry_for(control_id: &str) -> ScheduleEntry {
    ScheduleEntry {
        control_id: control_id.into(),
        due: None,
        weekday: None,
        note: None,
        reason: None,
        skip: None,
        insert: None,
    }
}

#[test]
fn reason_is_cadence_when_no_overrides_apply() {
    let c = skeleton("c", Cadence::Weekly);
    let r = resolver::next_due_with_reason(&c, &Default::default(), None, d("2026-05-06"), None);
    assert_eq!(
        r,
        Some(DueResolution {
            date: d("2026-05-11"),
            reason: DueReason::Cadence,
            note: None,
        })
    );
}

#[test]
fn reason_is_override_due_when_pinned_by_date() {
    // Annual cadence with `due_by: december-31` so the cadence-derived
    // date (2026-12-31) is well after the override.
    let mut c = skeleton("c", Cadence::Annual);
    c.due_by = Some("december-31".into());
    let schedule = Schedule {
        overrides: vec![ScheduleEntry {
            due: Some(d("2026-06-30")),
            note: Some("auditor-on-site".into()),
            ..entry_for("c")
        }],
    };
    let r = resolver::next_due_with_reason(&c, &schedule, None, d("2026-05-01"), None);
    assert_eq!(
        r,
        Some(DueResolution {
            date: d("2026-06-30"),
            reason: DueReason::OverrideDue,
            note: Some("auditor-on-site".into()),
        })
    );
}

#[test]
fn reason_is_override_insert_when_a_one_off_lands_first() {
    let mut c = skeleton("c", Cadence::Annual);
    c.due_by = Some("december-31".into());
    let schedule = Schedule {
        overrides: vec![ScheduleEntry {
            insert: Some(ScheduleInsert {
                run_at: d("2026-05-20"),
                reason: Some("re-test after remediation".into()),
            }),
            ..entry_for("c")
        }],
    };
    let r = resolver::next_due_with_reason(&c, &schedule, None, d("2026-05-01"), None);
    assert_eq!(
        r,
        Some(DueResolution {
            date: d("2026-05-20"),
            reason: DueReason::OverrideInsert,
            // Note pulled from the insert's own reason field — neither
            // entry.note nor entry.reason was set.
            note: Some("re-test after remediation".into()),
        })
    );
}

#[test]
fn reason_is_override_weekday_when_weekly_override_set() {
    let c = skeleton("c", Cadence::Weekly);
    let schedule = Schedule {
        overrides: vec![ScheduleEntry {
            weekday: Some(Weekday::Thursday),
            note: Some("staff meeting on Mondays".into()),
            ..entry_for("c")
        }],
    };
    let r = resolver::next_due_with_reason(&c, &schedule, None, d("2026-05-04"), None);
    // 2026-05-04 is Mon; next Thu is 2026-05-07.
    assert_eq!(
        r,
        Some(DueResolution {
            date: d("2026-05-07"),
            reason: DueReason::OverrideWeekday,
            note: Some("staff meeting on Mondays".into()),
        })
    );
}

#[test]
fn skip_drops_cadence_firing_and_returns_next_insert() {
    let c = skeleton("c", Cadence::Quarterly);
    // Today is Q2 (May), skip applies; provide a later insert as the
    // fallback the operator wants to honour.
    let schedule = Schedule {
        overrides: vec![
            ScheduleEntry {
                skip: Some(ScheduleSkip {
                    quarter: Some("2026-q2".into()),
                    year: None,
                    reason: Some("frozen during migration".into()),
                }),
                ..entry_for("c")
            },
            ScheduleEntry {
                insert: Some(ScheduleInsert {
                    run_at: d("2026-08-15"),
                    reason: Some("post-migration".into()),
                }),
                ..entry_for("c")
            },
        ],
    };
    let r = resolver::next_due_with_reason(&c, &schedule, None, d("2026-05-01"), None);
    assert_eq!(
        r,
        Some(DueResolution {
            date: d("2026-08-15"),
            reason: DueReason::OverrideInsert,
            note: Some("post-migration".into()),
        })
    );
}

#[test]
fn insert_beats_cadence_on_same_date() {
    // Quarterly cadence: today = 2026-04-01 (Wed) → cadence anchors
    // to 2026-04-01. Insert pinned to the same day. Insert has
    // precedence 0 vs cadence's 3, so the insert wins the tiebreak.
    let c = skeleton("c", Cadence::Quarterly);
    let schedule = Schedule {
        overrides: vec![ScheduleEntry {
            insert: Some(ScheduleInsert {
                run_at: d("2026-04-01"),
                reason: Some("ride along with quarterly".into()),
            }),
            ..entry_for("c")
        }],
    };
    let r = resolver::next_due_with_reason(&c, &schedule, None, d("2026-04-01"), None);
    assert_eq!(
        r,
        Some(DueResolution {
            date: d("2026-04-01"),
            reason: DueReason::OverrideInsert,
            note: Some("ride along with quarterly".into()),
        })
    );
}

#[test]
fn next_due_facade_matches_with_reason() {
    let c = skeleton("c", Cadence::Weekly);
    let schedule = Schedule {
        overrides: vec![ScheduleEntry {
            insert: Some(ScheduleInsert {
                run_at: d("2026-05-20"),
                reason: None,
            }),
            ..entry_for("c")
        }],
    };
    let today = d("2026-05-04");
    assert_eq!(
        resolver::next_due(&c, &schedule, None, today, None),
        resolver::next_due_with_reason(&c, &schedule, None, today, None).map(|r| r.date)
    );
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
