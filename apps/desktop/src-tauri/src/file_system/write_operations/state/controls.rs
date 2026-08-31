//! Reaching a REGISTERED operation by id and telling it something: stop, abort,
//! pause, resume, or "here's your answer to that conflict".
//!
//! Every function here is a lookup in `WRITE_OPERATION_STATE` followed by one
//! flag flip or one send, and every one of them tolerates an id that has
//! already settled: the caller holds a string a person or an agent typed, so
//! "no such operation" is an ordinary outcome, ❌ never a panic.
//!
//! Stopping has two tiers, and the split matters: `cancel_*` is what a click
//! reaches, `abort_*` is the quit deadline's and severs in-flight backend calls
//! outright. `transfer/DETAILS.md` § "Two tiers of cancel".

use std::sync::atomic::Ordering;

use super::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, ConflictResolutionResponse, OperationIntent,
    WRITE_OPERATION_STATE, WriteConflictEvent,
};

/// Cancels an in-progress write operation.
///
/// State transitions: `Running → RollingBack` (rollback=true), `Running → Stopped`
/// (rollback=false), `RollingBack → Stopped` (cancel during rollback). Other transitions are
/// no-ops.
///
/// # Arguments
/// * `operation_id` - The operation ID to cancel
/// * `rollback` - If true, roll back (delete created files). If false, stop and keep partial files.
pub fn cancel_write_operation(operation_id: &str, rollback: bool) {
    // Every exit from here logs. When Rollback appeared to do nothing in the
    // 2026-07-31 incident, the two silent no-op exits below (unknown operation,
    // invalid transition) were indistinguishable from "the intent was set and the
    // driver never observed it", which left the whole failure unexplained. See
    // `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`.
    let Some(state) = WRITE_OPERATION_STATE.get(operation_id) else {
        log::warn!("cancel_write_operation: op={operation_id} rollback={rollback}: no such operation, ignoring");
        return;
    };
    let target = if rollback {
        OperationIntent::RollingBack
    } else {
        OperationIntent::Stopped
    };
    let current = OperationIntent::from_u8(state.intent.load(Ordering::Relaxed));

    // Valid transitions: Running → RollingBack/Stopped, RollingBack → Stopped.
    // Stopped is terminal; no further transitions.
    let valid = matches!(
        (current, target),
        (OperationIntent::Running, _) | (OperationIntent::RollingBack, OperationIntent::Stopped)
    );
    if !valid {
        log::info!(
            "cancel_write_operation: op={operation_id} {current:?} -> {target:?} is not a valid transition, ignoring"
        );
        return;
    }

    log::info!("cancel_write_operation: op={operation_id} {current:?} -> {target:?}, signalling backends");
    state.intent.store(target as u8, Ordering::Relaxed);
    // Any transition out of `Running` should also stop in-flight backend
    // I/O (per-handle MTP loops, etc.) — not just the loop above it.
    state.backend_cancel.cancel();
    // Drop the conflict resolution sender to unblock any waiting receiver
    state.conflict_slot.abandon();
    // Cancellation wins over pause: wake a paused, parked op so it observes
    // the non-Running intent and bails. Leaves the paused flag set (the op
    // is going away regardless).
    state.pause_gate.wake();
}

/// TIER 1 for every live operation: stops them all, keeping partials.
///
/// Transitions to `Stopped` rather than `RollingBack` because a teardown must
/// never silently delete files with no visual feedback.
///
/// **The quit gate is the only caller** (`crate::quit::tear_down_and_exit`, step
/// one). A window going away is not a reason to stop work: an operation outlives
/// the view watching it, and a frontend unload handler calling this is the exact
/// defect the gate replaced. Pinned from the other side by
/// `src/lib/quit/no-teardown-cancel.test.ts`; the full rule is in `DETAILS.md`
/// § "Key patterns and gotchas (shared)".
pub fn cancel_all_write_operations() {
    WRITE_OPERATION_STATE.cancel_all();
}

/// TIER 2 for one operation: cancel it, and stop waiting for whatever in-flight
/// backend call doesn't answer.
///
/// A plain [`cancel_write_operation`] reaches a backend through its per-chunk
/// `on_progress` callback, so a write that never returns never sees it. This runs
/// that cancel AND fires [`super::WriteOperationState::backend_abort`], which the
/// cross-volume streaming write is raced against — so the wait ends on our clock
/// instead of the server's.
///
/// ❌ Not a cancel with a shorter fuse: the backend's own partial cleanup is
/// skipped, and the abandoned bytes are left to the staged-write sweep.
///
/// **Test-only, and that's the honest scope.** The one production caller is the
/// quit deadline, and a deadline always aborts EVERYTHING (see
/// [`abort_all_write_operations`]); there is no situation where one live
/// operation's wait is worth ending and its neighbour's isn't. The per-op form
/// stays because the tier-2 suites drive one operation at a time.
#[cfg(test)]
pub fn abort_write_operation(operation_id: &str) {
    cancel_write_operation(operation_id, false);
    let Some(state) = WRITE_OPERATION_STATE.get(operation_id) else {
        log::warn!("abort_write_operation: op={operation_id}: no such operation, ignoring");
        return;
    };
    log::info!("abort_write_operation: op={operation_id}: no longer waiting for in-flight backend calls");
    state.backend_abort.cancel();
}

