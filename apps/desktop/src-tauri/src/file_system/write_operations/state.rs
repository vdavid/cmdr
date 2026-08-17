//! Operation state management and caches.
//!
//! Contains state tracking for in-progress operations and status caches for query APIs.

use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use super::eta::{EtaEstimator, EtaSample};
use super::human_wait::HumanWaitClock;
use super::types::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, OperationEventSink, TransferActivity,
    TransferWaitReason, WriteConflictEvent, WriteOperationType, WriteProgressEvent, WriteSettledEvent,
};

// The conflict slot lives in its own module: arbitrating one answer per conflict
// is a state machine, and it's the whole reason a second surface can be told it
// lost. Re-exported so the established `state::ConflictResolutionResponse` path
// keeps resolving for every caller.
pub use super::conflict_slot::{ConflictResolutionResponse, ConflictSlot};

// The operation-intent / pause-gate state machines and the scan-preview map
// live in sibling modules. Re-export them here so the established
// `state::OperationIntent`, `state::PauseGate`, `state::FileInfo`, etc. paths
// keep resolving for every caller. The preview map itself is NOT re-exported:
// it's private to `scan_cache`, reachable only through its functions, so no
// caller can seed or read an entry that skipped the coherence canary and the
// request binding.
pub use super::operation_intent::PauseGate;
pub(crate) use super::operation_intent::{OperationIntent, is_cancelled, load_intent};
#[cfg(test)]
pub(super) use super::scan_cache::insert_scan_result;
pub(super) use super::scan_cache::{CachedScanResult, FileInfo, ScanPreviewState, ScanResult};

// ============================================================================
// Operation state
// ============================================================================

