//! The seam between a scan preview and the operation that owns it.
//!
//! A confirmed transfer is registered with the manager immediately, before its
//! `TransferDialog` preview has finished walking. That gives it an
//! `operationId`, a queue row, its lanes, and a place in the quit gate from the
//! moment the user confirms — but the preview still emits under its own
//! `previewId` and the operation still has to wait for it. Two jobs live here:
//!
//! - **The wait.** [`await_claimed_preview`] parks the operation's task until
//!   its claimed preview settles, so the write starts against a finished scan
//!   instead of racing a second walk down the same tree. A preview that names
//!   nothing (evicted, a stale id from a reloaded window, or one another
//!   operation already claimed) is a miss, and a miss falls through to the
//!   operation's own foolproof re-scan — never a hang.
//! - **The progress bridge.** [`forward_scan_progress`] republishes a claimed
//!   preview's counts as the OWNER's `write-progress` in `phase: 'scanning'`.
//!   Without it every scan-phase surface goes blank rather than live:
//!   `scan-preview-progress` is keyed by `previewId` and carries no
//!   `operationId`, and nothing else emits for an operation that is only
//!   waiting.
//! - **The park.** [`ScanPause`] lets the walk honor its owner's Pause. The
//!   scan is the minutes-long part of a big transfer, so it is where a person
//!   presses Pause; a scan that carried on regardless would make the button a
//!   lie for exactly as long as it mattered.
//!
//! Both events keep firing. A pre-confirm dialog may still be watching the same
//! preview by `previewId`, and it has no operation to watch instead.

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};

use super::event_sinks::OperationEventSink;
use super::manager;
use super::scan_cache::{PREVIEW_SETTLED, ScanOutcome, ScanPreviewState, abandon_claim, finish_claim, poll_claim};
use super::scan_watchdog::ScanWatchdog;
use super::state::{WRITE_OPERATION_STATE, WriteOperationState, is_cancelled};
use super::types::{
    CancelRollback, WriteCancelledEvent, WriteErrorEvent, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};

/// What the wait concluded, in the operation's own vocabulary.
///
/// Deliberately carries no reason. The wait runs BEFORE the journal row opens,
/// so there is nothing for a caller to journal, and the cancelled-vs-errored
/// distinction is already out as this operation's terminal event by the time the
/// caller sees `Stopped`. A caller's only decision is settle-or-carry-on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScanWait {
    /// Walk the plan: the preview finished, or there was nothing to wait for.
    /// Either way the operation proceeds, consuming the cached result if one is
    /// there and re-scanning if not.
    Proceed,
    /// The operation is over before it wrote anything, and its terminal event
    /// is already out. Settle and return.
    Stopped,
}

impl ScanWait {
    /// `true` when the caller must stop; `false` to carry on.
    pub(super) fn stopped(&self) -> bool {
        matches!(self, ScanWait::Stopped)
    }
}

/// Waits for this operation's claimed scan preview and emits its own terminal
/// event if the wait ends without a usable result.
///
/// Call it as the first thing an operation's deferred start does, BEFORE the
/// journal open: an operation that never got past its scan never wrote a byte,
/// and journaling one would record work that didn't happen.
///
/// The wait ends four ways. A completed preview leaves its result in the cache
/// for the ordinary `take_cached_scan_result` a few lines later. A preview that
/// errored fails the operation with the walk's own message. A cancelled
/// preview, or a cancel issued against the OPERATION while it waits, cancels
/// the operation. And a preview nobody knows about is simply not waited on.
pub(super) async fn await_claimed_preview(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &WriteOperationState,
) -> ScanWait {
    let Some(preview_id) = manager::manager().claimed_preview_of(operation_id) else {
        return ScanWait::Proceed;
    };

    let outcome = tokio::select! {
        // A cancel while the operation waits: stop the walk it was waiting on
        // rather than letting it finish for nobody.
        () = state.backend_cancel.cancelled() => ScanOutcome::Cancelled,
        outcome = settled_outcome(&preview_id) => outcome,
    };

    match outcome {
        ScanOutcome::Complete => {
            // The claim ends here, and deliberately without freeing anything:
            // the result is what the operation is about to consume.
            finish_claim(&preview_id);
            manager::manager().end_scan_wait(operation_id);
            ScanWait::Proceed
        }
        ScanOutcome::Cancelled => {
            abandon_claim(&preview_id);
            manager::manager().end_scan_wait(operation_id);
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type,
                files_processed: 0,
                rollback: CancelRollback::none(),
            });
            ScanWait::Stopped
        }
        ScanOutcome::Error(message) => {
            abandon_claim(&preview_id);
            manager::manager().end_scan_wait(operation_id);
            events.emit_error(WriteErrorEvent::new(
                operation_id.to_string(),
                operation_type,
                WriteOperationError::IoError {
                    path: String::new(),
                    message,
                },
            ));
            ScanWait::Stopped
        }
    }
}

