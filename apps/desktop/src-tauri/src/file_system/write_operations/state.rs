//! Operation state management and caches.
//!
//! Contains state tracking for in-progress operations and status caches for query APIs.

use crate::ignore_poison::IgnorePoison;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use super::eta::EtaEstimator;
use super::types::{
    ConflictResolution, OperationEventSink, OperationStatus, OperationSummary, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent, WriteSettledEvent,
};

// The operation-intent / pause-gate state machines and the scan-preview caches
// live in sibling modules. Re-export them here so the established
// `state::OperationIntent`, `state::PauseGate`, `state::FileInfo`, etc. paths
// keep resolving for every caller. The completed-result map itself is NOT
// re-exported: it's private to `scan_cache`, reachable only through
// `insert_scan_result` / `take_cached_scan_result` / `cached_scan_totals` /
// `release_scan_result`, so no caller can seed or read an entry that skipped
// the coherence canary and the request binding.
pub use super::operation_intent::PauseGate;
pub(crate) use super::operation_intent::{OperationIntent, is_cancelled, load_intent};
pub(super) use super::scan_cache::{
    CachedScanResult, FileInfo, SCAN_PREVIEW_STATE, ScanPreviewState, ScanResult, insert_scan_result,
    release_scan_result,
};

// ============================================================================
// Operation state
// ============================================================================

