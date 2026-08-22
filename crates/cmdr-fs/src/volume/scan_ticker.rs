//! Running counts for one `scan_for_copy*` call.
//!
//! ⚠️ A recursive scan over a network backend reports nothing until it returns,
//! so the transfer dialog sits on `0 bytes / 0 files / 0 dirs` for however long
//! the walk takes. It also leaves the scan watchdog
//! (`write_operations/scan_watchdog.rs`) blind: that bounds a preview by
//! INACTIVITY, and a backend that never reports activity is indistinguishable
//! from a server that has stopped answering.
//!
//! Shared rather than copied per backend: every remote backend needs exactly
//! this, and two copies drift on the one thing that has to stay true — the
//! counts are cumulative FOR THE CALL, which is what
//! [`Volume::scan_for_copy_batch_with_progress`](super::Volume::scan_for_copy_batch_with_progress)
//! promises its callers.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::ListingProgress;

/// Counts a scan as it walks, reporting each step to the caller's callback.
pub struct ScanTicker<'a> {
    on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    files: AtomicUsize,
    dirs: AtomicUsize,
    bytes: AtomicU64,
}

impl<'a> ScanTicker<'a> {
    /// A ticker reporting to `on_progress`, or counting silently without one.
    pub fn new(on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>) -> Self {
        Self {
            on_progress,
            files: AtomicUsize::new(0),
            dirs: AtomicUsize::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// One more directory entered.
    pub fn dir(&self) {
        self.dirs.fetch_add(1, Ordering::Relaxed);
        self.report();
    }

    /// One more file counted, at `size` bytes.
    pub fn file(&self, size: u64) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(size, Ordering::Relaxed);
        self.report();
    }

    /// The running totals so far.
    pub fn counts(&self) -> ListingProgress {
        ListingProgress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }

    fn report(&self) {
        if let Some(callback) = self.on_progress {
            callback(self.counts());
        }
    }
}

#[cfg(test)]
#[path = "scan_ticker_test.rs"]
mod scan_ticker_test;
