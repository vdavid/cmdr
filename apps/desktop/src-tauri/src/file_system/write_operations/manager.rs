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
//! Each op touches the [`LaneKey`]s of its
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
//! double-maintenance. See `lifecycle/state.rs` § "Busy-volumes set".

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use crate::file_system::volume::LaneKey;
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::OpKind;

use super::scan_cache;
use super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, forget_operation, register_operation_status,
    unregister_operation_status,
};
use super::types::{LifecycleStatus, WriteOperationError, WriteOperationType};

/// What a pause or resume request actually did, so every caller can say so
/// instead of assuming it worked. Pause and resume share it: the three outcomes
/// are the same in both directions.
///
/// The distinction is load-bearing at the MCP boundary, where an agent acts on
/// the answer: `Applied` means "the queue has stopped", `NotApplicable` means
/// nothing changed and nothing is remembered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PauseOutcome {
    /// The record flipped: `Running`→`Paused` (the operation parks at its next
    /// boundary — between files while writing, between entries while scanning)
    /// or `Paused`→`Running`.
    Applied,
    /// The operation is already in the state asked for (pausing a `Paused` one,
    /// resuming a `Running` one). Nothing changed because nothing had to, so a
    /// caller that retries its own request gets an honest yes rather than a
    /// refusal.
    AlreadyInState,
    /// Nothing happened and nothing is remembered: the operation is queued,
    /// over, or unknown.
    NotApplicable,
}

/// What a whole `pause_all` / `resume_all` sweep did: one count per
/// [`PauseOutcome`] it collected. Shared by both directions, for the reason
/// `PauseOutcome` is.
///
/// A sweep touches several operations, so "it worked" isn't a thing it can
/// truthfully say. A caller that assumes one tells its user (or its agent) the
/// device is free when a scan is still running or the set was empty all along,
/// which is the whole reason the per-operation outcome exists.
// DEFAULT-OK: all-zero is exactly what an empty sweep collected, and it's the value
// `FromIterator` folds onto. It makes no claim about a disk: `total() == 0` reads as "the
// sweep found nothing to ask", which is the truth in that case, and the reply builder
// says so out loud rather than reporting a success.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PauseAllOutcome {
    /// Operations that flipped right now.
    pub applied: usize,
    /// Operations already sitting where the caller wants them.
    pub already_in_state: usize,
    /// Operations the sweep couldn't touch. From a sweep this is the settle
    /// race alone: the snapshot named them, and they finished before the call
    /// landed.
    pub not_applicable: usize,
}

impl PauseAllOutcome {
    /// How many operations the sweep asked about.
    pub fn total(self) -> usize {
        self.applied + self.already_in_state + self.not_applicable
    }

    /// Whether the caller's intent now holds for at least one operation.
    pub fn took_effect_anywhere(self) -> bool {
        self.applied > 0
    }
}

/// Folding the per-operation outcomes into the sweep's answer. Keeping the
/// aggregation here (rather than inline in the loop) is what makes it testable
/// without touching the process-global manager.
impl FromIterator<PauseOutcome> for PauseAllOutcome {
    fn from_iter<I: IntoIterator<Item = PauseOutcome>>(outcomes: I) -> Self {
        let mut totals = Self::default();
        for outcome in outcomes {
            match outcome {
                PauseOutcome::Applied => totals.applied += 1,
                PauseOutcome::AlreadyInState => totals.already_in_state += 1,
                PauseOutcome::NotApplicable => totals.not_applicable += 1,
            }
        }
        totals
    }
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
    /// Whether cancelling this op can also UNDO what it wrote. DETAILS § "Rollback availability".
    ///
    /// A promise about the OPERATION, not about its current phase: it stays
    /// true through the scan-wait, when there is nothing to undo yet. Whether
    /// to OFFER Rollback is a view decision keyed on the progress phase.
    pub supports_rollback: bool,
    /// The scan preview this op intends to consume, from the confirming
    /// dialog. Claimed at registration so the op can wait on the walk instead
    /// of racing a second one down the same tree. `None` for an op with no
    /// preview (MCP, drag-and-drop, an instant op).
    pub preview_id: Option<String>,
    /// Set only when this op IS the reversal of a finished one, to the kind of
    /// the operation it reverses. `None` on every ordinary op, and on the
    /// in-flight `RollingBack` phase of a cancelled copy (that one reverses
    /// ITSELF, and the transfer dialog it already sits in says so).
    ///
    /// The ORIGINAL kind, not the inverse: the frontend feeds it to the same
    /// `rollbackConfirmVariant` map that worded the confirmation, so the
    /// running bar cannot contradict what the question promised.
    /// `apps/desktop/src/lib/file-operations/reversal-wording.ts`.
    pub reverses: Option<OpKind>,
}