/// Resolves once `preview_id` has settled. Re-reads the map after arming the
/// wakeup, so a settle landing between the two can't be missed.
async fn settled_outcome(preview_id: &str) -> ScanOutcome {
    loop {
        let notified = PREVIEW_SETTLED.notified();
        tokio::pin!(notified);
        // Registers this waiter BEFORE the poll below. Without it a settle
        // between the poll and the await would wake nobody and the operation
        // would park until the next unrelated settle.
        notified.as_mut().enable();
        if let Some(outcome) = poll_claim(preview_id) {
            return outcome;
        }
        notified.await;
    }
}

// ============================================================================
// The park
// ============================================================================

/// The owning operation's pause gate, as a scan walk sees it.
///
/// **Where a scan parks: exactly where it already observes cancel.** A walk that
/// stopped less often than it can be cancelled would make Pause the weaker of
/// two buttons that mean the same thing to a person, and one that stopped more
/// often would need park points the backends don't offer. So every call site
/// puts [`park_while_paused`](Self::park_while_paused) immediately after its
/// existing cancel check, and pause inherits cancel's granularity for free: per
/// entry on the local walk and the oracle-aware volume walk, per source group
/// inside a cold-cache volume batch.
///
/// **Why the owner is resolved lazily.** A preview walks detached from the
/// operation that will consume it: the walk holds a `preview_id`, the gate hangs
/// off the operation, and the claim that joins them lands when the user confirms
/// — which may be after the walk started. Asking the preview map per entry would
/// put a lookup on the walk's hot path, so [`resolve_owner`](Self::resolve_owner)
/// runs at the walk's progress tick (already off that path) and the answer is
/// kept for life: a claim is one-shot, and the `Arc` outlives the operation's
/// record.
pub(super) struct ScanPause {
    /// The claim that names the owner, for a walk that has to look it up. `None`
    /// for a scan running inside the operation itself, which knows.
    claim: Option<PreviewClaimant>,
    /// The owner's live state. Resolved at most once.
    owner: OnceLock<Arc<WriteOperationState>>,
    /// Fed on both edges of a park, so a scan waiting on a person can't read as
    /// a volume that stopped answering and get killed by the inactivity bound.
    watchdog: Option<Arc<ScanWatchdog>>,
}

/// What a preview walk needs to find its owner and to know when it must stop
/// regardless.
struct PreviewClaimant {
    preview_id: String,
    state: Arc<ScanPreviewState>,
}

impl ScanPause {
    /// For a preview worker, which learns its owner from the claim.
    pub(super) fn for_preview(
        preview_id: String,
        state: Arc<ScanPreviewState>,
        watchdog: Arc<ScanWatchdog>,
    ) -> Self {
        Self {
            claim: Some(PreviewClaimant { preview_id, state }),
            owner: OnceLock::new(),
            watchdog: Some(watchdog),
        }
    }

    /// For a scan the operation runs for itself (no preview to consume: an
    /// evicted id, a stale one from a reloaded window, or a second operation
    /// over the same sources). The owner is known from the start.
    pub(super) fn for_operation(state: Arc<WriteOperationState>) -> Self {
        let owner = OnceLock::new();
        let _ = owner.set(state);
        Self {
            claim: None,
            owner,
            watchdog: None,
        }
    }

