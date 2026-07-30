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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::ignore_poison::IgnorePoison;

use crate::indexing::store::{IndexFailure, IndexStoreError};

/// A one-shot, per-volume "the index DB has died" signal. See the module docs for
/// who shares it and why.
pub(crate) struct IndexFailureSignal {
    /// Where the first fatal error is reported. The signal is per volume, so this
    /// is that volume's sink.
    events: Arc<dyn crate::indexing::EventSink>,
    tripped: AtomicBool,
    reason: Mutex<Option<IndexFailure>>,
    notify: Notify,
    /// Every [`note`](IndexFailureSignal::note) call, fatal or not. Tests assert on
    /// it to prove a handler treated a situation as normal rather than routing it
    /// here: a non-fatal note logs a warn and returns, leaving no other trace.
    notes: AtomicUsize,
}

impl IndexFailureSignal {
    pub(crate) fn new(events: Arc<dyn crate::indexing::EventSink>) -> Self {
        Self {
            events,
            notes: AtomicUsize::new(0),
            tripped: AtomicBool::new(false),
            reason: Mutex::new(None),
            notify: Notify::new(),
        }
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
    pub(crate) fn note(&self, err: &IndexStoreError, context: &str) -> bool {
        self.notes.fetch_add(1, Ordering::Relaxed);
        let Some(failure) = err.as_index_failure() else {
            log::warn!("{context}: {err}");
            return false;
        };
        // CAS so only the FIRST fatal error logs + notifies; the rest are the
        // flood we're here to stop.
        if self
            .tripped
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            *self.reason.lock_ignore_poison() = Some(failure);
            self.events.emit(crate::indexing::IndexEvent::Error {
                report: crate::indexing::IndexErrorReport::StorageFailed {
                    failure,
                    context: crate::indexing::Diagnostic(context.to_string()),
                    detail: crate::indexing::Diagnostic(err.to_string()),
                },
            });
            self.notify.notify_one();
        }
        true
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

    /// The recorded failure reason, if tripped.
    pub(crate) fn reason(&self) -> Option<IndexFailure> {
        *self.reason.lock_ignore_poison()
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
        IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
            None,
        ))
    }

    use crate::indexing::events::RecordingSink;

    fn busy() -> IndexStoreError {
        IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        ))
    }

    #[test]
    fn a_fatal_error_trips_once_and_records_the_reason() {
        let signal = IndexFailureSignal::new(crate::indexing::NoopEventSink::shared());
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
        let signal = IndexFailureSignal::new(crate::indexing::NoopEventSink::shared());
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
        let events = Arc::new(RecordingSink::new());
        let signal = IndexFailureSignal::new(Arc::clone(&events) as Arc<dyn crate::indexing::EventSink>);

        signal.note(&busy(), "contended write");
        assert!(events.events().is_empty(), "a transient error is not report-worthy");

        signal.note(&ioerr(), "insert entries");
        signal.note(&ioerr(), "another write");

        let reported = events.events();
        assert_eq!(reported.len(), 1, "only the FIRST fatal error is reported");
        match &reported[0] {
            crate::indexing::IndexEvent::Error {
                report: crate::indexing::IndexErrorReport::StorageFailed { failure, context, .. },
            } => {
                assert_eq!(failure.code, rusqlite::ffi::SQLITE_IOERR);
                assert_eq!(context.as_str(), "insert entries");
            }
            other => panic!("expected a StorageFailed report, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn notified_resolves_when_already_tripped() {
        let signal = IndexFailureSignal::new(crate::indexing::NoopEventSink::shared());
        signal.note(&ioerr(), "write");
        // Must not hang: a trip that happened before we awaited still resolves.
        signal.notified().await;
        assert!(signal.is_tripped());
    }
}
