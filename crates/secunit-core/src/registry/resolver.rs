//! Cadence resolution and scope expansion.
//!
//! Pure functions over the loaded model. Cadence math follows the table
//! in `docs/storage.md`; scope follows the inventory + tag-filter rules in
//! the same doc. Anything date-shaped enters as `chrono::NaiveDate` so
//! tests can pin "today" deterministically.

use std::collections::HashSet;

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::model::{
    Cadence, Control, Inventory, LoadedRegistry, ResolvedSystem, Schedule, Scope, StateEntry,
    Weekday,
};

// ---------- due resolution --------------------------------------------------

/// Why a particular firing date won — i.e. which input to the resolver
/// produced it. The CLI surfaces this via `secunit due --why`; the GUI
/// renders it as a chip on the Schedule view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DueReason {
    /// The cadence rules produced the date with no override in play.
    Cadence,
    /// A `schedule.yaml` override pinned a specific date for this control.
    OverrideDue,
    /// A `schedule.yaml` insert added a one-off firing.
    OverrideInsert,
    /// A `schedule.yaml` override changed the weekday a weekly cadence
    /// fires on. The date is still cadence-derived; the weekday is the
    /// operator's pick.
    OverrideWeekday,
}

/// A firing date with provenance and (where the override carried one)
/// the operator's note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DueResolution {
    pub date: NaiveDate,
    pub reason: DueReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------- cadence ---------------------------------------------------------

/// Compute the next firing date for `control` on or after `today`,
/// taking schedule overrides and the last-run pointer into account.
///
/// Thin facade over [`next_due_with_reason`]; callers that need to
/// know *why* a date won (the GUI's Schedule view, a future
/// `secunit due --why` flag) should call the richer version directly.
pub fn next_due(
    control: &Control,
    schedule: &Schedule,
    state: Option<&StateEntry>,
    today: NaiveDate,
    config_default_weekday: Option<Weekday>,
) -> Option<NaiveDate> {
    next_due_with_reason(control, schedule, state, today, config_default_weekday)
        .map(|r| r.date)
}

/// Like [`next_due`] but returns the date together with the
/// [`DueReason`] that produced it and the override's note (if any).
///
/// Precedence rules:
///   * Earlier date always wins.
///   * On a tie, override sources beat cadence: insert > dated > weekday > cadence.
///   * A skip override removes the cadence firing for the matching
///     window; the next-earliest insert (if any) takes its place.
pub fn next_due_with_reason(
    control: &Control,
    schedule: &Schedule,
    state: Option<&StateEntry>,
    today: NaiveDate,
    config_default_weekday: Option<Weekday>,
) -> Option<DueResolution> {
    // Skip a single firing window if `schedule.yaml` says so.
    let skip_today = schedule
        .overrides
        .iter()
        .filter(|o| o.control_id == control.id)
        .any(|o| {
            if let Some(skip) = &o.skip {
                if let Some(q) = &skip.quarter {
                    return quarter_string(today) == *q;
                }
                if let Some(y) = skip.year {
                    return today.year() == y;
                }
            }
            false
        });

    // Candidate buckets, each carrying provenance for the reason field.
    let mut candidates: Vec<DatedCandidate> = Vec::new();

    // Inserts — one-off extra firings. Note precedence: explicit
    // entry note → insert's own reason → entry-level reason. This
    // covers both the YAML shape `entry.note: "x"` and the more
    // common `insert: { run_at, reason: "x" }`.
    for ov in schedule
        .overrides
        .iter()
        .filter(|o| o.control_id == control.id)
    {
        if let Some(insert) = &ov.insert {
            if insert.run_at >= today {
                candidates.push(DatedCandidate {
                    date: insert.run_at,
                    reason: DueReason::OverrideInsert,
                    note: ov
                        .note
                        .clone()
                        .or_else(|| insert.reason.clone())
                        .or_else(|| ov.reason.clone()),
                    precedence: 0,
                });
            }
        }
    }

    // Dated overrides — a pinned `due:` date.
    for ov in schedule
        .overrides
        .iter()
        .filter(|o| o.control_id == control.id)
    {
        if let Some(d) = ov.due {
            if d >= today {
                candidates.push(DatedCandidate {
                    date: d,
                    reason: DueReason::OverrideDue,
                    note: ov.note.clone().or_else(|| ov.reason.clone()),
                    precedence: 1,
                });
            }
        }
    }

    // Weekday override only changes the cadence-derived date for
    // weekly controls — capture the note so the cadence candidate can
    // pick it up if it ends up labelled `OverrideWeekday`.
    let weekday_override_entry = schedule
        .overrides
        .iter()
        .find(|o| o.control_id == control.id && o.weekday.is_some());
    let weekday_override = weekday_override_entry.and_then(|o| o.weekday);
    let weekday_note = weekday_override_entry
        .and_then(|o| o.note.clone().or_else(|| o.reason.clone()));

    // Cadence-derived date, accounting for any weekday override that
    // applies to a weekly cadence.
    let cadence_due = match control.cadence {
        Cadence::Continuous => None,
        Cadence::Weekly => {
            let wd = weekday_override
                .or(control.weekday)
                .or(config_default_weekday)
                .unwrap_or(Weekday::Monday);
            Some(next_weekly(today, wd, state.and_then(|s| s.next_due)))
        }
        Cadence::Monthly => Some(next_business_day(today, monthly_anchor(today))),
        Cadence::Quarterly => Some(next_business_day(today, quarterly_anchor(today))),
        Cadence::SemiAnnual => Some(next_business_day(today, semiannual_anchor(today))),
        Cadence::Annual => Some(next_annual(today, control.due_by.as_deref())),
        Cadence::Scheduled => control
            .due
            .as_ref()
            .and_then(|d| earliest_scheduled(d.as_slice(), today)),
    };

    if let Some(d) = cadence_due {
        let weekday_active = matches!(control.cadence, Cadence::Weekly)
            && weekday_override.is_some();
        let (reason, note, precedence) = if weekday_active {
            (DueReason::OverrideWeekday, weekday_note.clone(), 2u8)
        } else {
            (DueReason::Cadence, None, 3u8)
        };
        candidates.push(DatedCandidate {
            date: d,
            reason,
            note,
            precedence,
        });
    }

    // Pick the earliest date; on ties, lower precedence index wins
    // (insert > dated > weekday > cadence).
    let winner = candidates
        .iter()
        .min_by(|a, b| a.date.cmp(&b.date).then(a.precedence.cmp(&b.precedence)))
        .cloned();

    let winner = winner?;

    if skip_today && winner.reason == DueReason::Cadence {
        // Cadence firing is skipped — fall back to the earliest insert
        // (if any). Dated overrides survive a skip; only the cadence
        // window is removed, per the spec's `skip` semantics.
        return candidates
            .into_iter()
            .filter(|c| c.reason == DueReason::OverrideInsert)
            .min_by_key(|c| c.date)
            .map(Into::into);
    }

    Some(winner.into())
}

