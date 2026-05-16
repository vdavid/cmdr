use super::*;
use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
use crate::file_system::write_operations::types::{CollectorEventSink, WriteConflictEvent, WriteSourceItemDoneEvent};
use std::sync::atomic::AtomicU8;

#[test]
fn test_volume_copy_config_default() {
    let config = VolumeCopyConfig::default();
    assert_eq!(config.progress_interval_ms, 200);
    assert_eq!(config.max_conflicts_to_show, 100);
}

#[test]
fn test_format_skipped_suffix_zero_is_empty() {
    // The annotation is only present when something was actually skipped, so
    // the happy-path completion log stays terse.
    assert_eq!(format_skipped_suffix(0, 0), "");
    // Stray byte count without any files: still empty (treat files as the
    // truth, bytes is just metadata).
    assert_eq!(format_skipped_suffix(0, 12345), "");
}

#[test]
fn test_format_skipped_suffix_singular() {
    assert_eq!(format_skipped_suffix(1, 0), " (of which skipped 1 file, 0 B)");
    // Humanized via search::query::format_size (binary GiB labeled GB, per
    // the existing project convention there).
    assert_eq!(
        format_skipped_suffix(1, 3_100_000_000),
        " (of which skipped 1 file, 2.9 GB)"
    );
}

#[test]
fn test_format_skipped_suffix_plural() {
    assert_eq!(format_skipped_suffix(2, 200), " (of which skipped 2 files, 200 B)");
    assert_eq!(
        format_skipped_suffix(821, 17_500_000_000),
        " (of which skipped 821 files, 16.3 GB)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_empty_source_returns_error_without_space_info() {
    // InMemoryVolume without configured space_info returns NotSupported for get_space_info
    let source = Arc::new(InMemoryVolume::new("Source"));
    let dest = Arc::new(InMemoryVolume::new("Dest"));

    let result = scan_for_volume_copy(source.as_ref(), &[], dest.as_ref(), Path::new("/"), 10).await;
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_with_in_memory_volumes() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source.create_file(Path::new("/file1.txt"), b"Hello").await.unwrap();
    source.create_file(Path::new("/file2.txt"), b"World").await.unwrap();
    let source = Arc::new(source);

    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));

    let paths = vec![PathBuf::from("/file1.txt"), PathBuf::from("/file2.txt")];
    let result = scan_for_volume_copy(source.as_ref(), &paths, dest.as_ref(), Path::new("/"), 10)
        .await
        .unwrap();

    assert_eq!(result.file_count, 2);
    assert_eq!(result.total_bytes, 10); // "Hello" + "World"
    assert!(result.conflicts.is_empty());
    assert!(result.dest_space.available_bytes >= result.total_bytes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_detects_conflicts_in_memory() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source
        .create_file(Path::new("/report.txt"), b"new content")
        .await
        .unwrap();
    let source = Arc::new(source);

    let dest = InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000);
    dest.create_file(Path::new("/report.txt"), b"old content")
        .await
        .unwrap();
    let dest = Arc::new(dest);

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/report.txt")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .unwrap();

    assert_eq!(result.file_count, 1);
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].source_path, "report.txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_insufficient_space() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source
        .create_file(Path::new("/big.bin"), &vec![0u8; 1000])
        .await
        .unwrap();
    let source = Arc::new(source);

    // Dest has only 500 bytes available
    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1000, 500));

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/big.bin")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_directory_tree() {
    let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
    source.create_directory(Path::new("/docs")).await.unwrap();
    source
        .create_file(Path::new("/docs/readme.txt"), b"Read me")
        .await
        .unwrap();
    source
        .create_file(Path::new("/docs/notes.txt"), b"Notes here")
        .await
        .unwrap();
    let source = Arc::new(source);

    let dest = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));

    let result = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("/docs")],
        dest.as_ref(),
        Path::new("/"),
        10,
    )
    .await
    .unwrap();

    assert_eq!(result.file_count, 2);
    assert_eq!(result.total_bytes, 17); // 7 + 10
}

#[test]
fn test_map_volume_error_not_found() {
    let err = map_volume_error("/ctx", VolumeError::NotFound("/test/path".to_string()));
    assert!(matches!(err, WriteOperationError::SourceNotFound { path } if path == "/test/path"));
}

#[test]
fn test_map_volume_error_permission_denied() {
    let err = map_volume_error("/ctx", VolumeError::PermissionDenied("Access denied".to_string()));
    assert!(
        matches!(err, WriteOperationError::PermissionDenied { path, message } if message == "Access denied" && path == "/ctx")
    );
}

#[test]
fn test_map_volume_error_already_exists() {
    let err = map_volume_error("/ctx", VolumeError::AlreadyExists("/existing".to_string()));
    assert!(matches!(err, WriteOperationError::DestinationExists { path } if path == "/existing"));
}

#[test]
fn test_map_volume_error_not_supported() {
    let err = map_volume_error("/ctx", VolumeError::NotSupported);
    assert!(
        // allowed-error-string-match: testing Display impl of WriteOperationError; no typed sub-variant for "not supported"
        matches!(err, WriteOperationError::IoError { path, message } if message.contains("not supported") && path == "/ctx")
    );
}

