//! The one seam a copy scan touches per entry: it reports what the walk found,
//! and it answers whether the walk may keep going.
//!
//! ⚠️ A recursive scan over a network backend reports nothing until it returns,
//! so the transfer dialog sits on `0 bytes / 0 files / 0 dirs` for however long
//! the walk takes. It also leaves the scan watchdog
//! (`crates/cmdr-operations/src/scan_watchdog.rs`) blind: that bounds a preview by
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
//!
//! **Every entry carries its path**, so a caller that wants the selection's
//! shape hangs a [`ScanRecorder`] on the boundary (`recording_to`) and gets each
//! entry the walk counts. Same reasoning as the stop: the call a backend already
//! makes per entry is the only one it can't forget.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::{CopyScanResult, ListingProgress, ScanOutages, ScanReadNext, ScanRecorder, ScanStop, VolumeError};

/// Counts a scan as it walks and holds the cooperative stop it honors.
///
/// A caller builds one and hands it to the batch scan; a backend with nothing to
/// report and nobody to answer to uses [`silent`](Self::silent).
pub struct ScanBoundary<'a> {
    on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    stop: ScanStop,
    /// `Arc`-held for the same reason the stop is: a walk inside
    /// `spawn_blocking` (the local backend's) carries it across the closure.
    recorder: Option<Arc<dyn ScanRecorder>>,
    /// Who decides whether a failed read waits out an outage and runs again.
    /// Borrowed: only an async walk asks it, never one inside `spawn_blocking`.
    outages: Option<&'a dyn ScanOutages>,
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
            recorder: None,
            outages: None,
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

    /// The scan hands every entry it counts to `recorder` as well.
    pub fn recording_to(mut self, recorder: Arc<dyn ScanRecorder>) -> Self {
        self.recorder = Some(recorder);
        self
    }

    /// A read the walk sends that fails asks `outages` whether to wait and run
    /// it again ([`through_outages`](Self::through_outages)).
    pub fn waiting_out(mut self, outages: &'a dyn ScanOutages) -> Self {
        self.outages = Some(outages);
        self
    }

    /// Runs `read` (one listing or stat of `path`), and when it fails, asks the
    /// owner whether the volume went away and came back; if so, runs it again.
    /// With no owner to ask, the read's error is the answer, as it always was.
    ///
    /// ❗ Only a READ, and ❌ nothing counted inside it: the entries it returns
    /// go through [`dir`](Self::dir) / [`file`](Self::file) after it succeeds,
    /// so a read run again counts each entry once (`scan_outages.rs`).
    pub async fn through_outages<T, F, Fut>(&self, path: &Path, mut read: F) -> Result<T, VolumeError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, VolumeError>>,
    {
        let Some(outages) = self.outages else {
            return read().await;
        };
        let mut judge = outages.read(path);
        loop {
            let error = match read().await {
                Ok(value) => return Ok(value),
                Err(error) => error,
            };
            match judge.after_failure(&error).await {
                ScanReadNext::Again => {}
                ScanReadNext::Fail => return Err(error),
                ScanReadNext::Stopped => return Err(stopped()),
            }
        }
    }

    /// One more directory, about to be listed.
    ///
    /// ❗ Ask it BEFORE the listing round trip, which is the expensive part: over
    /// a sleeping NAS one listing is seconds, and a boundary on the far side of
    /// it is a boundary the user waits out.
    pub async fn dir(&self, path: &Path) -> Result<(), VolumeError> {
        if let Some(recorder) = &self.recorder {
            recorder.dir(path);
        }
        self.dirs.fetch_add(1, Ordering::Relaxed);
        self.report();
        self.check().await
    }

    /// One more file counted, at `size` bytes.
    pub async fn file(&self, path: &Path, size: u64) -> Result<(), VolumeError> {
        if let Some(recorder) = &self.recorder {
            recorder.file(path, size);
        }
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(size, Ordering::Relaxed);
        self.report();
        self.check().await
    }

    /// A whole subtree at `path` whose entries were counted somewhere else (a
    /// backend's own per-path scan), folded into the running totals in one step
    /// and recorded as a subtree whose children nobody walked.
    ///
    /// It asks the same boundary the per-entry calls do, so a backend that takes
    /// a shortcut past the walk still can't skip past the user's Cancel.
    pub async fn subtree(&self, path: &Path, scan: &CopyScanResult) -> Result<(), VolumeError> {
        if let Some(recorder) = &self.recorder {
            recorder.subtree(path, scan);
        }
        self.counted(scan).await
    }

    /// Totals from a walk that already handed its entries to
    /// [`recorder`](Self::recorder) itself: the local backend's blocking walk,
    /// which can't await this boundary per entry. Folds them in without
    /// recording anything, so the recorder doesn't see the subtree twice.
    pub async fn counted(&self, scan: &CopyScanResult) -> Result<(), VolumeError> {
        self.files.fetch_add(scan.file_count, Ordering::Relaxed);
        self.dirs.fetch_add(scan.dir_count, Ordering::Relaxed);
        self.bytes.fetch_add(scan.total_bytes, Ordering::Relaxed);
        self.report();
        self.check().await
    }

    /// The recorder this scan reports entries to, if anyone asked, for a walk
    /// that records on its own (see [`counted`](Self::counted)).
    pub fn recorder(&self) -> Option<&Arc<dyn ScanRecorder>> {
        self.recorder.as_ref()
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
    VolumeError::Cancelled("Operation cancelled by user".into())
}

#[cfg(test)]
#[path = "scan_boundary_test.rs"]
mod scan_boundary_test;
