//! The operation manager: the single coordinator every write op flows through.
//!
//! Before this existed, five independent spawn paths (`start_write_operation`
//! plus the volume-delete branch in `mod.rs`, `copy_between_volumes`,
//! `move_between_volumes`, `move_within_same_volume`) each hand-rolled a
//! `tokio::spawn` + state-cache insert + status register + settle guard, and an
//! op always spawned immediately. The manager unifies them behind one seam
//! (`spawn_managed`) and adds the missing backbone: a registry with real
//! lifecycle states and **lane-based admission** that can serialize ops which
//! would thrash a shared device.
//!
//! Streaming transfers/deletes (copy/move/delete/trash) flow through
//! `spawn_managed`. The scan-free, near-instant metadata ops (rename / mkdir /
//! mkfile) flow through [`run_instant`](OperationManager::run_instant) instead:
//! they register + mark their volumes busy but reserve NO lane and run NO
//! admission pass (a metadata syscall must never queue behind a multi-minute
//! transfer), run inline, and return their result. See `run_instant` for the
//! full contract; the sections below describe the `spawn_managed` path.
//!
//! ## Lanes
//!
//! Each op touches the [`LaneKey`](crate::file_system::volume::LaneKey)s of its
//! source and destination volumes (same-volume ops touch one). A lane has
//! budget 1 in v1: an op runs only when EVERY lane it touches is free, and
//! reserves all of them atomically. So two MTP ops (same device lane)
//! serialize, two ops on the same disk serialize, but an MTP→local op and a
//! local→other-disk op (disjoint lanes) run in parallel.
//!
//! ## Admission — global FIFO, atomic multi-lane reservation
//!
//! One ordered queue. An admission pass walks pending ops oldest-first and
//! admits the first whose every lane is free, reserving all its slots at once.
//! A two-lane op can't starve behind churn on a single lane (no per-lane
//! queues). On admission the op is marked Running, its volumes are registered
//! busy, and its deferred start spawns the real work.
//!
//! ## Deferred start, not "spawn then block on a semaphore"
//!
//! A queued op holds only DATA describing how to begin (a boxed `FnOnce`
//! returning a future), never a parked thread. Blocking a spawned op on a lane
//! semaphore would pin a `spawn_blocking` pool thread idle per queued op — a
//! leak that can deadlock the finite pool. We spawn only on admission.
//!
//! ## Dequeue on settle — explicit, NOT in `Drop`
//!
//! The spawned task calls [`on_settled`](OperationManager::on_settled) on
//! normal exit: it frees the op's lane slots, cleans the caches, and runs an
//! admission pass (which may spawn the next op). The `Drop` safety net only
//! frees slots and cleans caches — it NEVER spawns. Spawning during the
//! previous op's unwind would re-enter the manager mid-panic (abort) or
//! deadlock on a lock held up-stack. So a panicking op still releases its
//! lanes, but the next op is admitted only on a healthy settle.
//!
//! ## Busy-volumes set
//!
//! The "disable Eject while a device is in use" set derives from
//! `OPERATION_STATUS_CACHE`, which the manager populates ONLY for Running ops
//! (a Queued op isn't touching the device yet) and the external drag-out seam
//! (`register_external_volume_op`) populates directly. So the busy set stays
//! `(running manager ops' volumes) ∪ (external registrations)` with no
//! double-maintenance. See `state.rs` § "Busy-volumes set".

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use crate::file_system::volume::LaneKey;
use crate::ignore_poison::IgnorePoison;

use super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, register_operation_status, unregister_operation_status,
};
use super::types::WriteOperationType;

