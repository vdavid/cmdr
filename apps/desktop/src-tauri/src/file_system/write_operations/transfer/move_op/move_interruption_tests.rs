//! What an interrupted local move does: paused, rolled back at the last
//! moment, or cancelled with a staging folder still on disk.
//!
//! A `#[path]` child of `move_op`, like the other suites here, so it can drive
//! both engines and reach `merge_move_directory` directly. Engine behavior with
//! nobody interrupting lives in `move_op_tests.rs`.

use super::cross_fs::move_with_staging;
use super::test_support::make_state;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{WriteOperationPhase, WriteProgressEvent};
use crate::ignore_poison::IgnorePoison;

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
            // allowed-pluralize-noun: `{name}` is a file name, not a count.
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
                self.state
                    .intent
                    .store(OperationIntent::Stopped as u8, Ordering::SeqCst);
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
    assert!(
        dst_dir.join("file.bin").exists(),
        "the staged copy stays where it landed"
    );
    assert!(src_file.exists(), "and the cancel spared the original");
}

/// A Rollback pressed while the move is clearing the originals reports what is
/// actually on disk: the whole copy is at the destination, and the originals the
/// sweep hadn't reached are still where they were.
///
/// The undo itself is out of reach here — the bytes are already across a
/// filesystem boundary, and carrying them home would be minutes of I/O the user
/// never asked for. So the report's job is the truth, not a reversal: saying
/// nothing would leave a user who pressed Rollback believing nothing had
/// happened, while some of their originals were gone for good.
#[test]
fn a_rollback_during_the_source_sweep_reports_the_originals_it_did_not_reach() {
    use crate::file_system::write_operations::state::OperationIntent;
    use crate::file_system::write_operations::types::{
        CancelRollbackOutcome, ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent,
        WriteConflictEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Presses Rollback once the sweep has removed `after` originals, so the
    /// cancel lands in the MIDDLE of Phase 4 rather than at either end of it.
    struct RollBackMidSweep {
        state: Arc<WriteOperationState>,
        after: usize,
        removed: AtomicUsize,
        cancelled: Mutex<Vec<WriteCancelledEvent>>,
    }

    impl OperationEventSink for RollBackMidSweep {
        fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
            if event.source_removed && self.removed.fetch_add(1, Ordering::SeqCst) + 1 == self.after {
                self.state
                    .intent
                    .store(OperationIntent::RollingBack as u8, Ordering::SeqCst);
            }
        }
        fn emit_cancelled(&self, event: WriteCancelledEvent) {
            self.cancelled.lock_ignore_poison().push(event);
        }
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
        fn emit_complete(&self, _event: WriteCompleteEvent) {}
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
    let names = ["a.bin", "b.bin", "c.bin", "d.bin"];
    let sources: Vec<PathBuf> = names
        .iter()
        .map(|name| {
            let path = src_dir.join(name);
            fs::write(&path, vec![7u8; 512]).unwrap();
            path
        })
        .collect();

    let state = make_state(0);
    let events = Arc::new(RollBackMidSweep {
        state: Arc::clone(&state),
        after: 2,
        removed: AtomicUsize::new(0),
        cancelled: Mutex::new(Vec::new()),
    });

    let result = move_with_staging(
        &*events,
        "op-cross-fs-rollback-mid-sweep",
        &state,
        &sources,
        &dst_dir,
        &WriteOperationConfig::default(),
        0,
    );

    assert!(
        matches!(result, Err(WriteOperationError::Cancelled { .. })),
        "the Rollback ends the move as cancelled, got {result:?}"
    );

    // What is actually on disk, which is what the report has to match.
    for name in names {
        assert!(
            dst_dir.join(name).exists(),
            // allowed-pluralize-noun: `{name}` is a file name, not a count.
            "{name} landed in phase 3, before the sweep ever started"
        );
    }
    assert!(!sources[0].exists() && !sources[1].exists(), "the sweep removed two");
    assert!(
        sources[2].exists() && sources[3].exists(),
        "and the Rollback caught it before the other two"
    );

    let cancelled = events.cancelled.lock_ignore_poison();
    assert_eq!(cancelled.len(), 1, "exactly one write-cancelled event");
    let rollback = &cancelled[0].rollback;
    assert_eq!(
        rollback.outcome,
        CancelRollbackOutcome::NotRolledBack,
        "no reversal ran, and none can: the copy is already across the boundary"
    );
    let left = rollback
        .originals_still_in_place
        .as_ref()
        .expect("a sweep cancelled partway must account for the originals it left");
    assert_eq!(left.count, 2, "two originals are still where they were");
}

/// A Rollback pressed before the sweep touches anything reports every original
/// as still in place: the same account, at the other end of phase 4.
///
/// This is also the window a Rollback lands in when it arrives after the copy
/// loop drains. `cross_fs.rs` has no post-loop intent check, so phase 3 renames
/// the tree into place and the flush runs regardless; the sweep's first
/// `is_cancelled` is what catches the click, and this report is what it says.
#[test]
fn a_rollback_before_the_sweep_starts_reports_every_original_still_in_place() {
    use crate::file_system::write_operations::state::OperationIntent;
    use crate::file_system::write_operations::types::{
        ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
        WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::Mutex;
    use std::sync::atomic::Ordering;

    /// Presses Rollback the instant the flush announces itself, the last thing
    /// that happens before the sweep starts deleting the originals.
    struct RollBackAtTheFlush {
        state: Arc<WriteOperationState>,
        cancelled: Mutex<Vec<WriteCancelledEvent>>,
    }

    impl OperationEventSink for RollBackAtTheFlush {
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Flushing {
                self.state
                    .intent
                    .store(OperationIntent::RollingBack as u8, Ordering::SeqCst);
            }
        }
        fn emit_cancelled(&self, event: WriteCancelledEvent) {
            self.cancelled.lock_ignore_poison().push(event);
        }
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
        fn emit_complete(&self, _event: WriteCompleteEvent) {}
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
    let sources: Vec<PathBuf> = ["a.bin", "b.bin", "c.bin"]
        .iter()
        .map(|name| {
            let path = src_dir.join(name);
            fs::write(&path, vec![9u8; 512]).unwrap();
            path
        })
        .collect();

    let state = make_state(0);
    let events = Arc::new(RollBackAtTheFlush {
        state: Arc::clone(&state),
        cancelled: Mutex::new(Vec::new()),
    });

    let result = move_with_staging(
        &*events,
        "op-cross-fs-rollback-before-sweep",
        &state,
        &sources,
        &dst_dir,
        &WriteOperationConfig::default(),
        0,
    );

    assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
    assert!(
        sources.iter().all(|p| p.exists()),
        "the Rollback caught the sweep before its first delete"
    );

    let cancelled = events.cancelled.lock_ignore_poison();
    let left = cancelled[0]
        .rollback
        .originals_still_in_place
        .as_ref()
        .expect("the report accounts for the originals even when none were removed");
    assert_eq!(left.count, 3, "every original is still where it was");
}

/// A source the user Skipped is still in its old place too, so the sweep's
/// account has to count it alongside the ones it never reached.
///
/// Phase 3 discards a Skipped source's staged copy and phase 4 deliberately
/// spares the original. Counting only the unvisited sources would undercount the
/// originals the user can still see in the source folder.
#[test]
fn the_sweeps_account_counts_a_skipped_source_as_still_in_place() {
    use crate::file_system::write_operations::state::OperationIntent;
    use crate::file_system::write_operations::types::{
        ConflictInfo, ConflictResolution, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent,
        WriteConflictEvent, WriteErrorEvent, WriteSourceItemDoneEvent,
    };
    use std::sync::Mutex;
    use std::sync::atomic::Ordering;

    /// Presses Rollback once the sweep reports its first REMOVAL. With the
    /// Skipped source ordered first, that lands the cancel with one source
    /// skipped, one removed, and two untouched.
    struct RollBackAfterTheFirstRemoval {
        state: Arc<WriteOperationState>,
        cancelled: Mutex<Vec<WriteCancelledEvent>>,
    }

    impl OperationEventSink for RollBackAfterTheFirstRemoval {
        fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
            if event.source_removed {
                self.state
                    .intent
                    .store(OperationIntent::RollingBack as u8, Ordering::SeqCst);
            }
        }
        fn emit_cancelled(&self, event: WriteCancelledEvent) {
            self.cancelled.lock_ignore_poison().push(event);
        }
        fn emit_settled(&self, _event: crate::file_system::write_operations::types::WriteSettledEvent) {}
        fn emit_complete(&self, _event: WriteCompleteEvent) {}
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
    // `taken.bin` already exists at the destination, so Skip discards its staged
    // copy and the sweep spares the original.
    fs::write(dst_dir.join("taken.bin"), b"the one the user kept").unwrap();
    let sources: Vec<PathBuf> = ["taken.bin", "a.bin", "b.bin", "c.bin"]
        .iter()
        .map(|name| {
            let path = src_dir.join(name);
            fs::write(&path, vec![5u8; 512]).unwrap();
            path
        })
        .collect();

    let state = make_state(0);
    let events = Arc::new(RollBackAfterTheFirstRemoval {
        state: Arc::clone(&state),
        cancelled: Mutex::new(Vec::new()),
    });

    let result = move_with_staging(
        &*events,
        "op-cross-fs-rollback-after-skip",
        &state,
        &sources,
        &dst_dir,
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Skip,
            ..WriteOperationConfig::default()
        },
        0,
    );

    assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));
    assert!(sources[0].exists(), "the Skipped source keeps its original");
    assert!(!sources[1].exists(), "the sweep removed the one after it");
    assert!(sources[2].exists() && sources[3].exists(), "and stopped there");

    let cancelled = events.cancelled.lock_ignore_poison();
    let left = cancelled[0]
        .rollback
        .originals_still_in_place
        .as_ref()
        .expect("the sweep accounts for the originals it left");
    assert_eq!(
        left.count, 3,
        "the Skipped original counts alongside the two the sweep never reached"
    );
}
