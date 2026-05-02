//! Period identifiers tying runs to audit windows.
//!
//! Every run carries a `period_id` (e.g. `2026-W18`, `2026-q2`) chosen at
//! prepare time. Coverage queries enumerate expected periods over an audit
//! window and check each for a `complete` run claiming that period — no
//! cadence-math derivation at read time.
//!
//! `derive` formats a target date for the cadence; `bounds` reverses an id
//! into the inclusive date range it spans. `Continuous` has no period.

use chrono::{Datelike, NaiveDate};

use crate::model::Cadence;

/// Format the period containing `target_date` for the given cadence.
///
/// `target_date` is the deadline the run is *for* — not necessarily today.
/// Callers running early for an upcoming Monday should pass that Monday so
/// the run claims the correct ISO week.
pub fn derive(cadence: Cadence, target_date: NaiveDate) -> Option<String> {
    match cadence {
        Cadence::Continuous => None,
        Cadence::Weekly => {
            let iw = target_date.iso_week();
            Some(format!("{:04}-W{:02}", iw.year(), iw.week()))
        }
        Cadence::Monthly => Some(format!(
            "{:04}-{:02}",
            target_date.year(),
            target_date.month()
        )),
        Cadence::Quarterly => {
            let q = (target_date.month() - 1) / 3 + 1;
            Some(format!("{:04}-q{}", target_date.year(), q))
        }
        Cadence::SemiAnnual => {
            let h = if target_date.month() <= 6 { 1 } else { 2 };
            Some(format!("{:04}-H{}", target_date.year(), h))
        }
        Cadence::Annual => Some(format!("{:04}", target_date.year())),
        Cadence::Scheduled => Some(format!(
            "scheduled-{}",
            target_date.format("%Y-%m-%d")
        )),
    }
}

/// Parse a period id and return its inclusive date range.
///
/// Returns `None` for `Continuous` and for ids that don't match the
/// cadence's expected format.
pub fn bounds(cadence: Cadence, period_id: &str) -> Option<(NaiveDate, NaiveDate)> {
    match cadence {
        Cadence::Continuous => None,
        Cadence::Weekly => parse_iso_week(period_id),
        Cadence::Monthly => parse_month(period_id),
        Cadence::Quarterly => parse_quarter(period_id),
        Cadence::SemiAnnual => parse_half(period_id),
        Cadence::Annual => parse_year(period_id),
        Cadence::Scheduled => parse_scheduled(period_id),
    }
}

fn parse_iso_week(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (y, w) = s.split_once("-W")?;
    if y.len() != 4 || w.len() != 2 {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let week: u32 = w.parse().ok()?;
    let mon = NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)?;
    let sun = NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Sun)?;
    Some((mon, sun))
}

