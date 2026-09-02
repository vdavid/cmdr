//! Unit tests for `move_with_staging` (cross-FS local move).
//!
//! Drives the function directly with a `CollectorEventSink` + tempdir. Same-FS
//! moves go through `move_with_rename` (instant `fs::rename`); the staging
//! path is only reached when source and destination live on different
//! filesystems. Tests call `move_with_staging` directly to exercise that path
//! without needing two real mount points.

use super::cross_fs::move_with_staging;
use super::test_support::{make_state, run_cross_fs_move, run_same_fs_move};
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationPhase, WriteProgressEvent};
use crate::ignore_poison::IgnorePoison;

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
        0,
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
        0,
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

/// A same-FS move announces its closing flush too, so the FE's state machine
/// shows "Writing the last piece…" for both move kinds. What it flushes is the
/// directories the renames touched, not the moved files (`touched_directories`),
/// and this event is the observable proxy for that pass.
#[test]
fn same_fs_local_move_emits_flushing_phase_before_complete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = run_same_fs_move(
        std::slice::from_ref(&src_file),
        &dst_dir,
        ConflictResolution::Stop,
        "op-same-fs-move-flushing",
    )
    .expect("the move should land");

    assert!(!src_file.exists(), "the rename took the source with it");
    assert!(dst_dir.join("file.bin").exists(), "and the destination holds it");

    let progress = events.progress.lock().unwrap();
    assert!(
        progress.iter().any(|p| p.phase == WriteOperationPhase::Flushing),
        "same-FS move: expected a Flushing-phase progress event, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );
    assert_eq!(events.complete.lock().unwrap().len(), 1, "exactly one write-complete");
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
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
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
        fn emit_conflict_resolved(
            &self,
            _event: crate::file_system::write_operations::types::WriteConflictResolvedEvent,
        ) {
        }
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
        0,
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
        0,
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
        0,
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
        0,
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

// ============================================================================
// The preview cache is bound to the operation's own sources
// ============================================================================

/// A cross-FS local move asked to move `selected.bin` while the cache holds a
/// preview of `other.bin` must move `selected.bin` and leave `other.bin` where
/// it is, on both sides.
///
/// `move_with_staging` takes the file LIST from the cache but re-reads
/// `sources` for `create_scanned_dirs_at_destination` and again for Phase 3's
/// per-top-level staged rename. On a mismatched preview it stages the cached
/// tree and then looks in staging for the REQUESTED name, which isn't there —
/// a half-staged move, the worst shape available: bytes copied out of a file
/// nobody asked about, and a failure that has to unwind cleanly.
#[test]
fn a_local_move_never_acts_on_a_preview_of_a_different_selection() {
    use crate::file_system::volume::CopyScanResult;
    use crate::file_system::write_operations::state::{CachedScanResult, FileInfo, insert_scan_result};

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).expect("create src");
    fs::create_dir_all(&dst_dir).expect("create dst");

    let selected = src_dir.join("selected.bin");
    fs::write(&selected, b"the file the user picked").expect("write selected");
    let other = src_dir.join("other.bin");
    fs::write(&other, b"a file from an earlier scan").expect("write other");

    let preview_id = "move-binding-foreign-preview".to_string();
    let other_metadata = fs::symlink_metadata(&other).expect("other exists");
    insert_scan_result(
        preview_id.clone(),
        CachedScanResult::from_local_walk(
            vec![other.clone()],
            vec![FileInfo::new(other.clone(), src_dir.clone(), &other_metadata)],
            Vec::new(),
            other_metadata.len(),
            other_metadata.len(),
            vec![(
                other.clone(),
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: other_metadata.len(),
                    dedup_bytes: other_metadata.len(),
                    top_level_is_directory: false,
                },
            )],
            None,
        ),
    );

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = move_with_staging(
        &*events,
        "op-move-preview-binding",
        &state,
        std::slice::from_ref(&selected),
        &dst_dir,
        &config,
        0,
    );

    assert!(result.is_ok(), "the move must succeed: {result:?}");
    assert!(
        dst_dir.join("selected.bin").exists(),
        "the file the user selected must reach the destination"
    );
    assert!(!selected.exists(), "the moved source must be gone");
    assert!(
        other.exists(),
        "a preview of another file must never authorize moving it"
    );
    assert!(
        !dst_dir.join("other.bin").exists(),
        "a preview of another file must never put it at the destination"
    );
}