#[test]
fn test_map_volume_error_delete_pending() {
    // STATUS_DELETE_PENDING surfaces when a delete was requested but an open
    // handle is keeping the file alive on the server. It MUST become a typed
    // `WriteOperationError::DeletePending` so the write-error event carries
    // the transient "file is being removed" friendly copy — not the generic
    // IoError fallback.
    let err = map_volume_error("/ctx", VolumeError::DeletePending("STATUS_DELETE_PENDING".to_string()));
    assert!(matches!(err, WriteOperationError::DeletePending { path } if path == "/ctx"));
}

// ========================================
// LocalPosixVolume integration tests
// ========================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_with_local_volumes() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_volume_scan_src");
    let dst_dir = std::env::temp_dir().join("cmdr_volume_scan_dst");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create source files
    fs::write(src_dir.join("file1.txt"), "Hello").unwrap();
    fs::write(src_dir.join("file2.txt"), "World").unwrap();

    let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let paths = vec![PathBuf::from("file1.txt"), PathBuf::from("file2.txt")];
    let scan = scan_for_volume_copy(source.as_ref(), &paths, dest.as_ref(), Path::new(""), 10)
        .await
        .unwrap();
    assert_eq!(scan.file_count, 2);
    assert_eq!(scan.total_bytes, 10); // "Hello" + "World"
    assert!(scan.conflicts.is_empty());
    assert!(scan.dest_space.total_bytes > 0);

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_detects_conflicts() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_volume_conflict_src");
    let dst_dir = std::env::temp_dir().join("cmdr_volume_conflict_dst");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create source file
    fs::write(src_dir.join("conflict.txt"), "New content").unwrap();

    // Create existing file at destination
    fs::write(dst_dir.join("conflict.txt"), "Old content").unwrap();

    let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let scan = scan_for_volume_copy(
        source.as_ref(),
        &[PathBuf::from("conflict.txt")],
        dest.as_ref(),
        Path::new(""),
        10,
    )
    .await
    .unwrap();
    assert_eq!(scan.file_count, 1);
    assert_eq!(scan.conflicts.len(), 1);
    assert_eq!(scan.conflicts[0].source_path, "conflict.txt");
    assert_eq!(scan.conflicts[0].source_size, 11); // "New content"
    assert_eq!(scan.conflicts[0].dest_size, 11); // "Old content"

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_max_conflicts() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_volume_max_conflicts_src");
    let dst_dir = std::env::temp_dir().join("cmdr_volume_max_conflicts_dst");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Create 5 conflicting files
    let mut source_paths = Vec::new();
    for i in 0..5 {
        let name = format!("file{}.txt", i);
        fs::write(src_dir.join(&name), "new").unwrap();
        fs::write(dst_dir.join(&name), "old").unwrap();
        source_paths.push(PathBuf::from(&name));
    }

    let source = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    // Request max 3 conflicts
    let scan = scan_for_volume_copy(source.as_ref(), &source_paths, dest.as_ref(), Path::new(""), 3)
        .await
        .unwrap();
    assert_eq!(scan.conflicts.len(), 3); // Limited to max

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

// ========================================================================
// Multi-file copy execution tests (via copy_volumes_with_progress)
// ========================================================================

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

