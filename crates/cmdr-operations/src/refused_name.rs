//! What a transfer does when the destination refuses an item's NAME: it asks
//! the person, once per item, Skip / Skip all / Stop (cmdr-reports#83).
//!
//! A name the destination can't store (too long for an encrypted NAS share, a
//! reserved name like `CON`) fails that one item and nothing else: every other
//! file in the batch still has a perfectly good name. Ending a 1,000-file move
//! at the first one sank the whole move over a few files. So the item parks on
//! a person, exactly like a Stop-mode clash, and through the SAME machinery:
//! the operation's [`ConflictSlot`](super::conflict_slot::ConflictSlot), its
//! dispatch mutex, its ids, `write-conflict` / `write-conflict-resolved`, and the
//! human-wait clock. The prompt is a [`WriteConflictEvent`] whose
//! `refused_name` says what was refused, so every surface that can answer a
//! clash (the progress dialog, the standalone prompt, an agent over MCP) can
//! answer this one with no new plumbing.
//!
//! The answers:
//!
//! - **Skip**: the item stays where it is, counted as skipped. A move keeps its
//!   source: the caller records it the way it records a conflict Skip, which is
//!   what the source sweep spares.
//! - **Skip with apply-to-all**: also latches
//!   [`RefusedNames`](super::refused_names_latch::RefusedNames), so every later
//!   refused name in this operation skips without asking. Its own latch, ❌
//!   never the clash one: "Skip all" here says nothing about clashes.
//! - **Stop**: any other resolution. The operation ends with the refusal as its
//!   error, as it did before this prompt existed. The dialog's Cancel and
//!   Rollback are the ordinary cancel, which abandons the slot.
//!
//! Only a refusal Cmdr reaches AFTER the item's bytes were written into a
//! scratch name, or before any were, is asked about: either way nothing of the
//! item is left at the destination (the failed landing's temp is cleaned up by
//! the write that made it), so a Skip leaves nothing behind.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::oneshot;

use super::conflict_slot::ConflictResolutionResponse;
use super::event_sinks::OperationEventSink;
use super::state::WriteOperationState;
use super::types::{
    ConflictId, ConflictResolution, KnownSize, RefusedNameReason, WriteConflictEvent, WriteConflictResolvedEvent,
    WriteOperationError,
};
use cmdr_fs::log_private::LogPrivate;

/// Why `error` is the destination refusing a NAME, or `None` when it's any
/// other failure. Typed on the variant, ❌ never the message.
pub(crate) fn refusal_reason(error: &WriteOperationError) -> Option<RefusedNameReason> {
    match error {
        WriteOperationError::NameTooLong { .. } => Some(RefusedNameReason::TooLong),
        WriteOperationError::InvalidName { .. } => Some(RefusedNameReason::NotAllowed),
        _ => None,
    }
}

/// What the person decided about one refused name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefusedNameAnswer {
    /// Leave the item out and carry on.
    Skip,
    /// End the operation with the refusal as its error.
    Stop,
    /// The operation was cancelled while (or before) it asked: nothing was
    /// decided about the item.
    Cancelled,
}

/// The item a destination refused, as the prompt names it.
pub(crate) struct RefusedItem<'a> {
    pub operation_id: &'a str,
    pub source_path: &'a Path,
    /// Where the item was meant to land, under the name that was refused.
    pub dest_path: &'a Path,
    /// The source's size when known (a file's; a folder's recursive one is
    /// never walked for a prompt).
    pub source_size: Option<u64>,
    pub source_is_directory: bool,
    pub reason: RefusedNameReason,
}

impl RefusedItem<'_> {
    fn prompt(&self, conflict_id: ConflictId) -> WriteConflictEvent {
        WriteConflictEvent {
            operation_id: self.operation_id.to_string(),
            conflict_id,
            source_path: self.source_path.display().to_string(),
            destination_path: self.dest_path.display().to_string(),
            source_size: self.source_size.map(|bytes| KnownSize::Exact { bytes }),
            // Nothing is at the destination: the name is what it refused.
            destination_size: None,
            source_modified: None,
            destination_modified: None,
            destination_is_newer: false,
            size_difference: None,
            source_is_directory: self.source_is_directory,
            destination_is_directory: false,
            destination_is_look_alike: false,
            refused_name: Some(self.reason),
        }
    }
}

