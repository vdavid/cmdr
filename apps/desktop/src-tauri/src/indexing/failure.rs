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

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::ignore_poison::IgnorePoison;

use super::store::{IndexFailure, IndexStoreError};

/// A one-shot, per-volume "the index DB has died" signal. See the module docs for
/// who shares it and why.
pub(crate) struct IndexFailureSignal {
    tripped: AtomicBool,
    reason: Mutex<Option<IndexFailure>>,
    notify: Notify,
}

impl IndexFailureSignal {
    pub(crate) fn new() -> Self {
        Self {
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
            crate::log_error!(
                "Index storage failed ({context}): SQLite code {}/{}, stopping this volume's index: {err}",
                failure.code,
                failure.extended_code,
            );
            self.notify.notify_one();
        }
        true
    }

    /// Whether a fatal storage error has been recorded. Polled by the writer loop
    /// and the live event loop to stop.
    pub(crate) fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Acquire)
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

    fn busy() -> IndexStoreError {
        IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        ))
    }

    #[test]
    fn a_fatal_error_trips_once_and_records_the_reason() {
        let signal = IndexFailureSignal::new();
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
        let signal = IndexFailureSignal::new();
        assert!(!signal.note(&busy(), "contended write"), "BUSY is not fatal");
        assert!(!signal.is_tripped());
        assert!(signal.reason().is_none());
    }

    #[tokio::test]
    async fn notified_resolves_when_already_tripped() {
        let signal = IndexFailureSignal::new();
        signal.note(&ioerr(), "write");
        // Must not hang: a trip that happened before we awaited still resolves.
        signal.notified().await;
        assert!(signal.is_tripped());
    }
}
