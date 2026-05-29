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

/// A cross-FS local move must emit a `Flushing`-phase progress event before
/// `write-complete`. The staging copy lands real bytes that must be durable
/// before we delete the sources — on a move, a non-durable "complete" is
/// data loss (gone from source, not yet on disk at dest). The Flushing event
/// is the user-visible "Writing the last piece…" state and the observable
/// proxy for the end-of-op `fdatasync` over the moved destinations.
#[test]
fn cross_fs_local_move_emits_flushing_phase_before_complete() {
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

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-flushing",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert!(!src_file.exists(), "source should be removed after move");
    let dst_file = dst_dir.join("file.bin");
    assert!(dst_file.exists(), "destination should hold the moved file");

    let progress = events.progress.lock().unwrap();
    let saw_flushing = progress.iter().any(|p| p.phase == WriteOperationPhase::Flushing);
    assert!(
        saw_flushing,
        "cross-FS move: expected a Flushing-phase progress event, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "exactly one write-complete");
}

/// CRITICAL data-loss regression. A cross-FS move of a single file onto an
/// existing same-named destination, resolved as Skip, must leave the user's
/// ORIGINAL file intact at the source and the existing destination unchanged.
///
/// Pre-fix, Phase 3 discarded the staged copy on Skip and `continue`d, but
/// Phase 4 (`delete_sources_after_move`) iterated the FULL `sources` list and
/// unconditionally unlinked every original — including the skipped one. The
/// user clicked Skip to keep both files and lost their only original. This
/// mirrors the same-FS path (`move_with_rename`), where Skip just `continue`s
/// without touching the source.
#[test]
fn cross_fs_move_skip_preserves_source_and_dest() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("report.pdf");
    fs::write(&src_file, b"my only original").unwrap();
    // Pre-existing destination with the same name => collision.
    let dst_file = dst_dir.join("report.pdf");
    fs::write(&dst_file, b"pre-existing dest").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: crate::file_system::write_operations::types::ConflictResolution::Skip,
        ..WriteOperationConfig::default()
    };

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-skip-file",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // The source original MUST survive — Skip means "keep both."
    assert!(
        src_file.exists(),
        "Skip on a cross-FS move must NOT delete the source original (data loss)"
    );
    assert_eq!(fs::read(&src_file).unwrap(), b"my only original");

    // The pre-existing destination MUST be untouched.
    assert!(dst_file.exists(), "pre-existing destination must remain");
    assert_eq!(fs::read(&dst_file).unwrap(), b"pre-existing dest");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_skipped, 1, "the file was skipped");
}

/// CRITICAL data-loss regression, directory-merge variant. A cross-FS move of
/// a source directory whose one child collides with an existing dest child
/// (resolved Skip) and whose other child is new: the non-colliding child must
/// move, the skipped child's original must survive at the source, and the
/// source directory must NOT be removed wholesale (it still holds the skipped
/// child).
///
/// Pre-fix, Phase 4's `fs::remove_dir_all(source)` deleted the WHOLE source
/// directory, including the child that was skipped (and thus never landed at
/// the destination). That's silent data loss for the skipped child.
#[test]
fn cross_fs_move_dir_merge_skip_child_preserves_source_child() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    // Source dir with two children.
    let src_dir = src_root.join("photos");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("keep.jpg"), b"source keep").unwrap();
    fs::write(src_dir.join("collide.jpg"), b"source collide").unwrap();

    // Pre-existing dest dir with one colliding child.
    let dst_dir = dst_root.join("photos");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(dst_dir.join("collide.jpg"), b"dest collide").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: crate::file_system::write_operations::types::ConflictResolution::Skip,
        ..WriteOperationConfig::default()
    };

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-dir-skip-child",
        &state,
        std::slice::from_ref(&src_dir),
        &dst_root,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Non-colliding child moved to dest.
    assert!(
        dst_dir.join("keep.jpg").exists(),
        "non-colliding child should have moved to the destination"
    );
    assert_eq!(fs::read(dst_dir.join("keep.jpg")).unwrap(), b"source keep");
    // And is gone from the source.
    assert!(
        !src_dir.join("keep.jpg").exists(),
        "moved child should no longer be at the source"
    );

    // Colliding dest child untouched.
    assert_eq!(
        fs::read(dst_dir.join("collide.jpg")).unwrap(),
        b"dest collide",
        "Skip must leave the pre-existing dest child unchanged"
    );

    // CRITICAL: the skipped child's ORIGINAL must survive at the source, and the
    // source directory must NOT have been removed wholesale.
    assert!(
        src_dir.exists(),
        "source dir must remain — it still holds the skipped child"
    );
    assert!(
        src_dir.join("collide.jpg").exists(),
        "skipped child's original must survive at the source (data loss otherwise)"
    );
    assert_eq!(fs::read(src_dir.join("collide.jpg")).unwrap(), b"source collide");
}