/// Best-effort human-readable source/destination summary for the queue window.
// DEFAULT-OK: both sides `None` means "couldn't summarize", and the queue window renders
// that absence rather than an empty string.
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
    /// The scan preview this op claimed, once it holds the claim. `None` when
    /// it never had one, or once the wait ended.
    claimed_preview: Option<String>,
    /// The op is parked on its scan preview and has written nothing. Set at
    /// registration when the claim lands on a still-walking preview, cleared
    /// when the wait ends. Read by the progress bridge alone, to keep a tick
    /// that raced the end of the wait from dragging the phase back to
    /// `scanning`. Deliberately NOT on `OperationSnapshot`: the frontend
    /// already learns the same fact from `write-progress`'s `phase`, and two
    /// sources for one truth is how they drift.
    in_scan_wait: bool,
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
    /// See [`OperationDescriptor::supports_rollback`].
    pub supports_rollback: bool,
    /// See [`OperationDescriptor::reverses`]. Static for the op's whole life,
    /// which is why it rides the thin snapshot rather than the 200 ms progress
    /// tick.
    pub reverses: Option<OpKind>,
    /// Why the operation stopped, on a retained `Failed` row only; `None` on
    /// every live row. The typed variant, never rendered prose: the frontend's
    /// `transfer-error-messages.ts` owns the wording. DETAILS § "Retained
    /// failures".
    pub error: Option<WriteOperationError>,
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
    /// Retained failures, oldest first, capped at [`FAILURE_CAPACITY`]. The one
    /// piece of manager state whose lifetime ISN'T "while the op runs": a
    /// failure must outlive its record so the user can still read the reason
    /// after the operation settled. Out-of-band on purpose — `free_and_remove`'s
    /// removal-on-terminal discipline is untouched. DETAILS § "Retained
    /// failures".
    failures: VecDeque<OperationSnapshot>,
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

    /// Builds the thin snapshot for `operations-changed`: the live records in
    /// FIFO order, then the retained failures in failure order.
    ///
    /// ⚠️ A failure whose record is STILL LIVE is skipped, and that's
    /// load-bearing: `emit_error` fires inside the op's own task, before
    /// `on_settled` removes the record, so for a moment the same operation is
    /// both running and failed. Emitting both rows would put one `operationId`
    /// in the list twice, and the queue window's keyed `{#each}` throws on a
    /// duplicate key. The failure row surfaces on `on_settled`'s existing emit,
    /// which is the honest moment anyway: until then the op hasn't stopped.
    fn snapshot(&self) -> Vec<OperationSnapshot> {
        let live = self
            .order
            .iter()
            .filter_map(|id| self.records.get(id))
            .map(|rec| OperationSnapshot {
                operation_id: rec.descriptor.operation_id.clone(),
                operation_type: rec.descriptor.operation_type,
                status: rec.status,
                source: rec.descriptor.summary.source.clone(),
                destination: rec.descriptor.summary.destination.clone(),
                supports_rollback: rec.descriptor.supports_rollback,
                reverses: rec.descriptor.reverses,
                error: None,
            });
        let settled_failures = self
            .failures
            .iter()
            .filter(|failure| !self.records.contains_key(&failure.operation_id))
            .cloned();
        live.chain(settled_failures).collect()
    }
}