/// TIER 2 for every live operation: what the quit deadline fires once the
/// cooperative cancel has had its chance.
///
/// Cancels every live operation, then fires [`super::WriteOperationState::backend_abort`]
/// on each: the cross-volume streaming write is raced against it, so a wait that
/// a dead server would own ends on our clock instead. The backend's own partial
/// cleanup is skipped; the staging layer and the startup sweep own the leftovers.
///
/// The caller owns the "has had its chance" part: cancel first, give the
/// operations a beat to settle, and call this for whatever is still there.
/// `crate::quit` is that caller, and ❌ nothing a person clicked ever is.
pub fn abort_all_write_operations() {
    WRITE_OPERATION_STATE.abort_all();
}

/// Sets the pause flag on the live state for `operation_id`, if present.
/// Returns `true` if a state existed (the op is in `WRITE_OPERATION_STATE`,
/// i.e. Running — including pause-gated Running). The op parks at its next
/// between-files boundary. Cancellation still wins: a paused op that's then
/// cancelled unblocks immediately. The manager record's `LifecycleStatus` is
/// flipped separately (see `manager::set_paused`).
pub fn pause_write_operation(operation_id: &str) -> bool {
    if let Some(state) = WRITE_OPERATION_STATE.get(operation_id) {
        state.pause_gate.pause();
        return true;
    }
    false
}

/// Clears the pause flag on the live state for `operation_id`, waking the gate.
/// Returns `true` if a state existed. Resuming a not-paused op is a harmless
/// no-op.
pub fn resume_write_operation(operation_id: &str) -> bool {
    if let Some(state) = WRITE_OPERATION_STATE.get(operation_id) {
        state.pause_gate.resume();
        return true;
    }
    false
}

/// Answers a pending conflict for an in-progress write operation, and REPORTS
/// what that answer did.
///
/// When an operation hits a conflict in Stop mode it emits a `WriteConflictEvent`
/// and parks until this is called; it then carries on with the chosen resolution.
/// The event broadcasts to every webview, so several surfaces can show the prompt
/// and each of them can be answered. Only the first answer reaches the operation;
/// the returned [`ConflictResolutionOutcome`] is how the rest find out, and it
/// crosses IPC so a losing surface can take its own prompt down.
///
/// The answer names the clash it is FOR ([`ConflictId`], carried on the event).
/// An operation raises its clashes one at a time, but an answer can arrive after
/// the operation has parked on the next one, and applying it there would decide a
/// question the user was never shown. Naming it makes that case `StaleAnswer`
/// instead.
///
/// # Arguments
/// * `operation_id` - The operation ID that has a pending conflict
/// * `conflict_id` - Which clash of that operation is being answered
/// * `resolution` - How to resolve the conflict (Skip, Overwrite, or Rename)
/// * `apply_to_all` - If true, apply this resolution to all future conflicts in this operation
pub fn resolve_write_conflict(
    operation_id: &str,
    conflict_id: ConflictId,
    resolution: ConflictResolution,
    apply_to_all: bool,
) -> ConflictResolutionOutcome {
    let Some(state) = WRITE_OPERATION_STATE.get(operation_id) else {
        log::info!("resolve_write_conflict: op={operation_id}: no such operation, ignoring");
        return ConflictResolutionOutcome::UnknownOperation;
    };
    let outcome = state.conflict_slot.answer(
        conflict_id,
        ConflictResolutionResponse {
            resolution,
            apply_to_all,
        },
    );
    log::info!(
        "resolve_write_conflict: op={operation_id} clash={conflict_id:?} {resolution:?} apply_to_all={apply_to_all} -> {outcome:?}"
    );
    outcome
}

/// The clash `operation_id` is parked on right now, or `None` when it isn't
/// asking anything.
///
/// `write-conflict` is a broadcast, so it only reaches whoever was listening
/// when it went out. This is the pull side of the same question, for a reader
/// that arrives afterwards: `cmdr://state` renders it so an agent can see WHICH
/// clash is owed an answer, and answer that one by id rather than guessing.
pub fn pending_write_conflict(operation_id: &str) -> Option<WriteConflictEvent> {
    WRITE_OPERATION_STATE.get(operation_id)?.conflict_slot.pending()
}