/// State for an in-progress write operation.
pub struct WriteOperationState {
    /// Shared with native copy operations for cancellation checks.
    /// Encodes `OperationIntent` as a `u8`. Use `is_cancelled()` / `load_intent()` to read.
    pub intent: Arc<AtomicU8>,
    pub progress_interval: Duration,
    /// Where this operation's Stop-mode conflict stands: armed with the sender
    /// the parked operation is listening on, answered, or neither. Armed on
    /// demand when a conflict occurs, BEFORE the `write-conflict` event goes
    /// out. `resolve_write_conflict` answers through it; `abandon()` (what a
    /// cancel does) drops the sender, unblocking the receiver with an error the
    /// waiting code reads as cancellation. See [`ConflictSlot`].
    pub conflict_slot: ConflictSlot,
    /// Serializes Stop-mode conflict dispatch for this operation. There is exactly
    /// ONE human and ONE [`conflict_slot`](Self::conflict_slot), so two tasks that both hit
    /// a Stop-mode clash at once (the concurrent volume-copy spawn loop, or two deep
    /// directory merges running in parallel) must not race to emit a `write-conflict`
    /// and clobber each other's oneshot sender. The whole dispatch — re-check the
    /// latch, emit the conflict, await the response, store the latch — runs while
    /// holding this lock, so the prompts queue. The lock is NEVER held across the
    /// subsequent file write: we serialize the human, not the I/O.
    ///
    /// Lives here next to the conflict slot because they guard the same
    /// concern. A `tokio::sync::Mutex` (not std) so a task can `.await` the user's
    /// response — actually the response wait happens on the oneshot; the dispatch
    /// guard is dropped at end of the resolve step — but the guard itself must be
    /// held across `.await` points (the latch re-check reads volume state), which a
    /// std mutex can't span.
    pub conflict_dispatch_lock: tokio::sync::Mutex<()>,
    /// Per-operation ETA + throughput estimator. Fed by `enrich_progress_event`
    /// at every `write-progress` emit site, so every emitter (local copy/delete,
    /// volume copy/move, MTP, SMB) reports rates and ETA uniformly.
    pub estimator: std::sync::Mutex<EtaEstimator>,
    /// How long this operation has spent waiting on a PERSON: the pause the user
    /// pressed, and the conflict prompts they haven't answered yet. The
    /// [`pause_gate`](Self::pause_gate) and the
    /// [`conflict_slot`](Self::conflict_slot) each hold a handle and drive it
    /// from their own transitions; `enrich_progress` reads it so the estimator
    /// can charge those seconds to the person rather than to the transfer. See
    /// [`HumanWaitClock`].
    pub human_wait: Arc<HumanWaitClock>,
    /// Cooperative cancel flag for in-flight backend I/O. Flipped whenever the
    /// op transitions out of `Running` (via `cancel_write_operation`,
    /// `cancel_all_write_operations`, or `cancel_all_write_operations_with_rollback`).
    /// MTP volume ops thread this into `list_objects_with_cancel` /
    /// `delete_with_cancel` so an in-flight `GetObjectInfo` loop bails at the
    /// next per-handle USB boundary (≈one roundtrip) instead of running the
    /// full 950-photo `/DCIM/Camera` listing to completion. Non-MTP backends
    /// ignore it for now.
    ///
    /// A `CancellationToken` — the one cancellation primitive every layer of
    /// Cmdr speaks — rather than the mtp-rs `CancelToken` type, so this module
    /// doesn't pull mtp-rs onto non-MTP platforms; the MTP backend bridges the
    /// token to mtp-rs's poll-based flag at the entry point of each MTP-aware
    /// call.
    pub backend_cancel: CancellationToken,
    /// TIER 2: stop WAITING for in-flight backend I/O, rather than asking it to
    /// stop. Fired only by `abort_write_operation` / [`abort_all_write_operations`],
    /// which today means the quit deadline and nothing else.
    ///
    /// [`backend_cancel`](Self::backend_cancel) above is tier 1 and stays the
    /// default for every user-initiated cancel: it travels to the backend through
    /// the per-chunk `on_progress` callback, so the backend drops its own handle
    /// and deletes its own partial. That is the RIGHT wind-down, and the only
    /// thing wrong with it is that it is only observed once the in-flight chunk's
    /// read and write both return — on SMB, 20 s to send plus 30 s of server
    /// silence, so one chunk can hold a quit for ~30 s.
    ///
    /// Tier 2 is what a deadline holder fires when that is too long: the
    /// cross-volume streaming write is raced against this token
    /// (`transfer/volume/strategy.rs`), so the wait ends whether or not the
    /// backend ever comes back. The cost is that the backend's own cleanup is
    /// skipped, which is why it is NEVER fired by an ordinary cancel — the
    /// abandoned bytes are a registered `.cmdr-tmp-*` that the staging layer's
    /// startup sweep removes ([`super::in_flight_temps`]).
    ///
    /// ❌ Don't read this to decide anything a user asked for; it means "the app
    /// is going away", not "the user changed their mind".
    pub backend_abort: CancellationToken,
    /// Cooperative pause gate. The drivers call `pause_gate.wait_while_paused_*`
    /// at each between-files boundary, right after the `is_cancelled` check.
    /// Pause is orthogonal to `intent` (the cancel/rollback machine); the
    /// manager record's `LifecycleStatus` mirrors the paused bit for the UI.
    /// See [`PauseGate`].
    pub pause_gate: PauseGate,
    /// The operation-log journal target for a VOLUME (SMB / MTP) transfer: the
    /// `(source_volume_id, dest_volume_id)` the per-leaf record points journal
    /// under (a same-volume move passes the one id as both). Set by the volume
    /// copy/move deferreds via [`with_journal_volumes`](Self::with_journal_volumes);
    /// `None` for local-FS ops (which journal via the `"root"` helpers) and in
    /// tests that don't exercise journaling, so those record points no-op. Carried
    /// here — the op's shared context — because the `*_with_progress` bodies that
    /// own the per-leaf record points don't take the volume ids as params (they're
    /// called from ~80 test sites), mirroring how `op_id` reaches them.
    pub journal_volumes: Option<(String, String)>,
    /// Destination `.cmdr-tmp-*` paths this operation is CURRENTLY streaming
    /// bytes into, so an abandoned transfer's litter can be found and removed.
    ///
    /// Every cross-volume file write stages on a temp sibling and lands it only
    /// after its last byte (`transfer/staged_write.rs`). An entry is added before
    /// the first byte and removed the instant the write SUCCEEDS — after that the
    /// temp holds committed data, and a failed landing must leave it on disk, so
    /// a temp that is no longer listed here must never be swept. What remains
    /// after the driver's loop is exactly the set of half-written partials whose
    /// tasks were dropped mid-flight (a cancel that abandoned a wedged task);
    /// `transfer::volume::cleanup::clean_abandoned_staged_writes` removes them.
    ///
    /// Local-FS copies stage too (`overwrite::stage_and_land_file`) and register
    /// here, but they add and remove entries synchronously, so anything they
    /// leave belongs to a thread that never came back. No local driver runs
    /// `clean_abandoned_staged_writes`; the startup sweep is their answer.
    ///
    /// ❌ Don't push or retain here directly: go through
    /// [`super::in_flight_temps`], which also keeps the persisted half that
    /// outlives the process.
    pub in_flight_temps: std::sync::Mutex<Vec<PathBuf>>,
    /// The newest `write-progress` this operation emitted, kept so whoever has
    /// to speak for it while it stands still can re-send it
    /// ([`announce_human_wait`](Self::announce_human_wait), and the transfer
    /// probe's stall heartbeat). A wedged or parked operation emits nothing on
    /// its own, so without this the newest event any window holds is from
    /// before it stopped, and a confident speed stays on screen throughout.
    /// Cloning one event per emit is cheap next to the IPC hop it is already
    /// making.
    last_progress: std::sync::Mutex<Option<WriteProgressEvent>>,
    /// "This operation is still going." Dropped by [`end_liveness`](Self::end_liveness)
    /// when it settles.
    ///
    /// Everything this operation staged holds a [`Weak`](std::sync::Weak) to it
    /// (`file_system::staging`), so its scratch files stop being hidden the
    /// moment it ends. ❌ Not `Arc<WriteOperationState>` reachability: a task the
    /// driver ABANDONED after the cancel deadline still holds one of those, and
    /// the whole point is that a wedge's leftovers become visible anyway.
    liveness: std::sync::Mutex<Option<Arc<()>>>,
}

