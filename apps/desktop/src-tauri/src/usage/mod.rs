//! The launch-day ledger: which local calendar days Cmdr has been opened on.
//!
//! One durable file, `usage.json` in the app data dir, holding a plain list of `YYYY-MM-DD` days.
//! It's the substrate for usage-gated hints ("you've used Cmdr for a few days now: want it in your
//! Dock?"), and later for any other hint that should wait until someone has actually settled in.
//!
//! ❌ **The ledger never leaves the device.** It must not reach a crash report, an error-report
//! bundle, or the feedback digest. The privacy policy discloses it as local-only, so an accidental
//! upload would be a broken promise, not a bug.
//!
//! ❌ **It must not become a setting.** `analytics/config_shape.rs` auto-ships every bool and
//! number setting to PostHog, so a day count stored there would silently turn into telemetry. Its
//! own file is what keeps it out.
//!
//! Growth is unbounded on purpose: a decade of daily use is a few tens of KB.

mod ledger;

use std::collections::HashSet;
use std::path::Path;

/// Notes today's LOCAL calendar day in the ledger under `data_dir`. Call once per launch.
///
/// Local, never UTC: "your third day using Cmdr" is a claim about the person's own calendar, and a
/// late-evening session in Stockholm must not count as tomorrow.
///
/// Synchronous and best-effort. A write only happens on the first launch of a calendar day, so the
/// worst startup cost is one small durable write per day, matching `install_id::init` beside it;
/// running it inline is also what keeps [`launch_day_count`] from racing a spawned write. Every
/// failure is a logged warning: an unwritable ledger costs a hint, never a launch.
pub fn record_launch(data_dir: &Path) {
    ledger::record(&data_dir.join(ledger::FILE_NAME), chrono::Local::now().date_naive());
}

/// How many distinct local calendar days Cmdr has been launched on, per the ledger under
/// `data_dir`.
///
/// Distinct, because the file is plain text a person can edit and nothing on the read path dedupes
/// it. A missing or unreadable ledger counts as 0, which leaves every usage-gated hint silent.
pub fn launch_day_count(data_dir: &Path) -> u32 {
    let days = ledger::read(&data_dir.join(ledger::FILE_NAME));
    let distinct: HashSet<&String> = days.iter().collect();
    u32::try_from(distinct.len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_day_count_is_zero_without_a_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(launch_day_count(dir.path()), 0);
    }

    #[test]
    fn launch_day_count_grows_by_one_per_calendar_day() {
        let dir = tempfile::tempdir().expect("tempdir");

        record_launch(dir.path());
        record_launch(dir.path());

        // Two launches, one day: today's own day, whatever it happens to be.
        assert_eq!(launch_day_count(dir.path()), 1);
    }

    #[test]
    fn launch_day_count_ignores_a_duplicated_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("usage.json"),
            r#"{"_schemaVersion": 1, "launchDays": ["2026-09-08", "2026-09-08", "2026-09-09"]}"#,
        )
        .expect("seed ledger");

        assert_eq!(launch_day_count(dir.path()), 2);
    }
}