/// Lifecycle status of a managed operation, as shown in the queue window.
/// `Paused` is set only by the pause/resume path (`set_paused`); the rest flow
/// from admission and settle. Distinct from `WriteOperationPhase` (the progress
/// phase: Scanning/Copying/Flushing) and from `OperationIntent` (the
/// cancel/rollback machine) — a paused op is still `Running`-intent and may be
/// mid-`Copying`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    /// Registered, waiting for its lanes to free.
    Queued,
    /// Admitted; its deferred start has spawned the real work.
    Running,
    /// Running but pause-gated: the op is parked between files and still holds
    /// its lane slots. Set by the pause/resume path.
    Paused,
    /// Finished successfully.
    Done,
    /// Cancelled by the user (keep-partials).
    Cancelled,
    /// Could not complete.
    Failed,
}

/// What the manager needs to know about an op to register, schedule, and
/// surface it. The deferred start is held separately so the descriptor stays
/// cheaply cloneable for the `operations-changed` snapshot.
pub(crate) struct OperationDescriptor {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    /// Lanes this op occupies while running (deduped; usually 1 or 2). Source
    /// and destination volume lanes.
    pub lanes: Vec<LaneKey>,
    /// Volume IDs to mark busy while the op runs (eject guard). Empty for pure
    /// same-`root` local ops. Mirrors the old `register_operation_status` arg.
    pub volume_ids: Vec<String>,
    /// Short source→dest summary for the queue window. Best-effort.
    pub summary: OperationSummaryText,
}

/// Best-effort human-readable source/destination summary for the queue window.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationSummaryText {
    pub source: Option<String>,
    pub destination: Option<String>,
}

/// A deferred start: data describing how to begin the real work. Spawned only
/// on admission. The future it returns owns the whole op (settle guard, the
/// actual transfer/delete, terminal-event emit) and ends by calling
/// `OperationManager::on_settled(id)`.
type DeferredStart = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

struct OpRecord {
    descriptor: OperationDescriptor,
    status: LifecycleStatus,
    /// Taken on admission. `None` once the op is Running (or for an op admitted
    /// the instant it was registered).
    deferred: Option<DeferredStart>,
    /// Lanes currently reserved by this op (set on admission, cleared on
    /// free). Lets lane-freeing be idempotent across the happy-path
    /// `on_settled` and the `Drop` safety net.
    reserved_lanes: Vec<LaneKey>,
}

/// One thin registry snapshot row (membership + lifecycle status, NOT 200 ms
/// progress). The queue window subscribes to `operations-changed` for the row
/// set and to the per-file `write-progress` stream for live bars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationSnapshot {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub status: LifecycleStatus,
    pub source: Option<String>,
    pub destination: Option<String>,
}

/// Typed `operations-changed` Tauri event carrying the thin registry snapshot
/// (membership + lifecycle status, NOT 200 ms progress). The struct name
/// kebab-cases to `operations-changed`. The queue window subscribes to it for
/// the row set and to the per-file `write-progress` stream for live bars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct OperationsChanged {
    pub operations: Vec<OperationSnapshot>,
}

struct ManagerInner {
    /// id → record. Active ops only (removed on settle).
    records: HashMap<String, OpRecord>,
    /// FIFO admission order: every active op's id in registration order. Walked
    /// oldest-first on each admission pass.
    order: Vec<String>,
    /// lane key → in-use count. Budget 1 per lane in v1, so a lane is free iff
    /// its count is 0. A `HashMap` (not a set) keeps the door open for budgets
    /// > 1 in v2 without reshaping the reservation logic.
    lane_use: HashMap<LaneKey, usize>,
}

impl ManagerInner {
    fn lane_free(&self, lane: &LaneKey) -> bool {
        self.lane_use.get(lane).copied().unwrap_or(0) < LANE_BUDGET
    }

    fn reserve(&mut self, lanes: &[LaneKey]) {
        for lane in lanes {
            *self.lane_use.entry(lane.clone()).or_insert(0) += 1;
        }
    }

