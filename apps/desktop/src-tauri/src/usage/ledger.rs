//! The `usage.json` file: a schema-versioned envelope holding the local calendar days Cmdr was
//! launched on, the pure append the launch hook drives, and the quarantine for a file we can't
//! read.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// The ledger's name in the app data dir.
pub(super) const FILE_NAME: &str = "usage.json";

const LOG_TARGET: &str = "usage";

/// The only schema we know how to read. A file stamped with anything else is quarantined rather
/// than migrated, matching `recents/persistence.rs`; when a v2 lands, this check becomes a `match`.
const SCHEMA_VERSION: u32 = 1;

/// On-disk shape. `_schemaVersion` (leading underscore) is the house convention, shared with
/// `favorites.json` and every recents file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    launch_days: Vec<String>,
}

/// The `YYYY-MM-DD` key a calendar day is stored under.
fn day_key(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// Returns the ledger with `today` appended, or `None` when the day is already in it.
///
/// Membership, never a comparison against the last element: a timezone move or a clock adjustment
/// can leave the days out of order, and a second entry for a day the ledger already holds would
/// then slip in behind the odd tail. Existing entries are kept verbatim (no sort, no dedupe) so a
/// hand-edited or clock-skewed file stays recognizable instead of being quietly rewritten.
fn appended(days: &[String], today: NaiveDate) -> Option<Vec<String>> {
    let key = day_key(today);
    if days.contains(&key) {
        return None;
    }
    let mut next = days.to_vec();
    next.push(key);
    Some(next)
}

/// Reads the days at `path`. A missing file is a fresh ledger; one we can't parse, or one stamped
/// with a schema we don't know, is quarantined and this launch starts fresh in memory.
pub(super) fn read(path: &Path) -> Vec<String> {
    cleanup_tmp_file(path);

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    match serde_json::from_str::<Envelope>(&contents) {
        Ok(envelope) if envelope.schema_version == SCHEMA_VERSION => envelope.launch_days,
        Ok(envelope) => {
            log::warn!(
                target: LOG_TARGET,
                "Launch-day ledger at {path:?} is schema {} (we read {SCHEMA_VERSION}); quarantining and starting fresh",
                envelope.schema_version
            );
            quarantine_broken(path);
            Vec::new()
        }
        Err(e) => {
            log::warn!(target: LOG_TARGET, "Couldn't parse the launch-day ledger at {path:?}: {e}");
            quarantine_broken(path);
            Vec::new()
        }
    }
}

/// Notes `today` in the ledger at `path`, writing only when the day is new.
pub(super) fn record(path: &Path, today: NaiveDate) {
    let Some(next) = appended(&read(path), today) else {
        return;
    };
    if let Err(e) = write(path, &next) {
        log::warn!(target: LOG_TARGET, "Couldn't record today's launch in {path:?}: {e}");
    }
}

/// Writes the days durably: temp file + fsync + rename + parent-dir fsync, so the ledger survives a
/// power loss and not only a process death. See `crate::config::durable_write_json`.
fn write(path: &Path, days: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        launch_days: days.to_vec(),
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(std::io::Error::other)?;
    crate::config::durable_write_json(path, &path.with_extension("json.tmp"), &json)
}

/// Drops a temp file a killed write left behind, so it can't shadow a later one.
fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Renames the file to a `.broken` sibling so one unreadable ledger survives for debugging. A real
/// ledger is never wiped without a copy left behind. If the rename itself fails, leave the file
/// alone and let this launch run on an empty in-memory ledger: a hint that stays quiet costs less
/// than a deleted history.
fn quarantine_broken(path: &Path) {
    let broken = path.with_extension("json.broken");
    if broken.exists() {
        // Overwrite any previous quarantine; only the most recent corruption matters.
        let _ = fs::remove_file(&broken);
    }
    match fs::rename(path, &broken) {
        Ok(()) => log::warn!(target: LOG_TARGET, "Quarantined an unreadable launch-day ledger to {broken:?}"),
        Err(e) => log::warn!(
            target: LOG_TARGET,
            "Couldn't quarantine the unreadable launch-day ledger at {path:?}: {e}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(year: i32, month: u32, date: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, date).expect("a real calendar day")
    }

    fn days(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| (*k).to_string()).collect()
    }

    #[test]
    fn appended_records_the_first_day_in_an_empty_ledger() {
        assert_eq!(appended(&[], day(2026, 9, 9)), Some(days(&["2026-09-09"])));
    }

    #[test]
    fn appended_is_a_no_op_when_today_is_already_there() {
        let existing = days(&["2026-09-07", "2026-09-08", "2026-09-09"]);
        assert_eq!(appended(&existing, day(2026, 9, 9)), None);
    }

    #[test]
    fn appended_finds_today_even_when_the_days_are_out_of_order() {
        // A timezone move or a clock adjustment can leave the newest day mid-list. Comparing
        // against the tail would miss it and record the day twice.
        let existing = days(&["2026-09-09", "2026-09-07", "2026-09-08"]);
        assert_eq!(appended(&existing, day(2026, 9, 7)), None);
    }

    #[test]
    fn appended_keeps_the_existing_order_and_appends_at_the_end() {
        let existing = days(&["2026-09-09", "2026-09-07"]);
        assert_eq!(
            appended(&existing, day(2026, 9, 10)),
            Some(days(&["2026-09-09", "2026-09-07", "2026-09-10"]))
        );
    }

    #[test]
    fn appended_is_a_no_op_when_the_ledger_already_holds_the_day_twice() {
        let existing = days(&["2026-09-08", "2026-09-08"]);
        assert_eq!(appended(&existing, day(2026, 9, 8)), None);
    }

    #[test]
    fn missing_file_reads_as_no_days() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read(&dir.path().join(FILE_NAME)).is_empty());
    }

    #[test]
    fn record_creates_the_file_when_it_is_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);

        record(&path, day(2026, 9, 9));

        assert_eq!(read(&path), days(&["2026-09-09"]));
    }

    #[test]
    fn record_creates_a_missing_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/deeper").join(FILE_NAME);

        record(&path, day(2026, 9, 9));

        assert!(path.exists(), "expected the write to create {path:?}");
    }

    #[test]
    fn record_appends_a_new_day_to_an_existing_ledger() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);

        record(&path, day(2026, 9, 8));
        record(&path, day(2026, 9, 9));

        assert_eq!(read(&path), days(&["2026-09-08", "2026-09-09"]));
    }

    #[test]
    fn record_leaves_the_file_untouched_when_today_is_already_recorded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        record(&path, day(2026, 9, 9));
        let after_first = fs::read_to_string(&path).expect("read back");

        record(&path, day(2026, 9, 9));

        assert_eq!(fs::read_to_string(&path).expect("read back"), after_first);
    }

    #[test]
    fn stored_file_carries_the_house_schema_version_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);

        record(&path, day(2026, 9, 9));

        let raw = fs::read_to_string(&path).expect("read back");
        assert!(raw.contains("\"_schemaVersion\": 1"), "got {raw}");
        assert!(raw.contains("\"launchDays\""), "got {raw}");
    }

    #[test]
    fn a_corrupt_file_quarantines_and_the_launch_still_records() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, "{not valid json at all").expect("write garbage");

        record(&path, day(2026, 9, 9));

        assert_eq!(read(&path), days(&["2026-09-09"]));
        let broken = path.with_extension("json.broken");
        assert!(broken.exists(), "expected quarantine at {broken:?}");
        assert_eq!(
            fs::read_to_string(&broken).expect("read broken"),
            "{not valid json at all",
            "the unreadable ledger should survive as a copy, not be wiped"
        );
    }

    #[test]
    fn an_unknown_schema_version_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        fs::write(&path, r#"{"_schemaVersion": 2, "launchDays": ["2026-01-01"]}"#).expect("write");

        assert!(read(&path).is_empty());
        assert!(
            path.with_extension("json.broken").exists(),
            "a version mismatch should quarantine"
        );
    }

    #[test]
    fn a_stale_tmp_file_is_cleaned_up_on_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, "stale").expect("write tmp");

        let _ = read(&path);

        assert!(!tmp.exists(), "stale tmp should be removed");
    }
}
