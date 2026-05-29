//! Unit tests for `copy_files_with_progress_inner` (local-FS copy).
//!
//! Drives the sink-based inner function directly with a `CollectorEventSink`
//! against a real tempdir, the same shape `volume_copy_tests.rs` uses against
//! `InMemoryVolume`.

use super::*;
use crate::file_system::write_operations::types::CollectorEventSink;

fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)))
}

/// Local-FS copy of a single file must emit at least one `Copying`-phase
/// progress event with `files_done == N` (the full count). Without a per-
/// file milestone emit in the sync driver's `Transferred` arm, the
/// throttled emit inside `copy_single_item` is suppressed when the chunked
/// progress callback (or an instant clonefile) just reset the throttle —
/// for single-file ops the FE's files-axis never crosses `0/1` before the
/// dialog closes on the complete event.
///
/// Uses `progress_interval_ms: 200` (production default) to keep the
/// throttle window active. Pre-fix the test reliably saw zero Copying
/// events with `files_done = 1`; post-fix the driver milestone fires
/// unconditionally so the assertion holds regardless of throttle timing.
#[test]
fn local_copy_single_file_reaches_files_done_n() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    // 1 MB payload large enough to surface the throttle interaction on the
    // chunked-copy path; APFS clonefile completes instantly without firing
    // chunked progress, which is fine — the driver milestone still has to
    // land for the files-axis to cross `0/1`.
    fs::write(&src_file, vec![0u8; 1_048_576]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-files-n",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    let saw_files_done_n = copying.iter().any(|p| p.files_done == 1);
    assert!(
        saw_files_done_n,
        "local-FS copy: expected at least one Copying event with files_done = 1, got {:?}",
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );

    // Completion event accounts for the file.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 1);
    assert_eq!(complete[0].bytes_processed, 1_048_576);
}

/// A local-FS copy must emit a `Flushing`-phase progress event before the
/// `write-complete` fires. This is the user-visible "Writing the last piece…"
/// state: on slow media the end-of-op `fdatasync` over the created
/// destinations is a real multi-second pause, and the bar must not sit frozen
/// at 100% pretending the work is done. The event is the observable proxy for
/// the durability contract (the fsync itself isn't power-loss-testable in a
/// unit test).
#[test]
fn local_copy_emits_flushing_phase_before_complete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-flushing",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let saw_flushing = progress.iter().any(|p| p.phase == WriteOperationPhase::Flushing);
    assert!(
        saw_flushing,
        "local-FS copy: expected a Flushing-phase progress event, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );

    // The flush pass made the created destination durable; we can read it back.
    let dst_file = dst_dir.join("file.bin");
    assert!(dst_file.exists(), "destination should hold the copied file");
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "exactly one write-complete");
}