    fn release(&mut self, lanes: &[LaneKey]) {
        for lane in lanes {
            if let Some(count) = self.lane_use.get_mut(lane) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.lane_use.remove(lane);
                }
            }
        }
    }

    /// Builds the thin snapshot for `operations-changed`, in FIFO order.
    fn snapshot(&self) -> Vec<OperationSnapshot> {
        self.order
            .iter()
            .filter_map(|id| self.records.get(id))
            .map(|rec| OperationSnapshot {
                operation_id: rec.descriptor.operation_id.clone(),
                operation_type: rec.descriptor.operation_type,
                status: rec.status,
                source: rec.descriptor.summary.source.clone(),
                destination: rec.descriptor.summary.destination.clone(),
            })
            .collect()
    }
}

/// Lane budget per lane in v1: serialize within a lane. v2 makes this
/// per-lane and configurable (e.g. FTP = min(5, server limit)).
const LANE_BUDGET: usize = 1;

/// The single coordinator. Holds the registry, the FIFO order, and the lane
/// table under one mutex (the critical sections are tiny — register, admit,
/// free — so one lock keeps the invariants obvious without lock-ordering
/// hazards). Spawning happens OUTSIDE the lock.
pub(crate) struct OperationManager {
    inner: Mutex<ManagerInner>,
}

/// Global manager handle. `OnceLock` rather than `LazyLock` only because the
/// app handle for emitting `operations-changed` is set at startup; the manager
/// itself has no construction args, so a `LazyLock` backs it.
static MANAGER: LazyLock<OperationManager> = LazyLock::new(OperationManager::new);

/// App handle for emitting `operations-changed`. Set once at startup via
/// `init_operation_event_emitter`. Absent in unit tests (the emit is a no-op;
/// the registry is still queryable via `list_operations`).
static OPERATIONS_APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Returns the global operation manager.
pub(crate) fn manager() -> &'static OperationManager {
    &MANAGER
}

/// Stores the app handle used to broadcast `operations-changed`. Call once at
/// app setup, before any write op can run (mirrors `init_busy_volume_emitter`).
pub fn init_operation_event_emitter(app: &tauri::AppHandle) {
    let _ = OPERATIONS_APP.set(app.clone());
}

impl OperationManager {
    fn new() -> Self {
        Self {
            inner: Mutex::new(ManagerInner {
                records: HashMap::new(),
                order: Vec::new(),
                lane_use: HashMap::new(),
            }),
        }
    }

    /// Registers an op and runs an admission pass. Returns immediately (the UI
    /// shows the queued/running row at once); the real work spawns only if the
    /// op is admitted on this pass.
    ///
    /// `state` is inserted into `WRITE_OPERATION_STATE` here so the op id is
    /// valid for cancel/conflict-resolution the instant it's registered. The
    /// `deferred` future owns the op end-to-end and MUST call `on_settled(id)`
    /// on its normal exit.
    pub(crate) fn spawn_managed(
        &'static self,
        descriptor: OperationDescriptor,
        state: Arc<WriteOperationState>,
        deferred: DeferredStart,
    ) {
        let operation_id = descriptor.operation_id.clone();

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.insert(operation_id.clone(), state);
        }

        {
            let mut inner = self.inner.lock_ignore_poison();
            inner.records.insert(
                operation_id.clone(),
                OpRecord {
                    descriptor,
                    status: LifecycleStatus::Queued,
                    deferred: Some(deferred),
                    reserved_lanes: Vec::new(),
                },
            );
            inner.order.push(operation_id);
        }