// ============================================================================
// `write-source-item-done`: `source_removed` is the vanished-path contract
// ============================================================================
//
// The frontend's search-snapshot purge acts on this flag alone
// (`apps/desktop/src/lib/search/snapshot-purge.ts`), so a `true` for a path that
// is still on disk drops a row for a file the user can still open. Inferring
// removal from the operation type is exactly what these cases break.

/// The outcomes this run reported for `path`, in emit order. The LAST one is the
/// operation's verdict on that source (`types::SourceItemOutcome`).
fn outcomes_for(events: &CollectorEventSink, path: &Path) -> Vec<SourceItemOutcome> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|e| e.source_path == path.display().to_string())
        .map(|e| e.outcome)
        .collect()
}

/// The `source_removed` flags this run reported for `path`, in emit order.
fn removal_flags_for(events: &CollectorEventSink, path: &Path) -> Vec<bool> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|e| e.source_path == path.display().to_string())
        .map(|e| e.source_removed)
        .collect()
}

#[test]
fn a_same_fs_move_that_took_the_whole_item_reports_it_removed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    let source = src_root.join("a.bin");
    fs::write(&source, b"content").unwrap();

    let events = run_same_fs_move(
        std::slice::from_ref(&source),
        &dst_root,
        ConflictResolution::Stop,
        "same-fs-removed",
    )
    .expect("the move must succeed");

    assert!(!source.exists(), "precondition: the rename took the source");
    assert_eq!(removal_flags_for(&events, &source), vec![true]);
}

#[test]
fn a_same_fs_merge_that_skipped_a_child_reports_the_source_still_there() {
    // The directory stays behind holding its skipped child, so a purge keyed on
    // "it was a move" would drop a row for a folder that is still on disk.
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
    fs::write(dst_dir.join("collide.bin"), b"BBBB").unwrap();

    let events = run_same_fs_move(
        std::slice::from_ref(&src_dir),
        &dst_root,
        ConflictResolution::Skip,
        "same-fs-merge-skip",
    )
    .expect("the move must succeed");

    assert!(src_dir.exists(), "precondition: the skipped child keeps the source dir");
    assert_eq!(removal_flags_for(&events, &src_dir), vec![false]);
}

#[test]
fn a_cross_fs_move_reports_removal_only_after_the_source_delete_phase() {
    // Phase 2 finishes staging while the original is untouched; Phase 4 deletes
    // it. Reading the first event as "gone" would purge a live file for as long
    // as the copy takes, and forever if a Skip in between keeps the source.
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    let source = src_root.join("a.bin");
    fs::write(&source, b"content").unwrap();

    let events = run_cross_fs_move(
        std::slice::from_ref(&source),
        &dst_root,
        ConflictResolution::Stop,
        "cross-fs-removed",
    )
    .expect("the move must succeed");

    assert_eq!(removal_flags_for(&events, &source), vec![false, true]);
}

#[test]
fn a_cross_fs_move_that_skipped_the_item_never_reports_it_removed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    let source = src_root.join("a.bin");
    fs::write(&source, b"source").unwrap();
    fs::write(dst_root.join("a.bin"), b"dest").unwrap();

    let events = run_cross_fs_move(
        std::slice::from_ref(&source),
        &dst_root,
        ConflictResolution::Skip,
        "cross-fs-skip",
    )
    .expect("the move must succeed");

    assert!(source.exists(), "precondition: Skip keeps the source");
    assert!(
        !removal_flags_for(&events, &source).contains(&true),
        "a skipped source must never be reported removed"
    );
}

