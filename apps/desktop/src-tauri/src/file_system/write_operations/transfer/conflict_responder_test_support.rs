//! Shared test support for driving Stop-mode conflict prompts in the
//! folder-merge suites (`volume/merge_tests.rs` and `volume/rename_merge_tests.rs`).
//!
//! ## Why an event-driven responder, not a polling one
//!
//! A Stop-mode clash emits a `write-conflict` event and then blocks the
//! operation on a `tokio::sync::oneshot` receiver until something answers
//! `state.conflict_slot`. The merge engine ARMS that slot BEFORE emitting the
//! event (see `volume/conflict.rs`'s Stop branch), so by the time any sink
//! observes the `emit_conflict` call the sender is already in the slot.
//! [`ConflictResponderSink`] exploits exactly that: it wraps an inner
//! [`CollectorEventSink`], forwards every event, and — the instant it sees a
//! conflict — synchronously answers the slot with the scripted
//! [`ConflictResolutionResponse`]. The op's `rx.await` then returns immediately.
//!
//! This is order-independent by construction: there is no parallel counter to
//! race against the op future, and no 2 ms polling loop. Once the driven op
//! future completes, the inner collector's recorded conflicts ARE the
//! authoritative, race-free prompt count — `events` carries the paths and
//! file/folder flags too, so assertions derive from the sink, not a side-channel
//! `AtomicUsize`. See [`file_conflict_count`].

use std::sync::Arc;

use super::super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::super::state::{ConflictResolutionResponse, WriteOperationState};
use super::super::types::{
    ConflictId, ConflictInfo, ConflictResolution, DryRunResult, ScanProgressEvent, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent,
    WriteSettledEvent, WriteSourceItemDoneEvent,
};
use crate::ignore_poison::IgnorePoison;

/// An event sink that auto-answers Stop-mode `write-conflict` prompts with a
/// scripted resolution, the moment it observes them. Forwards every event to an
/// inner [`CollectorEventSink`], so the driving test can derive race-free prompt
/// counts (and richer path/flag assertions) from `sink.inner` after the op
/// completes.
///
/// Use it as the operation's `events` sink directly — it replaces the old
/// pattern of a `CollectorEventSink` plus a separately-spawned polling responder
/// task. Because it answers synchronously inside `emit_conflict` (the sender is
/// already stored by then), there is no task to abort and no polling window.
pub(super) struct ConflictResponderSink {
    pub inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    answer: Answer,
    /// Runs as each prompt arrives, before it's answered: the moment the
    /// operation is parked on the question, for a test that needs to see what
    /// the destination held then.
    on_prompt: Option<Box<dyn Fn() + Send + Sync>>,
}

/// How [`ConflictResponderSink`] answers a prompt.
enum Answer {
    /// Picks a resolution, as the dialog's buttons do.
    Resolve {
        resolution: ConflictResolution,
        apply_to_all: bool,
    },
    /// Refuses the prompt: the dialog's Cancel, which stops the whole operation
    /// (keeping what landed) rather than answering the clash.
    CancelOperation { operation_id: String },
}

impl ConflictResponderSink {
    /// Answers every prompt with `resolution` / `apply_to_all`.
    pub(super) fn new(state: &Arc<WriteOperationState>, resolution: ConflictResolution, apply_to_all: bool) -> Self {
        Self {
            inner: CollectorEventSink::new(),
            state: Arc::clone(state),
            answer: Answer::Resolve {
                resolution,
                apply_to_all,
            },
            on_prompt: None,
        }
    }

    /// Refuses every prompt by cancelling `operation_id` (without rollback), the
    /// way a person pressing Cancel on the conflict dialog does.
    pub(super) fn cancelling(state: &Arc<WriteOperationState>, operation_id: &str) -> Self {
        Self {
            inner: CollectorEventSink::new(),
            state: Arc::clone(state),
            answer: Answer::CancelOperation {
                operation_id: operation_id.to_string(),
            },
            on_prompt: None,
        }
    }

    /// Runs `probe` as each prompt arrives, before it's answered.
    pub(super) fn on_prompt(mut self, probe: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_prompt = Some(Box::new(probe));
        self
    }
}