        self.run_admission_pass();
        self.emit_changed();
    }

    /// Walks the pending queue oldest-first and admits the first op whose every
    /// lane is free, reserving all its slots atomically and spawning its
    /// deferred start. Repeats until no further op can be admitted on this pass
    /// (admitting one frees nothing, but a single pass may admit several
    /// disjoint-lane ops). Spawns OUTSIDE the lock.
    fn run_admission_pass(&'static self) {
        loop {
            let to_spawn = {
                let mut inner = self.inner.lock_ignore_poison();
                // Find the oldest Queued op whose every lane is free. Computed
                // without a nested borrow of `inner` (the iterator borrows
                // `order`; `records`/`lane_use` are read through `inner` in the
                // body), so resolve the id in a plain loop.
                let mut admit_id: Option<String> = None;
                for id in &inner.order {
                    if let Some(rec) = inner.records.get(id)
                        && rec.status == LifecycleStatus::Queued
                        && rec.descriptor.lanes.iter().all(|l| inner.lane_free(l))
                    {
                        admit_id = Some(id.clone());
                        break;
                    }
                }
                let Some(admit_id) = admit_id else {
                    break;
                };

                // Reserve + flip to Running + take the deferred start.
                let (lanes, volume_ids, op_type) = {
                    let rec = inner.records.get(&admit_id).expect("just found");
                    (
                        rec.descriptor.lanes.clone(),
                        rec.descriptor.volume_ids.clone(),
                        rec.descriptor.operation_type,
                    )
                };
                inner.reserve(&lanes);
                let deferred = {
                    let rec = inner.records.get_mut(&admit_id).expect("just found");
                    rec.status = LifecycleStatus::Running;
                    rec.reserved_lanes = lanes;
                    rec.deferred.take()
                };
                (admit_id, volume_ids, op_type, deferred)
            };

            let (admit_id, volume_ids, op_type, deferred) = to_spawn;
            // Mark the volumes busy now that the op is actually running (a
            // Queued op isn't touching the device). The external drag-out seam
            // registers directly; together they form the busy-set union.
            register_operation_status(&admit_id, op_type, volume_ids);
            log::info!(target: "op_manager", "admit op={admit_id}");

            match deferred {
                Some(start) => {
                    tokio::spawn(start());
                }
                None => {
                    // Should never happen: a Queued op always has its deferred
                    // start. Free what we reserved so a logic bug can't wedge
                    // the lane forever.
                    crate::log_error!(target: "op_manager", "admitted op={admit_id} had no deferred start; releasing lanes");
                    self.on_settled(&admit_id);
                }
            }
        }
    }

    /// Happy-path dequeue: frees the op's lane slots, cleans the caches,
    /// removes the record, and runs an admission pass (which may spawn the next
    /// op). Called by the spawned task on its NORMAL exit, sequenced after the
    /// terminal event like the old cache cleanup. Idempotent: a later `Drop`
    /// safety net for the same op is a no-op.
    pub(crate) fn on_settled(&'static self, operation_id: &str) {
        self.free_and_remove(operation_id);
        self.run_admission_pass();
        self.emit_changed();
    }

    /// Runs a scan-free, near-instant op (rename / mkdir / mkfile) INLINE under
    /// manager bookkeeping, returning the op's own result to the caller.
    ///
    /// Registers a `Running` record (so it shows in the queue snapshot and gets
    /// an id), marks its volumes busy (the eject guard, via
    /// `register_operation_status`), awaits `op` inline, then frees. It does NOT
    /// reserve a lane and does NOT go through admission: a metadata syscall must
    /// never queue behind a multi-minute transfer (an inline rename that hangs
    /// until its IPC timeout is worse than useless, and the MTP/SMB connection
    /// layer already serializes physical device access). The command layer wraps
    /// this in its own IPC timeout; nothing here spawns.
    ///
    /// **RAII cleanup is mandatory, not happy-path only.** The command wraps this
    /// in a `tokio::time::timeout`, so a slow op that exceeds it makes the timeout
    /// DROP this future mid-`op.await`; the async volume path can also panic.
    /// Either exit MUST still free the record AND unregister the busy status, or
    /// the eject guard sticks ON forever (the volume can never be ejected again)
    /// and a phantom `Running` row lingers. An `InstantTaskGuard` held across the
    /// `op.await` guarantees that on drop/unwind; the happy path frees explicitly
    /// then disarms it.
    ///
    /// No `WriteOperationState` is inserted (instant ops have no
    /// intent/pause/conflict oneshot). Consequence: `cancel_operation` on an
    /// instant op is a safe no-op (`cancel_if_queued` is false for a Running op,
    /// then `cancel_write_operation` finds no state).
    pub(crate) async fn run_instant<T>(
        &'static self,
        descriptor: OperationDescriptor,
        op: impl Future<Output = T>,
    ) -> T {
        let operation_id = descriptor.operation_id.clone();
        let op_type = descriptor.operation_type;
        let volume_ids = descriptor.volume_ids.clone();

        // Register a Running record directly — no lane reservation, no admission
        // gate. There are no `.await`s between the insert, the busy-register, and
        // arming the guard below, so no drop can slip in and orphan the busy set.
        {
            let mut inner = self.inner.lock_ignore_poison();
            inner.records.insert(
                operation_id.clone(),
                OpRecord {
                    descriptor,
                    status: LifecycleStatus::Running,
                    deferred: None,
                    reserved_lanes: Vec::new(),
                },
            );
            inner.order.push(operation_id.clone());
        }
        register_operation_status(&operation_id, op_type, volume_ids);
        self.emit_changed();
        log::info!(target: "op_manager", "run instant op={operation_id}");

        // The RAII net: on a timeout-drop of this future or a panic in `op`, the
        // guard's Drop frees the record + unregisters the busy status (and
        // re-emits the snapshot) during unwind. Held across the `op.await`.
        let guard = InstantTaskGuard::new(operation_id.clone());

        let result = op.await;

        // Happy path: free + re-emit, then disarm (its Drop is now a no-op).
        // Do NOT run an admission pass — instant ops reserve no lanes, so nothing
        // waits on them.
        self.free_and_remove(&operation_id);
        self.emit_changed();
        guard.disarm();

        result
    }

    /// Frees lanes + cleans caches + removes the record for `operation_id`,
    /// without admitting anything. The shared core of `on_settled` (happy
    /// path) and the `Drop` safety net. Idempotent.
    fn free_and_remove(&self, operation_id: &str) {
        let removed = {
            let mut inner = self.inner.lock_ignore_poison();
            match inner.records.remove(operation_id) {
                Some(rec) => {
                    inner.release(&rec.reserved_lanes);
                    inner.order.retain(|id| id != operation_id);
                    true
                }
                None => false,
            }
        };
        if removed {
            if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
                cache.remove(operation_id);
            }
            unregister_operation_status(operation_id);
        }
    }

    /// Cancels a Queued op WITHOUT spawning it: drops it from the registry and
    /// frees its (unreserved) state. Returns `true` if it removed a Queued op,
    /// `false` if the op was Running/Paused/absent (the caller then routes
    /// through the existing `cancel_write_operation` intent path).
    pub(crate) fn cancel_if_queued(&'static self, operation_id: &str) -> bool {
        let was_queued = {
            let mut inner = self.inner.lock_ignore_poison();
            match inner.records.get(operation_id) {
                Some(rec) if rec.status == LifecycleStatus::Queued => {
                    inner.records.remove(operation_id);
                    inner.order.retain(|id| id != operation_id);
                    true
                }
                _ => false,
            }
        };
        if was_queued {
            // A queued op never reserved lanes nor registered busy status, so
            // only the `WRITE_OPERATION_STATE` entry needs clearing.
            if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
                cache.remove(operation_id);
            }
            log::info!(target: "op_manager", "cancel queued op={operation_id}");
            self.emit_changed();
        }
        was_queued
    }

    /// Flips a Running op's record between `Running` and `Paused` and re-emits
    /// `operations-changed`. Pause does NOT touch lanes (a paused Running op
    /// keeps its slots — we don't want a queued op to start and then fight it on
    /// resume) nor `OperationIntent` (the cancel/rollback machine). It also does
    /// NOT run an admission pass: the op was already Running/holding its lanes,
    /// so resuming admits nobody new.
    ///
    /// Only the `Running`↔`Paused` pair flips; any other status (Queued, Done,
    /// terminal) is left untouched and returns `false`. A Queued op can't be
    /// "paused" in v1 — it simply isn't admitted yet (see the IPC layer's
    /// no-op-for-Queued note). Returns `true` if it flipped a record.
    pub(crate) fn set_paused(&self, operation_id: &str, paused: bool) -> bool {
        let flipped = {
            let mut inner = self.inner.lock_ignore_poison();
            match inner.records.get_mut(operation_id) {
                Some(rec) if paused && rec.status == LifecycleStatus::Running => {
                    rec.status = LifecycleStatus::Paused;
                    true
                }
                Some(rec) if !paused && rec.status == LifecycleStatus::Paused => {
                    rec.status = LifecycleStatus::Running;
                    true
                }
                _ => false,
            }
        };
        if flipped {
            self.emit_changed();
        }
        flipped
    }

    /// Ids of all currently `Running` (not Paused) ops, for `pause_all`.
    fn running_ids(&self) -> Vec<String> {
        let inner = self.inner.lock_ignore_poison();
        inner
            .order
            .iter()
            .filter(|id| {
                inner
                    .records
                    .get(*id)
                    .is_some_and(|r| r.status == LifecycleStatus::Running)
            })
            .cloned()
            .collect()
    }

    /// Ids of all currently `Paused` ops, for `resume_all`.
    fn paused_ids(&self) -> Vec<String> {
        let inner = self.inner.lock_ignore_poison();
        inner
            .order
            .iter()
            .filter(|id| {
                inner
                    .records
                    .get(*id)
                    .is_some_and(|r| r.status == LifecycleStatus::Paused)
            })
            .cloned()
            .collect()
    }

    /// The thin registry snapshot (membership + status), FIFO order.
    pub(crate) fn list(&self) -> Vec<OperationSnapshot> {
        self.inner.lock_ignore_poison().snapshot()
    }

    /// Test-only: lanes currently reserved (in-use count per lane).
    #[cfg(test)]
    pub(crate) fn lane_use_snapshot(&self) -> HashMap<String, usize> {
        self.inner
            .lock_ignore_poison()
            .lane_use
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), *v))
            .collect()
    }

    /// Test-only: lifecycle status of an op, if present.
    #[cfg(test)]
    pub(crate) fn status_of(&self, operation_id: &str) -> Option<LifecycleStatus> {
        self.inner
            .lock_ignore_poison()
            .records
            .get(operation_id)
            .map(|r| r.status)
    }

    fn emit_changed(&self) {
        let Some(app) = OPERATIONS_APP.get() else {
            return;
        };
        use tauri_specta::Event as _;
        let payload = OperationsChanged {
            operations: self.list(),
        };
        if let Err(e) = payload.emit(app) {
            log::warn!(target: "op_manager", "failed to emit operations-changed: {e}");
        }
    }
}

