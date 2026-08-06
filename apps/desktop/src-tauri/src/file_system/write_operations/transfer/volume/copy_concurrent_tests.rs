//! Concurrency tests for `copy_volumes_with_progress`, split out of
//! `volume_copy_tests.rs`. These exercise the `FuturesUnordered` batch path:
//! all-succeed, abort-on-first-error, and cancellation mid-batch. The
//! `PoisonedReadVolume` double injects a read failure, and the local
//! `CancelOnProgressSink` flips the operation's intent mid-flight.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume_copy_tests.rs`
//! (`super::tests`).

use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, ListingProgress, SpaceInfo, VolumeReadStream};
use crate::file_system::write_operations::types::{
    CollectorEventSink, WriteConflictEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::AtomicU8;

// ── Phase 4.2 concurrency tests ──────────────────────────────────
//
// Exercise the FuturesUnordered path in `copy_volumes_with_progress`.
// `InMemoryVolume` returns `max_concurrent_ops() = 32`, so batches of
// 3+ files automatically take the concurrent branch (clamped to 32).

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_copy_50_files_all_succeed() {
    let (source, dest) = make_volumes();

    // 50 small files, well over the threshold=3 and concurrency=32.
    for i in 0..50 {
        let name = format!("/file_{:02}.bin", i);
        source
            .create_file(Path::new(&name), &vec![i as u8; 1024])
            .await
            .unwrap();
    }

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0, // Emit on every progress call
        ..VolumeCopyConfig::default()
    };

    let paths: Vec<PathBuf> = (0..50).map(|i| PathBuf::from(format!("/file_{:02}.bin", i))).collect();
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-concurrent-50",
        &state,
        Arc::clone(&source),
        &paths,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected success, got {:?}", result);

    // All 50 files landed at destination with the right content.
    for i in 0..50 {
        let name = format!("/file_{:02}.bin", i);
        let mut stream = dest.open_read_stream(Path::new(&name)).await.unwrap();
        let mut collected = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, vec![i as u8; 1024], "wrong content for {}", name);
    }

    // Progress events were emitted (throttled, but >= 1 under concurrency).
    let progress = events.progress.lock().unwrap();
    assert!(
        !progress.is_empty(),
        "expected at least one progress event under concurrency"
    );

    // Completion event with correct totals.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 50);
    assert_eq!(complete[0].bytes_processed, 50 * 1024);
}

/// Volume wrapper that delegates everything to an inner `InMemoryVolume`
/// except for a single poisoned filename, which returns an I/O error on
/// read. Used to exercise abort-on-first-error under concurrency.
struct PoisonedReadVolume {
    inner: Arc<InMemoryVolume>,
    poisoned_file: String,
}

impl Volume for PoisonedReadVolume {
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
    fn supports_export(&self) -> bool {
        true
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        32
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let name = self.poisoned_file.clone();
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            if path.to_string_lossy() == name {
                return Err(VolumeError::IoError {
                    message: "Injected read failure".into(),
                    raw_os_error: Some(5), // EIO
                });
            }
            inner.open_read_stream(path).await
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_copy_aborts_on_first_error() {
    let inner_source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for i in 0..20 {
        let name = format!("/file_{:02}.bin", i);
        inner_source
            .create_file(Path::new(&name), &vec![0xAB; 1024])
            .await
            .unwrap();
    }
    // File 05 will fail when read.
    let source: Arc<dyn Volume> = Arc::new(PoisonedReadVolume {
        inner: Arc::clone(&inner_source),
        poisoned_file: "/file_05.bin".to_string(),
    });
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let paths: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("/file_{:02}.bin", i))).collect();
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-concurrent-err",
        &state,
        Arc::clone(&source),
        &paths,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    // Must return an IoError (the injected one). The in-flight tasks drop
    // cleanly and the outer loop returns the mapped error.
    assert!(matches!(
        result,
        Err(WriteFailure {
            error: WriteOperationError::IoError { .. },
            ..
        })
    ));

    // Not all 20 files should be at the dest (some were still in flight
    // or not yet started when the abort fired). The poisoned file itself
    // cannot have landed.
    assert!(!dest.exists(Path::new("/file_05.bin")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_copy_cancellation_mid_batch() {
    // Custom event sink that flips the intent to Stopped after a few
    // progress events land. Deterministic: doesn't rely on timing.
    struct CancelOnProgressSink {
        inner: CollectorEventSink,
        intent: Arc<AtomicU8>,
        cancel_after_events: usize,
        events_seen: AtomicUsize,
    }
    impl OperationEventSink for CancelOnProgressSink {
        fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
            self.inner.emit_settled(e);
        }
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Copying
                && self.events_seen.fetch_add(1, Ordering::Relaxed) >= self.cancel_after_events
            {
                self.intent.store(2, Ordering::Relaxed);
            }
            self.inner.emit_progress(event);
        }
        fn emit_complete(&self, e: WriteCompleteEvent) {
            self.inner.emit_complete(e);
        }
        fn emit_cancelled(&self, e: WriteCancelledEvent) {
            self.inner.emit_cancelled(e);
        }
        fn emit_error(&self, e: WriteErrorEvent) {
            self.inner.emit_error(e);
        }
        fn emit_conflict(&self, e: WriteConflictEvent) {
            self.inner.emit_conflict(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    }

    let (source, dest) = make_volumes();
    // 20 large-ish files so the batch stays in flight long enough
    // for the cancel to land while tasks are running.
    for i in 0..20 {
        let name = format!("/big_{:02}.bin", i);
        source
            .create_file(Path::new(&name), &vec![i as u8; 200_000])
            .await
            .unwrap();
    }

    let state = make_state();
    let events = Arc::new(CancelOnProgressSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        cancel_after_events: 2,
        events_seen: AtomicUsize::new(0),
    });
    let config = VolumeCopyConfig {
        progress_interval_ms: 0, // Emit on every chunk so we can trigger early.
        ..VolumeCopyConfig::default()
    };

    let paths: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("/big_{:02}.bin", i))).collect();
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-concurrent-cancel",
        &state,
        Arc::clone(&source),
        &paths,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    // Either Cancelled (pure cancel branch) or IoError (if a task's
    // progress callback returned Break). Both are valid cancellation
    // shapes, matching the sequential test `test_multi_file_copy_cancel_mid_flight`.
    assert!(
        matches!(
            result,
            Err(WriteFailure {
                error: WriteOperationError::Cancelled { .. },
                ..
            }) | Err(WriteFailure {
                error: WriteOperationError::IoError { .. },
                ..
            })
        ),
        "expected Cancelled or IoError, got {:?}",
        result
    );

    // Intent was flipped to Stopped by the sink; confirm we observed it.
    assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);

    // Less than all 20 landed (cancellation worked somewhere).
    let mut total = 0;
    for i in 0..20 {
        if dest.exists(Path::new(&format!("/big_{:02}.bin", i))).await {
            total += 1;
        }
    }
    assert!(total < 20, "expected fewer than 20 files at dest, got {}", total);
}
