use super::*;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, LocalPosixVolume};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{
    ConflictResolution, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
};
use crate::test_support::TestDir;
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
    let dest_space = result.dest_space.expect("this destination reports its space");
    assert!(dest_space.available_bytes().expect("bounded") >= result.total_bytes);
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_cannot_be_addressed_is_never_reported_as_a_missing_source() {
    // The destination volume answers `NotFound` for the folder the copy is
    // asked to create — the shape a share produces for a path it can't address.
    // The user's source file is sitting right there, so telling them it "no
    // longer exists" sends them hunting for data loss that never happened,
    // while the real fault (the destination) goes unnamed. This is the report
    // that reached us from a NAS user: a destination problem wearing the
    // source's name.
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)) as Arc<dyn Volume>;
    let dest = Arc::new(
        InMemoryVolume::new("Dest")
            .with_space_info(10_000_000, 10_000_000)
            .with_create_directory_not_found(),
    ) as Arc<dyn Volume>;

    source.create_file(Path::new("/report.pdf"), b"payload").await.unwrap();

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-dest-not-found",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/report.pdf")],
        Arc::clone(&dest),
        Path::new("/photos/2026"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("a destination that can't be created must fail the copy");

    // `/photos` rather than `/photos/2026`: `create_directory_all` walks
    // shallowest-first, so the ancestor it stopped on IS the honest answer to
    // "which folder couldn't be made".
    assert!(
        matches!(&failure.error, WriteOperationError::DestinationNotFound { path } if path == "/photos"),
        "expected a typed DestinationNotFound naming the destination folder, got: {:?}",
        failure.error
    );
}

// ========================================
// LocalPosixVolume integration tests
// ========================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_with_local_volumes() {
    use std::fs;

    let src_dir = TestDir::new("volume_scan_src");
    let dst_dir = TestDir::new("volume_scan_dst");

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
    assert!(
        scan.dest_space
            .expect("this destination reports its space")
            .total_bytes()
            .expect("bounded")
            > 0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_detects_conflicts() {
    use std::fs;

    let src_dir = TestDir::new("volume_conflict_src");
    let dst_dir = TestDir::new("volume_conflict_dst");

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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_scan_for_volume_copy_max_conflicts() {
    use std::fs;

    let src_dir = TestDir::new("volume_max_conflicts_src");
    let dst_dir = TestDir::new("volume_max_conflicts_dst");

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
}

// ========================================================================
// Multi-file copy execution tests (via copy_volumes_with_progress)
// ========================================================================

// `pub(super)` so the sibling `volume_copy_crashsafe_tests` and
// `volume_copy_rollback_tests` modules can share these fixtures without
// duplicating them.
pub(super) fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

pub(super) fn make_volumes() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
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

/// Serial cross-volume copy of large files emits multiple `Copying`-phase
/// progress events as chunks stream through. Pins the contract before the
/// per-file progress closure gets extracted into a shared helper, so a
/// regression there fails this test (and its move twin) loudly.
///
/// `source_paths.len() < 3` forces `use_concurrent_path = false`
/// (see `volume/copy.rs` § `use_concurrent_path` selection), so this
/// exercises the serial-driver `on_file_progress` site. Two files (rather
/// than one) so the second file's emits show `files_done = 1` after the
/// first file completes — making "files axis advances across files" pin
/// down too, not just "bytes axis advances within a file."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cross_volume_copy_serial_emits_intra_file_progress() {
    let (source, dest) = make_volumes();
    let payload: Vec<u8> = vec![0u8; 1_048_576];
    source.create_file(Path::new("/a.bin"), &payload).await.unwrap();
    source.create_file(Path::new("/b.bin"), &payload).await.unwrap();
    let total_bytes = (payload.len() * 2) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-copy-serial-intra",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.bin"), PathBuf::from("/b.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    assert!(
        copying.len() >= 4,
        "serial copy: expected multiple Copying events across 2 × 1 MB transfers, got {} ({:?})",
        copying.len(),
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // Serial path: receipt order matches emit order, no cross-task races.
    // bytes_done is the running aggregate (`bytes_done_so_far + file_bytes_done`),
    // so it strictly grows as chunks stream and resets only when the snapshot
    // shifts — but bytes_done_so_far accounts for completed files, so the
    // aggregate stays non-decreasing across the run.
    for w in copying.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "bytes_done must be non-decreasing across Copying events, got {} then {}",
            w[0].bytes_done,
            w[1].bytes_done,
        );
    }
    // Both files contributed to the running aggregate: at least one event
    // crosses the first-file boundary (bytes_done > one_file_size).
    let saw_second_file = copying.iter().any(|p| p.bytes_done > payload.len() as u64);
    assert!(
        saw_second_file,
        "expected at least one Copying event past the first-file boundary ({}), got {:?}",
        payload.len(),
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // After the first file completes the driver bumps files_done, so the
    // second file's emits show files_done = 1.
    let saw_files_done_1 = copying.iter().any(|p| p.files_done == 1);
    assert!(
        saw_files_done_1,
        "expected at least one Copying event with files_done = 1 (second file's emits), got {:?}",
        copying.iter().map(|e| e.files_done).collect::<Vec<_>>(),
    );
    // Cumulative correctness is pinned by the complete event.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].bytes_processed, total_bytes);
    assert_eq!(complete[0].files_processed, 2);
}

