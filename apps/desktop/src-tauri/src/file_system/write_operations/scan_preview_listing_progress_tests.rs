//! Regression tests for the `ListingProgress` callback shape used by volume
//! scan previews (MTP, SMB, future remote backends).
//!
//! Before the widening, `Volume::list_directory` / `scan_for_copy_batch_with_progress`
//! callbacks were `Fn(usize)` — file count only. The scan_preview wire event then
//! hardcoded `dirs_found: 0, bytes_found: 0` because backends had no way to
//! surface those numbers. Symptom on user-facing Direct SMB / MTP scans: "X files,
//! 0 bytes, 0 dirs" climbing during the scan, then both bytes and dirs jumping
//! to the real totals only on `scan-preview-complete`.
//!
//! These tests use a stub `Volume` whose `scan_for_copy_batch_with_progress`
//! emits a non-zero `ListingProgress { files, dirs, bytes }` mid-stream, and
//! verify the running tallies make it through `run_oracle_aware_batch_scan`
//! unmodified (modulo the cumulative baseline shift it applies to the file
//! count when stitching multiple parent groups together — see Q3 in commit
//! `a5dde3e3`).
//!
//! Pure unit tests (no Tauri runtime, no Docker, no USB), fast (< 50 ms).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::scan_preview::run_oracle_aware_batch_scan;
use crate::file_system::get_volume_manager;
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ListingProgress, Volume, VolumeError,
};

/// Stub Volume that emits a non-zero `ListingProgress` from its
/// `scan_for_copy_batch_with_progress` callback. Simulates the MTP/SMB shape
/// where the backend has access to per-entry size + is_directory and can
/// accumulate running tallies inside its enumeration loop.
struct ProgressEmittingVolume {
    inner: InMemoryVolume,
    /// What we'll emit mid-stream when scan_for_copy_batch_with_progress runs.
    emit: ListingProgress,
}

impl ProgressEmittingVolume {
    fn new(name: &str, emit: ListingProgress) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            emit,
        }
    }
}

impl Volume for ProgressEmittingVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        // Force the cold-cache path in `run_oracle_aware_batch_scan` so it
        // delegates to `scan_for_copy_batch_with_progress`.
        false
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }

    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        let emit = self.emit;
        Box::pin(async move {
            // Simulate a backend that has accumulated some progress and reports
            // running (files, dirs, bytes) before returning the final result.
            if let Some(cb) = on_progress {
                cb(emit);
            }
            // Aggregate must match what we emitted via on_progress so the
            // caller's baseline accumulator (which is built from `aggregate.*`
            // after each group) stays consistent with the running counter the
            // FE sees. Real MTP/SMB backends preserve this invariant by
            // returning a final aggregate that matches the per-entry totals
            // they streamed.
            let aggregate = CopyScanResult {
                file_count: emit.files,
                dir_count: emit.dirs,
                total_bytes: emit.bytes,
                dedup_bytes: emit.bytes,
                top_level_is_directory: false,
            };
            let per_path = paths
                .iter()
                .map(|p| {
                    (
                        p.clone(),
                        CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: 0,
                            dedup_bytes: 0,
                            top_level_is_directory: false,
                        },
                    )
                })
                .collect();
            Ok(BatchScanResult { aggregate, per_path })
        })
    }
}

fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "scanprev_lp_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// Recording helper for `on_progress` calls. Captures every `ListingProgress`
/// the callback receives so the test can assert on the running tallies.
#[derive(Default)]
struct Recorder {
    calls: Mutex<Vec<ListingProgress>>,
}

impl Recorder {
    fn record(&self, p: ListingProgress) {
        self.calls.lock().unwrap().push(p);
    }

    fn snapshot(&self) -> Vec<ListingProgress> {
        self.calls.lock().unwrap().clone()
    }
}

