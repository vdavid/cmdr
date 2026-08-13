//! What the move paths report while they run: byte totals, scan-phase tallies,
//! intra-file progress, and leaf-granular progress inside a folder.
//!
//! Every test here guards a bar the user watches. `bytes_total` has to reach
//! every Copying event or the FE hides the Size bar entirely; the Scanning
//! phase has to emit climbing tallies or a slow source sits on "0 bytes / 0
//! files" for its whole walk; a single large file has to stream progress rather
//! than jump at the end; and a directory source has to account its inner files
//! against a live aggregate, not a frozen snapshot.
//!
//! Shared fixtures live in `volume/move_test_support.rs`
//! (`super::test_support`).

use super::super::move_same::move_within_same_volume_with_progress;
use super::test_support::{make_state_with_interval_ms, make_volumes};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::types::{CollectorEventSink, TransferWaitReason};

/// Cross-volume move emits `bytes_total > 0` on every Copying-phase progress
/// event. Without this, the FE's `TransferProgressDialog` hides the Size
/// progress bar (the dialog gates it behind `{#if bytesTotal > 0}`), so the
/// user only saw the Files bar during MTP→local moves. The shared preflight
/// scan now feeds the real total into the driver and every per-source emit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_emits_bytes_total_on_progress() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo-bravo").await.unwrap();
    let expected_total = (b"alpha".len() + b"bravo-bravo".len()) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-bytes-total",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
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
    assert!(!copying.is_empty(), "expected at least one Copying progress event");
    for ev in &copying {
        assert_eq!(
            ev.bytes_total, expected_total,
            "every Copying progress event must carry the real bytes_total (got {} for files_done={})",
            ev.bytes_total, ev.files_done,
        );
    }

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].bytes_processed, expected_total);
}

/// A same-volume move is a rename — it transfers zero bytes — so every
/// Copying-phase progress event carries `bytes_total == 0`. The FE hides the
/// Size bar on `bytes_total == 0`, which is honest: a rename moves no bytes, so
/// showing a Size bar would be a lie. `files_total` is the count of top-level
/// items, not a recursive file count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_reports_zero_bytes_total() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("V").with_space_info(10_000_000, 10_000_000));
    volume.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    volume.create_file(Path::new("/b.txt"), b"bravo-bravo").await.unwrap();
    volume.create_directory(Path::new("/dst")).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-same-move-bytes-total",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Path::new("/dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    assert!(!copying.is_empty(), "expected at least one Copying progress event");
    for ev in &copying {
        assert_eq!(
            ev.bytes_total, 0,
            "a rename moves zero bytes, so every Copying progress event must carry bytes_total = 0 (got {} for files_done={})",
            ev.bytes_total, ev.files_done,
        );
        assert_eq!(ev.files_total, 2, "files_total is the top-level item count");
    }

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 2);
    assert_eq!(complete[0].bytes_processed, 0);
}