/// RAII safety net held by each manager-spawned task. On `Drop` (including a
/// panic that the runtime catches), it frees the op's lane slots and cleans
/// the caches — but NEVER spawns (no admission pass), so a panicking op can't
/// re-enter the manager mid-unwind. The happy path disarms it by calling
/// `on_settled` first (which removes the record, making the Drop a no-op).
///
/// This subsumes the old `OperationStateGuard`'s cache-cleanup-on-panic role
/// for managed ops, and adds lane release. The op's `WriteSettledGuard` (the FE
/// `write-settled` event) is separate and still lives inside each op's body.
pub(crate) struct ManagedTaskGuard {
    operation_id: String,
    armed: bool,
}

impl ManagedTaskGuard {
    pub(crate) fn new(operation_id: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            armed: true,
        }
    }

    /// Call on the happy path right BEFORE `on_settled` so the Drop doesn't
    /// re-run the (now redundant) cleanup. `on_settled` already removed the
    /// record, so even an armed Drop would be a no-op; disarming just makes
    /// that explicit and skips the lock.
    pub(crate) fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for ManagedTaskGuard {
    fn drop(&mut self) {
        if self.armed {
            log::warn!(target: "op_manager", "op={} task ended without on_settled (panic?); freeing lanes", self.operation_id);
            manager().free_and_remove(&self.operation_id);
        }
    }
}

