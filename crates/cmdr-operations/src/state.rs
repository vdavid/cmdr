//! Operation state management and caches.
//!
//! Contains state tracking for in-progress operations and status caches for query APIs.

use cmdr_fs::ignore_poison::IgnorePoison;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use super::eta::{EtaEstimator, EtaSample, ReversalBar};
use super::host::OperationEventSink;
use super::human_wait::HumanWaitClock;
use super::locked_sources_slot::LockedSourcesSlot;
use super::outage_slot::{CleanupPolicy, OutagePolicy, OutageSlot};
use super::scope::OpScope;
use super::sources_gone_slot::SourcesGoneSlot;
use super::types::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, TransferActivity, TransferWaitReason,
    WriteConflictEvent, WriteProgressEvent,
};

// The conflict slot lives in its own module: arbitrating one answer per conflict
// is a state machine, and it's the whole reason a second surface can be told it
// lost. Re-exported so the established `state::ConflictResolutionResponse` path
// keeps resolving for every caller.
pub(crate) use super::conflict_slot::{ConflictResolutionResponse, ConflictSlot};

// The operation-intent / pause-gate state machines and the scan-preview map
// live in sibling modules. Re-export them here so the established
// `state::OperationIntent`, `state::PauseGate`, `state::FileInfo`, etc. paths
// keep resolving for every caller. The preview map itself is NOT re-exported:
// it's private to `scan_cache`, reachable only through its functions, so no
// caller can seed or read an entry that skipped the coherence canary and the
// request binding.
pub(crate) use super::operation_intent::PauseGate;
pub use super::operation_intent::{IntentCell, OperationIntent};
pub(crate) use super::operation_intent::{StopMeans, StopRequest};
pub(super) use super::scan_cache::{CachedScanResult, FileInfo, ScanPreviewState, ScanResult};

// ============================================================================
// Operation state
// ============================================================================