impl OperationEventSink for ConflictResponderSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: WriteConflictEvent) {
        // Record the prompt first (so the count is authoritative even if the
        // send below races teardown), then answer it. The answer names the
        // clash this event carries, exactly as a real surface would.
        let clash = e.conflict_id;
        self.inner.emit_conflict(e);
        if let Some(probe) = &self.on_prompt {
            probe();
        }

        // The slot was armed before this event was emitted, so the answer can't
        // miss. Either way it unblocks the op's `rx.await` synchronously: an
        // answer through the slot, a cancel by abandoning it.
        match &self.answer {
            Answer::Resolve {
                resolution,
                apply_to_all,
            } => {
                let _ = self.state.conflict_slot.answer(
                    clash,
                    ConflictResolutionResponse {
                        resolution: *resolution,
                        apply_to_all: *apply_to_all,
                    },
                );
            }
            Answer::CancelOperation { operation_id } => {
                super::super::state::cancel_write_operation(operation_id, false);
            }
        }
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, e: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(e);
    }
    fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: DryRunResult) {}
    fn emit_settled(&self, _e: WriteSettledEvent) {}
}

/// Waits for the prompt an operation is parked on and hands back its
/// [`ConflictId`], for the tests that answer from a spawned resolver task rather
/// than from inside a sink.
///
/// Waits on the recorded EVENT rather than on `conflict_slot.is_awaiting()`,
/// because the id only exists on the event: an answer names the clash it is for,
/// and a resolver that reached into the slot for "whatever is parked" would be
/// re-creating the very confusion the id exists to prevent. The slot is armed
/// before the event is emitted, so an observed event always has a live sender
/// behind it.
pub(super) async fn await_prompted_clash(events: &CollectorEventSink) -> ConflictId {
    crate::test_support::wait_until_async(std::time::Duration::from_secs(5), "the conflict prompt", || {
        !events.conflicts.lock_ignore_poison().is_empty()
    })
    .await;
    let conflicts = events.conflicts.lock_ignore_poison();
    conflicts
        .last()
        .expect("the wait above proved one is recorded")
        .conflict_id
}

/// Counts `write-conflict` events that are a FILE-vs-FILE clash (neither side a
/// directory) — i.e. the per-file prompts a merge can legitimately raise. This
/// is the authoritative, race-free prompt count once the driven op future has
/// completed, replacing the old parallel `AtomicUsize` answer counter. Dir-vs-dir
/// merges never emit a conflict at all (the resolver short-circuits before the
/// emit), so this equals the total emitted conflicts in a pure file-clash merge;
/// filtering to file-vs-file keeps it honest if a cross-type clash is ever mixed
/// in.
pub(super) fn file_conflict_count(events: &CollectorEventSink) -> usize {
    events
        .conflicts
        .lock_ignore_poison()
        .iter()
        .filter(|c| !c.source_is_directory && !c.destination_is_directory)
        .count()
}

/// Counts `write-conflict` events where source AND destination are BOTH
/// directories — i.e. a true dir-vs-dir folder-level prompt. This is the
/// contract the cross-volume COPY merge defends: dir-vs-dir always merges
/// silently, so this count must be ZERO. Use this from `volume/merge_tests.rs`.
pub(super) fn folder_conflict_count_both_dirs(events: &CollectorEventSink) -> usize {
    events
        .conflicts
        .lock_ignore_poison()
        .iter()
        .filter(|c| c.source_is_directory && c.destination_is_directory)
        .count()
}

/// Counts `write-conflict` events where source OR destination is a directory —
/// i.e. any folder-touching prompt (including a file-vs-folder type mismatch).
/// This is the contract the same-volume RENAME-MERGE defends: a folder merge
/// must raise ZERO of these. Use this from `volume/rename_merge_tests.rs`.
pub(super) fn folder_conflict_count_any_dir(events: &CollectorEventSink) -> usize {
    events
        .conflicts
        .lock_ignore_poison()
        .iter()
        .filter(|c| c.source_is_directory || c.destination_is_directory)
        .count()
}
