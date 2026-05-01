//! Time source for capture envelopes. Real wall clock by default; can
//! be pinned for deterministic test output via either:
//! - the `SECUNIT_CAPTURE_FIXED_TIME` env var (for CLI integration tests
//!   that span a child process), or
//! - the in-process [`set_fixed_time_for_tests`] helper (for unit tests
//!   that don't want to fight other parallel tests for the env var).

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use chrono::{DateTime, SecondsFormat, Utc};

/// Returns the current UTC instant, formatted as `YYYY-MM-DDTHH:MM:SSZ`
/// (no fractional seconds — capture envelopes round to whole seconds so
/// hashes are stable across machines with different clock resolutions).
pub fn now_iso8601() -> String {
    fixed_now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn fixed_now() -> DateTime<Utc> {
    let pinned_ts = OVERRIDE_TS.load(Ordering::SeqCst);
    if pinned_ts != i64::MIN {
        if let Some(dt) = DateTime::from_timestamp(pinned_ts, 0) {
            return dt;
        }
    }
    if let Ok(s) = std::env::var("SECUNIT_CAPTURE_FIXED_TIME") {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
            return dt.with_timezone(&Utc);
        }
    }
    Utc::now()
}

/// Sentinel value meaning "no override". Tests pin specific instants;
/// `i64::MIN` is far outside any plausible captured_at.
static OVERRIDE_TS: AtomicI64 = AtomicI64::new(i64::MIN);

fn fixed_time_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Pin the clock for the duration of the returned guard. The guard
/// holds a process-wide mutex so concurrent time-pinning tests
/// serialize cleanly — without this, rust's parallel test runner can
/// have one test's "drop" race against another test's "read".
pub fn set_fixed_time_for_tests(rfc3339: &str) -> FixedTimeGuard {
    let dt = DateTime::parse_from_rfc3339(rfc3339)
        .expect("set_fixed_time_for_tests: invalid RFC-3339")
        .with_timezone(&Utc);
    let guard = fixed_time_lock().lock().unwrap_or_else(|p| p.into_inner());
    OVERRIDE_TS.store(dt.timestamp(), Ordering::SeqCst);
    FixedTimeGuard { _g: guard }
}

#[must_use = "drop the guard to release the time pin"]
pub struct FixedTimeGuard {
    _g: MutexGuard<'static, ()>,
}

impl Drop for FixedTimeGuard {
    fn drop(&mut self) {
        OVERRIDE_TS.store(i64::MIN, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_time_is_honored_via_helper() {
        let _g = set_fixed_time_for_tests("2026-05-01T12:00:00Z");
        assert_eq!(now_iso8601(), "2026-05-01T12:00:00Z");
    }
}
