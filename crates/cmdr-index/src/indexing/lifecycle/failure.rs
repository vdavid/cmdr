//! Per-volume fatal-storage-failure signal.
//!
//! When a volume's index DB dies with a fatal storage error (`SQLITE_IOERR`,
//! `SQLITE_CORRUPT`, a full or read-only disk, …), every subsequent read and
//! write fails the same way. The writer thread and the live event loop USED to
//! just `log::warn!` and retry forever: one real incident logged 12,700+
//! identical warnings over 8 minutes, pegged the CPU, and froze the UI.
//!
//! This one-shot signal is how a dead index fails loudly, stops cleanly, and
//! surfaces an honest state instead. It's shared (`Arc`) between:
//!
//! - the **writer thread** (the detector): its handlers classify each SQLite
//!   error and `note()` a fatal one, which trips the signal exactly once,
//! - the **live event loop**, which polls [`is_tripped`](IndexFailureSignal::is_tripped)
//!   each flush tick and stops promptly (so the reconciler side stops flooding too),
//! - the **supervisor task** (`state::spawn_failure_supervisor`), which awaits
//!   [`notified`](IndexFailureSignal::notified) and transitions the volume to the
//!   `Failed` phase.
//!
//! Tripping is idempotent: only the FIRST fatal error logs and wakes the
//! supervisor, so a dead index emits a handful of lines, never thousands.
//!
//! **A full disk trips it too, as a PAUSE** ([`IndexStop::OutOfSpace`]): either
//! SQLite reported `SQLITE_FULL`, or the writer saw the host's low-disk-space
//! answer go true (`HostPolicy::index_storage_low`). The writer stops exactly as
//! it does for a dead DB, but the supervisor pauses the volume and restarts it once
//! space returns (`state/space_pause.rs`), and the stop is logged at warn with no
//! `IndexEvent::Error`: it's the environment, not a bug, so it must not auto-report
//! (cmdr-reports#100 did). So does an `SQLITE_IOERR*` while the host says the disk
//! is low (cmdr-reports#105).
//!
//! **Maintenance never stops the index on its own** ([`note_maintenance`](IndexFailureSignal::note_maintenance)):
//! a failed checkpoint or vacuum logs, and the next tick retries. Only proven
//! corruption escalates.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::indexing::store::{IndexFailure, IndexStoreError};

/// Why a volume's writer stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexStop {
    /// The database is unusable: the volume FAILS, and recovery is a rebuild.
    Failed(IndexFailure),
    /// The disk the database lives on is full, or nearly: the volume PAUSES and
    /// comes back by itself once there's room.
    OutOfSpace(SpaceStop),
}

/// What said the disk is out of room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpaceStop {
    /// SQLite refused a write with `SQLITE_FULL`.
    DiskFull,
    /// Free space dropped below the user's low-disk-space threshold, so the index
    /// stopped before taking the last of it.
    BelowThreshold,
}

/// A one-shot, per-volume "the index DB has died" signal. See the module docs for
/// who shares it and why.
pub(crate) struct IndexFailureSignal {
    /// Where the first fatal error is reported. The signal is per volume, so this
    /// is that volume's sink.
    events: Arc<dyn crate::EventSink>,
    tripped: AtomicBool,
    stop: Mutex<Option<IndexStop>>,
    notify: Notify,
    /// Every [`note`](IndexFailureSignal::note) call, fatal or not. Tests assert on
    /// it to prove a handler treated a situation as normal rather than routing it
    /// here: a non-fatal note logs a warn and returns, leaving no other trace.
    notes: AtomicUsize,
}

impl IndexFailureSignal {
    pub(crate) fn new(events: Arc<dyn crate::EventSink>) -> Self {
        Self {
            events,
            notes: AtomicUsize::new(0),
            tripped: AtomicBool::new(false),
            stop: Mutex::new(None),
            notify: Notify::new(),
        }
    }

    /// Trip the signal with `stop` if nothing has yet. Returns whether this call
    /// was the one that tripped it.
    fn trip(&self, stop: IndexStop) -> bool {
        // CAS so only the FIRST stop records + notifies; the rest are the flood
        // we're here to stop.
        let first = self
            .tripped
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if first {
            *self.stop.lock_ignore_poison() = Some(stop);
        }
        first
    }