#[derive(Debug, Clone)]
struct DatedCandidate {
    date: NaiveDate,
    reason: DueReason,
    note: Option<String>,
    /// Lower wins when dates tie. 0=insert, 1=dated, 2=weekday, 3=cadence.
    precedence: u8,
}

impl From<DatedCandidate> for DueResolution {
    fn from(c: DatedCandidate) -> Self {
        DueResolution {
            date: c.date,
            reason: c.reason,
            note: c.note,
        }
    }
}

/// Has the control passed its grace window?
pub fn is_overdue(control: &Control, due: NaiveDate, today: NaiveDate) -> bool {
    today > due + grace(control.cadence)
}

/// Per-cadence grace period after which a due control is overdue.
pub fn grace(cadence: Cadence) -> Duration {
    match cadence {
        Cadence::Continuous => Duration::days(0),
        Cadence::Weekly => Duration::days(3),
        Cadence::Monthly => Duration::days(7),
        Cadence::Quarterly => Duration::days(14),
        Cadence::SemiAnnual => Duration::days(21),
        Cadence::Annual => Duration::days(30),
        Cadence::Scheduled => Duration::days(7),
    }
}

fn next_weekly(today: NaiveDate, weekday: Weekday, last_next_due: Option<NaiveDate>) -> NaiveDate {
    // If state already says "next due Monday Y", honour it as long as it
    // is in the future. Otherwise compute the upcoming target weekday.
    if let Some(d) = last_next_due {
        if d >= today {
            return d;
        }
    }
    let target = weekday.to_chrono().num_days_from_monday() as i64;
    let cur = today.weekday().num_days_from_monday() as i64;
    let mut delta = target - cur;
    if delta < 0 {
        delta += 7;
    }
    today + Duration::days(delta)
}

fn monthly_anchor(today: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap()
}

fn quarterly_anchor(today: NaiveDate) -> NaiveDate {
    let q_first = match today.month() {
        1..=3 => 1,
        4..=6 => 4,
        7..=9 => 7,
        _ => 10,
    };
    NaiveDate::from_ymd_opt(today.year(), q_first, 1).unwrap()
}

fn semiannual_anchor(today: NaiveDate) -> NaiveDate {
    let m = if today.month() <= 6 { 1 } else { 7 };
    NaiveDate::from_ymd_opt(today.year(), m, 1).unwrap()
}

