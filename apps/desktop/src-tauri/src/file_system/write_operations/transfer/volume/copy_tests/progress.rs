//! Multi-file copy execution via `copy_volumes_with_progress`: every file
//! arrives, and the progress stream (intra-file bytes, leaf-granular directory
//! sources, `files_done` reaching N) on both the serial and concurrent paths.

use super::*;

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