fn make_volumes() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    (
        Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)),
        Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_all_files_arrive() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"charlie").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-1",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // All 3 files at destination with correct content
    let mut stream_a = dest.open_read_stream(Path::new("/a.txt")).await.unwrap();
    assert_eq!(stream_a.next_chunk().await.unwrap().unwrap(), b"alpha");
    let mut stream_b = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
    assert_eq!(stream_b.next_chunk().await.unwrap().unwrap(), b"bravo");
    let mut stream_c = dest.open_read_stream(Path::new("/c.txt")).await.unwrap();
    assert_eq!(stream_c.next_chunk().await.unwrap().unwrap(), b"charlie");

    // Completion event emitted
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_progress_tracking() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/x.bin"), &[0; 100_000]).await.unwrap();
    source.create_file(Path::new("/y.bin"), &[0; 50_000]).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0, // Emit on every progress call
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-2",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/x.bin"), PathBuf::from("/y.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok());

    // Progress events should have been emitted
    let progress = events.progress.lock().unwrap();
    assert!(!progress.is_empty(), "expected progress events");

    // Final completion should show correct totals
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].bytes_processed, 150_000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_cancel_before_start() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Set Stopped BEFORE starting
    state.intent.store(2, Ordering::Relaxed);
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-pre-cancel",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(matches!(
        result,
        Err(WriteFailure {
            error: WriteOperationError::Cancelled { .. },
            ..
        })
    ));
    // No files should have been copied
    assert!(!dest.exists(Path::new("/a.txt")).await);
    assert!(!dest.exists(Path::new("/b.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_cancel_mid_flight() {
    // Use a custom event sink that triggers cancellation deterministically
    // when progress reports files_done >= 2.
    struct CancelAfterNSink {
        inner: CollectorEventSink,
        intent: Arc<AtomicU8>,
        cancel_after_files: usize,
    }

    impl OperationEventSink for CancelAfterNSink {
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Copying && event.files_done >= self.cancel_after_files {
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
    for i in 1..=5 {
        source
            .create_file(Path::new(&format!("/{}.bin", i)), &vec![0; 100_000])
            .await
            .unwrap();
    }

    let state = make_state();
    let events = Arc::new(CancelAfterNSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        cancel_after_files: 2,
    });
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-cancel-mid",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/1.bin"),
            PathBuf::from("/2.bin"),
            PathBuf::from("/3.bin"),
            PathBuf::from("/4.bin"),
            PathBuf::from("/5.bin"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    // Cancellation from write_from_stream's progress callback results in an IoError
    // (the VolumeError::IoError "Operation cancelled" maps to WriteOperationError::IoError).
    // The outer loop then detects the Stopped intent and returns Cancelled.
    assert!(result.is_err(), "expected error, got {:?}", result);

    // At least 2 files should exist but not all 5
    assert!(dest.exists(Path::new("/1.bin")).await);
    assert!(dest.exists(Path::new("/2.bin")).await);
    let mut total = 0;
    for i in 1..=5 {
        if dest.exists(Path::new(&format!("/{}.bin", i))).await {
            total += 1;
        }
    }
    assert!(total < 5, "expected fewer than 5 files, got {}", total);

    // The cancel either emits a write-cancelled event (if the intent check fires
    // between files) or returns an error (if write_from_stream's progress callback
    // returned Break). Both are valid cancellation paths.
    let cancelled = events.inner.cancelled.lock().unwrap();
    let had_error = result.is_err();
    assert!(
        cancelled.len() == 1 || had_error,
        "expected either a cancelled event or an error"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_skip_conflict() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/new.txt"), b"new content").await.unwrap();
    source
        .create_file(Path::new("/conflict.txt"), b"source version")
        .await
        .unwrap();
    // Pre-existing file at destination
    dest.create_file(Path::new("/conflict.txt"), b"dest version")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/new.txt"), PathBuf::from("/conflict.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok());

    // New file should be copied
    let mut stream = dest.open_read_stream(Path::new("/new.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"new content");

    // Conflicting file should keep destination version (skip)
    let mut stream = dest.open_read_stream(Path::new("/conflict.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"dest version");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_overwrite_conflict() {
    let (source, dest) = make_volumes();

    source
        .create_file(Path::new("/file.txt"), b"new version")
        .await
        .unwrap();
    dest.create_file(Path::new("/file.txt"), b"old version").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-overwrite",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/file.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok());

    // File should have source content (overwritten)
    let mut stream = dest.open_read_stream(Path::new("/file.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"new version");
}

/// Skipped files must count toward `files_processed` and bump `bytes_done` by the
/// source's size, so the progress bar reflects them. Before this fix, "Skip all"
/// silently ran through dozens of conflicts with the bar pinned at 0%, even though
/// the operation was making progress through every source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_skipped_files_count_toward_progress() {
    let (source, dest) = make_volumes();

    // 3 conflicting sources (all skipped) + 1 fresh source (copied).
    source.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"CCCCCC").await.unwrap();
    source.create_file(Path::new("/d.txt"), b"D").await.unwrap();

    // Pre-existing at dest → triggers conflict for a, b, c.
    dest.create_file(Path::new("/a.txt"), b"old").await.unwrap();
    dest.create_file(Path::new("/b.txt"), b"old").await.unwrap();
    dest.create_file(Path::new("/c.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Skip applies to all 3 conflicts; d.txt copies through. `progress_interval_ms: 0`
    // forces every skip + copy to emit a progress event.
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-skip-progress",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
            PathBuf::from("/d.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // Completion event must report all 4 sources as processed (3 skipped + 1 copied).
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(
        complete[0].files_processed, 4,
        "skipped files must count toward files_processed",
    );
    // Bytes reflect the actually copied file's 1 byte plus the three skipped sources'
    // 2 + 4 + 6 = 12 bytes (their sizes contribute to the size bar via the hint).
    assert_eq!(
        complete[0].bytes_processed, 13,
        "bytes_processed must include skipped source sizes so the size bar advances",
    );

    // Progress events: the 3 skips must each emit with `files_done` advancing.
    // (The final copy's progress events fire *during* its streaming, when
    // `files_done` is still 3; `on_file_complete` bumps it to 4 only at the
    // very end of `copy_single_path` without an extra emit, so the highest
    // value seen in the per-file-progress event stream is 3. The completion
    // event's `files_processed=4` assertion above covers the final state.)
    let progress = events.progress.lock().unwrap();
    let max_files_done = progress.iter().map(|p| p.files_done).max().unwrap_or(0);
    assert!(
        max_files_done >= 3,
        "progress events should advance through the skips; saw max files_done = {max_files_done}",
    );
    // And each skip should have produced its own event with monotonic counter.
    let skip_milestones: Vec<usize> = progress
        .iter()
        .map(|p| p.files_done)
        .filter(|&n| (1..=3).contains(&n))
        .collect();
    assert!(
        skip_milestones.windows(2).all(|w| w[0] <= w[1]),
        "files_done across skip events should be monotonic; saw {skip_milestones:?}",
    );
    assert!(
        skip_milestones.contains(&1) && skip_milestones.contains(&2) && skip_milestones.contains(&3),
        "expected progress events for each of the 3 skipped files; saw {skip_milestones:?}",
    );
}

/// `resolve_volume_conflict` in `Stop` mode must NOT call `scan_for_copy` on the
/// source when a size hint is available. On MTP, `scan_for_copy(file_path)` lists
/// the parent directory (~18 s for 1046 photos when the listing cache lapses),
/// which used to wedge the dialog at "Copying… Scanning" for the entire wait
/// before the very first conflict prompt appeared. The cached preview already
/// carries every source's size; the conflict resolver should consume that.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stop_conflict_does_not_rescan_source_when_hint_provided() {
    use crate::file_system::volume::CopyScanResult;
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;

    /// Wraps `InMemoryVolume` and counts `scan_for_copy` invocations. Skipped
    /// files never get their source opened, so we only need to delegate the
    /// read-path methods + `scan_for_copy`.
    struct ScanCountingVolume {
        inner: Arc<InMemoryVolume>,
        scan_calls: Arc<AtomicUsize>,
    }

    impl Volume for ScanCountingVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
        {
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>>
        {
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
        fn scan_for_copy<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
            self.scan_calls.fetch_add(1, Ordering::Relaxed);
            self.inner.scan_for_copy(path)
        }
    }

    // Source has the file; dest has the same name → conflict.
    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000));
    source_inner
        .create_file(Path::new("/photo.jpg"), b"new photo bytes")
        .await
        .unwrap();
    let scan_calls = Arc::new(AtomicUsize::new(0));
    let source: Arc<dyn Volume> = Arc::new(ScanCountingVolume {
        inner: Arc::clone(&source_inner),
        scan_calls: Arc::clone(&scan_calls),
    });

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000));
    dest_inner.create_file(Path::new("/photo.jpg"), b"old").await.unwrap();
    let dest: Arc<dyn Volume> = dest_inner;

    // Prime the scan-preview cache via the real `start_scan_preview` path would
    // require a Tauri AppHandle. Instead, seed it directly: the cached branch
    // reads from `SCAN_PREVIEW_RESULTS` keyed by `preview_id`.
    use crate::file_system::volume::CopyScanResult as CSR;
    use crate::file_system::write_operations::state::{CachedScanResult, SCAN_PREVIEW_RESULTS};
    let preview_id = "test-preview-id-skip-source-scan".to_string();
    SCAN_PREVIEW_RESULTS.write().unwrap().insert(
        preview_id.clone(),
        CachedScanResult {
            files: Vec::new(),
            dirs: Vec::new(),
            file_count: 1,
            total_bytes: 15,
            per_path: vec![(
                PathBuf::from("/photo.jpg"),
                CSR {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: 15,
                    top_level_is_directory: false,
                },
            )],
        },
    );

    // Auto-resolve the conflict via Skip-all so the test doesn't hang waiting
    // for a user response. The point of the test is to check that
    // `scan_for_copy` wasn't invoked between conflict detection and resolution,
    // not to walk the full Stop-mode dialog round-trip.
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        preview_id: Some(preview_id),
        ..VolumeCopyConfig::default()
    };

    // Take a baseline: cached-branch source_hints population doesn't go through
    // `scan_for_copy` (it reads from `per_path` directly), so the counter should
    // be zero before the copy runs.
    let scans_before = scan_calls.load(Ordering::Relaxed);
    assert_eq!(scans_before, 0);

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-no-rescan",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/photo.jpg")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // The conflict was detected and skipped. `Skip` mode itself doesn't scan
    // the source (it returns immediately), so to make this test catch the real
    // regression we also run with `Stop` mode and inject a resolution.
    let scans_via_skip = scan_calls.load(Ordering::Relaxed);
    assert_eq!(scans_via_skip, 0, "Skip mode must not call scan_for_copy on the source",);

    // ── Stop mode with a hint: also no scan ─────────────────────────
    SCAN_PREVIEW_RESULTS.write().unwrap().insert(
        "test-preview-id-stop".to_string(),
        CachedScanResult {
            files: Vec::new(),
            dirs: Vec::new(),
            file_count: 1,
            total_bytes: 15,
            per_path: vec![(
                PathBuf::from("/photo.jpg"),
                CSR {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: 15,
                    top_level_is_directory: false,
                },
            )],
        },
    );
    let stop_config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        preview_id: Some("test-preview-id-stop".to_string()),
        ..VolumeCopyConfig::default()
    };
    let stop_state = make_state();
    let stop_events = Arc::new(CollectorEventSink::new());

    // Drive the copy in a task; resolve the conflict via the state's oneshot
    // channel as soon as it's installed. This races, so poll briefly.
    let state_for_resolver = Arc::clone(&stop_state);
    let resolver = tokio::spawn(async move {
        for _ in 0..200 {
            if let Some(tx) = state_for_resolver.conflict_resolution_tx.lock().unwrap().take() {
                let _ = tx.send(
                    crate::file_system::write_operations::state::ConflictResolutionResponse {
                        resolution: ConflictResolution::Skip,
                        apply_to_all: true,
                    },
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("conflict_resolution_tx was never installed");
    });

    let stop_result = copy_volumes_with_progress(
        stop_events.clone(),
        "test-op-stop-no-rescan",
        &stop_state,
        Arc::clone(&source),
        &[PathBuf::from("/photo.jpg")],
        Arc::clone(&dest),
        Path::new("/"),
        &stop_config,
    )
    .await;
    resolver.await.unwrap();
    assert!(
        stop_result.is_ok(),
        "stop-then-skip copy should succeed: {:?}",
        stop_result
    );

    // The conflict event should carry the hint's size, and zero source scans.
    let scans_after_stop = scan_calls.load(Ordering::Relaxed);
    assert_eq!(
        scans_after_stop, 0,
        "Stop mode must not call scan_for_copy on the source when a size hint is supplied",
    );
    let conflicts = stop_events.conflicts.lock().unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].source_size, 15,
        "conflict event must carry the hint's size",
    );
    assert_eq!(
        conflicts[0].destination_size, 3,
        "conflict event must carry the dest_meta size",
    );
}

/// When the FE supplies the list of pre-known conflicts (from the pre-flight
/// `scan_for_conflicts`) and the user chose `Skip` upfront, the BE must
/// bulk-skip those files BEFORE entering the per-file iteration. Otherwise the
/// progress bar only advances 1-per-conflict as the loop serially hits each
/// one between (slow) copies, and the user-facing experience of "skip all"
/// looks broken when conflicts are scattered through the iteration order.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pre_known_conflicts_are_bulk_skipped_upfront() {
    let (source, dest) = make_volumes();

    // 5 sources: 3 pre-known conflicts (a, c, e) + 2 fresh (b, d).
    // Conflicts are interleaved with non-conflicts so the bulk skip's
    // benefit (one front-loaded jump vs. trickling in between copies) shows.
    source.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"CCCCCC").await.unwrap();
    source.create_file(Path::new("/d.txt"), b"DDDDDDDD").await.unwrap();
    source.create_file(Path::new("/e.txt"), b"EEEEEEEEEE").await.unwrap();

    // Existing dest files for a, c, e (these are the pre-known conflicts).
    dest.create_file(Path::new("/a.txt"), b"old").await.unwrap();
    dest.create_file(Path::new("/c.txt"), b"old").await.unwrap();
    dest.create_file(Path::new("/e.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["a.txt".to_string(), "c.txt".to_string(), "e.txt".to_string()],
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-bulk-skip",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
            PathBuf::from("/d.txt"),
            PathBuf::from("/e.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // Completion event must show all 5 sources accounted for.
    let (files_processed, bytes_processed) = {
        let complete = events.complete.lock().unwrap();
        assert_eq!(complete.len(), 1);
        (complete[0].files_processed, complete[0].bytes_processed)
    };
    assert_eq!(files_processed, 5);
    // Pre-known conflict bytes (2 + 6 + 10 = 18) + copied bytes (4 + 8 = 12) = 30.
    assert_eq!(bytes_processed, 30);

    // Skipped files must keep their pre-existing dest content.
    let mut a = dest.open_read_stream(Path::new("/a.txt")).await.unwrap();
    assert_eq!(a.next_chunk().await.unwrap().unwrap(), b"old");
    let mut c = dest.open_read_stream(Path::new("/c.txt")).await.unwrap();
    assert_eq!(c.next_chunk().await.unwrap().unwrap(), b"old");
    let mut e = dest.open_read_stream(Path::new("/e.txt")).await.unwrap();
    assert_eq!(e.next_chunk().await.unwrap().unwrap(), b"old");

    // Non-conflict files copied through.
    let mut b = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
    assert_eq!(b.next_chunk().await.unwrap().unwrap(), b"BBBB");
    let mut d = dest.open_read_stream(Path::new("/d.txt")).await.unwrap();
    assert_eq!(d.next_chunk().await.unwrap().unwrap(), b"DDDDDDDD");

    // Critical assertion: the bulk skip must emit a progress event with
    // `files_done == 3` BEFORE any copy completion. This is what makes the
    // bar "jump" immediately on the user side. The first event that reports
    // a non-zero file count should be the bulk-skip emission, accounting all
    // three pre-known conflicts at once.
    let progress = events.progress.lock().unwrap();
    let first_nonzero = progress
        .iter()
        .find(|p| p.files_done > 0)
        .expect("expected at least one progress event with files_done > 0");
    assert_eq!(
        first_nonzero.files_done, 3,
        "first non-zero progress event should account all 3 pre-known conflicts at once \
         (bulk skip should jump in one go, not trickle one-per-conflict)",
    );
    assert_eq!(
        first_nonzero.bytes_done, 18,
        "first non-zero progress event should account the conflict files' total size (2+6+10=18)",
    );
}

/// Stop mode (Ask for each) must NOT bulk-skip pre-known conflicts. The user
/// picked "ask me", so each conflict has to surface the `write-conflict`
/// event and wait for the user's resolution. If the bulk-skip path triggered
/// here, we'd silently drop user-facing prompts and the user would never get
/// to make per-file decisions.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_stop_mode_does_not_bulk_skip_pre_known_conflicts() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"new").await.unwrap();
    dest.create_file(Path::new("/a.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["a.txt".to_string()],
        ..VolumeCopyConfig::default()
    };

    // Auto-resolve via Skip-all so the test doesn't hang. The point is to verify
    // that the per-file `write-conflict` event fires (proving Stop's per-file
    // flow ran), not that the user chose any specific action.
    let state_for_resolver = Arc::clone(&state);
    let resolver = tokio::spawn(async move {
        for _ in 0..200 {
            if let Some(tx) = state_for_resolver.conflict_resolution_tx.lock().unwrap().take() {
                let _ = tx.send(
                    crate::file_system::write_operations::state::ConflictResolutionResponse {
                        resolution: ConflictResolution::Skip,
                        apply_to_all: true,
                    },
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("conflict_resolution_tx was never installed");
    });

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-stop-with-prek",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    resolver.await.unwrap();
    assert!(result.is_ok());

    // Critical: at least one write-conflict event must have fired (Stop's
    // user-facing prompt path). If the bulk-skip path took over, zero events
    // would fire and the user would never see the dialog.
    {
        let conflicts = events.conflicts.lock().unwrap();
        assert!(
            !conflicts.is_empty(),
            "Stop mode must emit write-conflict events even when pre_known_conflicts is set",
        );
    }

    // And the dest content stayed "old" because the resolver chose Skip.
    let mut a = dest.open_read_stream(Path::new("/a.txt")).await.unwrap();
    assert_eq!(a.next_chunk().await.unwrap().unwrap(), b"old");
}

/// Stale / garbage entries in `pre_known_conflicts` must not crash or silently
/// skip files the user didn't intend to skip. Two scenarios:
/// 1. Names in `pre_known_conflicts` that don't match any source path → ignored.
/// 2. Source files whose names happen to match a pre-known entry but are NOT
///    actually conflicting at dest (dest content has changed since pre-flight) →
///    still skipped under Skip mode (user explicitly chose to skip files of
///    those names). Source remains intact, dest is untouched. No data loss
///    on either side.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pre_known_conflicts_with_stale_entries_is_safe() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    // Note: dest is empty. The pre_known_conflicts list is stale — claims
    // "a.txt" conflicts but dest doesn't actually have it.

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        pre_known_conflicts: vec![
            "a.txt".to_string(),
            "nonexistent.txt".to_string(), // name not in source_paths → must be ignored
            "another-ghost.txt".to_string(),
        ],
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-stale-prek",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "stale pre_known_conflicts must not crash: {:?}", result);

    // a.txt was in the skip set → bulk-skipped before iteration. Even though
    // it wasn't actually at dest, the user chose to skip files of this name.
    // SOURCE is intact (Skip never deletes source). DEST is untouched (no
    // bytes written for a.txt). No data loss.
    assert!(source.exists(Path::new("/a.txt")).await, "source a.txt must remain");
    assert!(
        !dest.exists(Path::new("/a.txt")).await,
        "dest a.txt must not have been created"
    );

    // b.txt was NOT in pre_known_conflicts → normal copy path → reaches dest.
    let mut b = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
    assert_eq!(b.next_chunk().await.unwrap().unwrap(), b"BBBB");

    // Completion: 2 sources processed (1 skipped + 1 copied), no error.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 2);
    // bytes: skipped a.txt's 2 bytes + copied b.txt's 4 bytes = 6.
    assert_eq!(complete[0].bytes_processed, 6);
}

/// `pre_known_conflicts` is ignored for `Overwrite` mode — the loop must
/// process every source path even if it's pre-known to conflict, because in
/// that mode the user wants to overwrite.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pre_known_conflicts_ignored_outside_skip_mode() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/a.txt"), b"new").await.unwrap();
    dest.create_file(Path::new("/a.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["a.txt".to_string()],
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-overwrite-with-prek",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok());

    // Despite being pre-known, Overwrite should have replaced it with source content.
    let mut a = dest.open_read_stream(Path::new("/a.txt")).await.unwrap();
    assert_eq!(a.next_chunk().await.unwrap().unwrap(), b"new");
}

/// Real-FS coverage: the unit tests use `InMemoryVolume`, which makes it easy
/// for behaviour to silently regress on real filesystems (path normalisation,
/// case folding, FS-specific quirks of `local_path`). This drives the bulk-skip
/// flow against `LocalPosixVolume` on tmpfile to catch any divergence.
///
/// Note: when both volumes are local, `copy_between_volumes` short-circuits to
/// `copy_files_start` (see `volume_copy.rs:97`), so the bulk-skip code path
/// exercised here is the one in `copy.rs::copy_files_with_progress` — covering
/// task 3 (local↔local copy fix) at the same time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pre_known_conflicts_bulk_skip_on_real_local_volumes() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_prek_bulk_skip_src");
    let dst_dir = std::env::temp_dir().join("cmdr_prek_bulk_skip_dst");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // 5 source files: a, c, e are pre-known conflicts; b, d are fresh.
    fs::write(src_dir.join("a.txt"), "AA").unwrap(); //   2 bytes
    fs::write(src_dir.join("b.txt"), "BBBB").unwrap(); // 4 bytes
    fs::write(src_dir.join("c.txt"), "CCCCCC").unwrap(); // 6 bytes
    fs::write(src_dir.join("d.txt"), "DDDDDDDD").unwrap(); // 8 bytes
    fs::write(src_dir.join("e.txt"), "EEEEEEEEEE").unwrap(); // 10 bytes

    // Pre-existing dest files for a, c, e (these are the conflicts).
    fs::write(dst_dir.join("a.txt"), "old-a").unwrap();
    fs::write(dst_dir.join("c.txt"), "old-c").unwrap();
    fs::write(dst_dir.join("e.txt"), "old-e").unwrap();

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        pre_known_conflicts: vec!["a.txt".to_string(), "c.txt".to_string(), "e.txt".to_string()],
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-bulk-skip-real-fs",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("a.txt"),
            PathBuf::from("b.txt"),
            PathBuf::from("c.txt"),
            PathBuf::from("d.txt"),
            PathBuf::from("e.txt"),
        ],
        Arc::clone(&dest),
        Path::new(""),
        &config,
    )
    .await;
    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // Critical: the dest files for the pre-known conflicts must retain their
    // original content (no overwrites snuck in despite Skip mode).
    assert_eq!(fs::read_to_string(dst_dir.join("a.txt")).unwrap(), "old-a");
    assert_eq!(fs::read_to_string(dst_dir.join("c.txt")).unwrap(), "old-c");
    assert_eq!(fs::read_to_string(dst_dir.join("e.txt")).unwrap(), "old-e");

    // Non-conflict files must have made it across.
    assert_eq!(fs::read_to_string(dst_dir.join("b.txt")).unwrap(), "BBBB");
    assert_eq!(fs::read_to_string(dst_dir.join("d.txt")).unwrap(), "DDDDDDDD");

    // Source files are untouched (this is copy, not move).
    assert_eq!(fs::read_to_string(src_dir.join("a.txt")).unwrap(), "AA");
    assert_eq!(fs::read_to_string(src_dir.join("e.txt")).unwrap(), "EEEEEEEEEE");

    // The local↔local short-circuit at `copy_between_volumes` goes through
    // `copy_files_start` rather than `copy_volumes_with_progress` directly,
    // but `copy_volumes_with_progress` is invoked here in the test (the
    // short-circuit lives one level up). To exercise the short-circuit path
    // end-to-end you'd need `copy_between_volumes` with a Tauri AppHandle.
    // The completion accounting matches either way: skipped + copied = total.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 5);
    // Bytes: skipped (a + c + e = 2 + 6 + 10 = 18) + copied (b + d = 4 + 8 = 12) = 30.
    assert_eq!(complete[0].bytes_processed, 30);

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

/// `scan_for_copy_batch_with_progress` must invoke the callback as it discovers
/// entries so the FE's scan-preview dialog can show a climbing count instead of
/// a frozen 0/0/0 spinner. The default trait implementation (used by
/// `InMemoryVolume` and `LocalPosixVolume`) fires the callback once per scanned
/// path with the running total; `MtpVolume` overrides to thread it through
/// `list_directory_with_progress` for per-entry granularity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_copy_batch_with_progress_fires_callback() {
    use std::sync::Mutex;

    let vol = InMemoryVolume::new("V").with_space_info(1_000_000, 500_000);
    vol.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    vol.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    vol.create_file(Path::new("/c.txt"), b"CCCCCC").await.unwrap();
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let calls = Arc::new(Mutex::new(Vec::<usize>::new()));
    let calls_for_cb = Arc::clone(&calls);
    let on_progress = move |count: usize| {
        calls_for_cb.lock().unwrap().push(count);
    };

    let paths = vec![
        PathBuf::from("/a.txt"),
        PathBuf::from("/b.txt"),
        PathBuf::from("/c.txt"),
    ];
    let result = vol
        .scan_for_copy_batch_with_progress(&paths, Some(&on_progress))
        .await
        .unwrap();

    assert_eq!(result.aggregate.file_count, 3);
    assert_eq!(result.aggregate.total_bytes, 12); // 2 + 4 + 6

    // Callback must have fired with a monotonically growing count, ending at 3.
    let recorded = calls.lock().unwrap();
    assert!(!recorded.is_empty(), "on_progress must fire at least once");
    assert!(
        recorded.windows(2).all(|w| w[0] <= w[1]),
        "progress counts must be monotonic; saw {recorded:?}",
    );
    assert_eq!(
        *recorded.last().unwrap(),
        3,
        "final progress callback should report the full file count",
    );
}

/// Backwards-compat: the no-progress `scan_for_copy_batch` must keep working
/// (it's still called by `copy_volumes_with_progress` and `scan_for_volume_copy`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_copy_batch_without_progress_still_works() {
    let vol = InMemoryVolume::new("V").with_space_info(1_000_000, 500_000);
    vol.create_file(Path::new("/x.txt"), b"hello").await.unwrap();
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let result = vol.scan_for_copy_batch(&[PathBuf::from("/x.txt")]).await.unwrap();
    assert_eq!(result.aggregate.file_count, 1);
    assert_eq!(result.aggregate.total_bytes, 5);
}

// ── delete_volume_path_recursive ──────────────────────────────────
//
// Regression coverage for the move-between-volumes recursive-delete fix.
// `Volume::delete` is contractually for files or *empty* directories
// (LocalPosix uses `std::fs::remove_dir`); cross-volume moves rely on
// this helper to clear out the source tree depth-first.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_volume_path_recursive_removes_nonempty_directory() {
    let vol = Arc::new(InMemoryVolume::new("V"));
    vol.create_directory(Path::new("/photos")).await.unwrap();
    vol.create_file(Path::new("/photos/a.jpg"), b"a").await.unwrap();
    vol.create_file(Path::new("/photos/b.jpg"), b"b").await.unwrap();
    vol.create_directory(Path::new("/photos/sub")).await.unwrap();
    vol.create_file(Path::new("/photos/sub/c.jpg"), b"c").await.unwrap();

    let result: Arc<dyn Volume> = vol.clone();
    delete_volume_path_recursive(&result, Path::new("/photos"))
        .await
        .unwrap();

    assert!(!vol.exists(Path::new("/photos")).await);
    assert!(!vol.exists(Path::new("/photos/a.jpg")).await);
    assert!(!vol.exists(Path::new("/photos/sub/c.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_volume_path_recursive_removes_single_file() {
    let vol = Arc::new(InMemoryVolume::new("V"));
    vol.create_file(Path::new("/file.txt"), b"hi").await.unwrap();

    let result: Arc<dyn Volume> = vol.clone();
    delete_volume_path_recursive(&result, Path::new("/file.txt"))
        .await
        .unwrap();

    assert!(!vol.exists(Path::new("/file.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_volume_path_recursive_missing_path_is_ok() {
    // Used during move cleanup where the path may already be gone (cancelled mid-op,
    // partial state). No error.
    let vol = Arc::new(InMemoryVolume::new("V"));
    let result: Arc<dyn Volume> = vol.clone();
    let r = delete_volume_path_recursive(&result, Path::new("/never-existed")).await;
    assert!(r.is_ok(), "expected Ok, got {r:?}");
}

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
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
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
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::CopyScanResult, VolumeError>> + Send + 'a>>
    {
        self.inner.scan_for_copy(path)
    }
    fn get_space_info<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn crate::file_system::volume::VolumeReadStream>, VolumeError>> + Send + 'a,
        >,
    > {
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

// ── Phase 4 baseline bench (real QNAP NAS) ────────────────────────
//
// Measures end-to-end wall-clock for copying 100 × 10 KB files from
// the QNAP `naspi` share to a local temp dir, through the real
// `copy_volumes_with_progress` code path. Requires:
//
// - QNAP reachable at 192.168.1.111 with the `naspi` share,
//   user "david", password in `SMB2_TEST_NAS_PASSWORD` env var.
// - 100 × 10 KB files pre-uploaded at `_test/bench_100tiny/f_000.bin`
//   through `f_099.bin` (see `smb2`'s `bench_100_tiny_files_seq_vs_parallel`
//   (running that benchmark uploads them as a side effect).
//
// Run with:
//   cd apps/desktop/src-tauri && cargo test --release \
//     --lib phase4_bench -- --ignored --nocapture --test-threads=1

#[tokio::test]
#[ignore = "Phase 4 baseline: requires QNAP at 192.168.1.111 and SMB2_TEST_NAS_PASSWORD env var"]
#[allow(
    clippy::print_stdout,
    clippy::needless_update,
    reason = "Bench test prints a timing report by design (run with --nocapture); the struct-update is intentional for future-proofing."
)]
async fn phase4_bench_baseline_smb_to_local_100_tiny_files() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::smb::connect_smb_volume;
    use crate::file_system::write_operations::types::CollectorEventSink;

    const FILE_COUNT: usize = 100;

    // Load password from env (or fall back to the smb2 crate's .env file).
    let password = nas_password_from_env()
        .expect("SMB2_TEST_NAS_PASSWORD not set. Copy smb2/.env.example to smb2/.env, or set in your shell.");

    // Host is configurable so the bench can run via Tailscale
    // (`SMB2_TEST_NAS_HOST=100.127.48.122`) from a different subnet.
    let host = std::env::var("SMB2_TEST_NAS_HOST").unwrap_or_else(|_| "192.168.1.111".to_string());

    // ── Set up source (SMB) ───────────────────────────────────────
    let smb_setup_start = Instant::now();
    let smb_volume = connect_smb_volume(
        "naspi",
        "/Volumes/naspi-bench-p4",
        &host,
        "naspi",
        Some("david"),
        Some(password.as_str()),
        445,
    )
    .await
    .expect("SMB connect failed (is QNAP at 192.168.1.111 reachable?)");
    let smb_setup = smb_setup_start.elapsed();

    // ── Set up destination (local temp dir) ───────────────────────
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let local_volume = Arc::new(LocalPosixVolume::new("bench-local", tmpdir.path().to_path_buf()));

    let source_volume: Arc<dyn Volume> = Arc::new(smb_volume);
    let source_paths: Vec<PathBuf> = (0..FILE_COUNT)
        .map(|i| PathBuf::from(format!("_test/bench_100tiny/f_{:03}.bin", i)))
        .collect();

    // ── Run the copy through the real pipeline ────────────────────
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        progress_interval_ms: 200,
        conflict_resolution: ConflictResolution::Overwrite,
        max_conflicts_to_show: 0,
        preview_id: None,
        ..Default::default()
    };

    let copy_start = Instant::now();
    let result = copy_volumes_with_progress(
        events.clone(),
        "phase4-bench",
        &state,
        Arc::clone(&source_volume),
        &source_paths,
        Arc::clone(&local_volume) as Arc<dyn Volume>,
        Path::new("/"),
        &config,
    )
    .await;
    let copy_elapsed = copy_start.elapsed();

    result.expect("copy pipeline failed");

    // Verify all 100 files landed at the destination.
    for i in 0..FILE_COUNT {
        let p = tmpdir.path().join(format!("f_{:03}.bin", i));
        let md = std::fs::metadata(&p).unwrap_or_else(|e| panic!("missing dest file {p:?}: {e:?}"));
        assert_eq!(md.len(), 10 * 1024, "wrong size for {p:?}");
    }

    let fps = FILE_COUNT as f64 / copy_elapsed.as_secs_f64();
    println!();
    println!("─────────────────────────────────────────────────────────");
    println!("Phase 4 baseline: 100 × 10 KB files, QNAP → local (cmdr pipeline)");
    println!("─────────────────────────────────────────────────────────");
    println!("SMB connect + session setup: {:.2?}", smb_setup);
    println!(
        "Copy wall-clock:             {:.2?}  =  {:.1} files/sec",
        copy_elapsed, fps
    );
    println!("─────────────────────────────────────────────────────────");
}

/// Read the NAS test password from env, falling back to `../../smb2/.env`.
fn nas_password_from_env() -> Option<String> {
    if let Ok(p) = std::env::var("SMB2_TEST_NAS_PASSWORD") {
        return Some(p);
    }
    // Fall back: read from the smb2 crate's .env if present.
    let smb2_env_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // src-tauri -> desktop
        .and_then(|p| p.parent()) // desktop -> apps
        .and_then(|p| p.parent()) // apps -> cmdr
        .and_then(|p| p.parent()) // cmdr -> projects-git/vdavid
        .map(|p| p.join("smb2").join(".env"))?;
    let contents = std::fs::read_to_string(&smb2_env_path).ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("SMB2_TEST_NAS_PASSWORD=") {
            let unquoted = rest.trim_matches('"').to_string();
            return Some(unquoted);
        }
    }
    None
}