impl WriteOperationState {
    /// Construct a fresh state for a new operation. Use this from every
    /// `*_files_start` entry point; keeps the field list out of every call
    /// site so adding new state members (like the estimator) is one-line.
    pub fn new(progress_interval: Duration) -> Self {
        // One clock, two owners: whichever of them parks the operation, the
        // estimator reads the union.
        let human_wait = HumanWaitClock::shared();
        Self {
            intent: Arc::new(AtomicU8::new(OperationIntent::Running as u8)),
            progress_interval,
            conflict_slot: ConflictSlot::new(Arc::clone(&human_wait)),
            conflict_dispatch_lock: tokio::sync::Mutex::new(()),
            estimator: std::sync::Mutex::new(EtaEstimator::new()),
            backend_cancel: CancellationToken::new(),
            backend_abort: CancellationToken::new(),
            pause_gate: PauseGate::new(Arc::clone(&human_wait)),
            human_wait,
            journal_volumes: None,
            in_flight_temps: std::sync::Mutex::new(Vec::new()),
            last_progress: std::sync::Mutex::new(None),
            liveness: std::sync::Mutex::new(Some(Arc::new(()))),
        }
    }

    /// A handle to this operation's [`liveness`](Self::liveness), for tagging
    /// the scratch files it stages.
    pub fn liveness_token(&self) -> Option<std::sync::Weak<()>> {
        self.liveness.lock_ignore_poison().as_ref().map(Arc::downgrade)
    }

    /// Declares the operation over. Everything it staged stops being hidden.
    ///
    /// Called where the operation leaves `WRITE_OPERATION_STATE`, so it fires
    /// for every ending — done, cancelled, rolled back, or wedged and abandoned.
    pub(super) fn end_liveness(&self) {
        self.liveness.lock_ignore_poison().take();
    }

    /// Set the volume-transfer journal target (see [`journal_volumes`](Self::journal_volumes)).
    /// Chained before wrapping the state in an `Arc`, so the per-leaf record points
    /// in the volume copy/move bodies journal under the REAL volume ids.
    pub fn with_journal_volumes(mut self, source_volume_id: String, dest_volume_id: String) -> Self {
        self.journal_volumes = Some((source_volume_id, dest_volume_id));
        self
    }