/// Regression: when a backend's `scan_for_copy_batch_with_progress` emits a
/// `ListingProgress` with non-zero dirs and bytes, the running tally that
/// `run_oracle_aware_batch_scan` forwards to its caller carries those numbers
/// through. Before the `Fn(usize) → Fn(ListingProgress)` widening this could
/// not even be expressed: the callback signature didn't have bytes/dirs.
///
/// Catches two specific regressions:
/// 1. Anyone narrowing the callback back to `Fn(usize)` (won't compile).
/// 2. Anyone hardcoding `dirs: 0, bytes: 0` in the forwarding closure in
///    `run_oracle_aware_batch_scan` (or its callers' wrappers) — the recorded
///    `dirs` / `bytes` would drop to 0 and the assertion fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_oracle_aware_batch_scan_forwards_dirs_and_bytes_from_backend() {
    let vid = unique("forwards_dirs_bytes");
    let emit = ListingProgress {
        files: 5,
        dirs: 3,
        bytes: 1234,
    };
    let vol = Arc::new(ProgressEmittingVolume::new("emit-vol", emit));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    let sources = vec![PathBuf::from("/anywhere/a"), PathBuf::from("/anywhere/b")];
    let is_cancelled = || false;
    let recorder = Recorder::default();
    let on_progress = |p: ListingProgress| recorder.record(p);

    let _ = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("oracle-aware batch scan should succeed");

    let recorded = recorder.snapshot();
    assert!(
        !recorded.is_empty(),
        "expected at least one on_progress emit from the stub backend"
    );
    let saw_real_dirs = recorded.iter().any(|p| p.dirs > 0);
    let saw_real_bytes = recorded.iter().any(|p| p.bytes > 0);
    assert!(
        saw_real_dirs,
        "expected at least one progress emit with dirs > 0; recorded={:?}",
        recorded
    );
    assert!(
        saw_real_bytes,
        "expected at least one progress emit with bytes > 0; recorded={:?}",
        recorded
    );

    get_volume_manager().unregister(&vid);
}

/// Regression: the file-count baseline shift from Q3 (commit `a5dde3e3`)
/// keeps a backend's per-call local file count cumulative across multiple
/// parent groups. The same shift logic must apply to dirs and bytes — without
/// it, the FE display would show dirs/bytes dropping back to the latest group's
/// local values whenever the walker crossed a parent boundary.
///
/// Two sources in two different parent dirs → two cold-cache groups. Each
/// group emits `ListingProgress { files: 2, dirs: 1, bytes: 100 }`. After
/// both groups, the recording callback should have at least one entry whose
/// dirs ≥ 2 and bytes ≥ 200 (the accumulated baseline across both groups).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dirs_and_bytes_baseline_shifts_across_parent_groups() {
    let vid = unique("baseline_shift");
    let emit = ListingProgress {
        files: 2,
        dirs: 1,
        bytes: 100,
    };
    let vol = Arc::new(ProgressEmittingVolume::new("baseline-vol", emit));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Two sources, two different parent directories → two groups, two scan calls.
    let sources = vec![PathBuf::from("/parent_one/a"), PathBuf::from("/parent_two/b")];
    let is_cancelled = || false;
    let recorder = Recorder::default();
    let on_progress = |p: ListingProgress| recorder.record(p);

    let _ = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("oracle-aware batch scan should succeed");

    let recorded = recorder.snapshot();
    let max_dirs = recorded.iter().map(|p| p.dirs).max().unwrap_or(0);
    let max_bytes = recorded.iter().map(|p| p.bytes).max().unwrap_or(0);
    assert!(
        max_dirs >= 2,
        "expected dirs to accumulate across groups (>=2), got max={max_dirs}; recorded={:?}",
        recorded
    );
    assert!(
        max_bytes >= 200,
        "expected bytes to accumulate across groups (>=200), got max={max_bytes}; recorded={:?}",
        recorded
    );

    get_volume_manager().unregister(&vid);
}
