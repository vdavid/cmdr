//! Shared test support for driving Stop-mode conflict prompts in the
//! folder-merge suites (`volume_merge_tests.rs` and `volume_rename_merge_tests.rs`).
//!
//! ## Why an event-driven responder, not a polling one
//!
//! A Stop-mode clash emits a `write-conflict` event and then blocks the
//! operation on a `tokio::sync::oneshot` receiver until something fills the
//! `state.conflict_resolution_tx` slot. The merge engine stores that sender
//! BEFORE emitting the event (see `volume_conflict.rs`'s Stop branch), so by the
//! time any sink observes the `emit_conflict` call the sender is already in the
//! slot. [`ConflictResponderSink`] exploits exactly that: it wraps an inner
//! [`CollectorEventSink`], forwards every event, and — the instant it sees a
//! conflict — synchronously `take()`s the sender and sends the scripted
//! [`ConflictResolutionResponse`]. The op's `rx.await` then returns immediately.
//!
//! This is order-independent by construction: there is no parallel counter to
//! race against the op future, and no 2 ms polling loop. Once the driven op
//! future completes, the inner collector's recorded conflicts ARE the
//! authoritative, race-free prompt count — `events` carries the paths and
//! file/folder flags too, so assertions derive from the sink, not a side-channel
//! `AtomicUsize`. See [`file_conflict_count`].

use std::sync::Arc;

use super::super::state::{ConflictResolutionResponse, WriteOperationState};
use super::super::types::{
    CollectorEventSink, ConflictInfo, ConflictResolution, DryRunResult, OperationEventSink, ScanProgressEvent,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteErrorEvent, WriteProgressEvent,
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
    resolution: ConflictResolution,
    apply_to_all: bool,
}

impl ConflictResponderSink {
    /// Answers every prompt with `resolution` / `apply_to_all`.
    pub(super) fn new(state: &Arc<WriteOperationState>, resolution: ConflictResolution, apply_to_all: bool) -> Self {
        Self {
            inner: CollectorEventSink::new(),
            state: Arc::clone(state),
            resolution,
            apply_to_all,
        }
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
        // send below races teardown), then answer it.
        self.inner.emit_conflict(e);

        // The sender was stored before this event was emitted, so the `take()`
        // can't miss. Sending unblocks the op's `rx.await` synchronously.
        if let Some(tx) = self.state.conflict_resolution_tx.lock_ignore_poison().take() {
            let _ = tx.send(ConflictResolutionResponse {
                resolution: self.resolution,
                apply_to_all: self.apply_to_all,
            });
        }
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: DryRunResult) {}
    fn emit_settled(&self, _e: WriteSettledEvent) {}
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