/// A cross-filesystem move stages every source before it renames any of them, so
/// staging succeeding says nothing about where the item ended up. Before this,
/// a source the rename phase skipped kept `Done` from staging as its only word,
/// and a caller recording per-source outcomes wrote down that the move happened.
#[test]
fn a_cross_fs_move_that_skipped_the_item_ends_on_skipped_not_done() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    let source = src_root.join("a.bin");
    fs::write(&source, b"source").unwrap();
    fs::write(dst_root.join("a.bin"), b"dest").unwrap();

    let events = run_cross_fs_move(
        std::slice::from_ref(&source),
        &dst_root,
        ConflictResolution::Skip,
        "cross-fs-skip-outcome",
    )
    .expect("the move must succeed");

    assert!(source.exists(), "precondition: Skip keeps the source");
    assert_eq!(
        outcomes_for(&events, &source),
        vec![SourceItemOutcome::Done, SourceItemOutcome::Skipped],
        "staged, then left standing: the last word is the verdict"
    );
}

/// The ordinary cross-filesystem move still ends on `Done`, so the verdict rule
/// isn't just "the second event is always a skip".
#[test]
fn a_cross_fs_move_that_took_the_item_ends_on_done() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    let source = src_root.join("a.bin");
    fs::write(&source, b"content").unwrap();

    let events = run_cross_fs_move(
        std::slice::from_ref(&source),
        &dst_root,
        ConflictResolution::Stop,
        "cross-fs-done-outcome",
    )
    .expect("the move must succeed");

    assert_eq!(
        outcomes_for(&events, &source),
        vec![SourceItemOutcome::Done, SourceItemOutcome::Done]
    );
}

