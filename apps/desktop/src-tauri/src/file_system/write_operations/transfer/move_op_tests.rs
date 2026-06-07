//! Unit tests for `move_with_staging` (cross-FS local move).
//!
//! Drives the function directly with a `CollectorEventSink` + tempdir. Same-FS
//! moves go through `move_with_rename` (instant `fs::rename`); the staging
//! path is only reached when source and destination live on different
//! filesystems. Tests call `move_with_staging` directly to exercise that path
//! without needing two real mount points.

use super::*;
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};

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

/// CRITICAL ordering invariant. The final destination's dir entry must be
/// fsynced (the `Flushing` pass) BEFORE the source originals are deleted.
/// The source is the only other copy of the data; deleting it before the
/// rename-into-place is durable widens the crash window (file absent from its
/// final path AND source already gone on power loss). This sink snapshots
/// whether the source still exists at the instant the `Flushing`-phase event
/// fires. Pre-reorder the flush ran AFTER Phase 4's delete, so the source
/// would already be gone here; post-reorder it must still exist.
#[test]
fn cross_fs_local_move_flushes_final_dests_before_deleting_sources() {
    use crate::file_system::write_operations::types::{
        ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
        WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Records the source's existence the first time a `Flushing`-phase event
    /// is emitted. `saw_flushing` confirms the observation actually happened.
    struct FlushOrderSink {
        source: PathBuf,
        source_existed_at_flush: AtomicBool,
        saw_flushing: AtomicBool,
    }

    impl OperationEventSink for FlushOrderSink {
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Flushing && !self.saw_flushing.swap(true, Ordering::SeqCst) {
                self.source_existed_at_flush
                    .store(self.source.exists(), Ordering::SeqCst);
            }
        }
        fn emit_complete(&self, _event: WriteCompleteEvent) {}
        fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
        fn emit_error(&self, _event: WriteErrorEvent) {}
        fn emit_conflict(&self, _event: WriteConflictEvent) {}
        fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _conflict: ConflictInfo) {}
        fn emit_dry_run_complete(&self, _result: DryRunResult) {}
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = Arc::new(FlushOrderSink {
        source: src_file.clone(),
        source_existed_at_flush: AtomicBool::new(false),
        saw_flushing: AtomicBool::new(false),
    });
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-flush-before-delete",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert!(
        events.saw_flushing.load(Ordering::SeqCst),
        "expected a Flushing-phase event to observe ordering against"
    );
    assert!(
        events.source_existed_at_flush.load(Ordering::SeqCst),
        "source must still exist when the Flushing pass runs — the durable dir-entry fsync must precede the source delete"
    );
    // Sanity: the move still completed (source gone, dest present).
    assert!(!src_file.exists(), "source should be removed after move");
    assert!(
        dst_dir.join("file.bin").exists(),
        "destination should hold the moved file"
    );
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
        conflict_resolution: ConflictResolution::Skip,
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
        conflict_resolution: ConflictResolution::Skip,
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

// ============================================================================
// Local move conflict × resolution matrix sweep
// ============================================================================
//
// The systematic version of the Bug-1 regression. For every local move kind
// (same-FS via `move_with_rename`, cross-FS via `move_with_staging`) crossed
// with every conflict resolution, assert the core data-safety invariant:
//
//   the source survives IFF it was NOT actually moved to the destination.
//
//   - Skip / Skip-equivalent (conditional that doesn't meet its condition)
//     => nothing landed => source survives, dest unchanged.
//   - Overwrite that landed => dest holds the source's content, source deleted.
//   - Rename that landed => both the original dest and the renamed incoming
//     survive at the dest, source deleted.
//
// Single-file and dir-with-one-skipped-child shapes both appear. The volume
// move axis lives in `volume_move_tests.rs` (InMemoryVolume harness). Cells we
// cannot exercise in a tempdir (two real filesystems for a genuine cross-FS
// move) are simulated by calling `move_with_staging` directly, the same seam
// the existing cross-FS tests use.

/// Drives a same-FS local move (`move_with_rename`) of `sources` into `dst_dir`
/// under the given resolution. Within one tempdir, source and dest share a
/// filesystem, so `move_files_with_progress_inner` routes to the rename path.
fn run_same_fs_move(
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: resolution,
        ..WriteOperationConfig::default()
    };
    move_files_with_progress_inner(&*events, op_id, &state, sources, dst_dir, &config)?;
    Ok(events)
}

/// Drives a cross-FS local move (`move_with_staging`) of `sources` into
/// `dst_dir` under the given resolution.
fn run_cross_fs_move(
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: resolution,
        ..WriteOperationConfig::default()
    };
    move_with_staging(&*events, op_id, &state, sources, dst_dir, &config)?;
    Ok(events)
}

/// One single-file matrix cell: a source file collides with a same-named dest
/// file, resolved by `resolution`. Asserts the survives-iff-not-moved
/// invariant for the given expected outcome.
struct SingleFileCell {
    resolution: ConflictResolution,
    /// `None` => Skip-equivalent (source survives, dest unchanged).
    /// `Some(true)` => Overwrite-equivalent (dest := source content, source gone).
    /// `Some(false)` => Rename-equivalent (dest kept + renamed incoming, source gone).
    landed_as_overwrite: Option<bool>,
}