    /// Looks the owner up if it isn't known yet. Call it from the walk's
    /// progress tick — ❌ never per entry, which is what this design exists to
    /// keep free.
    pub(super) fn resolve_owner(&self) {
        if self.owner.get().is_some() {
            return;
        }
        let Some(claim) = &self.claim else { return };
        let Some(operation_id) = super::scan_cache::claimed_operation(&claim.preview_id) else {
            return;
        };
        let Some(state) = WRITE_OPERATION_STATE.get(&operation_id) else {
            return;
        };
        let _ = self.owner.set(state);
    }

    /// Parks the walking thread while the owner is paused. The whole cost on an
    /// unpaused walk is one atomic load, which is what lets this sit on the
    /// per-entry path next to the cancel check.
    pub(super) fn park_while_paused(&self) {
        let Some(owner) = self.paused_owner() else { return };
        self.enter_park();
        owner
            .pause_gate
            .wait_while_paused_sync_until(&|| self.should_stop(owner));
        self.leave_park();
    }

    /// Async twin of [`park_while_paused`](Self::park_while_paused), for the
    /// volume walk. ❌ Never park a volume scan on the sync waiter: it runs on a
    /// tokio worker, and a pause is as long as a person is thinking.
    pub(super) async fn park_while_paused_async(&self) {
        let Some(owner) = self.paused_owner() else { return };
        self.enter_park();
        owner
            .pause_gate
            .wait_while_paused_until(&|| self.should_stop(owner))
            .await;
        self.leave_park();
    }

    /// The owner, but only when it is actually paused: the fast-path filter
    /// both parks share.
    fn paused_owner(&self) -> Option<&Arc<WriteOperationState>> {
        let owner = self.owner.get()?;
        owner.pause_gate.is_paused().then_some(owner)
    }

    /// Everything that must end the park. The operation's intent covers a cancel
    /// aimed at the operation (which wakes the gate); the preview's own flag
    /// covers a walk told to stop directly. ❌ Dropping either leaves the thread
    /// on a gate nobody will open.
    fn should_stop(&self, owner: &WriteOperationState) -> bool {
        if is_cancelled(&owner.intent) {
            return true;
        }
        self.claim
            .as_ref()
            .is_some_and(|claim| claim.state.cancelled.load(Ordering::Relaxed))
    }

    fn enter_park(&self) {
        if let Some(watchdog) = &self.watchdog {
            watchdog.note_parked();
        }
    }

    fn leave_park(&self) {
        if let Some(watchdog) = &self.watchdog {
            watchdog.note_resumed();
        }
    }
}

/// Republishes a claimed preview's live counts as its owner's `write-progress`.
/// Called from the preview workers' own progress emits, alongside (never
/// instead of) `scan-preview-progress`.
///
/// `files_total` / `bytes_total` stay 0 for the whole scan: finding the totals
/// is what the scan is FOR, and a bar measured against a guess would jump when
/// the real numbers land. The index-derived expectation rides
/// `expected_files_total` / `expected_bytes_total` instead, which is what every
/// surface already treats as a hint.
pub(super) fn forward_scan_progress(preview_id: &str, counts: ScanCounts) {
    let Some(operation_id) = super::scan_cache::claimed_operation(preview_id) else {
        return;
    };
    emit_scan_progress(&operation_id, counts);
}

/// A scan tick, in the shape both preview workers can produce.
// DEFAULT-OK: the all-zero value is the honest opening tick — a walk that has
// counted nothing yet — which is exactly what `emit_initial_scan_tick` sends to
// turn a blank row live. It claims nothing about the disk.
#[derive(Debug, Clone, Default)]
pub(super) struct ScanCounts {
    pub files_found: usize,
    pub dirs_found: usize,
    pub bytes_found: u64,
    pub current_path: Option<String>,
    pub current_dir: Option<String>,
    pub expected_files_total: Option<u64>,
    pub expected_bytes_total: Option<u64>,
}

