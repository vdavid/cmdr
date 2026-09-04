//! Pausing a trash operation really stops it, mid-batch.
//!
//! Trash iterates top-level items and hands each whole tree to the OS in one
//! call, so the item boundary is the only place it can stop, and it's the same
//! boundary the two delete walkers next door park at. Without the gate, "Paused"
//! stood over a batch that kept emptying folders into the trash.
//!
//! The sources are paths that don't exist, so the loop runs its full per-item
//! shape (verdict, progress) without moving anything into the real trash: what's
//! under test is the boundary, not `trashItemAtURL`.

use super::trash::trash_files_with_progress;
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Counts per-item verdicts and pauses the operation the instant the first one
/// lands, so the pause is tied to the batch's own progress rather than a wall
/// clock.
struct PauseAfterTheFirstItem {
    state: Arc<WriteOperationState>,
    verdicts: AtomicUsize,
}

impl OperationEventSink for PauseAfterTheFirstItem {
    fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {
        if self.verdicts.fetch_add(1, Ordering::SeqCst) == 0 {
            self.state.pause_gate.pause();
        }
    }
    fn emit_progress(&self, _event: WriteProgressEvent) {}
    fn emit_complete(&self, _event: WriteCompleteEvent) {}
    fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
    fn emit_error(&self, _event: WriteErrorEvent) {}
    fn emit_conflict(&self, _event: WriteConflictEvent) {}
    fn emit_conflict_resolved(&self, _event: WriteConflictResolvedEvent) {}
    fn emit_settled(&self, _event: WriteSettledEvent) {}
    fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _conflict: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _result: DryRunResult) {}
}

#[test]
fn a_paused_trash_stops_between_items_until_it_resumes() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = Arc::new(PauseAfterTheFirstItem {
        state: Arc::clone(&state),
        verdicts: AtomicUsize::new(0),
    });

    let sources: Vec<PathBuf> = (0..4)
        .map(|i| PathBuf::from(format!("/nonexistent_trash_pause_test/item{i}.txt")))
        .collect();

    let state_for_trash = Arc::clone(&state);
    let events_for_trash = Arc::clone(&events);
    let trasher = std::thread::spawn(move || {
        trash_files_with_progress(&*events_for_trash, "op-trash-pause", &state_for_trash, &sources, None)
    });

    crate::test_support::wait_until(Duration::from_secs(5), "the first item's verdict", || {
        events.verdicts.load(Ordering::SeqCst) >= 1
    });

    // Parking has no "parked now" signal, so hold a window open: an ungated loop
    // would have run through the remaining three items many times over.
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        events.verdicts.load(Ordering::SeqCst),
        1,
        "a paused trash holds at the item boundary: no further item is touched"
    );

    state.pause_gate.resume();

    let result = trasher.join().expect("the trash thread joins");
    assert!(
        result.is_err(),
        "every source was missing, so the batch still reports itself failed"
    );
    assert_eq!(
        events.verdicts.load(Ordering::SeqCst),
        4,
        "and the whole batch runs to its end once the user resumes"
    );
}