fn assert_single_file_cell(
    cross_fs: bool,
    cell: &SingleFileCell,
    src_bytes: &[u8],
    dst_bytes: &[u8],
    src_mtime_newer: bool,
) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("f.bin");
    fs::write(&src_file, src_bytes).unwrap();
    let dst_file = dst_dir.join("f.bin");
    fs::write(&dst_file, dst_bytes).unwrap();

    // Force a deterministic mtime ordering for the conditional cells.
    set_mtimes(&src_file, &dst_file, src_mtime_newer);

    let op_id = format!(
        "matrix-single-{}-{:?}",
        if cross_fs { "xfs" } else { "samefs" },
        cell.resolution
    );
    let result = if cross_fs {
        run_cross_fs_move(std::slice::from_ref(&src_file), &dst_dir, cell.resolution, &op_id)
    } else {
        run_same_fs_move(std::slice::from_ref(&src_file), &dst_dir, cell.resolution, &op_id)
    };
    assert!(result.is_ok(), "{:?} on {}: {:?}", cell.resolution, op_id, result.err());

    match cell.landed_as_overwrite {
        None => {
            // Skip-equivalent: source survives, dest unchanged.
            assert!(src_file.exists(), "{op_id}: Skip-equivalent must preserve the source");
            assert_eq!(
                fs::read(&src_file).unwrap(),
                src_bytes,
                "{op_id}: source content intact"
            );
            assert_eq!(
                fs::read(&dst_file).unwrap(),
                dst_bytes,
                "{op_id}: dest must be unchanged"
            );
        }
        Some(true) => {
            // Overwrite: source moved, dest now holds the source's content.
            assert!(
                !src_file.exists(),
                "{op_id}: Overwrite that landed must delete the source"
            );
            assert_eq!(
                fs::read(&dst_file).unwrap(),
                src_bytes,
                "{op_id}: dest := source content"
            );
        }
        Some(false) => {
            // Rename: source moved, original dest kept, renamed incoming present.
            assert!(!src_file.exists(), "{op_id}: Rename that landed must delete the source");
            assert_eq!(fs::read(&dst_file).unwrap(), dst_bytes, "{op_id}: original dest kept");
            let renamed = dst_dir.join("f (1).bin");
            assert!(renamed.exists(), "{op_id}: renamed incoming must exist");
            assert_eq!(
                fs::read(&renamed).unwrap(),
                src_bytes,
                "{op_id}: renamed incoming has source bytes"
            );
        }
    }
}

/// Sets mtimes so the strict-comparison conditional cells are deterministic.
fn set_mtimes(src: &Path, dst: &Path, src_newer: bool) {
    let older = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    let newer = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    if src_newer {
        filetime::set_file_mtime(src, newer).unwrap();
        filetime::set_file_mtime(dst, older).unwrap();
    } else {
        filetime::set_file_mtime(src, older).unwrap();
        filetime::set_file_mtime(dst, newer).unwrap();
    }
}

#[test]
fn matrix_single_file_skip_preserves_source_both_move_kinds() {
    let cell = SingleFileCell {
        resolution: ConflictResolution::Skip,
        landed_as_overwrite: None,
    };
    assert_single_file_cell(false, &cell, b"SRC", b"DST", true);
    assert_single_file_cell(true, &cell, b"SRC", b"DST", true);
}

#[test]
fn matrix_single_file_overwrite_replaces_dest_both_move_kinds() {
    let cell = SingleFileCell {
        resolution: ConflictResolution::Overwrite,
        landed_as_overwrite: Some(true),
    };
    assert_single_file_cell(false, &cell, b"SRC-NEW", b"DST-OLD", true);
    assert_single_file_cell(true, &cell, b"SRC-NEW", b"DST-OLD", true);
}

#[test]
fn matrix_single_file_rename_keeps_both_both_move_kinds() {
    let cell = SingleFileCell {
        resolution: ConflictResolution::Rename,
        landed_as_overwrite: Some(false),
    };
    assert_single_file_cell(false, &cell, b"SRC", b"DST", true);
    assert_single_file_cell(true, &cell, b"SRC", b"DST", true);
}

#[test]
fn matrix_single_file_overwrite_smaller_strict_semantics() {
    // dest strictly smaller than source => overwrite lands.
    let landed = SingleFileCell {
        resolution: ConflictResolution::OverwriteSmaller,
        landed_as_overwrite: Some(true),
    };
    assert_single_file_cell(false, &landed, b"source-is-bigger", b"dst", true);
    assert_single_file_cell(true, &landed, b"source-is-bigger", b"dst", true);

    // dest NOT strictly smaller (equal size) => Skip-equivalent, source survives.
    let skipped = SingleFileCell {
        resolution: ConflictResolution::OverwriteSmaller,
        landed_as_overwrite: None,
    };
    assert_single_file_cell(false, &skipped, b"AAAA", b"BBBB", true);
    assert_single_file_cell(true, &skipped, b"AAAA", b"BBBB", true);
}