    /// Re-anchor the rate estimator on counters that jumped without any bytes
    /// moving (the bulk-skip prelude credits every pre-known conflict at once).
    /// The one caller-facing wrapper over `EtaEstimator::reseed_baseline`, so
    /// no emit site has to know the estimator lives behind a lock or that the
    /// human-wait clock feeds it. A poisoned lock skips the re-anchor: the rate
    /// display is advisory, and a panic here would take the transfer with it.
    pub fn reseed_estimator_baseline(&self, bytes_done: u64, files_done: usize) {
        let now = Instant::now();
        if let Ok(mut est) = self.estimator.lock() {
            est.reseed_baseline(now, self.human_wait.total_at(now), bytes_done, files_done);
        }
    }

    /// Populate `bytes_per_second`, `files_per_second`, `eta_seconds`, and
    /// `activity` on a `WriteProgressEvent` before it's emitted. Call this from
    /// every `write-progress` emit site (local copy, local delete, trash, volume
    /// copy, volume move, MTP, SMB) so the FE sees uniform rates, ETA, and stall
    /// classification regardless of which backend produced the event.
    pub fn enrich_progress(&self, event: &mut WriteProgressEvent) {
        // Looked up by operation id rather than threaded through every emit
        // site's signature. An operation that keeps no in-flight table (local
        // copy, delete, trash) misses the lookup and answers for itself: the one
        // thing it can still say is that it has parked on a PERSON, and that is
        // the one thing a view must not have to guess. § `person_wait`.
        if let Some(activity) =
            crate::file_system::write_operations::transfer::transfer_probe::activity_for(&event.operation_id)
        {
            event.activity = Some(activity);
        } else if let Some(waiting_on) = self.person_wait() {
            event.activity = Some(TransferActivity {
                // No table, so no honest count; and a parked operation has been
                // still for nobody's time but the person's.
                in_flight: 0,
                still_for_seconds: 0,
                waiting_on,
            });
        }

        let now = Instant::now();
        let stats = match self.estimator.lock() {
            Ok(mut est) => est.update(EtaSample {
                now,
                // The seconds a person spent deciding are not the transfer's,
                // so the rate window doesn't get to count them.
                human_wait_total: self.human_wait.total_at(now),
                phase: event.phase,
                bytes_done: event.bytes_done,
                bytes_total: event.bytes_total,
                files_done: event.files_done,
                files_total: event.files_total,
            }),
            // Poisoned mutex (another thread panicked). Skip the enrichment
            // rather than propagating the panic; progress events are advisory.
            Err(_) => return,
        };
        event.bytes_per_second = Some(stats.bytes_per_second);
        event.files_per_second = Some(stats.files_per_second);
        event.eta_seconds = stats.eta_seconds;

        // Stash the finished event: see [`last_progress`](Self::last_progress).
        // WITHOUT its activity, which is the one field a re-send must never
        // replay: an operation is re-sent precisely because what it is doing has
        // changed, and a stored "waiting on you" would outlive the answer and
        // freeze the speed off the screen for the rest of the transfer.
        *self.last_progress.lock_ignore_poison() = Some(WriteProgressEvent {
            activity: None,
            ..event.clone()
        });
    }

    /// Who this operation is waiting on, when the answer is a PERSON.
    ///
    /// Both sources are the ones the human-wait clock tracks
    /// ([`super::human_wait`]), read in the same order
    /// `transfer_probe::wait_reason` reads them, so an operation with an
    /// in-flight table and one without classify the same wait the same way.
    /// `None` means nobody is being asked, which is not the same as "it is
    /// moving": a local operation with no probe genuinely cannot tell.
    fn person_wait(&self) -> Option<TransferWaitReason> {
        if self.pause_gate.is_paused() {
            return Some(TransferWaitReason::Paused);
        }
        if self.conflict_slot.is_awaiting() {
            return Some(TransferWaitReason::You);
        }
        None
    }

    /// The newest progress event this operation emitted, if it has emitted one,
    /// carrying no activity: a re-sender classifies the wait itself, or lets
    /// [`enrich_progress`](Self::enrich_progress) do it on the way out.
    pub(super) fn last_progress(&self) -> Option<WriteProgressEvent> {
        self.last_progress.lock_ignore_poison().clone()
    }

