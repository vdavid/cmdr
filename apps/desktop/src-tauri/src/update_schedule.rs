//! When the background update check is due, remembered across relaunches.
//!
//! The update loop lives in the frontend (`#lib/updates/updater.svelte.ts`), but "when did we last
//! ask?" has to outlive the process, or every relaunch checks again. This keeps that answer in
//! `update-check.json` in the app data dir and applies the same throttle as the analytics heartbeat
//! (`crate::send_schedule`): at most one answered check per interval (the user's
//! `advanced.updateCheckInterval`), a check that got no answer retried no sooner than
//! [`RETRY_FLOOR`] later.
//!
//! It's deliberately independent of analytics consent: an install that opted out of analytics
//! still checks for updates.

use crate::ignore_poison::IgnorePoison;
use crate::send_schedule::{SendCadence, SendRecord, now_unix_ms};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

/// A check that got no answer is retried no sooner than this.
const RETRY_FLOOR: Duration = Duration::from_secs(15 * 60);

const STATE_FILE_NAME: &str = "update-check.json";

/// Serializes read-modify-write of the file within this process.
static LOCK: Mutex<()> = Mutex::new(());

/// Milliseconds until the next background update check is due, for an interval of `interval_ms`.
/// Zero means "check now". Manual checks don't ask; they just run and report through
/// [`record_update_check`].
#[tauri::command]
#[specta::specta]
pub async fn update_check_due_in(app: AppHandle, interval_ms: u64) -> u64 {
    let Some(path) = state_path(&app) else { return 0 };
    let _guard = LOCK.lock_ignore_poison();
    due_in_ms(&load(&path), now_unix_ms(), interval_ms)
}

/// Records that an update check finished: `answered` when the update server replied (whatever it
/// said), `false` when the check itself didn't land. A download or install failure after an
/// answer still counts as answered: the check is what this schedules.
#[tauri::command]
#[specta::specta]
pub async fn record_update_check(app: AppHandle, answered: bool) {
    let Some(path) = state_path(&app) else { return };
    let _guard = LOCK.lock_ignore_poison();
    let mut record = load(&path);
    record_outcome(&mut record, answered, now_unix_ms());
    save(&path, &record);
}

fn due_in_ms(record: &SendRecord, now_ms: i64, interval_ms: u64) -> u64 {
    let cadence = SendCadence {
        interval: Duration::from_millis(interval_ms),
        retry_floor: RETRY_FLOOR,
    };
    u64::try_from(record.due_in(now_ms, cadence).as_millis()).unwrap_or(u64::MAX)
}

fn record_outcome(record: &mut SendRecord, answered: bool, now_ms: i64) {
    if answered {
        record.record_success(now_ms);
    } else {
        record.record_failure(now_ms);
    }
}

/// Whether `data_dir` holds an update check on record, which only an earlier run writes:
/// `after_update` reads it as "this install ran before" when its own stamp is missing.
pub(crate) fn has_record(data_dir: &Path) -> bool {
    data_dir.join(STATE_FILE_NAME).exists()
}

fn state_path(app: &AppHandle) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .inspect_err(|e| log::debug!(target: "updater", "No app data dir for the update schedule: {e}"))
        .ok()
        .map(|dir| dir.join(STATE_FILE_NAME))
}

fn load(path: &Path) -> SendRecord {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save(path: &Path, record: &SendRecord) {
    let Ok(content) = serde_json::to_string_pretty(record) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = crate::config::durable_write_json(path, &tmp, &content) {
        log::debug!(target: "updater", "Couldn't persist the update schedule: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR_MS: u64 = 60 * 60 * 1000;
    const THREE_HOURS_MS: u64 = 3 * HOUR_MS;

    #[test]
    fn a_first_launch_checks_right_away() {
        assert_eq!(due_in_ms(&SendRecord::default(), 1_000, THREE_HOURS_MS), 0);
    }

    /// The point of persisting it: a relaunch an hour after an answered check waits two more hours.
    #[test]
    fn a_relaunch_within_the_interval_waits_out_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(STATE_FILE_NAME);
        let mut record = load(&path);
        record_outcome(&mut record, true, 0);
        save(&path, &record);

        let after_relaunch = load(&path);
        assert_eq!(due_in_ms(&after_relaunch, HOUR_MS as i64, THREE_HOURS_MS), 2 * HOUR_MS);
        assert_eq!(due_in_ms(&after_relaunch, THREE_HOURS_MS as i64, THREE_HOURS_MS), 0);
    }

    #[test]
    fn an_unanswered_check_is_retried_after_the_floor() {
        let mut record = SendRecord::default();
        record_outcome(&mut record, true, 0);
        record_outcome(&mut record, false, 4 * HOUR_MS as i64);
        let floor_ms = RETRY_FLOOR.as_millis() as u64;
        assert_eq!(due_in_ms(&record, 4 * HOUR_MS as i64, THREE_HOURS_MS), floor_ms);
    }

    /// The interval is the user's setting, read at every ask, so shortening it takes effect on the
    /// next wake rather than after the old interval runs out.
    #[test]
    fn the_interval_is_whatever_the_caller_passes() {
        let mut record = SendRecord::default();
        record_outcome(&mut record, true, 0);
        let ten_minutes = 10 * 60 * 1000;
        assert_eq!(due_in_ms(&record, HOUR_MS as i64, ten_minutes), 0);
        assert_eq!(due_in_ms(&record, 0, ten_minutes), ten_minutes);
    }
}