#[test]
fn matrix_single_file_overwrite_older_strict_semantics() {
    // dest strictly older than source => overwrite lands (src_newer = true).
    let landed = SingleFileCell {
        resolution: ConflictResolution::OverwriteOlder,
        landed_as_overwrite: Some(true),
    };
    assert_single_file_cell(false, &landed, b"SRC", b"DST", true);
    assert_single_file_cell(true, &landed, b"SRC", b"DST", true);

    // dest newer than source => Skip-equivalent, source survives (src_newer = false).
    let skipped = SingleFileCell {
        resolution: ConflictResolution::OverwriteOlder,
        landed_as_overwrite: None,
    };
    assert_single_file_cell(false, &skipped, b"SRC", b"DST", false);
    assert_single_file_cell(true, &skipped, b"SRC", b"DST", false);
}

/// Directory-with-one-skipped-child cell for the conditional policies: the
/// colliding child does NOT meet the condition (Skip-equivalent), the other
/// child is new. The skipped child's source must survive; the new child moves.
/// This is the conditional sibling of `cross_fs_move_dir_merge_skip_child_*`.
fn assert_dir_one_skipped_child(cross_fs: bool, resolution: ConflictResolution, src_newer_for_collide: bool) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let src_dir = src_root.join("d");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("keep.bin"), b"new child").unwrap();
    fs::write(src_dir.join("collide.bin"), b"AAAA").unwrap();

    let dst_dir = dst_root.join("d");
    fs::create_dir_all(&dst_dir).unwrap();
    // Equal size keeps OverwriteSmaller Skip-equivalent; mtime drives OverwriteOlder.
    fs::write(dst_dir.join("collide.bin"), b"BBBB").unwrap();
    set_mtimes(
        &src_dir.join("collide.bin"),
        &dst_dir.join("collide.bin"),
        src_newer_for_collide,
    );

    let op_id = format!(
        "matrix-dir-{}-{:?}",
        if cross_fs { "xfs" } else { "samefs" },
        resolution
    );
    let result = if cross_fs {
        run_cross_fs_move(std::slice::from_ref(&src_dir), &dst_root, resolution, &op_id)
    } else {
        run_same_fs_move(std::slice::from_ref(&src_dir), &dst_root, resolution, &op_id)
    };
    assert!(result.is_ok(), "{op_id}: {:?}", result.err());

    // The non-colliding child moved.
    assert!(
        dst_dir.join("keep.bin").exists(),
        "{op_id}: new child should land at dest"
    );
    assert!(
        !src_dir.join("keep.bin").exists(),
        "{op_id}: new child should leave the source"
    );

    // The colliding child is Skip-equivalent: dest unchanged, source survives.
    assert_eq!(
        fs::read(dst_dir.join("collide.bin")).unwrap(),
        b"BBBB",
        "{op_id}: Skip-equivalent leaves the dest child unchanged"
    );
    assert!(
        src_dir.exists() && src_dir.join("collide.bin").exists(),
        "{op_id}: the skipped child's source must survive (data loss otherwise)"
    );
    assert_eq!(fs::read(src_dir.join("collide.bin")).unwrap(), b"AAAA");
}

#[test]
fn matrix_dir_overwrite_smaller_skips_equal_child_preserves_source() {
    // Equal-size collide child => not strictly smaller => Skip-equivalent.
    assert_dir_one_skipped_child(false, ConflictResolution::OverwriteSmaller, true);
    assert_dir_one_skipped_child(true, ConflictResolution::OverwriteSmaller, true);
}

#[test]
fn matrix_dir_overwrite_older_skips_newer_child_preserves_source() {
    // Collide child where source is NOT newer (dest newer) => not strictly
    // older => Skip-equivalent.
    assert_dir_one_skipped_child(false, ConflictResolution::OverwriteOlder, false);
    assert_dir_one_skipped_child(true, ConflictResolution::OverwriteOlder, false);
}

/// A cross-FS move of a tree containing an EMPTY directory must land that
/// directory at the destination — and, critically, must not destroy it. The
/// staging copy iterates `scan_result.files` only, so an empty dir never
/// staged, never renamed into place, and then Phase 4 deleted the source:
/// the directory vanished entirely. That's silent data loss, the worst case
/// of the empty-dir hole (the copy sibling merely failed to create it).
#[test]
fn cross_fs_move_preserves_empty_directories() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("tree/populated")).unwrap();
    fs::create_dir_all(src_dir.join("tree/sub-empty")).unwrap();
    fs::write(src_dir.join("tree/populated/file.txt"), b"content").unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let source = src_dir.join("tree");
    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-empty-dir",
        &state,
        std::slice::from_ref(&source),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert!(
        dst_dir.join("tree/sub-empty").is_dir(),
        "the empty directory must arrive at the destination"
    );
    assert!(
        dst_dir.join("tree/populated/file.txt").is_file(),
        "the regular file must arrive at the destination"
    );
    assert!(!source.exists(), "the source tree should be removed after the move");
}