fn parse_month(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (y, m) = s.split_once('-')?;
    if y.len() != 4 || m.len() != 2 {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let month: u32 = m.parse().ok()?;
    let start = NaiveDate::from_ymd_opt(year, month, 1)?;
    let next = next_month_start(year, month)?;
    Some((start, next.pred_opt()?))
}

fn parse_quarter(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (y, q) = s.split_once("-q")?;
    if y.len() != 4 {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let q: u32 = q.parse().ok()?;
    if !(1..=4).contains(&q) {
        return None;
    }
    let start_month = (q - 1) * 3 + 1;
    let end_month = start_month + 2;
    let start = NaiveDate::from_ymd_opt(year, start_month, 1)?;
    let next = next_month_start(year, end_month)?;
    Some((start, next.pred_opt()?))
}

fn parse_half(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (y, h) = s.split_once("-H")?;
    if y.len() != 4 {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    let (sm, em) = match h {
        "1" => (1, 6),
        "2" => (7, 12),
        _ => return None,
    };
    let start = NaiveDate::from_ymd_opt(year, sm, 1)?;
    let next = next_month_start(year, em)?;
    Some((start, next.pred_opt()?))
}

fn parse_year(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    if s.len() != 4 {
        return None;
    }
    let year: i32 = s.parse().ok()?;
    let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
    let end = NaiveDate::from_ymd_opt(year, 12, 31)?;
    Some((start, end))
}

fn parse_scheduled(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    let date_str = s.strip_prefix("scheduled-")?;
    let d = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    Some((d, d))
}

fn next_month_start(year: i32, month: u32) -> Option<NaiveDate> {
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Cadence;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn derive_weekly_iso_week_format() {
        // 2026-05-04 is a Monday in ISO week 19 of 2026.
        assert_eq!(derive(Cadence::Weekly, d(2026, 5, 4)), Some("2026-W19".into()));
        // 2026-05-02 is a Saturday in ISO week 18 of 2026.
        assert_eq!(derive(Cadence::Weekly, d(2026, 5, 2)), Some("2026-W18".into()));
        // Year-boundary: 2026-01-01 is a Thursday in ISO week 1 of 2026.
        assert_eq!(derive(Cadence::Weekly, d(2026, 1, 1)), Some("2026-W01".into()));
        // 2024-12-30 (Mon) is in ISO week 1 of 2025 — chrono should report
        // the ISO year, not the calendar year.
        assert_eq!(derive(Cadence::Weekly, d(2024, 12, 30)), Some("2025-W01".into()));
    }

    #[test]
    fn derive_monthly_quarterly_semiannual_annual() {
        assert_eq!(derive(Cadence::Monthly, d(2026, 5, 4)), Some("2026-05".into()));
        assert_eq!(derive(Cadence::Monthly, d(2026, 12, 31)), Some("2026-12".into()));

        assert_eq!(derive(Cadence::Quarterly, d(2026, 1, 1)), Some("2026-q1".into()));
        assert_eq!(derive(Cadence::Quarterly, d(2026, 5, 4)), Some("2026-q2".into()));
        assert_eq!(derive(Cadence::Quarterly, d(2026, 7, 1)), Some("2026-q3".into()));
        assert_eq!(derive(Cadence::Quarterly, d(2026, 12, 31)), Some("2026-q4".into()));

        assert_eq!(derive(Cadence::SemiAnnual, d(2026, 6, 30)), Some("2026-H1".into()));
        assert_eq!(derive(Cadence::SemiAnnual, d(2026, 7, 1)), Some("2026-H2".into()));

        assert_eq!(derive(Cadence::Annual, d(2026, 5, 4)), Some("2026".into()));
    }

    #[test]
    fn derive_scheduled_uses_target_date() {
        assert_eq!(
            derive(Cadence::Scheduled, d(2026, 5, 4)),
            Some("scheduled-2026-05-04".into())
        );
    }

    #[test]
    fn derive_continuous_is_none() {
        assert_eq!(derive(Cadence::Continuous, d(2026, 5, 4)), None);
    }

    #[test]
    fn bounds_weekly() {
        // ISO week 19 of 2026: Mon May 4 – Sun May 10.
        assert_eq!(
            bounds(Cadence::Weekly, "2026-W19"),
            Some((d(2026, 5, 4), d(2026, 5, 10)))
        );
    }

    #[test]
    fn bounds_monthly_quarterly_semiannual_annual() {
        assert_eq!(
            bounds(Cadence::Monthly, "2026-05"),
            Some((d(2026, 5, 1), d(2026, 5, 31)))
        );
        assert_eq!(
            bounds(Cadence::Monthly, "2026-12"),
            Some((d(2026, 12, 1), d(2026, 12, 31)))
        );
        assert_eq!(
            bounds(Cadence::Quarterly, "2026-q2"),
            Some((d(2026, 4, 1), d(2026, 6, 30)))
        );
        assert_eq!(
            bounds(Cadence::Quarterly, "2026-q4"),
            Some((d(2026, 10, 1), d(2026, 12, 31)))
        );
        assert_eq!(
            bounds(Cadence::SemiAnnual, "2026-H1"),
            Some((d(2026, 1, 1), d(2026, 6, 30)))
        );
        assert_eq!(
            bounds(Cadence::SemiAnnual, "2026-H2"),
            Some((d(2026, 7, 1), d(2026, 12, 31)))
        );
        assert_eq!(
            bounds(Cadence::Annual, "2026"),
            Some((d(2026, 1, 1), d(2026, 12, 31)))
        );
    }

    #[test]
    fn bounds_scheduled_is_single_day() {
        assert_eq!(
            bounds(Cadence::Scheduled, "scheduled-2026-05-04"),
            Some((d(2026, 5, 4), d(2026, 5, 4)))
        );
    }

    #[test]
    fn bounds_continuous_is_none() {
        assert_eq!(bounds(Cadence::Continuous, "anything"), None);
    }

    #[test]
    fn bounds_rejects_malformed_ids() {
        assert_eq!(bounds(Cadence::Weekly, "2026-19"), None);
        assert_eq!(bounds(Cadence::Weekly, "2026-W99"), None);
        assert_eq!(bounds(Cadence::Monthly, "2026-13"), None);
        assert_eq!(bounds(Cadence::Quarterly, "2026-q5"), None);
        assert_eq!(bounds(Cadence::SemiAnnual, "2026-H3"), None);
        assert_eq!(bounds(Cadence::Annual, "26"), None);
        assert_eq!(bounds(Cadence::Scheduled, "2026-05-04"), None);
    }

    #[test]
    fn round_trip_derive_then_bounds_contains_target() {
        let cases = [
            (Cadence::Weekly, d(2026, 5, 4)),
            (Cadence::Weekly, d(2024, 12, 30)),
            (Cadence::Monthly, d(2026, 5, 4)),
            (Cadence::Quarterly, d(2026, 5, 4)),
            (Cadence::SemiAnnual, d(2026, 7, 1)),
            (Cadence::Annual, d(2026, 5, 4)),
            (Cadence::Scheduled, d(2026, 5, 4)),
        ];
        for (cad, date) in cases {
            let id = derive(cad, date).expect("derive should produce a period id");
            let (start, end) = bounds(cad, &id).expect("bounds should parse derived id");
            assert!(
                date >= start && date <= end,
                "cadence {cad:?} target {date} not within bounds of {id}: ({start}, {end})"
            );
        }
    }
}
