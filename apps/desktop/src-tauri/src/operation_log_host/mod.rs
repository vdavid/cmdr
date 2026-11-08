//! This app, acting as the operation log's host.
//!
//! [`install`] builds the app's one [`OperationLog`] first thing in `setup()`,
//! detached, so nothing can reach the log before it exists; [`start`] opens it
//! later, once the data dir and settings are known, on the SAME handle. The
//! answers the log needs from the app live here: the data dir, the recording
//! switch and the retention limits from settings, the app's runtime, and the
//! volume registry the rollback planner looks volumes up in (`volumes.rs`).

// The backend suites' rollback cells (`operations_host::backend_suites`) pass a
// private registry; the engine's own lookups go through
// `operations_host::volumes`.
#[cfg(test)]
mod volumes;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tauri::AppHandle;
use tauri_specta::Event as _;

use cmdr_operation_log::OpenError;
use cmdr_operation_log::OperationLog;
use cmdr_operation_log::retention::{RetentionLimits, RetentionLimitsSource};
use cmdr_operation_log::store::operation_log_db_path;

/// The app's operation log, once [`install`] built it.
static LOG: OnceLock<OperationLog> = OnceLock::new();

/// Builds the app's operation log, detached, and returns it.
///
/// Call early in `setup()`, before anything can journal or roll back. The
/// handle journals nothing until [`start`] opens it.
pub fn install() -> OperationLog {
    let mut built = false;
    let log = LOG.get_or_init(|| {
        built = true;
        OperationLog::detached()
    });
    if !built {
        crate::log_error!(target: "operation_log", "the operation log was installed twice; keeping the first");
    }
    // Something read the log before this ran and got a detached one: what it
    // journaled, it journaled nowhere.
    if early_reads::happened() {
        crate::log_error!(target: "operation_log", "the operation log was read before install(); early journal calls were dropped");
    }
    log.clone()
}

/// The app's operation log.
///
/// In the test binary nothing installs, so the first read builds the
/// shared handle and keeps it, quietly. In the app a read before [`install`] is
/// a bug that loses journal rows, so it's loud, and it answers from a SEPARATE
/// detached handle: the slot stays free for [`install`], which then reports the
/// early read.
pub(crate) fn operation_log() -> &'static OperationLog {
    if let Some(log) = LOG.get() {
        return log;
    }
    #[cfg(test)]
    {
        LOG.get_or_init(OperationLog::detached)
    }
    #[cfg(not(test))]
    {
        early_reads::note()
    }
}

/// Open `operation-log.db` in the app's data dir on `log` and start retention.
/// Failure is non-fatal: the app runs without the journal rather than refusing
/// to start.
pub fn start(app: &AppHandle, log: &OperationLog) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: "operation_log", "operation log not started: {e}");
            return;
        }
    };
    log.set_recording(crate::settings::load_operation_log_enabled(app));
    match log.open(&operation_log_db_path(&data_dir), &set_aside_unreadable) {
        // Prune on startup and on a slow timer, on the app's runtime. Runs before
        // the app is under load; the size loop is a no-op while under budget.
        Ok(()) => {
            log.spawn_retention(tauri::async_runtime::handle().inner(), retention_limits(app));
            announce_tree_indexes(app, log);
        }
        Err(OpenError::Store(e)) => log::warn!(target: "operation_log", "operation log store not opened: {e}"),
        Err(OpenError::Writer(e)) => log::warn!(target: "operation_log", "operation log writer not spawned: {e}"),
        Err(OpenError::AlreadyOpen) => {
            crate::log_error!(target: "operation_log", "the operation log was started twice; keeping the first");
        }
    }
}

/// `operation-log-tree-indexes-ready`: the journal's folder indexes exist, so an item
/// tree that answered `indexesBuilding` can read again. Once per launch, at most; the
/// first launch after the upgrade that added them is the only one that waits.
#[derive(Debug, Clone, serde::Serialize, specta::Type, tauri_specta::Event)]
pub struct OperationLogTreeIndexesReady {}

/// Emit [`OperationLogTreeIndexesReady`] once the writer has built the folder indexes
/// (at once when they already exist), so the Operation log's tree subscribes rather than
/// polls. Runs on the writer thread: an emit, nothing that waits on the writer.
fn announce_tree_indexes(app: &AppHandle, log: &OperationLog) {
    let app = app.clone();
    log.tree_backfills().when_indexes_ready(Box::new(move || {
        if let Err(e) = (OperationLogTreeIndexesReady {}).emit(&app) {
            log::warn!(target: "operation_log", "couldn't say the item tree's folder indexes are ready: {e}");
        }
    }));
}

/// Keeps an `operation-log.db` the store can't read beside the fresh one, and
/// tells the person once (`state_recovery`).
fn set_aside_unreadable(db_path: &Path) -> Option<PathBuf> {
    crate::state_recovery::set_aside(db_path, crate::state_recovery::StateFile::OperationLog)
}

/// The retention limits from `settings.json`, read fresh on every tick so a
/// change applies without a restart.
fn retention_limits(app: &AppHandle) -> RetentionLimitsSource {
    let app = app.clone();
    Arc::new(move || {
        let limits = crate::settings::load_operation_log_retention_limits(&app);
        RetentionLimits {
            max_age_secs: limits.max_age_secs,
            max_size_bytes: limits.max_size_bytes,
        }
    })
}

#[cfg(test)]
mod tests {
    use cmdr_operation_log::OperationLog;
    use cmdr_operation_log::store::operation_log_db_path;

    /// An unparseable `operation-log.db` opened the way `start` opens it is kept
    /// beside the fresh one and reported once, and the log still opens.
    #[test]
    fn an_unreadable_operation_log_is_kept_and_reported() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = operation_log_db_path(dir.path());
        std::fs::write(&path, b"this is not a sqlite database at all").expect("write garbage");

        let notices = crate::state_recovery::testing::capture_recoveries();
        let log = OperationLog::detached();
        log.open(&path, &super::set_aside_unreadable)
            .expect("open recreates an unparseable file");
        let kept = crate::state_recovery::testing::kept_copies(&path);
        assert_eq!(kept.len(), 1, "the unreadable DB is kept beside the new one: {kept:?}");
        assert_eq!(
            std::fs::read(&kept[0]).expect("read the copy"),
            b"this is not a sqlite database at all"
        );
        let recoveries = notices.recoveries();
        assert_eq!(recoveries.len(), 1, "one notice");
        assert_eq!(recoveries[0].file, crate::state_recovery::StateFile::OperationLog);
        assert!(log.writer().is_some(), "the fresh DB opened");
    }
}

#[cfg(not(test))]
mod early_reads {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use cmdr_operation_log::OperationLog;

    static HAPPENED: AtomicBool = AtomicBool::new(false);
    static DETACHED: OnceLock<OperationLog> = OnceLock::new();

    pub(super) fn note() -> &'static OperationLog {
        if !HAPPENED.swap(true, Ordering::Relaxed) {
            crate::log_error!(target: "operation_log", "the operation log was read before operation_log_host::install(); answering detached");
        }
        DETACHED.get_or_init(OperationLog::detached)
    }

    pub(super) fn happened() -> bool {
        HAPPENED.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod early_reads {
    /// The test binary builds its handle on first read, so there's
    /// never an early read to report.
    pub(super) fn happened() -> bool {
        false
    }
}