/// RAII net for [`OperationManager::run_instant`]. On `Drop` (the command's
/// IPC-timeout dropping the `run_instant` future mid-`op.await`, or a panic in
/// the awaited op) it frees the op's record and unregisters its busy status via
/// `free_and_remove`, then re-emits `operations-changed` so the queue snapshot
/// drops the now-gone row too. The busy-set release is the load-bearing part:
/// without it the eject guard would stick ON forever for the op's volume.
/// Instant ops reserve no lanes, so unlike `ManagedTaskGuard` there's nothing to
/// release there. The happy path disarms it after an explicit `free_and_remove`
/// + `emit_changed`, making the Drop a no-op.
struct InstantTaskGuard {
    operation_id: String,
    armed: bool,
}

impl InstantTaskGuard {
    fn new(operation_id: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            armed: true,
        }
    }

    /// Call on the happy path right after the explicit `free_and_remove` so the
    /// Drop doesn't re-run the (now redundant) cleanup.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for InstantTaskGuard {
    fn drop(&mut self) {
        if self.armed {
            log::warn!(target: "op_manager", "instant op={} dropped/panicked before completion; freeing record + busy status", self.operation_id);
            let mgr = manager();
            mgr.free_and_remove(&self.operation_id);
            mgr.emit_changed();
        }
    }
}