/// Lane budget per lane in v1: serialize within a lane. v2 makes this
/// per-lane and configurable (e.g. FTP = min(5, server limit)).
const LANE_BUDGET: usize = 1;

/// How many failed operations stay readable after they settle. Mirrors
/// `mcp::terminal_ops::CAPACITY` and its reasoning: enough that a user coming
/// back from lunch still finds what went wrong, small enough that a busy batch
/// session pays a bounded memory cost. Runtime-only — a restart clears them, and
/// the operation log is where a failure lives permanently.
const FAILURE_CAPACITY: usize = 20;

/// The single coordinator. Holds the registry, the FIFO order, and the lane
/// table under one mutex (the critical sections are tiny — register, admit,
/// free — so one lock keeps the invariants obvious without lock-ordering
/// hazards). Spawning happens OUTSIDE the lock.
pub(crate) struct OperationManager {
    inner: Mutex<ManagerInner>,
    /// Completed admission passes, ever. Bumped once at the END of
    /// `run_admission_pass`, so an advance means a pass walked the whole queue
    /// and admitted everything it could. Nothing in production reads it: it's
    /// the signal tests wait on instead of sleeping. `SeqCst`, and see DETAILS
    /// § "Observing an admission pass" for why.
    admission_passes: AtomicU64,
    /// `operations-changed` broadcasts attempted, ever. Bumped at the top of
    /// `emit_changed`, BEFORE the "no app handle" early return, so it counts in
    /// unit tests too. Nothing in production reads it: like `admission_passes`,
    /// it's what lets a test assert that a mutation told the windows about
    /// itself (or, for `record_failure`, deliberately didn't).
    emits: AtomicU64,
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

/// The startup-wired app handle, if present. Lets the instant-op forks
/// (mkdir/mkfile/rename) build a `TauriEventSink` to route an archive target to
/// the managed edit driver without threading an `AppHandle` through every
/// command signature. `None` before wiring (unit tests).
pub(crate) fn operations_app_handle() -> Option<tauri::AppHandle> {
    OPERATIONS_APP.get().cloned()
}

impl OperationManager {
    fn new() -> Self {
        Self {
            inner: Mutex::new(ManagerInner {
                records: HashMap::new(),
                order: Vec::new(),
                lane_use: HashMap::new(),
                failures: VecDeque::new(),
            }),
            admission_passes: AtomicU64::new(0),
            emits: AtomicU64::new(0),
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

        WRITE_OPERATION_STATE.insert(operation_id.clone(), state);

        // Claim the confirming dialog's preview before the record exists, so a
        // second op naming the same id is refused rather than racing for one
        // consumable result. A refusal or an unknown id is a miss: the op keeps
        // no claim and falls back to its own walk.
        let (claimed_preview, in_scan_wait) = match &descriptor.preview_id {
            None => (None, false),
            Some(preview_id) => match scan_cache::claim_preview(preview_id, &operation_id) {
                scan_cache::PreviewClaim::Waiting => (Some(preview_id.clone()), true),
                // Already settled: the claim still holds (it exempts the result
                // from TTL eviction until this op consumes it), but there is
                // nothing to wait for, so pause behaves normally from the start.
                scan_cache::PreviewClaim::AlreadySettled => (Some(preview_id.clone()), false),
                scan_cache::PreviewClaim::Unknown | scan_cache::PreviewClaim::Refused => (None, false),
            },
        };

        {
            let mut inner = self.inner.lock_ignore_poison();
            inner.records.insert(
                operation_id.clone(),
                OpRecord {
                    descriptor,
                    status: LifecycleStatus::Queued,
                    deferred: Some(deferred),
                    reserved_lanes: Vec::new(),
                    claimed_preview,
                    in_scan_wait,
                },
            );
            inner.order.push(operation_id.clone());
        }

        self.run_admission_pass();
        self.emit_changed();

        // AFTER `emit_changed`, on purpose: the frontend store drops progress
        // for an id it has no snapshot row for yet. See `emit_initial_scan_tick`.
        if in_scan_wait {
            super::scan_bridge::emit_initial_scan_tick(&operation_id);
        }
    }

    /// Walks the pending queue oldest-first and admits the first op whose every
    /// lane is free, reserving all its slots atomically and spawning its
    /// deferred start. Repeats until no further op can be admitted on this pass
    /// (admitting one frees nothing, but a single pass may admit several
    /// disjoint-lane ops). Spawns OUTSIDE the lock.
    ///
    /// Ends by bumping `admission_passes`; keep that bump last and unconditional
    /// (it's what makes "a pass ran and declined" observable to a test).
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
                    // Spawn on the app's long-lived runtime, NOT the ambient one
                    // (`tokio::spawn`). The admission pass runs on whatever runtime
                    // triggered it — this op's own `spawn_managed`, or a CONCURRENT
                    // op's `on_settled` that reached the pass first (admission is
                    // global, and there's a lock-free window between an op's
                    // registration and its own pass). In production that's always
                    // the single Tauri runtime, so this is a no-op; but under the
                    // per-test-runtime harness, spawning onto a caller runtime that
                    // is then torn down orphans the task, leaks its lane, and wedges
                    // every later same-lane op. `async_runtime::spawn` pins every op
                    // to the one process-global runtime that outlives them all.
                    tauri::async_runtime::spawn(start());
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
        self.admission_passes.fetch_add(1, Ordering::SeqCst);
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
                    // An instant op is a metadata syscall: no preview, nothing
                    // to wait on.
                    claimed_preview: None,
                    in_scan_wait: false,
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
        let (removed, claimed_preview) = {
            let mut inner = self.inner.lock_ignore_poison();
            match inner.records.remove(operation_id) {
                Some(rec) => {
                    inner.release(&rec.reserved_lanes);
                    inner.order.retain(|id| id != operation_id);
                    (true, rec.claimed_preview)
                }
                None => (false, None),
            }
        };
        if removed {
            // A claim still held here means the op never finished its wait (it
            // panicked, or the quit deadline dropped its task). Stop the walk
            // and drop its result rather than leaving tens of thousands of
            // `FileInfo` for the TTL sweep. A wait that ended normally already
            // cleared the claim, so this is a no-op on the happy path.
            if let Some(preview_id) = claimed_preview {
                scan_cache::abandon_claim(&preview_id);
            }
            forget_operation(operation_id);
            unregister_operation_status(operation_id);
        }
    }