    /// The disk ran out of room: pause rather than fail. Logged at warn, ONCE, and
    /// ❌ never as an `IndexEvent::Error`: a full disk is the user's environment, so
    /// it isn't worth an error report.
    fn trip_for_space(&self, why: SpaceStop, context: &str, detail: &dyn std::fmt::Display) {
        if self.trip(IndexStop::OutOfSpace(why)) {
            log::warn!(
                "{context}: the disk the index lives on is out of room ({detail}); pausing this volume's index until space returns"
            );
            self.notify.notify_one();
        }
    }

    /// The host says free space is below the user's low-disk-space threshold, so
    /// the writer stops before the index takes what's left. Same pause as a full
    /// disk. Returns `true`: the caller should stop.
    pub(crate) fn note_low_disk_space(&self, context: &str) -> bool {
        self.trip_for_space(
            SpaceStop::BelowThreshold,
            context,
            &"below the low-disk-space threshold",
        );
        true
    }

    /// Classify `err` and, if it's a FATAL storage error, trip the signal the
    /// first time it happens: store the typed reason, log ONCE at error level, and
    /// wake the supervisor. Returns whether the error was fatal.
    ///
    /// - Fatal: returns `true`. On the first call this logs + notifies; later
    ///   fatal calls are suppressed (no re-log, no re-notify) so the flood stops.
    ///   The caller should stop its work.
    /// - Non-fatal (transient contention, a benign miss): returns `false` and logs
    ///   at `warn`, preserving the previous warn-and-continue behavior at the site.
    ///
    /// Only ever called from an error branch, so the `context` format never runs on
    /// the hot success path.
    ///
    /// An `SQLITE_IOERR*` while the host says the index's disk is below its
    /// free-space threshold pauses instead of failing (cmdr-reports#105): on a disk
    /// that full, an I/O error is the environment far more often than a dying drive.
    /// The pause doesn't hide a dying drive: once space returns the volume restarts,
    /// and the same error with room to spare fails it as usual.
    pub(crate) fn note(&self, err: &IndexStoreError, context: &str) -> bool {
        self.notes.fetch_add(1, Ordering::Relaxed);
        if err.is_disk_full() {
            self.trip_for_space(SpaceStop::DiskFull, context, err);
            return true;
        }
        if err.is_io_error() && crate::indexing::host::policy::current().index_storage_low() {
            self.trip_for_space(SpaceStop::BelowThreshold, context, err);
            return true;
        }
        let Some(failure) = err.as_index_failure() else {
            log::warn!("{context}: {err}");
            return false;
        };
        if self.trip(IndexStop::Failed(failure)) {
            self.events.emit(crate::IndexEvent::Error {
                report: crate::IndexErrorReport::StorageFailed {
                    failure,
                    context: crate::Diagnostic(context.to_string()),
                    detail: crate::Diagnostic(err.to_string()),
                },
            });
            self.notify.notify_one();
        }
        true
    }

    /// Note an error from MAINTENANCE: a WAL checkpoint, an incremental vacuum, or
    /// the freelist read that sizes one. Maintenance writes nothing the index
    /// answers from, and the next 30 s tick retries it, so its failure alone never
    /// stops the index: it logs at warn and the writer carries on (cmdr-reports#105:
    /// a checkpoint's `SQLITE_IOERR_TRUNCATE` on a nearly full disk failed a healthy
    /// index, which cost a full rebuild). A disk that's really dying still stops the
    /// index at the next real write, through [`note`](Self::note).
    ///
    /// The one exception is proven corruption ([`IndexStoreError::indicates_corruption`]):
    /// the file is unusable no matter which statement found out, so it goes
    /// through [`note`](Self::note) and fails the index.
    pub(crate) fn note_maintenance(&self, err: &IndexStoreError, context: &str) {
        if err.indicates_corruption() {
            self.note(err, context);
            return;
        }
        self.notes.fetch_add(1, Ordering::Relaxed);
        log::warn!("{context}: {err}; maintenance retries on the next tick");
    }