// ============================================================================
// Public API (backs the IPC commands)
// ============================================================================

/// The thin registry snapshot (membership + lifecycle status) for the queue
/// window. Backs the `list_operations` IPC command.
pub fn list_operations() -> Vec<OperationSnapshot> {
    manager().list()
}

/// Cancels one operation, keeping already-copied files (the existing
/// `rollback=false` path). A Queued op is dropped from the registry without
/// ever spawning; a Running/Paused op routes through the intent state machine.
/// Backs the `cancel_operation(id)` IPC command.
pub fn cancel_operation(operation_id: &str) {
    if !manager().cancel_if_queued(operation_id) {
        super::state::cancel_write_operation(operation_id, false);
    }
}

/// Cancels several operations (keep-partials each). Backs the
/// `cancel_operations(ids)` IPC command (the queue window's "Cancel selected").
pub fn cancel_operations(operation_ids: &[String]) {
    for id in operation_ids {
        cancel_operation(id);
    }
}

/// Pauses one Running operation: parks it at its next between-files boundary and
/// flips its `LifecycleStatus` to `Paused` (re-emitting `operations-changed`).
/// A paused op keeps its lane slots. Pausing a Queued op is a v1 no-op (it isn't
/// touching a device yet — it stays Queued and admits normally when its lanes
/// free); pausing a Done/absent op is a no-op. Backs `pause_operation(id)`.
pub fn pause_operation(operation_id: &str) {
    // Flip the live gate (so the driver parks) and the record status (so the UI
    // shows Paused). `set_paused` only flips a Running record, so a Queued op's
    // gate is intentionally left untouched: parking a not-yet-spawned op would
    // do nothing and risk a Paused-but-Queued limbo.
    if manager().set_paused(operation_id, true) {
        super::state::pause_write_operation(operation_id);
    }
}

/// Resumes one Paused operation: clears its gate (waking the parked driver) and
/// flips its `LifecycleStatus` back to `Running`. No admission pass — it never
/// freed its lanes. Resuming a non-paused op is a no-op. Backs
/// `resume_operation(id)`.
pub fn resume_operation(operation_id: &str) {
    if manager().set_paused(operation_id, false) {
        super::state::resume_write_operation(operation_id);
    }
}

/// Pauses every currently-Running operation. Backs `pause_all` (the queue
/// window's global Pause all). Snapshots the running set first so the iteration
/// is stable.
pub fn pause_all() {
    for id in manager().running_ids() {
        pause_operation(&id);
    }
}

/// Resumes every currently-Paused operation. Backs `resume_all` (Resume all).
pub fn resume_all() {
    for id in manager().paused_ids() {
        resume_operation(&id);
    }
}

#[cfg(test)]
mod tests;