/// State for an in-progress write operation.
pub struct WriteOperationState {
    /// What this operation reaches its engine through: the host's seams, the
    /// status cache, and its journal. Fixed when the engine built the state.
    scope: Arc<OpScope>,
    /// Where this operation is headed: running, rolling back, or stopped.
    /// Private, so [`Self::request_stop`] and [`Self::stop_for_teardown`] are
    /// its only writers; readers get the cell through [`Self::intent`].
    intent: IntentCell,
    /// How often the operation reports progress.
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
    /// ❗ A waiter that owns the futures it might queue behind must keep polling
    /// them while it waits, or it waits on itself: the concurrent copy driver
    /// does (`transfer/volume/DETAILS.md` § conflict-dispatch mutex).
    ///
    /// Lives here next to the conflict slot because they guard the same
    /// concern. A `tokio::sync::Mutex` (not std) so a task can `.await` the user's
    /// response — actually the response wait happens on the oneshot; the dispatch
    /// guard is dropped at end of the resolve step — but the guard itself must be
    /// held across `.await` points (the latch re-check reads volume state), which a
    /// std mutex can't span.
    pub conflict_dispatch_lock: tokio::sync::Mutex<()>,
    /// The question a queued operation asks when sources left while it waited:
    /// armed by `queued_sources.rs`, answered by `continue_without_gone_sources`,
    /// and abandoned by a cancel exactly like [`conflict_slot`](Self::conflict_slot).
    pub(super) sources_gone: SourcesGoneSlot,
    /// The question a move asks when some of what it moves is Locked: armed by
    /// `locked_sources.rs`, answered by `answer_locked_sources`, and abandoned by
    /// a cancel like [`sources_gone`](Self::sources_gone).
    pub(super) locked_sources: LockedSourcesSlot,
    /// Set by the manager when this operation's registration couldn't admit it,
    /// so it sat `Queued` behind another operation's lanes. Only an operation
    /// that waited checks its sources on admission: one admitted at once runs
    /// against sources its dialog looked at a moment ago.
    waited_in_queue: AtomicBool,
    /// Per-operation ETA + throughput estimator. Fed by `enrich_progress_event`
    /// at every `write-progress` emit site, so every emitter (local copy/delete,
    /// volume copy/move, MTP, SMB) reports rates and ETA uniformly.
    pub estimator: std::sync::Mutex<EtaEstimator>,
    /// Running totals of what this operation SKIPPED: counted by the progress
    /// bars, subtracted from what the rate estimator sees.
    ///
    /// A skipped file is done — a person watching `3 of 900` needs it to reach
    /// `900`, and a merge that skips every clashing child would otherwise leave
    /// both bars frozen at zero for the whole run (a real transfer of 119,204
    /// files did exactly that: `ERR-AYVM4`, 2026-08-27). But no bytes moved for
    /// it, so charging it to throughput reads as an instant burst and blows the
    /// speed and ETA apart. Hence: gross on the event, net in the sample.
    ///
    /// Every skip goes through [`note_skipped`](Self::note_skipped) — the bulk
    /// prelude, a per-source outcome, a conflict decision, and a deep merge
    /// child alike — so `enrich_progress` can do the subtraction in ONE place.
    skipped_files: AtomicUsize,
    skipped_bytes: AtomicU64,
    /// `true` once a reversal that DRAINS the progress bar has started. Set by
    /// the in-flight reversals; left `false` for the history dialog's, whose bar
    /// fills. See [`ReversalBar`].
    reversal_drains: AtomicBool,
    /// How long this operation has spent waiting on a PERSON: the pause the user
    /// pressed, and the conflict prompts they haven't answered yet. The
    /// [`pause_gate`](Self::pause_gate) and the
    /// [`conflict_slot`](Self::conflict_slot) each hold a handle and drive it
    /// from their own transitions; `enrich_progress` reads it so the estimator
    /// can charge those seconds to the person rather than to the transfer. See
    /// [`HumanWaitClock`].
    pub human_wait: Arc<HumanWaitClock>,
    /// The volume this operation is waiting on to come back, if any: a share
    /// that dropped mid-transfer (`transfer/volume_outage.rs`). Read by the one
    /// activity classifier, so every window and an agent polling `cmdr://state`
    /// say "waiting for the NAS" rather than "stalled". Drives
    /// [`human_wait`](Self::human_wait) from its own edges, like a pause does.
    pub outage: Arc<OutageSlot>,
    /// How patient this operation's outage waits are. Always the default in the
    /// app; a test shrinks it through
    /// `with_outage_policy` (test builds only).
    pub(crate) outage_policy: OutagePolicy,
    /// How long its cleanup gives the destination once it stops
    /// (`transfer/leftover_cleanup.rs`); like `outage_policy`, test-shrinkable.
    pub(crate) cleanup_policy: CleanupPolicy,
    /// When this operation was created. An operation queued before a volume's
    /// outage gave up doesn't start a fresh fifteen-minute wait for the same
    /// outage when it's admitted (`transfer/volume_outage.rs` § the lane's
    /// outage record); one created after (a Retry) does.
    pub(crate) born: Instant,
    /// Cooperative cancel flag for in-flight backend I/O. Flipped whenever the
    /// op transitions out of `Running` (via `cancel_write_operation` or
    /// `cancel_all_write_operations`).
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
    /// stop. Fired only by `abort_write_operation` / `Operations::abort_all_write_operations`,
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
    /// (`transfer/volume/stream_pipe.rs`), so the wait ends whether or not the
    /// backend ever comes back. The cost is that the backend's own cleanup is
    /// skipped, which is why it is NEVER fired by an ordinary cancel — the
    /// abandoned bytes are a registered `.cmdr-tmp-*` that the staging layer's
    /// startup sweep removes ([`super::in_flight_temps`]).
    ///
    /// ❌ Don't read this to decide anything a user asked for; it means "the app
    /// is going away", not "the user changed their mind".
    pub backend_abort: CancellationToken,
    /// "Stop starting work", for either reason: a person's Cancel or Rollback
    /// (a child of [`backend_cancel`](Self::backend_cancel), so every stop
    /// fires it), or the first failure in a concurrent copy's window
    /// ([`stop_work_after_failure`](Self::stop_work_after_failure), which fires
    /// ONLY this). Every place that stops for either reason watches it: the
    /// per-chunk `on_progress` check (so a backend runs its own abort and
    /// delete), the retry and outage waits, the walkers' next child. An async
    /// `select!` arm awaits `stop_work.cancelled()`; a sync check asks
    /// [`work_stopped`](Self::work_stopped).
    ///
    /// ❌ Never read it to tell the two apart (rollback or not, which terminal
    /// event): that's the [`intent`](Self::intent)'s job, and a failure leaves
    /// the intent `Running`. ❌ Never hand it to a backend in place of
    /// `backend_cancel` (MTP bridges that token): a backend learns of the
    /// failure drain through `on_progress`, the way it learns of a cancel.
    pub stop_work: CancellationToken,
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
    /// The two volumes this transfer runs between, captured when it started:
    /// `None` for an operation with no second side (a delete, a trash) and for
    /// the engine tests that drive an engine directly.
    ///
    /// Carried here — the operation's shared context — because the engines that
    /// need it (`transfer/move_op/cross_fs.rs`'s Phase 4 gate, its source sweep,
    /// and every terminal error) are called from ~120 test sites that take the
    /// state and not the volume ids, exactly like `journal_volumes` above.
    /// `super::transfer_sides` says why the names can't be looked up later.
    pub(crate) sides: Option<super::transfer_sides::TransferSides>,
    /// The NAME a top-level source takes at the destination when it isn't its
    /// own, keyed by source path: a rename that runs as a move (a folder on an
    /// object store, `Volume::rename_work`). Empty for every ordinary
    /// transfer. Read by the async transfer driver, the one place a
    /// destination path is built; set through
    /// [`with_target_names`](Self::with_target_names). Carried here for the
    /// same reason as `journal_volumes`.
    pub(crate) target_names: super::target_names::TargetNames,
    /// Destination `.cmdr-tmp-*` paths this operation is CURRENTLY streaming
    /// bytes into, so an abandoned transfer's litter can be found and removed.
    ///
    /// Every cross-volume file write stages on a temp sibling and lands it only
    /// after its last byte (`transfer/staged_write.rs`). An entry is added before
    /// the first byte and removed once the temp landed, is gone, or may hold
    /// committed data (a landing about to clear the way), so a temp no longer
    /// listed here is never swept. What remains after the driver's loop is a
    /// task the drain's deadline dropped mid-write, or an abandon whose delete
    /// the destination refused; `volume::cleanup::clean_abandoned_staged_writes`
    /// takes those. Local copies register here synchronously, and only the
    /// startup sweep answers for what they leave.
    ///
    /// ❌ Don't push or retain here directly: go through
    /// [`super::in_flight_temps`], which also keeps the persisted half that
    /// outlives the process.
    pub in_flight_temps: std::sync::Mutex<Vec<PathBuf>>,
    /// Final names of landings left recorded for the destination's next arrival
    /// (`Landing::keep_landing`): complete files, ❌ never removable, that the
    /// stop's summary names.
    pub(crate) kept_landings: std::sync::Mutex<Vec<PathBuf>>,
    /// The ` (recovered)` names landings gave the new bytes when they couldn't
    /// take their own after the way was cleared (`staged_write_landing.rs`'s
    /// `rescue`): complete files, possibly the only copy, that the stop's
    /// summary or the failure names, ❌ never just a log line.
    pub(crate) rescued_landings: std::sync::Mutex<Vec<PathBuf>>,
    /// A test's own persisted ledger, which the in-flight temps above are also
    /// recorded in (`with_in_flight_ledger`, test builds only). `None`, always so
    /// in the app, means the engine's, through the scope; `in_flight_temps`
    /// resolves it. ❌ Don't resolve it here: a call from this module into
    /// `in_flight_temps` welds the two and `transfer_sides` into one module
    /// cycle.
    pub(super) in_flight_ledger: Option<super::in_flight_temps::Ledger>,
    /// The newest `write-progress` this operation emitted, kept so whoever has
    /// to speak for it while it stands still can re-send it
    /// ([`announce_human_wait`](Self::announce_human_wait), and the transfer
    /// probe's stall heartbeat). A wedged or parked operation emits nothing on
    /// its own, so without this the newest event any window holds is from
    /// before it stopped, and a confident speed stays on screen throughout.
    /// Cloning one event per emit is cheap next to the IPC hop it is already
    /// making.
    last_progress: std::sync::Mutex<Option<WriteProgressEvent>>,
    /// The names it handed out but hasn't written yet: [`super::unique_name::ClaimedNames`].
    pub(crate) claimed_names: super::unique_name::ClaimedNames,
    /// What this operation couldn't carry of its sources' metadata, per class,
    /// for the note on its completion: [`super::metadata_losses`].
    pub(crate) metadata_losses: super::metadata_losses::MetadataLosses,
    /// The `com.apple.quarantine` of the local archive this operation extracts
    /// from, keyed by that archive's path and read once per operation. Every file
    /// and folder the extract creates carries it:
    /// `transfer/volume/extract_quarantine.rs`.
    #[cfg(target_os = "macos")]
    pub(crate) extract_quarantine: std::sync::OnceLock<(PathBuf, Option<Vec<u8>>)>,
    /// A copy's, move's, or compress's live tree, set once its scan is done (never for a delete or trash).
    pub(super) transfer_tree: super::transfer_tree::TransferTreeSlot,
    /// "This operation is still going." Dropped by [`end_liveness`](Self::end_liveness)
    /// when it settles.
    ///
    /// Everything this operation staged holds a [`Weak`](std::sync::Weak) to it
    /// (`cmdr_fs::staging`), so its scratch files stop being hidden the
    /// moment it ends. ❌ Not `Arc<WriteOperationState>` reachability: a task the
    /// driver ABANDONED after the cancel deadline still holds one of those, and
    /// the whole point is that a wedge's leftovers become visible anyway.
    liveness: std::sync::Mutex<Option<Arc<()>>>,
    /// Runs once, between an intent writer reading the current intent and
    /// writing the next one, so a test can land a second writer in exactly that
    /// gap. See [`with_intent_write_gap`](Self::with_intent_write_gap).
    #[cfg(test)]
    intent_write_gap: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl WriteOperationState {
    /// A fresh state for a new operation that reaches its engine through
    /// `scope`. Keeps the field list out of every call site so adding new state
    /// members (like the estimator) is one-line.
    pub(super) fn scoped(progress_interval: Duration, scope: Arc<OpScope>) -> Self {
        // One clock, two owners: whichever of them parks the operation, the
        // estimator reads the union.
        let human_wait = HumanWaitClock::shared();
        let tree_events = Arc::clone(scope.host().events_arc());
        let backend_cancel = CancellationToken::new();
        let stop_work = backend_cancel.child_token();
        Self {
            scope,
            intent: IntentCell::running(),
            progress_interval,
            conflict_slot: ConflictSlot::new(Arc::clone(&human_wait)),
            conflict_dispatch_lock: tokio::sync::Mutex::new(()),
            sources_gone: SourcesGoneSlot::default(),
            locked_sources: LockedSourcesSlot::new(Arc::clone(&human_wait)),
            waited_in_queue: AtomicBool::new(false),
            estimator: std::sync::Mutex::new(EtaEstimator::new()),
            skipped_files: AtomicUsize::new(0),
            skipped_bytes: AtomicU64::new(0),
            reversal_drains: AtomicBool::new(false),
            backend_cancel,
            backend_abort: CancellationToken::new(),
            stop_work,
            pause_gate: PauseGate::new(Arc::clone(&human_wait)),
            outage: Arc::new(OutageSlot::new(Arc::clone(&human_wait))),
            outage_policy: OutagePolicy::default(),
            cleanup_policy: CleanupPolicy::default(),
            born: Instant::now(),
            human_wait,
            journal_volumes: None,
            sides: None,
            target_names: super::target_names::TargetNames::default(),
            in_flight_temps: std::sync::Mutex::new(Vec::new()),
            kept_landings: std::sync::Mutex::new(Vec::new()),
            rescued_landings: std::sync::Mutex::new(Vec::new()),
            in_flight_ledger: None,
            last_progress: std::sync::Mutex::new(None),
            liveness: std::sync::Mutex::new(Some(Arc::new(()))),
            claimed_names: super::unique_name::ClaimedNames::default(),
            metadata_losses: super::metadata_losses::MetadataLosses::default(),
            #[cfg(target_os = "macos")]
            extract_quarantine: std::sync::OnceLock::new(),
            transfer_tree: super::transfer_tree::TransferTreeSlot::new(tree_events),
            #[cfg(test)]
            intent_write_gap: std::sync::Mutex::new(None),
        }
    }

    /// What this operation reaches its engine through.
    pub(crate) fn scope(&self) -> &OpScope {
        &self.scope
    }

    /// The same scope, for a part of the operation that runs apart from its
    /// state (a blocking task holding only what it needs).
    pub(crate) fn shared_scope(&self) -> Arc<OpScope> {
        Arc::clone(&self.scope)
    }

    /// Records that this operation had to wait in the queue. The manager's call.
    pub(super) fn note_waited_in_queue(&self) {
        self.waited_in_queue.store(true, Ordering::Relaxed);
    }

    /// Whether this operation sat `Queued` before it was admitted.
    pub(super) fn waited_in_queue(&self) -> bool {
        self.waited_in_queue.load(Ordering::Relaxed)
    }

    /// Declare that the reversal starting now DRAINS the progress bar. Call it
    /// before the first `RollingBack` frame, which is where the estimator
    /// reseeds and reads the direction.
    pub(super) fn reversal_drains_the_bar(&self) {
        self.reversal_drains.store(true, Ordering::Relaxed);
    }

    /// Which way this operation's `RollingBack` frames run.
    fn reversal_bar(&self) -> ReversalBar {
        if self.reversal_drains.load(Ordering::Relaxed) {
            ReversalBar::Drains
        } else {
            ReversalBar::Fills
        }
    }

    /// The one cooperative boundary a serial loop asks at: `true` means stop
    /// now, `false` means carry on with the next item.
    ///
    /// Everything a loop owes Cancel and Pause lives in here, so no caller can
    /// get the order wrong. Cancel is read FIRST, so a stopping operation never
    /// parks and no destructive call runs between the two reads; only a live
    /// operation parks; and a cancel landing WHILE parked wakes the gate and is
    /// answered `true` at that same boundary, rather than one item later.
    ///
    /// ❌ Ask it exactly where the loop already observes cancel, and only
    /// there. A loop that parks less often than it can be cancelled makes Pause
    /// the weaker of two buttons that mean the same thing to a person; one that
    /// parks more often needs boundaries the backends don't offer. `DETAILS.md`
    /// § "The park".
    ///
    /// This twin parks the calling blocking-pool thread; an async driver calls
    /// [`stop_or_park_async`](Self::stop_or_park_async) instead. A caller whose
    /// reading of "stop" isn't `work_stopped` (a reversal, a detached scan
    /// preview) drives [`PauseGate`]'s `*_until` helpers directly and names its
    /// own predicate.
    pub fn stop_or_park_sync(&self) -> bool {
        if self.work_stopped() {
            return true;
        }
        self.pause_gate.wait_while_paused_sync_until(&|| self.work_stopped());
        self.work_stopped()
    }

    /// Async sibling of [`stop_or_park_sync`](Self::stop_or_park_sync): same
    /// answer, parking the calling task instead of an executor thread.
    pub async fn stop_or_park_async(&self) -> bool {
        if self.work_stopped() {
            return true;
        }
        self.park_while_paused().await;
        self.work_stopped()
    }

    /// Parks the calling task while the operation is paused, returning on a
    /// resume or the moment its work stops (a cancel, or the failure drain:
    /// both wake the gate). For a checkpoint that parks and then carries on to
    /// a check of its own; a loop boundary asks
    /// [`stop_or_park_async`](Self::stop_or_park_async).
    pub(crate) async fn park_while_paused(&self) {
        self.pause_gate.wait_while_paused_until(&|| self.work_stopped()).await;
    }

    /// The cheap half of the boundary: `true` when there is anything to ask
    /// about. Two atomic loads, which is what lets a copy SCAN put the boundary
    /// on its per-entry path (`cmdr_fs::volume::ScanStopSignal`).
    pub fn is_stopping_or_paused(&self) -> bool {
        self.work_stopped() || self.pause_gate.is_paused()
    }

    /// Whether this operation should stop starting work: the sync reading of
    /// [`stop_work`](Self::stop_work). The intent half covers the moment
    /// between a stop's intent write and its token firing, and a test that sets
    /// an intent without the stop path.
    pub(crate) fn work_stopped(&self) -> bool {
        self.stop_work.is_cancelled() || self.intent.is_cancelled()
    }

    /// Starts the FAILURE DRAIN: the concurrent copy driver's answer to its
    /// first failed task. Fires [`stop_work`](Self::stop_work) and nothing a
    /// person's stop would also say: the intent stays `Running`, so the
    /// failure stays the operation's error (`write-error`, ❌ never
    /// `write-cancelled`), and `backend_cancel` stays quiet. Everything that
    /// stops on a cancel stops on this too, through the same paths: the
    /// pending conflict prompt is dropped (its waiter reads that as a stop),
    /// and a paused, parked task is woken so it can close its handle and
    /// delete its own partial rather than wait for the drain deadline to drop
    /// it. The pause flag stays set; nothing starts again. Idempotent.
    pub(crate) fn stop_work_after_failure(&self) {
        self.stop_work.cancel();
        self.conflict_slot.abandon();
        self.pause_gate.wake();
    }

    /// A handle to this operation's [`liveness`](Self::liveness), for tagging
    /// the scratch files it stages.
    pub fn liveness_token(&self) -> Option<std::sync::Weak<()>> {
        self.liveness.lock_ignore_poison().as_ref().map(Arc::downgrade)
    }

    /// Declares the operation over. Everything it staged stops being hidden.
    ///
    /// Called where the operation leaves its engine's registry, so it fires
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

    /// Set the names top-level sources take at the destination (see
    /// [`target_names`](Self::target_names)). Chained before wrapping the
    /// state in an `Arc`.
    pub(crate) fn with_target_names(mut self, target_names: super::target_names::TargetNames) -> Self {
        self.target_names = target_names;
        self
    }

    /// Set the transfer's two volumes (see [`sides`](Self::sides)). Chained
    /// before wrapping the state in an `Arc`, from the one place that resolves
    /// both of them.
    pub(crate) fn with_sides(mut self, sides: Option<super::transfer_sides::TransferSides>) -> Self {
        self.sides = sides;
        self
    }

    /// Shrinks how long this operation waits for a volume to come back, so a
    /// test can drive a whole outage (give-up included) in milliseconds.
    #[cfg(any(test, feature = "testing"))]
    pub fn with_outage_policy(mut self, policy: OutagePolicy) -> Self {
        self.outage_policy = policy;
        self
    }

    /// Shrinks the cleanup's patience, so a test drives it in milliseconds.
    #[cfg(any(test, feature = "testing"))]
    pub fn with_cleanup_policy(mut self, policy: CleanupPolicy) -> Self {
        self.cleanup_policy = policy;
        self
    }

    /// This operation's intent, for reading (and for handing to a native copy
    /// callback that checks it between chunks).
    pub fn intent(&self) -> &IntentCell {
        &self.intent
    }

    /// A person's Stop or Rollback, atomically against any other stop. Returns
    /// the intent it moved from, or the one that refused. See [`IntentCell`].
    pub(super) fn request_stop(&self, request: StopRequest) -> Result<OperationIntent, OperationIntent> {
        self.intent.request_stop(request, || {
            #[cfg(test)]
            self.pass_intent_write_gap();
        })
    }

    /// A teardown's stop: `Stopped`, keeping partials. Returns the intent it
    /// moved from, or `Stopped` when the operation already was.
    pub(super) fn stop_for_teardown(&self) -> Result<OperationIntent, OperationIntent> {
        self.intent.stop_for_teardown(|| {
            #[cfg(test)]
            self.pass_intent_write_gap();
        })
    }

    /// Parks the first intent writer between its read and its write by running
    /// `gap` there, once. The race tests use it to land a second stop in that
    /// window deterministically.
    #[cfg(test)]
    pub(super) fn with_intent_write_gap(self, gap: impl FnOnce() + Send + 'static) -> Self {
        *self.intent_write_gap.lock_ignore_poison() = Some(Box::new(gap));
        self
    }

    /// The test seam's call site: runs the armed gap, if any, exactly once.
    #[cfg(test)]
    fn pass_intent_write_gap(&self) {
        let gap = self.intent_write_gap.lock_ignore_poison().take();
        if let Some(gap) = gap {
            gap();
        }
    }

    /// Records this operation's leftovers into a test's own ledger rather than
    /// the process's, so a test that sweeps its log meets only its own records.
    #[cfg(test)]
    pub(super) fn with_in_flight_ledger(mut self, ledger: super::in_flight_temps::Ledger) -> Self {
        self.in_flight_ledger = Some(ledger);
        self
    }

    /// The volume this operation's destination writes land on, when it has one.
    ///
    /// Read from [`journal_volumes`](Self::journal_volumes), which is where the
    /// operation's `(source, dest)` volume identity lives. The in-flight temp
    /// ledger needs the destination half: a staged partial is a sibling of the
    /// destination file, so it lives in THAT volume's path space and only that
    /// volume can delete it (`in_flight_temps`).
    ///
    /// `None` means no volume was named. Every volume copy/move deferred names
    /// one; a local-FS operation doesn't (it stages through `overwrite.rs`,
    /// which records against the local filesystem explicitly), and neither do
    /// the tests that build a bare state.
    pub(super) fn dest_volume_id(&self) -> Option<&str> {
        self.journal_volumes.as_ref().map(|(_, dest)| dest.as_str())
    }

    /// Records that `files` files worth `bytes` were credited to the progress
    /// counters without being transferred.
    ///
    /// Call it for EVERY kind of skip, right where the counters are credited.
    /// `enrich_progress` subtracts the running totals before sampling, so the
    /// bars move and the rate doesn't notice. See
    /// [`skipped_files`](Self::skipped_files).
    pub fn note_skipped(&self, files: usize, bytes: u64) {
        self.skipped_files.fetch_add(files, Ordering::Relaxed);
        self.skipped_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// What this operation has skipped so far: `(files, bytes)`.
    pub fn skipped_totals(&self) -> (usize, u64) {
        (
            self.skipped_files.load(Ordering::Relaxed),
            self.skipped_bytes.load(Ordering::Relaxed),
        )
    }

    /// Populate `bytes_per_second`, `files_per_second`, `eta_seconds`, and
    /// `activity` on a `WriteProgressEvent` before it's emitted. Call this from
    /// every `write-progress` emit site (local copy, local delete, trash, volume
    /// copy, volume move, MTP, SMB) so the FE sees uniform rates, ETA, and stall
    /// classification regardless of which backend produced the event.
    pub fn enrich_progress(&self, event: &mut WriteProgressEvent) {
        // Classified here so no emit site has to remember, EXCEPT for a caller
        // that already holds the answer: the stall watchdog emits from the probe
        // it is stepping, and its copy is the one that just decided the transfer
        // is wedged. Clobbering that with a second lookup is how a re-emitted
        // event loses the very activity it exists to carry.
        if event.activity.is_none() {
            event.activity = self.activity(&event.operation_id, Some(event));
        }

        let now = Instant::now();
        // Net of everything skipped: the event keeps the gross counters the two
        // bars render, the estimator only ever sees work that moved bytes. The
        // remaining work is identical either way (both sides lose the same
        // term), so this changes the RATE, never the ETA's numerator.
        let (skipped_files, skipped_bytes) = self.skipped_totals();
        // Recovered on poison: the estimate is advisory, so a panic mid-update
        // costs at most a skewed rate, while skipping would also drop the event
        // from `last_progress` and read to the stall watchdog as no movement.
        let stats = self.estimator.lock_ignore_poison().update(EtaSample {
            now,
            // The seconds a person spent deciding are not the transfer's,
            // so the rate window doesn't get to count them.
            human_wait_total: self.human_wait.total_at(now),
            phase: event.phase,
            reversal_bar: self.reversal_bar(),
            bytes_done: event.bytes_done.saturating_sub(skipped_bytes),
            bytes_total: event.bytes_total.saturating_sub(skipped_bytes),
            files_done: event.files_done.saturating_sub(skipped_files),
            files_total: event.files_total.saturating_sub(skipped_files),
        });
        event.bytes_per_second = Some(stats.bytes_per_second);
        event.files_per_second = Some(stats.files_per_second);
        event.eta_seconds = stats.eta_seconds;
        event.bottleneck = Some(stats.bottleneck);

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

    /// The file count behind that same last event: the OTHER axis a transfer
    /// can advance along.
    ///
    /// A merge walking into a folder the user already has can decline thousands
    /// of children in a row. Every one of them is progress a person can see on
    /// the file bar, and none of them moves a byte, so a watchdog reading bytes
    /// alone calls a working transfer stalled. Read together with
    /// [`last_progress_bytes`](Self::last_progress_bytes), never instead of it:
    /// a big file mid-stream advances bytes with the file count still.
    pub(super) fn last_progress_files(&self) -> Option<usize> {
        self.last_progress.lock_ignore_poison().as_ref().map(|e| e.files_done)
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

/// This operation, as the copy scan inside a `Volume` backend sees it.
///
/// A backend crate can't name a `WriteOperationState` (`crates/` carry no
/// `tauri` and no app), so the scan asks a `cmdr_fs` vocabulary type and this is
/// what answers it. ❗ Every arm forwards to the same
/// [`stop_or_park_sync`](WriteOperationState::stop_or_park_sync) /
/// [`stop_or_park_async`](WriteOperationState::stop_or_park_async) the rest of
/// the operation asks, so a scan can't end up with its own reading of
/// cancel-outranks-pause. `DETAILS.md` § "The park".
impl cmdr_fs::volume::ScanStopSignal for WriteOperationState {
    fn is_stopping_or_paused(&self) -> bool {
        WriteOperationState::is_stopping_or_paused(self)
    }

    fn stop_or_park<'a>(&'a self) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(self.stop_or_park_async())
    }

    fn stop_or_park_blocking(&self) -> bool {
        self.stop_or_park_sync()
    }
}

// The by-id controls (cancel / abort / pause / resume / conflict answer) live in
// `state/controls.rs`: each is a registry lookup plus one flag flip, and keeping
// them together is what makes the two stopping tiers legible side by side.
mod activity;
mod controls;
// The registry every by-id lookup goes through, and its walks over all of them.
mod registry;
// The RAII guard behind every op's one `write-settled`.
mod settled_guard;
// What a Rollback-cancelled transfer's own reversal sees of it.
mod wind_down;
pub(super) use registry::WriteOperationRegistry;
pub(crate) use settled_guard::WriteSettledGuard;
pub(crate) use wind_down::WindDown;

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
