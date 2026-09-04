//! Pausing a cross-volume folder merge mid-walk really stops it.
//!
//! The walk in `merge.rs` observes cancel per entry but is otherwise unbounded
//! work: listings, destination creation, and a conflict decision per child, none
//! of which goes through the between-chunks checkpoint the byte path parks at
//! (`checkpoint_stream.rs`). So without a gate on the walk itself, "Paused" stood
//! over a merge that kept prompting, creating, and skipping its way down a tree.
//!
//! Every child here CLASHES under `Stop`, so no bytes stream at all and the walk
//! is the only thing that can advance: the prompt count is therefore exact
//! evidence about the walk. The pause is wired to the merge's own progress
//! (answering the first prompt, then pausing), never a wall clock.

use super::super::super::conflict_responder_test_support::{ConflictResponderSink, file_conflict_count};
use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::types::{
    ConflictInfo, ConflictResolution, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent,
    WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Clashing children in the fixture. Enough that an ungated walk would have
/// prompted for all of them long before the test's window closes.
const CHILDREN: usize = 40;

/// Answers every Stop-mode prompt with Skip (through the shared responder) and
/// pauses the operation the instant the FIRST one is answered, so the pause lands
/// with the walk provably mid-tree.
struct PauseAfterTheFirstPrompt {
    responder: ConflictResponderSink,
    state: Arc<WriteOperationState>,
    prompts: AtomicUsize,
}

impl OperationEventSink for PauseAfterTheFirstPrompt {
    fn emit_conflict(&self, event: WriteConflictEvent) {
        if self.prompts.fetch_add(1, Ordering::SeqCst) == 0 {
            self.state.pause_gate.pause();
        }
        // Delegating answers the clash, so the walk resumes from its prompt and
        // meets the pause at its next per-entry boundary.
        self.responder.emit_conflict(event);
    }
    fn emit_progress(&self, e: WriteProgressEvent) {
        self.responder.emit_progress(e);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.responder.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.responder.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.responder.emit_error(e);
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.responder.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, e: WriteSourceItemDoneEvent) {
        self.responder.emit_source_item_done(e);
    }
    fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: DryRunResult) {}
    fn emit_settled(&self, _e: WriteSettledEvent) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_cross_volume_merge_stops_walking_until_it_resumes() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    for i in 0..CHILDREN {
        let name = format!("/album/c{:02}.txt", i);
        source.create_file(Path::new(&name), b"SRC").await.unwrap();
        dest.create_file(Path::new(&name), b"DEST").await.unwrap();
    }

    let state = make_state();
    let events = Arc::new(PauseAfterTheFirstPrompt {
        responder: ConflictResponderSink::new(&state, ConflictResolution::Skip, false),
        state: Arc::clone(&state),
        prompts: AtomicUsize::new(0),
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let events_for_copy = Arc::clone(&events);
    let state_for_copy = Arc::clone(&state);
    let source_for_copy = Arc::clone(&source);
    let dest_for_copy = Arc::clone(&dest);
    let copier = tokio::spawn(async move {
        copy_volumes_with_progress(
            events_for_copy,
            "op-merge-pause",
            &state_for_copy,
            source_for_copy,
            &[PathBuf::from("/album")],
            dest_for_copy,
            Path::new("/"),
            &config,
        )
        .await
    });

    let prompts = || file_conflict_count(&events.responder.inner);
    crate::test_support::wait_until_async(Duration::from_secs(5), "the first child's prompt", || prompts() >= 1).await;

    // Parking has no "parked now" signal, so hold a window open: an ungated walk
    // would have prompted for the remaining children many times over inside it.
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        prompts(),
        1,
        "a paused merge holds at its per-entry boundary: no further child is even looked at"
    );

    state.pause_gate.resume();

    let result = tokio::time::timeout(Duration::from_secs(10), copier)
        .await
        .expect("a resumed merge finishes")
        .expect("the copy task joins");
    assert!(result.is_ok(), "the resumed merge completes, got {result:?}");
    assert_eq!(
        prompts(),
        CHILDREN,
        "and the walk reaches every child once the user resumes"
    );

    // Skip was the answer, so every destination file keeps its own bytes: the
    // pause changed nothing about the merge's outcome.
    let mut stream = dest.open_read_stream(Path::new("/album/c39.txt")).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk);
    }
    assert_eq!(bytes, b"DEST", "a skipped child's destination is untouched");
}
