//! What a local move DOES to the files: skips preserved, empty directories
//! landed, durability ordered ahead of the source delete, and the per-source
//! outcome each one ends on. What it REPORTS while it runs is the sibling suite,
//! `move_progress_tests.rs`.
//!
//! Drives `move_with_staging` directly with a `CollectorEventSink` + tempdir.
//! Same-FS moves go through `move_with_rename` (instant `fs::rename`); the
//! staging path is only reached when source and destination live on different
//! filesystems, and calling it directly exercises that path without needing two
//! real mount points.

use super::cross_fs::move_with_staging;
use super::test_support::{make_state, run_cross_fs_move, run_same_fs_move};
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationPhase, WriteProgressEvent};
use crate::ignore_poison::IgnorePoison;

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