/// Serial cross-volume copy of a single DIRECTORY source must report
/// progress at LEAF-file granularity, not top-level-source granularity:
/// `bytes_done` accumulates across the directory's inner files (the Size
/// bar climbs smoothly 0 → total instead of resetting to 0 at each inner
/// file), and `files_done` advances per inner file (the File bar climbs
/// 0 → N instead of sitting at 0 until the whole folder finishes).
///
/// Regression guard for the directory-source progress bug: a single folder
/// of N files was emitting every inner file's progress against a frozen
/// `bytes_done_so_far = 0` / `files_done_so_far = 0` snapshot, so the Size
/// bar sawtoothed back to 0 per inner file and the File bar never left 0.
///
/// One top-level source (`< 3`) forces the serial path. Three inner files
/// (each large enough to emit several chunked progress events) so the
/// crossing of inner-file boundaries is observable mid-stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cross_volume_copy_directory_source_progress_is_leaf_granular() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/folder")).await.unwrap();
    let one_mb: Vec<u8> = vec![0u8; 1_048_576];
    source.create_file(Path::new("/folder/a.bin"), &one_mb).await.unwrap();
    source.create_file(Path::new("/folder/b.bin"), &one_mb).await.unwrap();
    source.create_file(Path::new("/folder/c.bin"), &one_mb).await.unwrap();
    let one_file = one_mb.len() as u64;
    let total_bytes = one_file * 3;

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-copy-dir-leaf",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/folder")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();

    // Size bar: bytes_done is the running aggregate across ALL inner files,
    // so it stays non-decreasing for the whole directory (no per-leaf reset).
    for w in copying.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "bytes_done must be non-decreasing across the directory's inner files, got {} then {} ({:?})",
            w[0].bytes_done,
            w[1].bytes_done,
            copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
        );
    }
    // The aggregate must cross BOTH inner-file boundaries: at least one event
    // reports more than two inner files' worth of bytes. With the frozen
    // snapshot bug, no event ever exceeds a single inner file's size.
    let crossed_two_files = copying.iter().any(|p| p.bytes_done > one_file * 2);
    assert!(
        crossed_two_files,
        "expected at least one Copying event past the second inner-file boundary ({}), got {:?}",
        one_file * 2,
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // File bar: files_done advances per inner file. By the time the third
    // inner file streams, two inner files are done. With the bug, files_done
    // is pinned at 0 for the whole directory.
    let saw_files_done_2 = copying.iter().any(|p| p.files_done >= 2);
    assert!(
        saw_files_done_2,
        "expected at least one Copying event with files_done >= 2 (inner files complete), got {:?}",
        copying.iter().map(|e| e.files_done).collect::<Vec<_>>(),
    );
    drop(progress);

    // Cumulative correctness is pinned by the complete event.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].bytes_processed, total_bytes);
}