/// Cross-volume move (no `preview_id`) emits multiple `Scanning`-phase
/// progress events with climbing tallies as `scan_for_copy_batch_with_progress`
/// walks the source list, not just one frozen event at `0/0/0/0`. Without the
/// per-listing progress wiring, programmatic / MCP-triggered moves against a
/// slow source (cold MTP, large SMB tree) sit on "Scanning... 0 bytes / 0
/// files / 0 dirs" for the entire scan duration.
///
/// `InMemoryVolume` inherits the default `scan_for_copy_batch_with_progress`,
/// which fires `on_progress` once per top-level path. With 4 sources we
/// expect the kickoff emit plus at least one mid-scan event showing a partial
/// tally before the scan finishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_emits_scan_phase_tallies_during_walk() {
    let (source, dest) = make_volumes();
    let payload = vec![0u8; 4096];
    for i in 0..4 {
        source
            .create_file(Path::new(&format!("/a_{}.bin", i)), &payload)
            .await
            .unwrap();
    }
    let total_bytes = (payload.len() * 4) as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let sources: Vec<PathBuf> = (0..4).map(|i| PathBuf::from(format!("/a_{}.bin", i))).collect();
    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-scan-tally",
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
    let scanning: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Scanning)
        .collect();

    // Pre-fix: exactly one Scanning event (the kickoff emit at 0/0/0/0).
    // Post-fix: kickoff + one event per scanned top-level path.
    assert!(
        scanning.len() >= 2,
        "expected multiple Scanning events during a 4-source walk, got {} ({:?})",
        scanning.len(),
        scanning
            .iter()
            .map(|e| (e.files_done, e.bytes_done))
            .collect::<Vec<_>>(),
    );
    for w in scanning.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "scan bytes_done must be non-decreasing across Scanning events, got {} then {}",
            w[0].bytes_done,
            w[1].bytes_done,
        );
    }
    let last = scanning.last().expect("at least one Scanning event");
    assert_eq!(
        last.files_done, 4,
        "final Scanning event should tally all 4 source files"
    );
    assert_eq!(
        last.bytes_done, total_bytes,
        "final Scanning event should tally all source bytes"
    );
}

/// Cross-volume move of a single large file emits multiple `Copying`-phase
/// progress events as chunks stream through, not just one event after the
/// whole file lands. Without intra-file progress the FE's "Moving..." dialog
/// shows `0 bytes / 0 files / 0 dirs` for the entire upload — bug observed
/// against an SMB destination with a 3.7 GB file.
///
/// `InMemoryVolume` streams in 64 KB chunks (see
/// `volume/backends/in_memory.rs::CHUNK_SIZE`), so 1 MB ≈ 16 callback
/// invocations; with `progress_interval_ms: 0` the throttle is open and
/// every chunk emits. The `>= 3` floor is well above "one tail emit per
/// source" (current buggy floor: 1) and well below the ~16 events the
/// fix produces, so it's robust against scheduler jitter on busy CI.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_emits_intra_file_progress() {
    let (source, dest) = make_volumes();
    let payload: Vec<u8> = vec![0u8; 1_048_576];
    source.create_file(Path::new("/big.bin"), &payload).await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-intra-file",
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

    // Pre-fix: exactly one event (the post-source throttled emit at the end).
    // Post-fix: ~16 intra-file events plus the post-source tail emit.
    assert!(
        copying.len() >= 3,
        "expected multiple Copying events to stream during a 1 MB move, got {} ({:?})",
        copying.len(),
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );

    // bytes_done is non-decreasing as the stream advances.
    for w in copying.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "bytes_done must be non-decreasing across Copying events, got {} then {}",
            w[0].bytes_done,
            w[1].bytes_done,
        );
    }

    // Final Copying event accounts for the whole transfer.
    let last = copying.last().expect("at least one Copying event");
    assert_eq!(last.bytes_done, payload.len() as u64);
    assert_eq!(last.files_done, 1);
}

/// Cross-volume move of a single DIRECTORY source reports progress at
/// LEAF-file granularity. This is the exact shape of the reported bug: moving
/// a folder of large files from a USB stick to an SMB NAS showed the Size bar
/// resetting to 0 at every inner file and the File bar frozen at 0 the whole
/// time, because every inner file emitted against a frozen
/// `bytes_done_so_far = 0` / `files_done_so_far = 0` snapshot.
///
/// Twin of `volume::copy::tests::test_cross_volume_copy_directory_source_progress_is_leaf_granular`,
/// guarding the move path (`SerialLeafProgress` is shared, but the move wires
/// it separately and sets `emit_per_source_milestone: false`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_directory_source_progress_is_leaf_granular() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/folder")).await.unwrap();
    let one_mb: Vec<u8> = vec![0u8; 1_048_576];
    source.create_file(Path::new("/folder/a.bin"), &one_mb).await.unwrap();
    source.create_file(Path::new("/folder/b.bin"), &one_mb).await.unwrap();
    source.create_file(Path::new("/folder/c.bin"), &one_mb).await.unwrap();
    let one_file = one_mb.len() as u64;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-dir-leaf",
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

    // Size bar: bytes_done climbs monotonically across all three inner files.
    for w in copying.windows(2) {
        assert!(
            w[0].bytes_done <= w[1].bytes_done,
            "bytes_done must be non-decreasing across the folder's inner files, got {} then {} ({:?})",
            w[0].bytes_done,
            w[1].bytes_done,
            copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
        );
    }
    // The aggregate crosses both inner-file boundaries (never true under the bug).
    assert!(
        copying.iter().any(|p| p.bytes_done > one_file * 2),
        "expected a Copying event past the second inner-file boundary ({}), got {:?}",
        one_file * 2,
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // File bar: files_done advances per inner file (pinned at 0 under the bug).
    assert!(
        copying.iter().any(|p| p.files_done >= 2),
        "expected a Copying event with files_done >= 2, got {:?}",
        copying.iter().map(|e| e.files_done).collect::<Vec<_>>(),
    );
}