    /// The aggregate byte count of the newest progress event this operation
    /// published, without cloning the event: the number on the user's screen.
    ///
    /// `None` before the first emit. This is what the stall watchdog judges
    /// movement by (`transfer/transfer_probe.rs`), so the log, the dialog's
    /// stall notice, and the progress bar can't disagree about whether a
    /// transfer is moving. Every emit site funnels through
    /// [`enrich_progress`](Self::enrich_progress), so no driver has to remember
    /// to feed a second counter.
    pub(super) fn last_progress_bytes(&self) -> Option<u64> {
        self.last_progress.lock_ignore_poison().as_ref().map(|e| e.bytes_done)
    }

    /// Tell every window that this operation has just parked on a person, or
    /// just stopped being parked. Call it on BOTH edges of a wait.
    ///
    /// A parked operation emits nothing at all: it is between files, holding
    /// still on purpose, and the newest tick every window holds was measured
    /// while it was moving. Left alone, a copy frozen on a clash keeps a speed
    /// on screen, and a queue row watching it says "Running" (`AGENTS.md`
    /// principle 2: honest progress). This re-sends that same tick — counters
    /// untouched, because nothing moved — and [`enrich_progress`](Self::enrich_progress)
    /// re-classifies it on the way out, so the window learns what changed.
    ///
    /// A no-op before the first progress event, which is a state no clash can
    /// reach: every transfer emits its phase-transition tick before it opens a
    /// destination.
    pub(super) fn announce_human_wait(&self, sink: &dyn OperationEventSink) {
        let Some(event) = self.last_progress() else { return };
        self.emit_progress_via_sink(sink, event);
    }

    /// Enrich and emit a `WriteProgressEvent` via an `OperationEventSink`. The
    /// single emit path for all write ops: production wraps a Tauri AppHandle
    /// in `TauriEventSink`; tests use `CollectorEventSink` to capture events
    /// in a `Vec` instead of round-tripping through the Tauri runtime.
    pub fn emit_progress_via_sink(&self, sink: &dyn OperationEventSink, mut event: WriteProgressEvent) {
        self.enrich_progress(&mut event);
        sink.emit_progress(event);
    }
}

/// RAII guard that emits exactly one `write-settled` event when dropped.
///
/// Place at the top of every write-op spawned task. Whatever way the task
/// exits — happy path, error path, cancel, or panic via `JoinError` propagation
/// — the guard's `Drop` impl fires, so the FE always learns "this op's
/// background work is fully torn down; the volume is ready for the next op."
///
/// The guard takes the same injected `Arc<dyn OperationEventSink>` the rest of
/// the pipeline uses (built at the IPC edge as `TauriEventSink`, or a
/// `CollectorEventSink` in tests), so the full spawn-task lifecycle runs with no
/// Tauri runtime under test.
pub(crate) struct WriteSettledGuard {
    inner: Option<WriteSettledGuardInner>,
}

struct WriteSettledGuardInner {
    sink: Arc<dyn OperationEventSink>,
    operation_id: String,
    operation_type: WriteOperationType,
    volume_id: Option<String>,
}

impl WriteSettledGuard {
    /// Builds a guard that emits `write-settled` via `sink.emit_settled(...)`
    /// on drop.
    pub(crate) fn new(
        sink: Arc<dyn OperationEventSink>,
        operation_id: impl Into<String>,
        operation_type: WriteOperationType,
        volume_id: Option<String>,
    ) -> Self {
        Self {
            inner: Some(WriteSettledGuardInner {
                sink,
                operation_id: operation_id.into(),
                operation_type,
                volume_id,
            }),
        }
    }
}

impl Drop for WriteSettledGuard {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else { return };
        inner.sink.emit_settled(WriteSettledEvent {
            operation_id: inner.operation_id,
            operation_type: inner.operation_type,
            volume_id: inner.volume_id,
        });
    }
}

