//! Unit tests for `move_with_staging` (cross-FS local move).
//!
//! Drives the function directly with a `CollectorEventSink` + tempdir. Same-FS
//! moves go through `move_with_rename` (instant `fs::rename`); the staging
//! path is only reached when source and destination live on different
//! filesystems. Tests call `move_with_staging` directly to exercise that path
//! without needing two real mount points.

use super::*;
use crate::file_system::write_operations::types::CollectorEventSink;

fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(std::time::Duration::from_millis(
        progress_interval_ms,
    )))
}

/// Cross-FS local move of a single file must emit at least one `Copying`-
/// phase progress event with `files_done == N`. `move_with_staging` doesn't
/// use the transfer driver — it has its own copy loop calling
/// `copy_single_item` per file. The per-file milestone has to live inside
/// `copy_single_item` so both this loop and the driver-driven loop see it.
///
/// Uses `progress_interval_ms: 200` (production default) to keep the
/// throttle window active. Pre-fix the test reliably sees zero Copying
/// events with `files_done = 1` (the chunked progress callback absorbs the
/// throttle, the milestone is missing); post-fix `copy_single_item` fires
/// the milestone unconditionally so the assertion holds.
#[test]
fn cross_fs_local_move_single_file_reaches_files_done_n() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 1_048_576]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-files-n",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Move semantics: source gone, dest has the file.
    assert!(!src_file.exists(), "source should be removed after move");
    let dst_file = dst_dir.join("file.bin");
    assert!(dst_file.exists(), "destination should hold the moved file");

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    let saw_files_done_n = copying.iter().any(|p| p.files_done == 1);
    assert!(
        saw_files_done_n,
        "cross-FS local move: expected at least one Copying event with files_done = 1, got {:?}",
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // The milestone event accounts for the full file.
    let milestone = copying
        .iter()
        .find(|p| p.files_done == 1)
        .expect("at least one Copying event with files_done = 1");
    assert_eq!(milestone.bytes_done, 1_048_576);

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 1);
    assert_eq!(complete[0].bytes_processed, 1_048_576);
}