fn next_annual(today: NaiveDate, due_by: Option<&str>) -> NaiveDate {
    if let Some(due) = due_by {
        if let Some(d) = parse_due_by(due, today.year()) {
            if d >= today {
                return d;
            }
            return parse_due_by(due, today.year() + 1).unwrap_or(d);
        }
    }
    NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap_or(today)
}

fn parse_due_by(s: &str, year: i32) -> Option<NaiveDate> {
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d);
    }
    let mut parts = s.splitn(2, '-');
    let month = parts.next()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let m = match month.to_lowercase().as_str() {
        "january" | "jan" => 1,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sep" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    };
    NaiveDate::from_ymd_opt(year, m, day)
}

fn earliest_scheduled(values: Vec<&str>, today: NaiveDate) -> Option<NaiveDate> {
    values
        .into_iter()
        .filter_map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .or_else(|_| NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d"))
                .ok()
        })
        .filter(|d| *d >= today)
        .min()
}

fn next_business_day(today: NaiveDate, anchor: NaiveDate) -> NaiveDate {
    let mut d = anchor.max(today);
    while matches!(d.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
        d += Duration::days(1);
    }
    if d < today {
        // Anchor is in the past — push to next month/quarter window.
        // Caller can re-anchor; here we just bump by a month as a safe
        // default.
        return today;
    }
    d
}

fn quarter_string(date: NaiveDate) -> String {
    let q = (date.month() - 1) / 3 + 1;
    format!("{:04}-q{}", date.year(), q)
}

// ---------- scope -----------------------------------------------------------

/// Expand a control's scope against the inventory on the given run date.
pub fn resolve_scope(
    control: &Control,
    inventory: &Inventory,
    run_date: NaiveDate,
) -> Vec<ResolvedSystem> {
    match &control.scope {
        None => Vec::new(),
        Some(Scope::Inline(inline)) => inline
            .inline
            .iter()
            .map(|e| ResolvedSystem {
                name: e.name.clone(),
                kind: e.kind.clone(),
                tags: e.tags.clone(),
                extras: Default::default(),
            })
            .collect(),
        Some(Scope::Inventory(spec)) => {
            let entries = inventory.entries(&spec.kind);
            let want_tags: HashSet<&str> = spec.has_tags.iter().map(String::as_str).collect();
            let control_excludes: HashSet<&str> =
                spec.excludes.iter().map(String::as_str).collect();
            let all = spec.all.unwrap_or(false);

            let mut out: Vec<ResolvedSystem> = entries
                .iter()
                .filter(|e| e.is_active_on(run_date))
                .filter(|e| {
                    if all {
                        true
                    } else {
                        let entry_tags: HashSet<&str> = e.tags.iter().map(String::as_str).collect();
                        want_tags.iter().all(|t| entry_tags.contains(t))
                    }
                })
                .filter(|e| !control_excludes.contains(e.name.as_str()))
                .filter(|e| !e.excludes.iter().any(|s| s == &control.skill))
                .map(|e| ResolvedSystem {
                    name: e.name.clone(),
                    kind: spec.kind.clone(),
                    tags: e.tags.clone(),
                    extras: e.extras.clone(),
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            out
        }
    }
}

// ---------- registry-wide helpers ------------------------------------------

#[derive(Debug, Clone)]
pub struct DueRow {
    pub control_id: String,
    pub cadence: Cadence,
    pub next_due: Option<NaiveDate>,
    pub overdue: bool,
}

/// Compute next-due rows for every control in `reg` as of `today`. Sorted
/// by `(next_due ascending, control_id)`; controls without a computable
/// firing date come last.
pub fn due_rows(reg: &LoadedRegistry, today: NaiveDate) -> Vec<DueRow> {
    let mut rows: Vec<DueRow> = reg
        .controls
        .values()
        .map(|c| {
            let state = reg.state.controls.get(&c.id);
            let next = next_due(
                c,
                &reg.schedule,
                state,
                today,
                reg.config.weekly_default_weekday,
            );
            let overdue = next.map(|d| is_overdue(c, d, today)).unwrap_or(false);
            DueRow {
                control_id: c.id.clone(),
                cadence: c.cadence,
                next_due: next,
                overdue,
            }
        })
        .collect();
    rows.sort_by(|a, b| match (a.next_due, b.next_due) {
        (Some(x), Some(y)) => (x, &a.control_id).cmp(&(y, &b.control_id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.control_id.cmp(&b.control_id),
    });
    rows
}

/// Return controls due within `window` days of `today` (inclusive).
pub fn due_within(reg: &LoadedRegistry, today: NaiveDate, window_days: i64) -> Vec<DueRow> {
    let cutoff = today + Duration::days(window_days);
    due_rows(reg, today)
        .into_iter()
        .filter(|r| match r.next_due {
            Some(d) => d <= cutoff,
            None => false,
        })
        .collect()
}
