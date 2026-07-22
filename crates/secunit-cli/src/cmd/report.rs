//! `secunit report data` — assemble the structured JSON a report skill
//! renders to prose. Read-only; always emits JSON (the `--json` global is
//! implied).

use std::path::Path;
use std::process::ExitCode;

use anyhow::{bail, Result};
use chrono::NaiveDate;
use secunit_core::model::Cadence;
use secunit_core::registry::period;
use secunit_core::reports;

use super::Ctx;

/// One period selector: exactly one of `--week/--month/--quarter/--year`,
/// enforced by a clap group in `main.rs`.
pub struct PeriodArg<'a> {
    pub week: Option<&'a str>,
    pub month: Option<&'a str>,
    pub quarter: Option<&'a str>,
    pub year: Option<&'a str>,
}

impl PeriodArg<'_> {
    /// Resolve to `(label, cadence, start, end)` via `period::bounds`.
    fn resolve(&self) -> Result<(String, Cadence, NaiveDate, NaiveDate)> {
        let (cadence, raw, hint) = match (self.week, self.month, self.quarter, self.year) {
            (Some(w), None, None, None) => (Cadence::Weekly, w, "YYYY-Wnn (e.g. 2026-W30)"),
            (None, Some(m), None, None) => (Cadence::Monthly, m, "YYYY-MM (e.g. 2026-07)"),
            (None, None, Some(q), None) => (Cadence::Quarterly, q, "YYYY-qN (e.g. 2026-q3)"),
            (None, None, None, Some(y)) => (Cadence::Annual, y, "YYYY (e.g. 2026)"),
            _ => bail!("pass exactly one of --week, --month, --quarter, --year"),
        };
        let label = canonicalize(cadence, raw);
        match period::bounds(cadence, &label) {
            Some((start, end)) => Ok((label, cadence, start, end)),
            None => bail!("`{raw}` is not a valid period id; expected {hint}"),
        }
    }
}

/// Canonicalize operator spellings to the exact form `period::bounds`
/// parses (the `period::derive` output shape): `2026-Q3` → `2026-q3`,
/// `2026-w5` → `2026-W05`, `2026-7` → `2026-07`. Case and zero-padding
/// carry no meaning, so rejecting them would only break scripts; the
/// canonical label is also what lands in the payload. Anything that
/// doesn't match the rough shape passes through untouched and fails in
/// `bounds` with the format hint.
fn canonicalize(cadence: Cadence, raw: &str) -> String {
    let raw = raw.trim();
    let is_digits = |s: &str, max: usize| {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit())
    };
    let (y, rest) = match raw.split_once('-') {
        Some(parts) => parts,
        None => return raw.to_string(),
    };
    match cadence {
        Cadence::Weekly => rest
            .strip_prefix(['w', 'W'])
            .filter(|w| is_digits(w, 2))
            .map(|w| format!("{y}-W{w:0>2}")),
        Cadence::Monthly => is_digits(rest, 2).then(|| format!("{y}-{rest:0>2}")),
        Cadence::Quarterly => rest
            .strip_prefix(['q', 'Q'])
            .filter(|q| is_digits(q, 1))
            .map(|q| format!("{y}-q{q}")),
        _ => None,
    }
    .unwrap_or_else(|| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::canonicalize;
    use secunit_core::model::Cadence;

    #[test]
    fn canonicalize_accepts_conventional_spellings() {
        assert_eq!(canonicalize(Cadence::Quarterly, "2026-Q3"), "2026-q3");
        assert_eq!(canonicalize(Cadence::Quarterly, "2026-q3"), "2026-q3");
        assert_eq!(canonicalize(Cadence::Weekly, "2026-W5"), "2026-W05");
        assert_eq!(canonicalize(Cadence::Weekly, "2026-w05"), "2026-W05");
        assert_eq!(canonicalize(Cadence::Monthly, "2026-7"), "2026-07");
        assert_eq!(canonicalize(Cadence::Annual, " 2026 "), "2026");
    }

    #[test]
    fn canonicalize_passes_garbage_through_for_bounds_to_reject() {
        assert_eq!(canonicalize(Cadence::Weekly, "2026-19"), "2026-19");
        assert_eq!(canonicalize(Cadence::Quarterly, "2026-q12"), "2026-q12");
        assert_eq!(canonicalize(Cadence::Monthly, "2026-131"), "2026-131");
    }
}

pub fn data(ctx: &Ctx, period: &PeriodArg<'_>, out: Option<&Path>) -> Result<ExitCode> {
    let (label, cadence, start, end) = period.resolve()?;

    let (reg, report) = ctx.load()?;
    // A control that failed to load is absent from the registry and would
    // silently vanish from every section of the payload — for a compliance
    // artifact that is worse than no report, so refuse to assemble.
    if !report.is_clean() {
        for e in &report.errors {
            eprintln!("error {}: {}", e.path.display(), e.message);
        }
        return Ok(ExitCode::from(1));
    }

    let data = reports::assemble(&reg, &label, cadence, start, end, ctx.today)?;
    let json = serde_json::to_string_pretty(&data)?;
    match out {
        Some(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, format!("{json}\n"))?;
            eprintln!("wrote {}", path.display());
        }
        None => println!("{json}"),
    }
    Ok(ExitCode::SUCCESS)
}