// ========================================================================
// The stall signal: a cross-volume move keeps an in-flight table like a copy
// ========================================================================

/// A cross-volume move must hand the dialog a `TransferActivity` on its
/// progress events, the same as a copy does.
///
/// That struct is the ENTIRE input to the stalled-transfer notice, the ETA's
/// decision to stop being confident, and the watchdog's heartbeat. Without a
/// registered probe, `state.rs::enrich_progress` misses the lookup and leaves
/// `activity` at `None`, so a wedged move shows a frozen bar with a confident
/// ETA and says nothing — a SILENT failure on the one operation that leaves the
/// user's only copy of their data mid-flight.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_tells_the_dialog_what_it_is_doing() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-activity",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt")],
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
    assert!(!copying.is_empty(), "expected at least one Copying progress event");
    assert!(
        copying
            .iter()
            .any(|p| p.activity.as_ref().is_some_and(|a| a.in_flight >= 1)),
        "every Copying event must carry activity, and one of them must show the source in flight; got {:?}",
        copying.iter().map(|e| e.activity).collect::<Vec<_>>(),
    );
    // The classifier ran and reached a verdict, rather than the dialog being
    // handed a populated-looking struct it can't read. A move that is streaming
    // bytes is `Moving`; anything else here would put a stall reason on screen.
    assert!(
        copying
            .iter()
            .filter_map(|p| p.activity.as_ref())
            .all(|a| a.waiting_on == TransferWaitReason::Moving),
        "a streaming move is moving, not waiting on anything; got {:?}",
        copying.iter().map(|e| e.activity).collect::<Vec<_>>(),
    );
}

/// The in-flight table must name what the move is doing, not just that
/// something is happening.
///
/// Sampled from inside the destination's write, the one window where the row
/// exists. `streaming` is the load-bearing half: the phase is recorded through
/// the `CURRENT_TASK_PROBE` task-local, so a transfer that registers a probe but
/// never binds the scope shows its row parked at `spawned` forever — the dump
/// explains nothing, `wait_reason` can never answer `Source` or `Destination`,
/// and `stream_pipe_file` can't arm a stall-abort. That is a strictly worse
/// failure than no probe at all, because it looks wired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_volume_move_in_flight_shows_its_source_and_phase() {
    let (source, dest) = make_volumes();
    source
        .create_file(Path::new("/holiday.mov"), b"a-few-bytes")
        .await
        .unwrap();

    let events = Arc::new(test_support::SampleInFlightTableSink::new("op-move-table"));
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-table",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/holiday.mov")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let table = events
        .in_flight_table()
        .expect("a running cross-volume move must keep an in-flight table");
    assert!(table.contains("in_flight=1/1"), "one source in flight: {table}");
    assert!(
        table.contains("streaming"),
        "the row must carry the phase the task-local records, not `spawned`: {table}"
    );
    assert!(table.contains("holiday.mov"), "the row must name the source: {table}");
}
