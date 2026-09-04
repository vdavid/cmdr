//! The one seam a copy scan touches per entry: it reports what the walk found,
//! and it answers whether the walk may keep going.
//!
//! ⚠️ A recursive scan over a network backend reports nothing until it returns,
//! so the transfer dialog sits on `0 bytes / 0 files / 0 dirs` for however long
//! the walk takes. It also leaves the scan watchdog
//! (`write_operations/scan_watchdog.rs`) blind: that bounds a preview by
//! INACTIVITY, and a backend that never reports activity is indistinguishable
//! from a server that has stopped answering.
//!
//! ❗ **Reporting and stopping are one call, on purpose.** A walk that counted an
//! entry without asking whether to carry on is exactly the walk a person can't
//! cancel, and that shape used to be the easy one to write. Now
//! [`ScanBoundary::file`] and [`ScanBoundary::dir`] both hand back a
//! `Result<(), VolumeError>`, so a backend author reaches the stop by writing
//! `?` on a call they were already making.
//!
//! Shared rather than copied per backend: every remote backend needs exactly
//! this, and two copies drift on the one thing that has to stay true — the
//! counts are cumulative FOR THE CALL, which is what
//! [`Volume::scan_for_copy_batch_with_boundary`](super::Volume::scan_for_copy_batch_with_boundary)
//! promises its callers.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::{ListingProgress, ScanStop, VolumeError};

/// Counts a scan as it walks and holds the cooperative stop it honors.
///
/// A caller builds one and hands it to the batch scan; a backend with nothing to
/// report and nobody to answer to uses [`silent`](Self::silent).
pub struct ScanBoundary<'a> {
    on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    stop: ScanStop,
    files: AtomicUsize,
    dirs: AtomicUsize,
    bytes: AtomicU64,
}

impl<'a> ScanBoundary<'a> {
    /// A boundary reporting to `on_progress`, or counting silently without one.
    /// Nothing can stop it until [`stopping_at`](Self::stopping_at) says so.
    pub fn new(on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>) -> Self {
        Self {
            on_progress,
            stop: ScanStop::none(),
            files: AtomicUsize::new(0),
            dirs: AtomicUsize::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    /// Nobody to report to and nobody to answer to: what the single-path
    /// [`Volume::scan_for_copy`](super::Volume::scan_for_copy) has, since the
    /// trait hands it neither.
    pub fn silent() -> Self {
        Self::new(None)
    }

    /// The scan answers to `stop`: Cancel ends it, Pause holds it still.
    pub fn stopping_at(mut self, stop: ScanStop) -> Self {
        self.stop = stop;
        self
    }

    /// One more directory, about to be listed.
    ///
    /// ❗ Ask it BEFORE the listing round trip, which is the expensive part: over
    /// a sleeping NAS one listing is seconds, and a boundary on the far side of
    /// it is a boundary the user waits out.
    pub async fn dir(&self) -> Result<(), VolumeError> {
        self.dirs.fetch_add(1, Ordering::Relaxed);
        self.report();
        self.check().await
    }

    /// One more file counted, at `size` bytes.
    pub async fn file(&self, size: u64) -> Result<(), VolumeError> {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(size, Ordering::Relaxed);
        self.report();
        self.check().await
    }

    /// A whole subtree whose entries were counted somewhere else — a backend's
    /// own recursion, or a watcher-fresh cached listing — folded into the
    /// running totals in one step.
    ///
    /// It asks the same boundary the per-entry calls do, so a backend that takes
    /// a shortcut past the walk still can't skip past the user's Cancel.
    pub async fn subtree(&self, files: usize, dirs: usize, bytes: u64) -> Result<(), VolumeError> {
        self.files.fetch_add(files, Ordering::Relaxed);
        self.dirs.fetch_add(dirs, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.report();
        self.check().await
    }

    /// The stop boundary with nothing to count: for a backend asking between
    /// source paths, or before a round trip that isn't an entry of its own.
    pub async fn check(&self) -> Result<(), VolumeError> {
        if self.stop.should_stop().await {
            return Err(stopped());
        }
        Ok(())
    }

    /// The caller's progress callback as it stands, for a backend that hands it
    /// to a lower primitive doing its own reporting: MTP's streaming
    /// `list_directory` over a 1,000-entry folder, where the ~17 s of USB round
    /// trips is exactly what the dialog has to look alive through.
    ///
    /// ❗ A backend that takes this reports through the callback and ❌ NOT
    /// through [`file`](Self::file) / [`dir`](Self::dir): two count streams into
    /// one callback make the dialog's numbers jump backwards. It still owes
    /// [`check`](Self::check) at its seams — a sync callback has nothing to
    /// await, so the stop can't ride along inside it.
    pub fn raw_progress(&self) -> Option<&'a (dyn Fn(ListingProgress) + Sync)> {
        self.on_progress
    }

    /// The running totals so far.
    pub fn counts(&self) -> ListingProgress {
        ListingProgress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }

    /// The stop on its own, for a walk that runs where it can't `.await` — the
    /// local backend's `WalkDir` loop inside `spawn_blocking` is the one in the
    /// tree. The clone is an `Arc` bump, paid once per scan.
    pub fn stop(&self) -> ScanStop {
        self.stop.clone()
    }

    fn report(&self) {
        if let Some(callback) = self.on_progress {
            callback(self.counts());
        }
    }
}

/// What a stopped scan hands back.
///
/// ❗ The same `VolumeError::Cancelled` a cancel-aware listing or delete already
/// returns, ❌ never a partial `BatchScanResult`: a caller reads a scan's totals
/// as the size of the transfer it is about to run, and a truncated total that
/// looks successful is a progress bar that finishes at 30% and a space check
/// that passes when it shouldn't. Every caller of the batch scan already handles
/// this variant.
pub fn stopped() -> VolumeError {
    VolumeError::Cancelled("Operation cancelled by user".to_string())
}

#[cfg(test)]
#[path = "scan_boundary_test.rs"]
mod scan_boundary_test;
