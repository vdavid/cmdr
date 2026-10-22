//! Tracked rollback: removes the files a copy created, emitting reverse progress.

use std::time::Instant;

use crate::event_sinks::OperationEventSink;
use crate::ledger::CopyTransaction;
use crate::reversal::{ReversalTally, drained, remove_local_dir_if_empty, remove_local_file};
use crate::state::WindDown;
use crate::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};

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
    wind_down: &WindDown,
    operation_type: WriteOperationType,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> ReversalTally {
    let files_to_process = transaction.created_files().len();
    let mut tally = ReversalTally::default();
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event (same values as cancellation point)
    let emit = |current_file: Option<String>, files_left: usize, bytes_left: u64| {
        wind_down.report(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                operation_type,
                WriteOperationPhase::RollingBack,
                current_file,
                files_left,
                files_total,
                bytes_left,
                bytes_total,
            ),
        );
    };
    emit(None, files_at_cancel, bytes_at_cancel);

    // Reverse newest first, draining the ledger as it goes and checking for
    // cancellation before each. The intent is read BEFORE the pop: an entry taken
    // off the ledger and then left standing would be a file on disk nothing
    // claims any more.
    loop {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if wind_down.stop_requested() {
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
        if last_progress_time.elapsed() >= wind_down.progress_interval() {
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

        // E2E-only, like the copy loop's throttle: the host's rollback pace.
        if let Some(pause) = wind_down.pace() {
            std::thread::sleep(pause);
        }
    }

    // The directories this copy created, deepest-first and empty-only. No
    // progress events: this is fast, and it's leftovers rather than anything a
    // person is waiting on.
    for dir in transaction.created_dirs.iter().rev() {
        tally.record(remove_local_dir_if_empty(dir), dir);
    }

    // And the entries this copy renamed out of the way come home, now that the
    // directories standing on their names are gone. Not tallied: the counters
    // are about the items the person asked to copy, and one of their own files
    // returning isn't one of those. `reversal.rs` says the same.
    transaction.restore_displaced();

    // The frame that lands on zero, so a run whose last items fell inside the
    // throttle window still ends where it ended.
    emit(None, 0, 0);
    tally
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
