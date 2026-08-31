//! Tracked rollback: removes the files a copy created, emitting reverse progress.

use std::sync::Arc;
use std::time::Instant;

use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::ledger::CopyTransaction;
use crate::file_system::write_operations::reversal::{
    ReversalTally, drained, remove_local_dir_if_empty, remove_local_file,
};
use crate::file_system::write_operations::state::{
    OperationIntent, WriteOperationState, load_intent, update_operation_status,
};
use crate::file_system::write_operations::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};

/// Rolls back created files with progress events, checking for cancellation between deletions.
///
/// Emits progress events with _decreasing_ `files_done` / `bytes_done` so the frontend's
/// progress bars count backwards from the cancellation point toward zero (no UI flicker,
/// no separate rollback view). Both axes are interpolated over the LEDGER's own length, so
/// a reversal that walks its whole ledger lands the bar on zero whether it removed every
/// entry or left some standing — a bar stranded at 94% reads as a crash, and a user who
/// thinks the app crashed never reads the summary that would have explained things.
///
/// Every entry is rechecked immediately before it's removed; one something else changed
/// since is left alone and counted, never deleted. What got left, and why, comes back in
/// the returned [`ReversalTally`]. Does NOT call `transaction.commit()`: the caller must
/// commit unconditionally, since this function already removed whatever it removed.
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
pub(super) fn rollback_with_progress(
    transaction: &mut CopyTransaction,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    operation_type: WriteOperationType,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> ReversalTally {
    let files_to_process = transaction.created_files().len();
    let mut tally = ReversalTally::default();
    let mut last_progress_time = Instant::now();

    // The bar drains from here, so tell the estimator which way it runs before
    // the first frame.
    state.reversal_drains_the_bar();

    // Emit initial rollback phase event (same values as cancellation point)
    let emit = |current_file: Option<String>, files_left: usize, bytes_left: u64| {
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                operation_type,
                WriteOperationPhase::RollingBack,
                current_file.clone(),
                files_left,
                files_total,
                bytes_left,
                bytes_total,
            ),
        );
        update_operation_status(
            operation_id,
            WriteOperationPhase::RollingBack,
            current_file,
            files_left,
            files_total,
            bytes_left,
            bytes_total,
        );
    };
    emit(None, files_at_cancel, bytes_at_cancel);

    // Reverse newest first, draining the ledger as it goes and checking for
    // cancellation before each. The intent is read BEFORE the pop: an entry taken
    // off the ledger and then left standing would be a file on disk nothing
    // claims any more.
    loop {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if load_intent(&state.intent) == OperationIntent::Stopped {
            log::info!(
                "rollback_with_progress: rollback cancelled at {}/{} files, keeping remaining",
                tally.processed(),
                files_to_process,
            );
            tally.mark_canceled();
            return tally;
        }

        let Some(entry) = transaction.pop_file() else {
            break;
        };
        // Rechecked here, one item before the act — ❌ never in a batch, where a
        // verification would age while other items were processed.
        tally.record(remove_local_file(&entry), &entry.path);

        // Throttled progress events with decreasing values. The counters advance
        // for every entry the reversal walked past, removed or not.
        if last_progress_time.elapsed() >= state.progress_interval {
            let (files_left, bytes_left) =
                drained(files_at_cancel, bytes_at_cancel, tally.processed(), files_to_process);
            let current_file_name = entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            emit(Some(current_file_name), files_left, bytes_left);
            last_progress_time = Instant::now();
        }
    }

    // The directories this copy created, deepest-first and empty-only. No
    // progress events: this is fast, and it's leftovers rather than anything a
    // person is waiting on.
    for dir in transaction.created_dirs.iter().rev() {
        tally.record(remove_local_dir_if_empty(dir), dir);
    }

    // The frame that lands on zero, so a run whose last items fell inside the
    // throttle window still ends where it ended.
    emit(None, 0, 0);
    tally
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