/// A Rollback the user clicks as the last item lands actually reverses the move.
///
/// A same-FS move renames a whole directory in one call, so the loop can drain
/// between the click and the next `is_cancelled` check — reliably, on a
/// one-item move. The intent has to be read once more after the loop, or the
/// operation reports "complete" and leaves everything at the destination, which
/// is the one answer the user didn't choose. `copy/mod.rs`'s
/// `PostLoopIntent::Completed` arm is the same guard on the copy side.
#[test]
fn a_rollback_clicked_as_the_last_item_lands_puts_the_move_back() {
    use crate::file_system::write_operations::state::OperationIntent;
    use crate::file_system::write_operations::types::{
        CancelRollbackOutcome, ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent,
        WriteConflictEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Clicks Rollback the instant the move reports the item landed — after the
    /// rename, and before the post-loop intent check reads it.
    struct RollBackWhenTheItemLands {
        state: Arc<WriteOperationState>,
        cancelled: Mutex<Vec<WriteCancelledEvent>>,
        completes: AtomicUsize,
    }

    impl OperationEventSink for RollBackWhenTheItemLands {
        fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {
            self.state
                .intent
                .store(OperationIntent::RollingBack as u8, Ordering::SeqCst);
        }
        fn emit_cancelled(&self, event: WriteCancelledEvent) {
            self.cancelled.lock_ignore_poison().push(event);
        }
        fn emit_complete(&self, _event: WriteCompleteEvent) {
            self.completes.fetch_add(1, Ordering::SeqCst);
        }
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
        fn emit_progress(&self, _event: WriteProgressEvent) {}
        fn emit_error(&self, _event: WriteErrorEvent) {}
        fn emit_conflict(&self, _event: WriteConflictEvent) {}
        fn emit_conflict_resolved(
            &self,
            _event: crate::file_system::write_operations::types::WriteConflictResolvedEvent,
        ) {
        }
        fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _conflict: ConflictInfo) {}
        fn emit_dry_run_complete(&self, _result: DryRunResult) {}
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let src_file = src_dir.join("notes.txt");
    fs::write(&src_file, b"the only copy").unwrap();

    let state = make_state(200);
    let events = Arc::new(RollBackWhenTheItemLands {
        state: Arc::clone(&state),
        cancelled: Mutex::new(Vec::new()),
        completes: AtomicUsize::new(0),
    });

    let result = move_files_with_progress_inner(
        &*events,
        "op-same-fs-late-rollback",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &WriteOperationConfig::default(),
    );

    assert!(
        matches!(result, Err(WriteOperationError::Cancelled { .. })),
        "a move the user rolled back ends cancelled, got {result:?}"
    );
    assert_eq!(
        fs::read(&src_file).unwrap(),
        b"the only copy",
        "the file the user asked back is back where it started"
    );
    assert!(
        !dst_dir.join("notes.txt").exists(),
        "and it's off the destination again"
    );
    assert_eq!(
        events.completes.load(Ordering::SeqCst),
        0,
        "a rolled-back move never reports itself complete"
    );
    let cancelled = events.cancelled.lock_ignore_poison();
    assert_eq!(cancelled.len(), 1, "exactly one write-cancelled");
    assert_eq!(cancelled[0].rollback.outcome, CancelRollbackOutcome::RolledBack);
    assert_eq!(cancelled[0].rollback.reversed, 1, "the one rename it made, reversed");
}

/// A paused same-FS move really stops renaming.
///
/// Pause is a promise: the UI says "Paused", and the person who hit it because
/// they picked the wrong destination believes they have time to intervene. The
/// rename loop has to park at its item boundary like every other driver
/// (`sync_driver.rs`, `async_driver.rs`, `delete/walker.rs`,
/// `archive_edit/engine.rs`), or the promise is empty and the files keep
/// arriving at full speed.
#[test]
fn a_paused_move_stops_renaming_until_it_resumes() {
    use crate::file_system::write_operations::types::{
        ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
        WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Hits Pause the moment the first item has landed, the way a user would
    /// the instant they see the wrong destination filling up.
    struct PauseAfterTheFirstItem {
        state: Arc<WriteOperationState>,
        items_done: AtomicUsize,
    }

    impl OperationEventSink for PauseAfterTheFirstItem {
        fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {
            if self.items_done.fetch_add(1, Ordering::SeqCst) == 0 {
                self.state.pause_gate.pause();
            }
        }
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
        fn emit_progress(&self, _event: WriteProgressEvent) {}
        fn emit_complete(&self, _event: WriteCompleteEvent) {}
        fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
        fn emit_error(&self, _event: WriteErrorEvent) {}
        fn emit_conflict(&self, _event: WriteConflictEvent) {}
        fn emit_conflict_resolved(
            &self,
            _event: crate::file_system::write_operations::types::WriteConflictResolvedEvent,
        ) {
        }
        fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _conflict: ConflictInfo) {}
        fn emit_dry_run_complete(&self, _result: DryRunResult) {}
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let first = src_dir.join("first.txt");
    let second = src_dir.join("second.txt");
    fs::write(&first, b"one").unwrap();
    fs::write(&second, b"two").unwrap();

    let state = make_state(200);
    let events = Arc::new(PauseAfterTheFirstItem {
        state: Arc::clone(&state),
        items_done: AtomicUsize::new(0),
    });

    let sources = vec![first.clone(), second.clone()];
    let dst_for_move = dst_dir.clone();
    let state_for_move = Arc::clone(&state);
    let events_for_move = Arc::clone(&events);
    let mover = std::thread::spawn(move || {
        move_files_with_progress_inner(
            &*events_for_move,
            "op-same-fs-pause",
            &state_for_move,
            &sources,
            &dst_for_move,
            &WriteOperationConfig::default(),
        )
    });

    crate::test_support::wait_until(Duration::from_secs(5), "the first item to land", || {
        dst_dir.join("first.txt").exists()
    });

    // Parking has no "parked now" signal, so hold a window open: an ungated
    // loop would have renamed the second file many times over inside it.
    // allowed-test-sleep: negative assertion over a window; the condvar park has nothing to await.
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        second.exists(),
        "a paused move must stop renaming: the second file is still at its source"
    );
    assert!(
        !dst_dir.join("second.txt").exists(),
        "and hasn't reached the destination"
    );

    state.pause_gate.resume();

    let result = mover.join().expect("the move thread joins");
    assert!(result.is_ok(), "the resumed move finishes, got {result:?}");
    assert!(
        dst_dir.join("second.txt").exists(),
        "and the second file lands once the user resumes"
    );
}

/// The same promise, one level down: a folder-into-folder move does all its
/// renaming inside `merge_move_directory`, where the top-level gate never
/// reaches. Without a gate on the child loop, pausing a merge stops nothing.
#[test]
fn a_paused_folder_merge_stops_renaming_its_children() {
    use std::time::Duration;

    let tmp = tempfile::tempdir().expect("tempdir");
    let source_dir = tmp.path().join("from/album");
    let dest_dir = tmp.path().join("into/album");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();
    let children = ["a.txt", "b.txt", "c.txt", "d.txt"];
    for name in children {
        fs::write(source_dir.join(name), b"pixels").unwrap();
    }

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    state.pause_gate.pause();

    let source_for_merge = source_dir.clone();
    let dest_for_merge = dest_dir.clone();
    let state_for_merge = Arc::clone(&state);
    let events_for_merge = Arc::clone(&events);
    let merger = std::thread::spawn(move || {
        let mut skipped = 0usize;
        merge_move_directory(
            &source_for_merge,
            &dest_for_merge,
            &WriteOperationConfig::default(),
            &*events_for_merge,
            "op-merge-pause",
            &state_for_merge,
            &mut ApplyToAll::default(),
            &mut MoveTransaction::new(),
            &mut skipped,
            &mut None,
        )
    });

    // Paused before the first child, so nothing may cross while the window is open.
    // allowed-test-sleep: negative assertion over a window; the condvar park has nothing to await.
    std::thread::sleep(Duration::from_millis(150));
    for name in children {
        assert!(
            source_dir.join(name).exists(),
            "a paused merge must not move {name} out of the source folder"
        );
    }

    state.pause_gate.resume();
    merger.join().expect("the merge thread joins").expect("the merge lands");

    for name in children {
        assert!(
            dest_dir.join(name).exists(),
            "{name} lands in the destination once the user resumes"
        );
    }
}

/// A cancelled cross-FS move leaves no `.cmdr-staging-<op>` folder behind.
///
/// Phase 3 renames the staged tree into place, so by Phase 5 the staging folder
/// is an empty shell — but Phase 5 sat after Phase 4's source delete, which
/// returns `Err` on cancel, so a cancel in that window left the shell sitting in
/// the user's destination folder forever. Removing it on the cancel path is safe
/// precisely because there is nothing left inside it.
#[test]
fn a_cancelled_cross_fs_move_takes_its_staging_folder_with_it() {
    use crate::file_system::write_operations::state::OperationIntent;
    use crate::file_system::write_operations::types::{
        ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
        WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::atomic::Ordering;

    /// Cancels the instant the flush announces itself — the last thing that
    /// happens before Phase 4 starts deleting the originals.
    struct CancelAtTheFlush {
        state: Arc<WriteOperationState>,
    }

    impl OperationEventSink for CancelAtTheFlush {
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Flushing {
                self.state.intent.store(OperationIntent::Stopped as u8, Ordering::SeqCst);
            }
        }
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
        fn emit_complete(&self, _event: WriteCompleteEvent) {}
        fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
        fn emit_error(&self, _event: WriteErrorEvent) {}
        fn emit_conflict(&self, _event: WriteConflictEvent) {}
        fn emit_conflict_resolved(
            &self,
            _event: crate::file_system::write_operations::types::WriteConflictResolvedEvent,
        ) {
        }
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
    fs::write(&src_file, vec![3u8; 4096]).unwrap();

    let state = make_state(200);
    let events = Arc::new(CancelAtTheFlush {
        state: Arc::clone(&state),
    });
    let op_id = "op-cross-fs-cancel-staging";

    let result = move_with_staging(
        &*events,
        op_id,
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &WriteOperationConfig::default(),
        0,
    );

    assert!(
        matches!(result, Err(WriteOperationError::Cancelled { .. })),
        "the cancel ends the move as cancelled, got {result:?}"
    );
    assert!(
        !dst_dir.join(format!(".cmdr-staging-{op_id}")).exists(),
        "a cancelled move must not leave its staging folder in the user's destination"
    );
    // What the cancel kept: the file landed in Phase 3, and the source it was
    // about to delete is still there. Both are the existing contract.
    assert!(dst_dir.join("file.bin").exists(), "the staged copy stays where it landed");
    assert!(src_file.exists(), "and the cancel spared the original");
}