/// Asks about a refused name on an ASYNC engine (every volume transfer),
/// serialized with the operation's clash prompts on the dispatch mutex.
///
/// ❌ Never from a future only a merge walker polls (a merge LEAF): the walker
/// may hold the mutex for a clash of its own, and would wait on itself. The
/// walker asks for its leaves (`transfer/volume/merge.rs`).
pub(crate) async fn ask_async(
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    item: &RefusedItem<'_>,
) -> RefusedNameAnswer {
    // One human and one slot, so a refused name queues behind any clash on
    // screen, and the other way round. Held across the wait, ❌ never across a
    // write: the caller writes nothing for this item.
    let _dispatch_guard = state.conflict_dispatch_lock.lock().await;
    if let Some(decided) = decided_without_asking(state, item) {
        return decided;
    }
    let (tx, rx) = oneshot::channel();
    let conflict_id = arm_and_emit(state, events, item, tx);
    let response = rx.await;
    settle(state, events, item, conflict_id, response.ok())
}

/// [`ask_async`] for the LOCAL engine, which runs on a blocking-pool thread and
/// blocks it on the answer, as its clash prompt does (`conflict.rs`). The local
/// engine is serial, so nothing else can be asking.
pub(crate) fn ask_blocking(
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    item: &RefusedItem<'_>,
) -> RefusedNameAnswer {
    if let Some(decided) = decided_without_asking(state, item) {
        return decided;
    }
    let (tx, rx) = oneshot::channel();
    let conflict_id = arm_and_emit(state, events, item, tx);
    let response = rx.blocking_recv();
    settle(state, events, item, conflict_id, response.ok())
}

/// A stopped operation asks nothing (a prompt emitted now would never be
/// answered: the dialog is tearing down), and a latched "Skip all" already
/// answered.
fn decided_without_asking(state: &WriteOperationState, item: &RefusedItem<'_>) -> Option<RefusedNameAnswer> {
    if state.work_stopped() {
        return Some(RefusedNameAnswer::Cancelled);
    }
    if state.refused_names.skips_all() {
        log::info!(
            "refused name: skipping {} under an earlier \"Skip all\"",
            LogPrivate::path(item.dest_path)
        );
        return Some(RefusedNameAnswer::Skip);
    }
    None
}

/// Arms the slot, THEN announces the wait, THEN emits: a surface can answer from
/// inside `emit_conflict`, and the order is what keeps that answer landing on an
/// armed slot and the wait announced before it's over
/// (`DETAILS.md` § "Parking on a person").
fn arm_and_emit(
    state: &WriteOperationState,
    events: &dyn OperationEventSink,
    item: &RefusedItem<'_>,
    tx: oneshot::Sender<ConflictResolutionResponse>,
) -> ConflictId {
    let event = state.conflict_slot.arm(tx, |conflict_id| item.prompt(conflict_id));
    let conflict_id = event.conflict_id;
    log::info!(
        "refused name: {} can't be stored at the destination ({:?}); asking",
        LogPrivate::path(item.dest_path),
        // allowed-log-private: a `RefusedNameReason`, a closed enum of our own
        item.reason
    );
    state.announce_human_wait(events);
    events.emit_conflict(event);
    conflict_id
}

/// Turns the slot's answer into the decision, closing the prompt for every
/// surface. `None` is a dropped sender: a cancel.
fn settle(
    state: &WriteOperationState,
    events: &dyn OperationEventSink,
    item: &RefusedItem<'_>,
    conflict_id: ConflictId,
    response: Option<ConflictResolutionResponse>,
) -> RefusedNameAnswer {
    let Some(response) = response else {
        return RefusedNameAnswer::Cancelled;
    };
    state.announce_human_wait(events);
    events.emit_conflict_resolved(WriteConflictResolvedEvent {
        operation_id: item.operation_id.to_string(),
        conflict_id,
    });
    match response.resolution {
        ConflictResolution::Skip => {
            if response.apply_to_all {
                state.refused_names.latch_skip_all();
            }
            RefusedNameAnswer::Skip
        }
        // No other answer can do anything with a name the destination won't
        // store (there's nothing to overwrite, and a free ` (1)` name is just as
        // long), so it stops rather than guessing at consent.
        other => {
            log::info!(
                "refused name: {:?} can't apply to {}, so the operation stops",
                other,
                LogPrivate::path(item.dest_path)
            );
            RefusedNameAnswer::Stop
        }
    }
}

#[cfg(test)]
#[path = "refused_name_tests.rs"]
mod tests;