    /// Cancels a Queued op WITHOUT spawning it: drops it from the registry and
    /// frees its (unreserved) state. Returns `true` if it removed a Queued op,
    /// `false` if the op was Running/Paused/absent (the caller then routes
    /// through the existing `cancel_write_operation` intent path).
    pub(crate) fn cancel_if_queued(&'static self, operation_id: &str) -> bool {
        let (was_queued, claimed_preview) = {
            let mut inner = self.inner.lock_ignore_poison();
            match inner.records.get(operation_id) {
                Some(rec) if rec.status == LifecycleStatus::Queued => {
                    let claimed_preview = inner.records.remove(operation_id).and_then(|rec| rec.claimed_preview);
                    inner.order.retain(|id| id != operation_id);
                    (true, claimed_preview)
                }
                _ => (false, None),
            }
        };
        if was_queued {
            // This path drops the record WITHOUT ever running its
            // `DeferredStart`, so nothing else will end the op's scan claim:
            // the walk would keep going for an operation that no longer exists
            // and its result would sit until a TTL sweep. Cancelling a queued
            // op on a busy lane is the ordinary case, not an exotic one.
            if let Some(preview_id) = claimed_preview {
                scan_cache::abandon_claim(&preview_id);
            }
            // A queued op never reserved lanes nor registered busy status, so
            // only the `WRITE_OPERATION_STATE` entry needs clearing.
            forget_operation(operation_id);
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
    /// terminal) is left untouched and reports `NotApplicable`. A Queued op
    /// can't be "paused" in v1 — it simply isn't admitted yet.
    ///
    /// **A scan-waiting op is not a special case.** It is `Running`, it flips
    /// like any other, and the walk it is waiting on parks with it
    /// (`scan_bridge::ScanPause`) — so `Paused` in the snapshot and "Paused" in
    /// the dialog title describe what the operation is actually doing. ❌ Don't
    /// reintroduce a refusal here: an operation that holds its lane while a
    /// walk it claims to have stopped runs at full speed is the defect this
    /// whole path exists to prevent.
    ///
    /// A QUEUED op scanning behind a busy lane therefore answers
    /// `NotApplicable`, the same as any other queued op: no surface offers
    /// Pause on a queued row, and `pause_all` walks `running_ids()` only. If
    /// "don't start this one" ever becomes a thing to offer, it wants a
    /// deliberate design, not a pause that means something else here.
    pub(crate) fn set_paused(&self, operation_id: &str, paused: bool) -> PauseOutcome {
        let outcome = {
            let mut inner = self.inner.lock_ignore_poison();
            match inner.records.get_mut(operation_id) {
                Some(rec) if paused && rec.status == LifecycleStatus::Running => {
                    rec.status = LifecycleStatus::Paused;
                    PauseOutcome::Applied
                }
                Some(rec) if !paused && rec.status == LifecycleStatus::Paused => {
                    rec.status = LifecycleStatus::Running;
                    PauseOutcome::Applied
                }
                // Asked for what it already is. Separate from `NotApplicable`
                // because the caller's intent IS satisfied, so a retry (an
                // agent's, a double-click) shouldn't read as a refusal.
                Some(rec)
                    if (paused && rec.status == LifecycleStatus::Paused)
                        || (!paused && rec.status == LifecycleStatus::Running) =>
                {
                    PauseOutcome::AlreadyInState
                }
                _ => PauseOutcome::NotApplicable,
            }
        };
        if outcome == PauseOutcome::Applied {
            self.emit_changed();
        }
        outcome
    }

    /// Marks the op's scan-wait over. Called once, by the wait itself.
    ///
    /// A pause issued during the wait needs nothing from here: it already set
    /// the op's `PauseGate`, so the driver that takes over from the walk parks
    /// at its first boundary the same way the walk did.
    pub(super) fn end_scan_wait(&'static self, operation_id: &str) {
        let mut inner = self.inner.lock_ignore_poison();
        if let Some(rec) = inner.records.get_mut(operation_id) {
            rec.in_scan_wait = false;
            rec.claimed_preview = None;
        }
    }

    /// The preview this op claimed, if it still holds one.
    pub(super) fn claimed_preview_of(&self, operation_id: &str) -> Option<String> {
        self.inner
            .lock_ignore_poison()
            .records
            .get(operation_id)?
            .claimed_preview
            .clone()
    }

    /// This op's type, for a surface that has only its id. `None` once the
    /// record is gone.
    pub(super) fn operation_type_of(&self, operation_id: &str) -> Option<WriteOperationType> {
        self.inner
            .lock_ignore_poison()
            .records
            .get(operation_id)
            .map(|rec| rec.descriptor.operation_type)
    }

    /// Whether the op is parked on its scan preview. The progress bridge reads
    /// it so a tick that raced the end of the wait can't land afterwards and
    /// drag the phase back to `scanning`.
    pub(crate) fn is_in_scan_wait(&self, operation_id: &str) -> bool {
        self.inner
            .lock_ignore_poison()
            .records
            .get(operation_id)
            .is_some_and(|rec| rec.in_scan_wait)
    }

    /// Retains a failed operation so its reason outlives the record. Called from
    /// `TauriEventSink::emit_error`, next to the terminal-ops ring. Three things
    /// it deliberately does NOT do (rationale: DETAILS § "Retained failures"):
    ///
    /// - **Retain a non-failure**: `write-error` also carries `Cancelled` and
    ///   `ArchiveNeedsPassword` (a recoverable prompt). Excluded by typed
    ///   variant, never by message text.
    /// - **Overwrite an existing entry**: `write-error` can fire twice for one
    ///   op, and the FIRST error is the one that stopped it.
    /// - **Emit while the record is LIVE**: the snapshot would carry the same
    ///   `operation_id` twice (see [`ManagerInner::snapshot`]), and `on_settled`
    ///   always comes to emit it. With the record already GONE it DOES emit: no
    ///   duplicate is possible, and nothing else would broadcast the row.
    pub(crate) fn record_failure(
        &self,
        operation_id: &str,
        operation_type: WriteOperationType,
        error: &WriteOperationError,
    ) {
        if matches!(
            error,
            WriteOperationError::Cancelled { .. } | WriteOperationError::ArchiveNeedsPassword { .. }
        ) {
            return;
        }

        // Everything below runs inside this block so the lock is released before
        // the log call and the emit: the manager's critical sections stay tiny,
        // and a logger doing file I/O must never hold up admission.
        let record_gone = {
            let mut inner = self.inner.lock_ignore_poison();
            if inner.failures.iter().any(|f| f.operation_id == operation_id) {
                return;
            }

            // Prefer the live record's descriptor, so the failed row reads like the
            // running row it replaces. It's gone only if the op settled before its
            // own error event landed, and then the event's own type is all there is.
            let (operation_type, source, destination, reverses, record_gone) = match inner.records.get(operation_id) {
                Some(rec) => (
                    rec.descriptor.operation_type,
                    rec.descriptor.summary.source.clone(),
                    rec.descriptor.summary.destination.clone(),
                    rec.descriptor.reverses,
                    false,
                ),
                None => (operation_type, None, None, None, true),
            };

            if inner.failures.len() == FAILURE_CAPACITY {
                inner.failures.pop_front();
            }
            inner.failures.push_back(OperationSnapshot {
                operation_id: operation_id.to_string(),
                operation_type,
                status: LifecycleStatus::Failed,
                source,
                destination,
                // A settled failure offers no rollback from this row: the op is over,
                // and there's no live intent machine left to reverse it.
                supports_rollback: false,
                // Kept from the live record: a reversal that stopped early is still
                // a reversal, and the row that explains it must not rename itself.
                reverses,
                error: Some(error.clone()),
            });
            record_gone
        };
        log::info!(target: "op_manager", "retain failure op={operation_id}");
        // No record means no `on_settled` to carry the row out, so without this
        // there'd be no toast and no chip until the queue window next opens.
        if record_gone {
            self.emit_changed();
        }
    }

    /// Drops one retained failure and re-broadcasts. Unknown id: a no-op that
    /// doesn't broadcast either.
    pub(crate) fn dismiss_failure(&self, operation_id: &str) {
        let removed = {
            let mut inner = self.inner.lock_ignore_poison();
            let before = inner.failures.len();
            inner.failures.retain(|f| f.operation_id != operation_id);
            inner.failures.len() != before
        };
        if removed {
            self.emit_changed();
        }
    }

    /// Drops every retained failure and re-broadcasts. No-op (and no broadcast)
    /// when nothing is retained.
    pub(crate) fn dismiss_all_failures(&self) {
        let removed = {
            let mut inner = self.inner.lock_ignore_poison();
            let had_any = !inner.failures.is_empty();
            inner.failures.clear();
            had_any
        };
        if removed {
            self.emit_changed();
        }
    }

    /// Ids of all currently `Running` (not Paused) ops, for `pause_all`.
    pub(super) fn running_ids(&self) -> Vec<String> {
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

    /// Completed admission passes so far. A test reads it, acts, and waits for it
    /// to grow: once it has, a pass has considered every queued op and either
    /// admitted it or declined.
    #[cfg(test)]
    pub(crate) fn admission_pass_count(&self) -> u64 {
        self.admission_passes.load(Ordering::SeqCst)
    }

    /// Runs an admission pass inline, returning once it has completed. For the
    /// paths production never runs one on: pause admits nobody, so a test proving
    /// "not even a pass would admit this queued op" has to run one itself.
    #[cfg(test)]
    pub(crate) fn force_admission_pass(&'static self) {
        self.run_admission_pass();
    }

    /// The lifecycle status of a LIVE record, or `None` once the op has settled
    /// and left the registry.
    ///
    /// Deliberately records-only, so `None` means one thing: the manager no
    /// longer tracks this operation. A retained failure is not consulted because
    /// it can't be reached this way — a failure is retained from the same
    /// cleanup that unregisters the status-cache row, so the one caller that
    /// joins by id (`status_cache::get_operation_status`) has already returned
    /// `None` by then. `snapshot()` is where the two sources are joined.
    pub(crate) fn lifecycle_status(&self, operation_id: &str) -> Option<LifecycleStatus> {
        self.inner
            .lock_ignore_poison()
            .records
            .get(operation_id)
            .map(|r| r.status)
    }

    /// `operations-changed` broadcasts attempted so far. See [`Self::emits`].
    #[cfg(test)]
    pub(crate) fn emit_count(&self) -> u64 {
        self.emits.load(Ordering::SeqCst)
    }

    fn emit_changed(&self) {
        self.emits.fetch_add(1, Ordering::SeqCst);
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
///
/// The [`PauseOutcome`] rides all the way out to the IPC command and the MCP
/// `queue` tool: a surface that reports a pause it didn't get sends its user (or
/// its agent) off believing the device is free.
pub fn pause_operation(operation_id: &str) -> PauseOutcome {
    // Flip the live gate (so the driver parks) and the record status (so the UI
    // shows Paused). `set_paused` only flips a Running record, so a Queued op's
    // gate is intentionally left untouched: parking a not-yet-spawned op would
    // do nothing and risk a Paused-but-Queued limbo.
    let outcome = manager().set_paused(operation_id, true);
    if outcome == PauseOutcome::Applied {
        super::state::pause_write_operation(operation_id);
    }
    outcome
}

/// Resumes one Paused operation: clears its gate (waking the parked driver) and
/// flips its `LifecycleStatus` back to `Running`. No admission pass — it never
/// freed its lanes. Resuming a non-paused op is a no-op. Backs
/// `resume_operation(id)`, and reports what it did for the same reason
/// [`pause_operation`] does.
pub fn resume_operation(operation_id: &str) -> PauseOutcome {
    let outcome = manager().set_paused(operation_id, false);
    if outcome == PauseOutcome::Applied {
        super::state::resume_write_operation(operation_id);
    }
    outcome
}

/// Pauses every currently-Running operation. Backs `pause_all` (the queue
/// window's global Pause all). Snapshots the running set first so the iteration
/// is stable.
///
/// Reports the whole sweep as a [`PauseAllOutcome`], for the reason
/// [`pause_operation`] reports its own: an empty running set, a scan still
/// walking, and three parked copies are three different answers, and a caller
/// that can't tell them apart says "paused" to all three.
pub fn pause_all() -> PauseAllOutcome {
    manager().running_ids().iter().map(|id| pause_operation(id)).collect()
}

/// Resumes every currently-Paused operation. Backs `resume_all` (Resume all),
/// and reports the sweep the way [`pause_all`] does.
pub fn resume_all() -> PauseAllOutcome {
    manager().paused_ids().iter().map(|id| resume_operation(id)).collect()
}

/// Drops one retained failure and re-broadcasts the snapshot. Backs
/// `dismiss_failed_operation(id)`: the queue row's Dismiss button, and the
/// foreground error dialog's close path for the operation it was showing.
/// Dismissal is ALWAYS explicit — ❌ never a timer, a window close, or the next
/// operation starting. Unknown id: a no-op.
pub fn dismiss_failed_operation(operation_id: &str) {
    manager().dismiss_failure(operation_id);
}

/// Drops every retained failure and re-broadcasts the snapshot. Backs
/// `dismiss_all_failed_operations` (the queue toolbar's "Dismiss all").
pub fn dismiss_all_failed_operations() {
    manager().dismiss_all_failures();
}

#[cfg(test)]
mod tests;