    /// Whether a fatal storage error has been recorded. Polled by the writer loop
    /// and the live event loop to stop.
    pub(crate) fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Acquire)
    }

    /// How many errors have been noted here, fatal or not.
    #[cfg(test)]
    pub(crate) fn note_count(&self) -> usize {
        self.notes.load(Ordering::Relaxed)
    }

    /// The recorded failure reason, if a fatal error tripped it (`None` for a
    /// pause, which isn't a failure).
    #[cfg(test)]
    pub(crate) fn reason(&self) -> Option<IndexFailure> {
        match self.stop() {
            Some(IndexStop::Failed(failure)) => Some(failure),
            _ => None,
        }
    }

    /// Why the writer stopped, if it did.
    pub(crate) fn stop(&self) -> Option<IndexStop> {
        *self.stop.lock_ignore_poison()
    }

    /// Await the first trip. Resolves immediately if already tripped. `Notify`
    /// stores a single permit, so a trip that races ahead of this call is not lost.
    pub(crate) async fn notified(&self) {
        if self.is_tripped() {
            return;
        }
        self.notify.notified().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ioerr() -> IndexStoreError {
        sqlite(rusqlite::ffi::SQLITE_IOERR)
    }

    use crate::indexing::events::RecordingSink;

    fn busy() -> IndexStoreError {
        sqlite(rusqlite::ffi::SQLITE_BUSY)
    }

    use crate::indexing::host::policy::{FakeHostPolicy, HostPolicy, TestPolicyGuard, install_for_test};

    /// Install a host whose disk is as full as `storage_low` says, holding the
    /// process-wide test lock: `note` asks the installed host about an I/O error,
    /// so a test that notes one must pin the answer.
    fn host_with_storage_low(storage_low: bool) -> (std::sync::MutexGuard<'static, ()>, TestPolicyGuard) {
        let serialized = crate::indexing::handle::test_lock();
        let host = FakeHostPolicy::shared();
        host.note_index_storage_low(storage_low);
        let installed = install_for_test(host as Arc<dyn HostPolicy>);
        (serialized, installed)
    }

    fn sqlite(code: std::os::raw::c_int) -> IndexStoreError {
        IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None))
    }

    #[test]
    fn a_fatal_error_trips_once_and_records_the_reason() {
        let _host = host_with_storage_low(false);
        let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
        assert!(!signal.is_tripped());

        assert!(signal.note(&ioerr(), "test write"), "a fatal error returns true");
        assert!(signal.is_tripped());
        let reason = signal.reason().expect("reason recorded");
        assert_eq!(reason.code, rusqlite::ffi::SQLITE_IOERR);

        // A second fatal error still reports fatal (caller bails) but doesn't change
        // the recorded reason (first-wins) — the suppression that stops the flood.
        assert!(signal.note(&ioerr(), "another write"));
        assert_eq!(signal.reason().map(|r| r.code), Some(rusqlite::ffi::SQLITE_IOERR));
    }

    #[test]
    fn a_transient_error_does_not_trip() {
        let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
        assert!(!signal.note(&busy(), "contended write"), "BUSY is not fatal");
        assert!(!signal.is_tripped());
        assert!(signal.reason().is_none());
    }

    #[test]
    fn the_first_fatal_error_is_reported_once_with_its_sqlite_codes() {
        // This is the only way a dead index reaches the host's error-report
        // pipeline: a subsystem can't call the crate-root `log_error!` macro. And
        // "once" is the whole point of the signal — a flood is what it exists to
        // stop, so a second fatal error must add nothing.
        let _host = host_with_storage_low(false);
        let events = Arc::new(RecordingSink::new());
        let signal = IndexFailureSignal::new(Arc::clone(&events) as Arc<dyn crate::EventSink>);

        signal.note(&busy(), "contended write");
        assert!(events.events().is_empty(), "a transient error is not report-worthy");

        signal.note(&ioerr(), "insert entries");
        signal.note(&ioerr(), "another write");

        let reported = events.events();
        assert_eq!(reported.len(), 1, "only the FIRST fatal error is reported");
        match &reported[0] {
            crate::IndexEvent::Error {
                report: crate::IndexErrorReport::StorageFailed { failure, context, .. },
            } => {
                assert_eq!(failure.code, rusqlite::ffi::SQLITE_IOERR);
                assert_eq!(context.as_str(), "insert entries");
            }
            other => panic!("expected a StorageFailed report, got {other:?}"),
        }
    }

    fn full() -> IndexStoreError {
        sqlite(rusqlite::ffi::SQLITE_FULL)
    }

    /// cmdr-reports#100: a replay filled a nearly full disk, SQLite said
    /// `SQLITE_FULL`, and the index failed and auto-sent an error report. A full
    /// disk still stops the writer (every write would fail the same way), but as a
    /// PAUSE, and with nothing for the error pipeline.
    #[test]
    fn a_full_disk_stops_the_writer_as_a_pause_and_reports_nothing() {
        let events = Arc::new(RecordingSink::new());
        let signal = IndexFailureSignal::new(Arc::clone(&events) as Arc<dyn crate::EventSink>);

        assert!(signal.note(&full(), "upsert entry"), "the caller stops");
        assert!(signal.is_tripped());
        assert_eq!(signal.stop(), Some(IndexStop::OutOfSpace(SpaceStop::DiskFull)));
        assert_eq!(signal.reason(), None, "a pause isn't a failure");
        assert!(events.events().is_empty(), "a full disk isn't an error report");

        signal.note(&ioerr(), "a later write");
        assert_eq!(
            signal.stop(),
            Some(IndexStop::OutOfSpace(SpaceStop::DiskFull)),
            "first stop wins"
        );
        assert!(events.events().is_empty());
    }

    #[test]
    fn low_disk_space_pauses_the_same_way() {
        let events = Arc::new(RecordingSink::new());
        let signal = IndexFailureSignal::new(Arc::clone(&events) as Arc<dyn crate::EventSink>);
        assert!(signal.note_low_disk_space("writer"));
        assert_eq!(signal.stop(), Some(IndexStop::OutOfSpace(SpaceStop::BelowThreshold)));
        assert!(events.events().is_empty());
    }

    /// cmdr-reports#105: a WAL truncate failed `SQLITE_IOERR_TRUNCATE` at 0.2% free.
    /// With the host saying the disk is below the threshold, an I/O error pauses
    /// like a full disk and reports nothing.
    #[test]
    fn an_io_error_on_a_nearly_full_disk_pauses_and_reports_nothing() {
        let _host = host_with_storage_low(true);
        let events = Arc::new(RecordingSink::new());
        let signal = IndexFailureSignal::new(Arc::clone(&events) as Arc<dyn crate::EventSink>);

        assert!(signal.note(&sqlite(rusqlite::ffi::SQLITE_IOERR_TRUNCATE), "upsert entry"));
        assert_eq!(signal.stop(), Some(IndexStop::OutOfSpace(SpaceStop::BelowThreshold)));
        assert!(events.events().is_empty(), "the environment isn't an error report");
    }

    /// The low-disk answer only excuses an I/O error: corruption on a nearly full
    /// disk is still corruption.
    #[test]
    fn corruption_on_a_nearly_full_disk_still_fails() {
        let _host = host_with_storage_low(true);
        let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
        assert!(signal.note(&sqlite(rusqlite::ffi::SQLITE_CORRUPT), "read entry"));
        assert_eq!(signal.reason().map(|r| r.code), Some(rusqlite::ffi::SQLITE_CORRUPT));
    }

    /// cmdr-reports#105: a maintenance failure, even an I/O error with room to
    /// spare, logs and leaves the index running; the next tick retries.
    #[test]
    fn a_maintenance_error_never_stops_the_index() {
        let _host = host_with_storage_low(false);
        let events = Arc::new(RecordingSink::new());
        let signal = IndexFailureSignal::new(Arc::clone(&events) as Arc<dyn crate::EventSink>);

        for code in [
            rusqlite::ffi::SQLITE_IOERR_TRUNCATE,
            rusqlite::ffi::SQLITE_FULL,
            rusqlite::ffi::SQLITE_READONLY,
            rusqlite::ffi::SQLITE_BUSY,
        ] {
            signal.note_maintenance(&sqlite(code), "wal_checkpoint");
        }
        assert!(!signal.is_tripped());
        assert_eq!(signal.note_count(), 4, "each one is still noted");
        assert!(events.events().is_empty());
    }

    /// Proven corruption is the exception: the file is unusable whichever
    /// statement found out.
    #[test]
    fn corruption_found_by_maintenance_still_fails_the_index() {
        let _host = host_with_storage_low(false);
        let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
        signal.note_maintenance(&sqlite(rusqlite::ffi::SQLITE_CORRUPT), "incremental_vacuum");
        assert_eq!(signal.reason().map(|r| r.code), Some(rusqlite::ffi::SQLITE_CORRUPT));
    }

    #[tokio::test]
    async fn notified_resolves_when_already_tripped() {
        let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
        signal.note(&ioerr(), "write");
        // Must not hang: a trip that happened before we awaited still resolves.
        signal.notified().await;
        assert!(signal.is_tripped());
    }
}