/// The live [`WriteOperationState`] of every registered operation, keyed by
/// operation id.
///
/// A struct with methods rather than a bare `static` map, because
/// [`cancel_all`](Self::cancel_all) is a WALK: it touches every entry at once.
/// A walk is only honestly testable against a registry the caller OWNS — driving
/// the process-global one from a test cancels whatever operations OTHER tests
/// have in flight at that moment. DETAILS § "Test isolation".
pub(super) struct WriteOperationRegistry {
    entries: RwLock<HashMap<String, Arc<WriteOperationState>>>,
}

impl WriteOperationRegistry {
    fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Registers `state` under `operation_id`, so cancel / pause / conflict
    /// resolution can reach it by id.
    pub(super) fn insert(&self, operation_id: String, state: Arc<WriteOperationState>) {
        if let Ok(mut entries) = self.entries.write() {
            entries.insert(operation_id, state);
        }
    }

    /// The live state for `operation_id`, if it's still registered. Returns an
    /// `Arc` rather than a guard so callers don't hold the registry lock while
    /// they touch the operation.
    pub(super) fn get(&self, operation_id: &str) -> Option<Arc<WriteOperationState>> {
        Some(Arc::clone(self.entries.read().ok()?.get(operation_id)?))
    }

    /// Whether `operation_id` still has a state entry.
    ///
    /// ❌ Test-only, and it must stay that way: this is presence in the state
    /// map, ❌ never "is it running". A paused operation keeps its entry and a
    /// queued one has none yet, so answering a lifecycle question with this got
    /// both backwards. `manager().lifecycle_status()` is that answer; the tests
    /// here use this to pin the difference.
    #[cfg(test)]
    pub(super) fn contains(&self, operation_id: &str) -> bool {
        self.entries
            .read()
            .is_ok_and(|entries| entries.contains_key(operation_id))
    }

    /// Drops `operation_id`'s state, ending its liveness first.
    ///
    /// ❌ Every removal goes through here. Removing the entry alone would leave a
    /// wedged operation's staged temps hidden forever (a task the driver
    /// abandoned still holds an `Arc` to the state, so the map entry going away
    /// doesn't drop it): `WriteOperationState::end_liveness`.
    pub(super) fn forget(&self, operation_id: &str) {
        let Ok(mut entries) = self.entries.write() else {
            return;
        };
        if let Some(state) = entries.remove(operation_id) {
            state.end_liveness();
        }
    }

    /// Stops every registered operation, keeping partials.
    ///
    /// Transitions each to `Stopped` (never `RollingBack`: teardown must not
    /// silently delete files with no visual feedback), flips `backend_cancel` so
    /// in-flight backend I/O bails, drops any pending conflict sender to unblock
    /// its waiter, and wakes a paused op so it observes the cancel. Already-
    /// `Stopped` operations are left alone.
    pub(super) fn cancel_all(&self) {
        let Ok(entries) = self.entries.read() else {
            return;
        };
        for (id, state) in entries.iter() {
            let current = load_intent(&state.intent);
            if current != OperationIntent::Stopped {
                log::info!("cancel_all_write_operations: stopping op={id}");
                state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
                state.backend_cancel.cancel();
                // Drop the conflict resolution sender to unblock any waiting receiver
                state.conflict_slot.abandon();
                // Wake a paused, parked op so teardown's cancel is observed.
                state.pause_gate.wake();
            }
        }
    }

    /// Stops every registered operation and stops WAITING for the ones that
    /// don't answer. See [`abort_all_write_operations`].
    pub(super) fn abort_all(&self) {
        // Tier 1 first, and unconditionally: an abort is a cancel that ran out
        // of patience, so an operation must never observe tier 2 without the
        // cooperative signal that gives its backends the chance to wind down
        // cleanly in the moments before the process goes.
        self.cancel_all();
        let Ok(entries) = self.entries.read() else {
            return;
        };
        for (id, state) in entries.iter() {
            if !state.backend_abort.is_cancelled() {
                log::info!("abort_all_write_operations: no longer waiting for op={id}");
                state.backend_abort.cancel();
            }
        }
    }
}

/// Global registry of in-progress write operation states.
pub(super) static WRITE_OPERATION_STATE: LazyLock<WriteOperationRegistry> = LazyLock::new(WriteOperationRegistry::new);