/// Concurrent cross-volume copy of several large files emits multiple
/// `Copying`-phase progress events as chunks stream through across
/// in-flight tasks. Pins the contract before the per-task progress
/// closure gets extracted into a shared helper.
///
/// `source_paths.len() >= 3` AND `InMemoryVolume::max_concurrent_ops()`
/// returning 32 force `use_concurrent_path = true` (see `volume/copy.rs`
/// § `use_concurrent_path` selection), so this exercises the per-task
/// `on_file_progress` site that the helper must continue to satisfy.
///
/// Cross-task interleaving means per-event monotonicity / "last event
/// equals the total" don't hold — two tasks can fetch_add then emit in
/// either order, so the receipt order can carry a smaller tail value.
/// The complete event covers the cumulative side; here we only pin
/// "intra-file progress flows" and "the bytes_done axis crossed at
/// least one mid-transfer value."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cross_volume_copy_concurrent_emits_intra_file_progress() {
    let (source, dest) = make_volumes();
    let payload: Vec<u8> = vec![0u8; 524_288]; // 512 KB × 5 sources = 2.5 MB
    for i in 0..5 {
        source
            .create_file(Path::new(&format!("/big_{}.bin", i)), &payload)
            .await
            .unwrap();
    }
    let total_bytes = (payload.len() * 5) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let sources: Vec<PathBuf> = (0..5).map(|i| PathBuf::from(format!("/big_{}.bin", i))).collect();
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-copy-concurrent-intra",
        &state,
        Arc::clone(&source),
        &sources,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    assert!(
        copying.len() >= 3,
        "concurrent copy: expected multiple Copying events across 5 × 512 KB transfers, got {} ({:?})",
        copying.len(),
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // At least one intra-transfer event must show a strict mid-flight
    // value: > 0 (the bulk-skip / initial-Copying emit shows 0) and
    // strictly less than total (a true mid-transfer sample, not the
    // post-completion final).
    let saw_mid_flight = copying.iter().any(|p| p.bytes_done > 0 && p.bytes_done < total_bytes);
    assert!(
        saw_mid_flight,
        "expected at least one mid-flight Copying event (0 < bytes_done < {}), got {:?}",
        total_bytes,
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // Cumulative correctness is pinned by the complete event.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].bytes_processed, total_bytes);
    assert_eq!(complete[0].files_processed, 5);
}

/// Serial cross-volume copy must emit at least one `Copying`-phase event
/// with `files_done == N` (the full count) — the per-file milestone the
/// FE's files-axis bar needs to reach `N/N` before the operation ends.
///
/// The chunked `on_progress` emits all carry `files_done_so_far` (the
/// driver's iteration snapshot, taken before this file started), so for
/// a single-file op the chunked emits show `files = 0` throughout. Only
/// a per-file milestone emit (after `Transferred`) can bump the axis to
/// `1/1` in a `Copying` event. Pre-fix, no such emit existed — the user
/// saw "Copying... 99% / 0 of 1 files" then the dialog vanished on the
/// complete event without ever showing "1 of 1."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cross_volume_copy_serial_reaches_files_done_n() {
    let (source, dest) = make_volumes();
    let payload: Vec<u8> = vec![0u8; 1_048_576];
    source.create_file(Path::new("/big.bin"), &payload).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-copy-serial-files-n",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/big.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    let saw_files_done_n = copying.iter().any(|p| p.files_done == 1);
    assert!(
        saw_files_done_n,
        "serial copy: expected at least one Copying event with files_done = 1, got {:?}",
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // The "files_done = N" event should also carry bytes_done = total
    // (it's the per-file milestone, not a partial intra-file emit).
    let milestone = copying
        .iter()
        .find(|p| p.files_done == 1)
        .expect("at least one Copying event with files_done = 1");
    assert_eq!(milestone.bytes_done, payload.len() as u64);
}

/// Concurrent cross-volume copy must emit at least one `Copying`-phase
/// event with `files_done == N` (the full count). The concurrent path's
/// chunked emit reads `files_done_atomic.load()`, but each task's
/// `on_file_complete` only increments AFTER the file's last chunk fired
/// its callback; without a per-file milestone emit, the axis ratchets
/// up to `N-1` and stops (the last increment has no event behind it).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cross_volume_copy_concurrent_reaches_files_done_n() {
    let (source, dest) = make_volumes();
    let payload: Vec<u8> = vec![0u8; 524_288];
    for i in 0..5 {
        source
            .create_file(Path::new(&format!("/big_{}.bin", i)), &payload)
            .await
            .unwrap();
    }

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let sources: Vec<PathBuf> = (0..5).map(|i| PathBuf::from(format!("/big_{}.bin", i))).collect();
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-copy-concurrent-files-n",
        &state,
        Arc::clone(&source),
        &sources,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    let saw_files_done_n = copying.iter().any(|p| p.files_done == 5);
    assert!(
        saw_files_done_n,
        "concurrent copy: expected at least one Copying event with files_done = 5, got {:?}",
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
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
    // Custom event sink that flips intent to Stopped after a handful of
    // `Copying` progress events. Counting EVENTS (which fire per chunk, early in
    // the batch) rather than completed FILES makes the cancel land while tasks
    // are still mid-stream — robust regardless of scheduler interleaving. The
    // concurrent streaming path parks/checks between chunks, so the in-flight
    // tasks break at their next checkpoint and not all sources finish.
    struct CancelAfterNSink {
        inner: CollectorEventSink,
        intent: Arc<AtomicU8>,
        cancel_after_events: usize,
        events_seen: AtomicUsize,
    }

    impl OperationEventSink for CancelAfterNSink {
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
        fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
            self.inner.emit_conflict_resolved(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    }

    let (source, dest) = make_volumes();
    // Files large enough (many 64 KB chunks) that the three not-yet-complete
    // in-flight tasks reliably observe the cancel at a between-chunk checkpoint
    // before finishing. With tiny files the whole batch can complete inside one
    // scheduler turn, making "not all 5 land" a coin flip.
    for i in 1..=5 {
        source
            .create_file(Path::new(&format!("/{}.bin", i)), &vec![0; 2_000_000])
            .await
            .unwrap();
    }

    let state = make_state();
    let events = Arc::new(CancelAfterNSink {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        cancel_after_events: 3,
        events_seen: AtomicUsize::new(0),
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

    // A mid-flight cancel leaves the batch partially done: fewer than all 5
    // sources land. Completion order under concurrency isn't deterministic, so
    // assert on the COUNT, not on specific names.
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

/// File→folder overwrite (volume copy): source is a file, dest holds a folder
/// at the same path. Picking Overwrite must delete the dest folder (recursively)
/// before the streaming writer lands the source file, otherwise the writer
/// fails or no-ops because the path isn't writable as a file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_volume_overwrite_file_over_existing_folder() {
    let (source, dest) = make_volumes();

    source
        .create_file(Path::new("/clash"), b"I am the new file")
        .await
        .unwrap();
    // Dest is a folder with children at the same path
    dest.create_directory(Path::new("/clash")).await.unwrap();
    dest.create_file(Path::new("/clash/inner.txt"), b"inner").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-file-over-folder",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);
    // Old folder + its child gone, replaced by the source's file bytes
    assert!(
        !dest.is_directory(Path::new("/clash")).await.unwrap_or(false),
        "dest should no longer be a directory"
    );
    let mut stream = dest.open_read_stream(Path::new("/clash")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"I am the new file");
    assert!(!dest.exists(Path::new("/clash/inner.txt")).await);
}

/// Folder→file overwrite (volume copy): source is a folder, dest is a file at
/// the same path. Overwrite must delete the dest file before the recursive
/// copy creates the directory tree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_volume_overwrite_folder_over_existing_file() {
    let (source, dest) = make_volumes();

    source.create_directory(Path::new("/clash")).await.unwrap();
    source
        .create_file(Path::new("/clash/inside.txt"), b"inside content")
        .await
        .unwrap();
    // Dest is a file at the same top-level path
    dest.create_file(Path::new("/clash"), b"i am the old file")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-folder-over-file",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);
    // Dest is now a directory containing the source's file
    assert!(
        dest.is_directory(Path::new("/clash")).await.unwrap_or(false),
        "dest should now be a directory"
    );
    let mut stream = dest.open_read_stream(Path::new("/clash/inside.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"inside content");
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
    let on_progress = move |p: ListingProgress| {
        calls_for_cb.lock().unwrap().push(p.files);
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

// ========================================================================
// Volume-aware destination auto-create (recursive `create_directory_all`).
//
// A cross-volume copy into a not-yet-existing nested destination folder
// creates the folder (and any missing ancestors) on the dest volume, then
// lands the files. Parity with the local-FS `ensure_destination_dir`.
// ========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_copy_creates_missing_nested_dest() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

    // `/incoming/2026/trip` does not exist on the dest volume yet.
    assert!(!dest.exists(Path::new("/incoming")).await);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-mkdir-copy",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/incoming/2026/trip"),
        &config,
    )
    .await;

    assert!(
        result.is_ok(),
        "copy into a missing nested dest should succeed: {:?}",
        result
    );

    // Every ancestor was created as a directory.
    for dir in ["/incoming", "/incoming/2026", "/incoming/2026/trip"] {
        assert!(
            dest.is_directory(Path::new(dir)).await.expect("ancestor statable"),
            "{dir} should be a directory"
        );
    }

    // Both files landed in the freshly-created dest.
    let mut stream_a = dest
        .open_read_stream(Path::new("/incoming/2026/trip/a.txt"))
        .await
        .unwrap();
    assert_eq!(stream_a.next_chunk().await.unwrap().unwrap(), b"alpha");
    let mut stream_b = dest
        .open_read_stream(Path::new("/incoming/2026/trip/b.txt"))
        .await
        .unwrap();
    assert_eq!(stream_b.next_chunk().await.unwrap().unwrap(), b"bravo");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_copy_into_existing_dest_is_a_no_op_create() {
    // Re-running into an already-existing dest must not fail the create gate
    // (a merge into an existing folder is a no-op create).
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    dest.create_directory(Path::new("/existing")).await.unwrap();
    dest.create_file(Path::new("/existing/keep.txt"), b"keep")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-mkdir-copy-existing",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt")],
        Arc::clone(&dest),
        Path::new("/existing"),
        &config,
    )
    .await;

    assert!(
        result.is_ok(),
        "copy into an existing dest should succeed: {:?}",
        result
    );
    // The pre-existing dest file survived (no wholesale recreate).
    assert!(dest.exists(Path::new("/existing/keep.txt")).await);
    assert!(dest.exists(Path::new("/existing/a.txt")).await);
}