/// State for an in-progress write operation.
pub struct WriteOperationState {
    /// Shared with native copy operations for cancellation checks.
    /// Encodes `OperationIntent` as a `u8`. Use `is_cancelled()` / `load_intent()` to read.
    pub intent: Arc<AtomicU8>,
    pub progress_interval: Duration,
    /// Sender for conflict resolution. Created on demand when a conflict occurs;
    /// the receiver is held by the waiting operation. `resolve_write_conflict` takes
    /// the sender and sends the resolution. Dropping the sender unblocks the receiver
    /// with an error, which the waiting code interprets as cancellation.
    pub conflict_resolution_tx: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<ConflictResolutionResponse>>>,
    /// Serializes Stop-mode conflict dispatch for this operation. There is exactly
    /// ONE human and ONE `conflict_resolution_tx` slot, so two tasks that both hit
    /// a Stop-mode clash at once (the concurrent volume-copy spawn loop, or two deep
    /// directory merges running in parallel) must not race to emit a `write-conflict`
    /// and clobber each other's oneshot sender. The whole dispatch — re-check the
    /// latch, emit the conflict, await the response, store the latch — runs while
    /// holding this lock, so the prompts queue. The lock is NEVER held across the
    /// subsequent file write: we serialize the human, not the I/O.
    ///
    /// Lives here next to `conflict_resolution_tx` because they guard the same
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
    /// stop. Fired only by [`abort_write_operation`] / [`abort_all_write_operations`],
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
        Self {
            intent: Arc::new(AtomicU8::new(OperationIntent::Running as u8)),
            progress_interval,
            conflict_resolution_tx: std::sync::Mutex::new(None),
            conflict_dispatch_lock: tokio::sync::Mutex::new(()),
            estimator: std::sync::Mutex::new(EtaEstimator::new()),
            backend_cancel: CancellationToken::new(),
            backend_abort: CancellationToken::new(),
            pause_gate: PauseGate::new(),
            journal_volumes: None,
            in_flight_temps: std::sync::Mutex::new(Vec::new()),
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

    /// Populate `bytes_per_second`, `files_per_second`, `eta_seconds`, and
    /// `activity` on a `WriteProgressEvent` before it's emitted. Call this from
    /// every `write-progress` emit site (local copy, local delete, trash, volume
    /// copy, volume move, MTP, SMB) so the FE sees uniform rates, ETA, and stall
    /// classification regardless of which backend produced the event.
    pub fn enrich_progress(&self, event: &mut WriteProgressEvent) {
        // Looked up by operation id rather than threaded through every emit
        // site's signature. Operations that keep no in-flight table (local copy,
        // delete, trash) miss the lookup and keep whatever the caller set, which
        // is `None` everywhere except the probe's own stall heartbeat.
        if let Some(activity) =
            crate::file_system::write_operations::transfer::transfer_probe::activity_for(&event.operation_id)
        {
            event.activity = Some(activity);
        }

        let stats = match self.estimator.lock() {
            Ok(mut est) => est.update(
                Instant::now(),
                event.phase,
                event.bytes_done,
                event.bytes_total,
                event.files_done,
                event.files_total,
            ),
            // Poisoned mutex (another thread panicked). Skip the enrichment
            // rather than propagating the panic; progress events are advisory.
            Err(_) => return,
        };
        event.bytes_per_second = Some(stats.bytes_per_second);
        event.files_per_second = Some(stats.files_per_second);
        event.eta_seconds = stats.eta_seconds;

        // Stash the finished event so the probe's watchdog can re-send it while
        // nothing moves. A wedged transfer fires no chunk callbacks, so without
        // this the UI's newest event is from before the wedge and keeps a
        // confident ETA on screen for as long as the wedge lasts.
        crate::file_system::write_operations::transfer::transfer_probe::record_progress(event);
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

/// Response to a conflict resolution request.
#[derive(Debug, Clone)]
pub struct ConflictResolutionResponse {
    pub resolution: ConflictResolution,
    pub apply_to_all: bool,
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

    /// Whether `operation_id` is still registered, i.e. still running.
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
                let _ = state.conflict_resolution_tx.lock_ignore_poison().take();
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

/// Global cache for operation status (for query APIs).
static OPERATION_STATUS_CACHE: LazyLock<RwLock<HashMap<String, OperationStatusInternal>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Internal status tracking for operations.
#[derive(Debug, Clone)]
struct OperationStatusInternal {
    operation_type: WriteOperationType,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    started_at: u64,
    /// Volume IDs this operation reads from or writes to (source and/or
    /// destination). Drives the "busy volumes" set the volume picker reads to
    /// disable Eject while a transfer touches that device. Empty for pure
    /// same-`root` local ops (root is never ejectable). Populated only for
    /// cross-volume copy/move and volume-aware delete, where an ejectable
    /// USB / DMG / SMB / MTP volume can be involved.
    volume_ids: Vec<String>,
}

// ============================================================================
// Status cache management
// ============================================================================

/// Updates the internal status for an operation.
pub(super) fn update_operation_status(
    operation_id: &str,
    phase: WriteOperationPhase,
    current_file: Option<String>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
) {
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write()
        && let Some(status) = cache.get_mut(operation_id)
    {
        status.phase = phase;
        status.current_file = current_file;
        status.files_done = files_done;
        status.files_total = files_total;
        status.bytes_done = bytes_done;
        status.bytes_total = bytes_total;
    }
}

/// Registers a new operation in the status cache.
///
/// `volume_ids` lists every volume the operation touches (source and/or
/// destination). Pass an empty `Vec` for pure same-`root` local ops. Any
/// ejectable volume in the list is marked "busy" until the op unregisters, so
/// the volume picker can disable Eject for it (see `busy_volume_ids`).
pub(super) fn register_operation_status(
    operation_id: &str,
    operation_type: WriteOperationType,
    volume_ids: Vec<String>,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Raise the priority transfer gauge alongside the eject busy set: these two
    // register/unregister functions are the ONE write-op lifecycle choke point, so
    // indexing's "a transfer trumps me" signal (`crate::priority::transfers`) and
    // eject's guard can't drift apart. The finish fires from the same panic-safe
    // cleanup paths that unregister.
    crate::priority::transfers::note_transfer_started(&volume_ids);
    if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.insert(
            operation_id.to_string(),
            OperationStatusInternal {
                operation_type,
                phase: WriteOperationPhase::Scanning,
                current_file: None,
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
                started_at: now,
                volume_ids,
            },
        );
    }
    recompute_and_emit_busy_volumes();
}

/// Removes an operation from the status cache.
pub(super) fn unregister_operation_status(operation_id: &str) {
    let removed_volume_ids = if let Ok(mut cache) = OPERATION_STATUS_CACHE.write() {
        cache.remove(operation_id).map(|status| status.volume_ids)
    } else {
        None
    };
    // Lower the priority transfer gauge with the SAME ids the register raised it
    // with (they ride the cache entry, so the pair can't drift). A double
    // unregister removed nothing and lowers nothing.
    if let Some(volume_ids) = removed_volume_ids {
        crate::priority::transfers::note_transfer_finished(&volume_ids);
    }
    recompute_and_emit_busy_volumes();
}

// ============================================================================
// External busy-volume seam (drag-out file promises)
// ============================================================================
//
// `register_operation_status` / `unregister_operation_status` are `pub(super)`,
// reachable only inside `write_operations`. The drag-out fulfillment service
// (`crate::native_drag::fulfillment`) lives outside this module but needs the
// same eject guard: while it streams bytes off an MTP/SMB device into a
// Finder-chosen destination, the source volume must register as busy so the
// user can't eject the phone mid-download (the server-side `eject_volume` guard
// reads `busy_volume_ids()`). It is NOT a real write operation — no
// `WRITE_OPERATION_STATE` entry, no progress events, no settle — so it can't go
// through `start_write_operation`. This thin `pub(crate)` pair is the smallest
// honest seam: it touches only the `OPERATION_STATUS_CACHE` half (which is what
// `recompute_and_emit_busy_volumes` reads), keeping the busy set and the
// `volumes-busy-changed` event firing exactly as a real op would.

/// Marks `volume_ids` busy for the duration of an external (non-write-op)
/// operation, keyed by `op_id`. Used by the drag-out fulfillment service to
/// guard the source volume against eject while a promise is streaming. Pair
/// every call with [`release_external_volume_op`] (the fulfillment service uses
/// an RAII guard so release fires on every exit path).
///
/// Registers under `WriteOperationType::Copy` because a drag-out download IS a
/// copy from the device to local disk — the type only affects diagnostics
/// (`list_active_operations`), and the busy set itself is type-agnostic.
///
/// macOS-only: the sole caller is `native_drag::fulfillment`, which is
/// `#[cfg(target_os = "macos")]`. On other targets this would be dead code under
/// `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) fn register_external_volume_op(op_id: &str, volume_ids: Vec<String>) {
    register_operation_status(op_id, WriteOperationType::Copy, volume_ids);
}

/// Clears the busy mark registered by [`register_external_volume_op`].
#[cfg(target_os = "macos")]
pub(crate) fn release_external_volume_op(op_id: &str) {
    unregister_operation_status(op_id);
}

// ============================================================================
// Busy-volumes set (drives "disable Eject while an op touches this device")
// ============================================================================

/// Typed `volumes-busy-changed` Tauri event. Wraps the busy volume-ID list in a
/// struct because `tauri_specta::Event` payloads must be named types (a bare
/// `Vec<String>` can't derive `Event`). The struct name kebab-cases to
/// `volumes-busy-changed`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumesBusyChanged {
    /// IDs of volumes with an in-flight copy / move / delete operation (sorted).
    pub volume_ids: Vec<String>,
}

/// App handle for emitting `volumes-busy-changed`. Set once at startup via
/// `init_busy_volume_emitter`. Absent in unit tests, where the recompute is a
/// no-op emit (the set is still queryable via `busy_volume_ids`).
static BUSY_APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Last busy set we emitted, so a progress update that doesn't change
/// membership produces no event (the cache changes on every progress tick, but
/// the busy set only changes when an op starts or finishes).
static LAST_EMITTED_BUSY: LazyLock<std::sync::Mutex<HashSet<String>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));