/// Drops `operation_id`'s state, ending its liveness first. See
/// [`WriteOperationRegistry::forget`].
pub(super) fn forget_operation(operation_id: &str) {
    WRITE_OPERATION_STATE.forget(operation_id);
}

// The operation status cache, the busy-volume set it drives, and the queries
// over it live in `status_cache`. Re-exported here so the established
// `state::register_operation_status`, `state::busy_volume_ids`, etc. paths keep
// resolving for every caller, the same way `operation_intent` and `scan_cache`
// are surfaced above.
pub use super::status_cache::{
    VolumesBusyChanged, busy_volume_ids, get_operation_status, init_busy_volume_emitter, list_active_operations,
};
#[cfg(target_os = "macos")]
pub(crate) use super::status_cache::{register_external_volume_op, release_external_volume_op};
pub(super) use super::status_cache::{register_operation_status, unregister_operation_status, update_operation_status};

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

/// Stops all in-progress write operations without rollback.
///
/// Used as a safety net when the frontend is tearing down (beforeunload, hot-reload).
/// Transitions to `Stopped` (not `RollingBack`) because teardown must never silently
/// delete files in the background without visual feedback.
pub fn cancel_all_write_operations() {
    WRITE_OPERATION_STATE.cancel_all();
}

/// TIER 2 for one operation: cancel it, and stop waiting for whatever in-flight
/// backend call doesn't answer.
///
/// A plain [`cancel_write_operation`] reaches a backend through its per-chunk
/// `on_progress` callback, so a write that never returns never sees it. This runs
/// that cancel AND fires [`WriteOperationState::backend_abort`], which the
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
/// Cancels every live operation, then fires [`WriteOperationState::backend_abort`]
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
pub(super) fn pause_write_operation(operation_id: &str) -> bool {
    if let Some(state) = WRITE_OPERATION_STATE.get(operation_id) {
        state.pause_gate.pause();
        return true;
    }
    false
}

/// Clears the pause flag on the live state for `operation_id`, waking the gate.
/// Returns `true` if a state existed. Resuming a not-paused op is a harmless
/// no-op.
pub(super) fn resume_write_operation(operation_id: &str) -> bool {
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

// ============================================================================
// Copy transaction for rollback
// ============================================================================

/// Tracks created files/directories for rollback on failure.
///
/// If dropped without calling `commit()`, automatically rolls back
/// (deletes) all recorded files and directories. This ensures cleanup
/// even if a thread panics during the copy loop.
#[cfg_attr(test, derive(Debug))]
pub(crate) struct CopyTransaction {
    /// In creation order.
    pub created_files: Vec<PathBuf>,
    /// In creation order.
    pub created_dirs: Vec<PathBuf>,
    /// Set to `true` by `commit()` to prevent rollback on drop.
    committed: bool,
}

impl CopyTransaction {
    pub fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
            committed: false,
        }
    }

    pub fn record_file(&mut self, path: PathBuf) {
        self.created_files.push(path);
    }

    pub fn record_dir(&mut self, path: PathBuf) {
        self.created_dirs.push(path);
    }

    /// Rolls back all created files and directories.
    ///
    /// Intentional: rollback removes the files THIS operation created; it does
    /// NOT restore an original that an Overwrite replaced (we keep no per-file
    /// backup — see `overwrite::safe_overwrite_file` step 4). Keeping backups for
    /// the whole operation risks unexpectedly filling the user's drive on a
    /// large Overwrite. Revisit if users complain. See transfer/volume/DETAILS.md
    /// § "Overwrite isn't reversible".
    pub fn rollback(&self) {
        // Delete files first (in reverse order)
        for file in self.created_files.iter().rev() {
            let _ = std::fs::remove_file(file);
        }
        // Then directories (deepest first, already in reverse due to creation order)
        for dir in self.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(dir);
        }
    }

    /// Marks the transaction as committed, preventing rollback on drop.
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for CopyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            log::warn!(
                "CopyTransaction dropped without commit, rolling back {} files and {} dirs",
                self.created_files.len(),
                self.created_dirs.len()
            );
            self.rollback();
        }
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