/// Emits one scanning-phase `write-progress` for `operation_id`.
///
/// The operation type is read back from the manager rather than threaded in:
/// every caller here is off the operation's own hot path, and a wrong type on a
/// progress event picks the wrong noun in the UI. An id the manager no longer
/// knows emits nothing, which is how a walk that outlives its cancelled owner
/// goes quiet.
pub(super) fn emit_scan_progress(operation_id: &str, counts: ScanCounts) {
    use tauri_specta::Event as _;

    let Some(operation_type) = manager::manager().operation_type_of(operation_id) else {
        return;
    };
    // A tick that raced the end of the wait would drag the frontend's phase back
    // to `scanning` after the write had already started, and the readout would
    // jump with it. Both emit paths are scan-phase-only, so one guard covers the
    // opening tick and every forwarded preview tick.
    if !manager::manager().is_in_scan_wait(operation_id) {
        return;
    }
    let event = WriteProgressEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        phase: WriteOperationPhase::Scanning,
        current_file: counts.current_path,
        current_dir: counts.current_dir,
        files_done: counts.files_found,
        files_total: 0,
        bytes_done: counts.bytes_found,
        bytes_total: 0,
        dirs_done: counts.dirs_found,
        bytes_per_second: None,
        files_per_second: None,
        eta_seconds: None,
        expected_files_total: counts.expected_files_total,
        expected_bytes_total: counts.expected_bytes_total,
        activity: None,
    };
    #[cfg(test)]
    record_tick_for_test(&event);
    // Absent before the app handle is wired (unit tests), where recording above
    // is the whole point of the call.
    let Some(app) = manager::operations_app_handle() else {
        return;
    };
    if let Err(e) = event.emit(&app) {
        log::warn!(target: "op_manager", "failed to forward scan progress for op={operation_id}: {e}");
    }
}

/// Emits the one synthetic tick that turns a scanning row from blank into live.
///
/// ⚠️ Call it AFTER the `operations-changed` that first carries the row.
/// `applyProgress` in the frontend store early-returns for an id with no
/// snapshot yet, so a tick that beats its own snapshot is dropped and the row
/// stays blank until the next real preview event — which on a preview near its
/// end may never come, and that is precisely the case this tick exists for.
pub(super) fn emit_initial_scan_tick(operation_id: &str) {
    emit_scan_progress(operation_id, ScanCounts::default());
}

// ============================================================================
// Test observation
// ============================================================================

/// One forwarded scan tick, stamped with the manager's broadcast counter at the
/// moment it went out. The stamp is what makes the ORDERING assertable: a tick
/// carrying a count lower than the `operations-changed` that first announced the
/// row is a tick the frontend store discards, which is indistinguishable from no
/// tick at all.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct ObservedScanTick {
    pub operation_id: String,
    pub files_done: usize,
    pub bytes_done: u64,
    pub emits_before: u64,
}

#[cfg(test)]
static OBSERVED_TICKS: std::sync::LazyLock<std::sync::Mutex<Vec<ObservedScanTick>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

#[cfg(test)]
fn record_tick_for_test(event: &WriteProgressEvent) {
    use crate::ignore_poison::IgnorePoison;
    OBSERVED_TICKS.lock_ignore_poison().push(ObservedScanTick {
        operation_id: event.operation_id.clone(),
        files_done: event.files_done,
        bytes_done: event.bytes_done,
        emits_before: manager::manager().emit_count(),
    });
}

/// Every scanning-phase tick forwarded for `operation_id` so far, oldest first.
#[cfg(test)]
pub(crate) fn observed_scan_ticks(operation_id: &str) -> Vec<ObservedScanTick> {
    use crate::ignore_poison::IgnorePoison;
    OBSERVED_TICKS
        .lock_ignore_poison()
        .iter()
        .filter(|t| t.operation_id == operation_id)
        .cloned()
        .collect()
}