/// Stores the app handle used to broadcast `volumes-busy-changed`. Call once at
/// app setup, before any write op can run.
pub fn init_busy_volume_emitter(app: &tauri::AppHandle) {
    let _ = BUSY_APP.set(app.clone());
}

/// Computes the current set of busy volume IDs: the union of every active
/// operation's touched volumes, minus the default `root` volume (never
/// ejectable, so marking it busy is pointless noise).
fn compute_busy_volume_ids() -> HashSet<String> {
    let Ok(cache) = OPERATION_STATUS_CACHE.read() else {
        return HashSet::new();
    };
    cache
        .values()
        .flat_map(|status| status.volume_ids.iter())
        .filter(|id| id.as_str() != crate::file_system::volume::DEFAULT_VOLUME_ID)
        .cloned()
        .collect()
}

/// Returns the busy volume IDs (sorted, for a stable payload / bootstrap).
/// Used by the `get_busy_volume_ids` bootstrap command, the `eject_volume`
/// guard, and the native breadcrumb-menu builder.
pub fn busy_volume_ids() -> Vec<String> {
    let mut ids: Vec<String> = compute_busy_volume_ids().into_iter().collect();
    ids.sort();
    ids
}

/// Recomputes the busy set and emits `volumes-busy-changed` only when its
/// membership changed. Called from register/unregister (the only two points
/// where membership can change), so it's panic-safe: unregister runs from the
/// manager's `ManagedTaskGuard` / external-seam cleanup that fires even on unwind.
fn recompute_and_emit_busy_volumes() {
    let current = compute_busy_volume_ids();

    {
        let mut last = LAST_EMITTED_BUSY.lock_ignore_poison();
        if *last == current {
            return;
        }
        *last = current.clone();
    }

    let Some(app) = BUSY_APP.get() else {
        return;
    };
    let mut volume_ids: Vec<String> = current.into_iter().collect();
    volume_ids.sort();
    use tauri_specta::Event as _;
    let payload = VolumesBusyChanged { volume_ids };
    if let Err(e) = payload.emit(app) {
        crate::log_error!(target: "eject", "Failed to emit volumes-busy-changed: {}", e);
    }
}

// Panic-safe cache cleanup for managed write ops now lives in
// `manager::ManagedTaskGuard` (it also frees the op's lane slots). The old
// standalone `OperationStateGuard` was retired when every spawn path moved
// behind the operation manager.

// ============================================================================
// Public query functions
// ============================================================================

/// Lists all active write operations.
///
/// Returns a list of operation summaries for all currently running operations.
/// This is useful for showing a global progress view or managing multiple concurrent operations.
pub fn list_active_operations() -> Vec<OperationSummary> {
    let cache = match OPERATION_STATUS_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .map(|(id, status)| {
            let percent_complete = if status.bytes_total > 0 {
                ((status.bytes_done as f64 / status.bytes_total as f64) * 100.0).min(100.0) as u8
            } else if status.files_total > 0 {
                ((status.files_done as f64 / status.files_total as f64) * 100.0).min(100.0) as u8
            } else {
                0
            };

            OperationSummary {
                operation_id: id.clone(),
                operation_type: status.operation_type,
                phase: status.phase,
                percent_complete,
                started_at: status.started_at,
            }
        })
        .collect()
}

/// Gets the detailed status of a specific operation.
///
/// Returns `None` if the operation is not found (either never existed or already completed).
pub fn get_operation_status(operation_id: &str) -> Option<OperationStatus> {
    let cache = OPERATION_STATUS_CACHE.read().ok()?;
    let status = cache.get(operation_id)?;

    // Check if the operation is still running
    let is_running = WRITE_OPERATION_STATE.contains(operation_id);

    Some(OperationStatus {
        operation_id: operation_id.to_string(),
        operation_type: status.operation_type,
        phase: status.phase,
        is_running,
        current_file: status.current_file.clone(),
        files_done: status.files_done,
        files_total: status.files_total,
        bytes_done: status.bytes_done,
        bytes_total: status.bytes_total,
        started_at: status.started_at,
    })
}

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
    let _ = state.conflict_resolution_tx.lock_ignore_poison().take();
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

/// Resolves a pending conflict for an in-progress write operation.
///
/// When an operation encounters a conflict in Stop mode, it emits a WriteConflictEvent
/// and waits for this function to be called. The operation will then proceed with the
/// chosen resolution.
///
/// # Arguments
/// * `operation_id` - The operation ID that has a pending conflict
/// * `resolution` - How to resolve the conflict (Skip, Overwrite, or Rename)
/// * `apply_to_all` - If true, apply this resolution to all future conflicts in this operation
pub fn resolve_write_conflict(operation_id: &str, resolution: ConflictResolution, apply_to_all: bool) {
    if let Some(state) = WRITE_OPERATION_STATE.get(operation_id) {
        // Take the sender and send the resolution through the oneshot channel
        let tx = state.conflict_resolution_tx.lock_ignore_poison().take();
        if let Some(tx) = tx {
            let _ = tx.send(ConflictResolutionResponse {
                resolution,
                apply_to_all,
            });
        }
    }
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
    /// large Overwrite. Revisit if users complain. See transfer/CLAUDE.md
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
