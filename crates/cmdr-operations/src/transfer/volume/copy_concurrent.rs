//! The concurrent driver behind a volume-to-volume copy.
//!
//! A sliding window of spawned tasks (a `JoinSet` on the engine's runtime), one
//! per top-level source item, sized by `transfer_concurrency`. It is the only
//! driver in the transfer subsystem that runs several sources at once (moves are
//! serial, local-FS copy is serial), which is why it lives here rather than
//! inside `transfer_driver/`: sharing one abstraction with the serial driver
//! would mean reconciling `Send + 'static` tasks with the serial driver's
//! per-call `FnMut`, for a 1-of-4 win.
//!
//! The tasks are SPAWNED, ❌ never futures only the driver polls: a task moves
//! whatever the driver is awaiting, so nothing the driver waits on (the
//! conflict-dispatch mutex a deep merge holds, a person's answer to that merge's
//! prompt) can wait on the driver in turn. With polled futures that was a
//! self-deadlock (ERR-8MEX6, pinned by `merge_dispatch_mutex_tests.rs`).
//!
//! Three files, one per stage of a source's life:
//!
//! - **This one** owns the window and the state that changes as it turns:
//!   `ConcurrentDriver::run` fills, waits, records.
//! - **`copy_concurrent_source.rs`** gets one source ready to spawn, including
//!   conflict resolution — which runs HERE, on the driver, before the task
//!   exists, so a Stop prompt holds back the source it asks about and every one
//!   after it instead of racing per-task prompts.
//! - **`copy_concurrent_task.rs`** streams one source end to end.
//!
//! `copy_volumes_with_progress` owns everything around all three: the phases
//! before (dest-dir creation, temp reap, preflight, space check), the shared
//! counters and rollback ledgers, and all post-loop bookkeeping. This module
//! only drives the window and reports what happened.
//!
//! Semantics, the cancel-drain contract, and the merge invariant: `DETAILS.md`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::task::{Id as TaskId, JoinError, JoinSet};

use super::super::super::conflict::ApplyToAll;
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::host::OperationErrorReport;
use super::super::super::journal;
use super::super::super::ledger::WrittenFile;
use super::super::super::state::{OperationIntent, WriteOperationState};
use super::super::super::types::{SourceItemOutcome, VolumeCopyConfig, WriteOperationError};
use super::super::dest_name_index::DestNameIndex;
use super::super::transfer_driver::{LeafProgressLedger, emit_source_outcome};
use super::super::transfer_probe::OperationProbe;
use super::copy::drain_deadline as drain_deadline_for;
use super::copy_concurrent_task::{CopyTask, CopyTaskFailure, CopyTaskSuccess, run_copy_task};
use super::displaced_destination::DisplacedLedger;
use super::merge_ctx::CreatedPaths;
use super::preflight::SourceHint;
use super::transfer_error::WriteFailure;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{Volume, VolumeError};

/// Everything the concurrent driver needs from its caller.
///
/// The `Arc` fields are clones of `copy_volumes_with_progress`'s own shared
/// counters and ledgers, so the caller keeps reading and unwrapping them after
/// the driver returns.
pub(super) struct ConcurrentCopy<'a> {
    pub(super) events: Arc<dyn OperationEventSink>,
    pub(super) operation_id: &'a str,
    pub(super) state: &'a Arc<WriteOperationState>,
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) source_paths: &'a [PathBuf],
    pub(super) dest_volume: Arc<dyn Volume>,
    pub(super) dest_path: &'a Path,
    pub(super) config: &'a VolumeCopyConfig,
    /// Window width, from `transfer_concurrency`.
    pub(super) concurrency: usize,
    /// The operation's file-copy window, the same width as `concurrency` and
    /// shared with every merge walker under it. A top-level FILE task takes a
    /// permit for its own write, and a DIRECTORY task's leaves take theirs, so
    /// the operation never has more than `concurrency` files in flight no matter
    /// how the batch is shaped. ❌ Never a second, per-walker width: `W` sources
    /// × `W` leaves is `W²` files on one connection.
    pub(super) file_window: super::merge_ctx::FileWindow,
    /// The destination directory was created by THIS operation (Phase 0.5), so
    /// nothing the user already had can be inside it and every pre-check is a
    /// guaranteed miss.
    pub(super) dest_dir_is_ours: bool,
    /// The one destination listing Phase 0.6 already paid for, indexed for name
    /// lookups. `None` ⇒ probe per file.
    pub(super) dest_index: &'a Option<DestNameIndex>,
    pub(super) pre_skip_paths: &'a HashSet<PathBuf>,
    pub(super) source_hints: &'a HashMap<PathBuf, SourceHint>,
    pub(super) progress_interval: Duration,
    /// Real (source, dest) volume ids for the operation-log journal. `None` for
    /// the both-local shortcut and in tests that install no journal.
    pub(super) journal_volumes: &'a Option<(String, String)>,
    pub(super) op_probe: &'a Option<Arc<OperationProbe>>,
    pub(super) leaf_ledger: Arc<LeafProgressLedger>,
    pub(super) files_skipped_atomic: Arc<AtomicUsize>,
    pub(super) bytes_skipped_atomic: Arc<AtomicU64>,
    pub(super) last_progress_mutex: Arc<std::sync::Mutex<Instant>>,
    pub(super) apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>>,
    pub(super) copied_paths: Arc<std::sync::Mutex<Vec<WrittenFile>>>,
    pub(super) created_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    /// What cross-type Overwrites set aside, for the post-loop to settle.
    pub(super) displaced: Arc<DisplacedLedger>,
    pub(super) deep_skipped_files: Arc<AtomicUsize>,
    pub(super) deep_skipped_bytes: Arc<AtomicU64>,
}

/// What the driver leaves for the caller's post-loop bookkeeping. The counters
/// aren't in here: they live in the shared atomics the caller already holds.
///
/// A directory source's rollback runs per-file off the `copied_paths` /
/// `created_dirs` ledgers; a file source's staged temp is the staged-write
/// cleanup's.
pub(super) struct ConcurrentOutcome {
    /// The first failure, if any: a task's, a panic, or conflict resolution's.
    /// The driver stops starting new work and drains on one.
    pub(super) copy_error: Option<WriteFailure>,
}

/// Runs the sliding window to completion, cancellation, or the first failure
/// and the drain after it.
///
/// ❗ **Answers an outcome, never an `Err`**, and that is a data-safety property
/// rather than a style choice. EVERY failure this driver can meet, a task's and
/// conflict resolution's own alike, comes back as `ConcurrentOutcome::copy_error`, so
/// the caller's post-loop always runs: counter sync, deep-skip folding, the
/// created-dir journal rows, the cancel reclassification, the abandoned-staged-
/// write sweep, the rollback branch, and the terminal event. A resolver error
/// that short-circuited out of here skipped all of it and left the user with a
/// half-built destination nothing had swept and no `write-cancelled` to explain
/// it.
pub(super) async fn drive_transfer_concurrent(ctx: ConcurrentCopy<'_>) -> ConcurrentOutcome {
    let mut driver = ConcurrentDriver::new(ctx);
    driver.run().await;
    driver.finish().await
}

/// What one task per top-level source item hands back (`copy_concurrent_task.rs`).
type CopyTaskOutput = Result<CopyTaskSuccess, Box<CopyTaskFailure>>;

/// One task out of the window: its result, or why it has none (a panic, or the
/// driver's own abort).
type Joined = Result<(TaskId, CopyTaskOutput), JoinError>;

/// How long the driver waits for the tasks it aborted to stop, so their teardown
/// (a backend writer's abort, the probe row) runs before the post-loop sweeps.
/// A task parked on an `.await` stops at once; only one stuck INSIDE a poll can
/// use this up, and nothing can stop that one, so the driver lets go of it.
const ABORTED_TASKS_GRACE: Duration = Duration::from_secs(1);

/// The sliding window and the state that changes as it runs.
///
/// The borrowed context stays whole in `ctx` — it is what the per-source
/// preparation in `copy_concurrent_source.rs` reads, and none of it changes
/// during a run. Everything that DOES change lives here, which is what makes
/// each phase a method you can read (and drive from a test) on its own.
struct ConcurrentDriver<'a> {
    ctx: ConcurrentCopy<'a>,
    /// The spawned tasks. Dropping the set aborts them, so no task outlives the
    /// driver, even one dropped mid-run.
    in_flight: JoinSet<CopyTaskOutput>,
    /// The engine's runtime, which every task spawns on (`src/host/CLAUDE.md`).
    runtime: tokio::runtime::Handle,
    /// The sources still to prepare, in the order the user selected them.
    sources: std::iter::Enumerate<std::slice::Iter<'a, PathBuf>>,
    /// Set once the driver has SEEN its work stop (`state.stop_work`: the
    /// user's cancel or rollback, or its own first failure): from then on it
    /// stops waiting indefinitely for its tasks and gives them a bounded window
    /// to stop. See `copy.rs::CANCEL_DRAIN_DEADLINE`.
    drain_deadline: Option<tokio::time::Instant>,
    /// Whether that window is the hard-abort tier's short one rather than the
    /// cooperative cancel's. Latched, so the abort re-arms the deadline once.
    drain_shortened: bool,
    settled: Settlements,
}

/// What the window's settled tasks left for the caller, and which are still
/// out.
///
/// Kept apart from the rest of the driver so a task that lands while a source
/// is being prepared is recorded on the spot: preparing borrows `ctx` for as
/// long as it waits, and recording needs only that same `ctx`, read-only,
/// beside this.
// DEFAULT-OK: the empty value is the truth before the first task settles: no
// failure, nothing in flight.
#[derive(Default)]
struct Settlements {
    /// Handed back to the caller's post-loop; see `ConcurrentOutcome`.
    copy_error: Option<WriteFailure>,
    /// A handle on every in-flight task's own rollback ledger, keyed by its
    /// task and dropped when the task settles. What's left at
    /// [`ConcurrentDriver::finish`] belongs to tasks the driver aborted, whose
    /// hand-over at the end of `run_copy_task` never ran. In spawn order, so
    /// the journal rows they hand over come out in the user's order.
    unsettled: Vec<(TaskId, UnsettledSource)>,
}

/// One in-flight source's ledger, as the driver holds it.
struct UnsettledSource {
    source_path: PathBuf,
    dest_path: PathBuf,
    source_is_dir: bool,
    created: Arc<CreatedPaths>,
}

/// How one trip through the driver's await ended.
enum AwaitStep {
    /// The drain's window was armed or shortened. Nothing settled; go round
    /// again so the new deadline is the one being waited on.
    Rearmed,
    /// A task came back. Boxed because the payload dwarfs the other two
    /// variants (a failure carries its whole per-file rollback ledger), and this
    /// enum is built once per settled task on a path that has just finished
    /// streaming a file: one allocation is nothing next to that, while an
    /// unboxed variant sizes every trip through the await.
    Settled(Box<Joined>),
    /// The window emptied, or the drain's deadline expired with tasks still
    /// in it — those are aborted in `finish`, and their staged partials are
    /// cleaned up by the caller's post-loop.
    Finished,
}

impl<'a> ConcurrentDriver<'a> {
    fn new(ctx: ConcurrentCopy<'a>) -> Self {
        Self {
            sources: ctx.source_paths.iter().enumerate(),
            runtime: ctx.state.scope().host().runtime(),
            ctx,
            in_flight: JoinSet::new(),
            drain_deadline: None,
            drain_shortened: false,
            settled: Settlements::default(),
        }
    }

    /// Fill the window, wait for something, record it. Ends when the sources and
    /// the window are both empty, or when a drain's deadline expires.
    ///
    /// ❗ A failure DRAINS, it doesn't abort: the first one (a task's, a panic,
    /// or conflict resolution's) stops preparing sources
    /// (`WriteOperationState::stop_work_after_failure`) and every task still in
    /// the window is awaited under the drain deadline, the same as after a
    /// Cancel. A sibling mid-stream stops at its next chunk, through tier 1's
    /// `on_progress` path, so its backend closes its own handle and deletes its
    /// own partial; one inside its landing finishes it and is recorded. Only a
    /// task still running at the deadline is aborted, in `finish`.
    async fn run(&mut self) {
        loop {
            if let Err(failure) = self.spawn_ready_tasks().await {
                // Conflict resolution refused (the destination couldn't be
                // probed, a Stop prompt lost its answer). The batch's failure,
                // recorded like a task's: the window drains and the same
                // post-loop gives the user a terminal event.
                self.record_resolver_failure(failure);
            }
            if self.in_flight.is_empty() {
                break;
            }
            match self.await_next().await {
                AwaitStep::Rearmed => continue,
                AwaitStep::Finished => break,
                AwaitStep::Settled(joined) => self.settled.record_joined(&self.ctx, *joined),
            }
        }
    }

    /// Keep preparing and spawning sources until either they run out or the
    /// window is full.
    ///
    /// Preparing a source is where conflict resolution happens, on this driver,
    /// before the task exists — so while a person is answering a Stop prompt
    /// neither the source they're asked about nor any after it moves, and
    /// nothing streams into a destination they're still being asked about. The
    /// tasks already spawned keep running meanwhile, a merge parked on its own
    /// prompt included, which is why preparing may wait on them (the dispatch
    /// mutex) without waiting on itself.
    async fn spawn_ready_tasks(&mut self) -> Result<(), WriteFailure> {
        while self.in_flight.len() < self.ctx.concurrency {
            if self.ctx.state.work_stopped() {
                break;
            }
            let Some((source_index, source_path)) = self.sources.next() else {
                break;
            };
            if let Some(task) = self.prepare_while_draining(source_index, source_path).await? {
                let unsettled = UnsettledSource {
                    source_path: task.source_path.clone(),
                    dest_path: task.dest_path.clone(),
                    source_is_dir: task.source_is_dir,
                    created: Arc::clone(&task.created),
                };
                let spawned = self.in_flight.spawn_on(run_copy_task(task), &self.runtime);
                self.settled.unsettled.push((spawned.id(), unsettled));
            }
        }
        Ok(())
    }

    /// Prepares one source while recording what the window hands back.
    ///
    /// Preparing can wait a long time: on the person answering this source's
    /// clash, or on the conflict-dispatch mutex while another task's prompt is
    /// up. The tasks don't need the driver to move (they're spawned), but their
    /// bookkeeping does: a source that lands meanwhile gets its
    /// `write-source-item-done`, its journal rows, and its files-axis tick now,
    /// not minutes later when the person answers. Pinned by
    /// `copy_concurrent_tests.rs::a_top_level_conflict_prompt_holds_back_its_source_and_every_later_one`.
    ///
    /// A failure recorded here stops the work like any other
    /// (`Settlements::keep_the_first_failure`), so nothing after this source is
    /// prepared.
    ///
    /// No stop arm, on purpose: a resolver dropped mid-answer can leave a
    /// reserved name or a destination set aside that nothing settles. A stop (a
    /// Cancel or the first failure) ends preparation by itself instead: it
    /// abandons the prompt the resolver is waiting on, and a resolver that gets
    /// the mutex checks `work_stopped` before it asks anything.
    async fn prepare_while_draining(
        &mut self,
        source_index: usize,
        source_path: &Path,
    ) -> Result<Option<CopyTask>, WriteFailure> {
        let Self {
            ctx,
            in_flight,
            settled,
            ..
        } = self;
        let ctx: &ConcurrentCopy<'a> = ctx;
        let mut prepare = std::pin::pin!(ctx.prepare_source(source_index, source_path));
        loop {
            let draining = !in_flight.is_empty();
            tokio::select! {
                biased;
                prepared = &mut prepare => return prepared,
                Some(joined) = in_flight.join_next_with_id(), if draining => settled.record_joined(ctx, joined),
            }
        }
    }

    /// Wait for the next task, for the work to stop, or for the drain's
    /// deadline — whichever comes first.
    ///
    /// Observing the stop HERE is what makes Cancel and Rollback work at all.
    /// `spawn_ready_tasks` checks it too, but a driver whose window is full —
    /// or whose sources have run out — never gets back to it while its tasks
    /// are parked, which is why Rollback did nothing in the 2026-07-31
    /// incident. `stop_work` fires on every transition out of `Running` and on
    /// the driver's own first failure, so a Cancel, a Rollback, and a failure
    /// all land here.
    async fn await_next(&mut self) -> AwaitStep {
        // The await the driver parked on for 20 minutes in that incident.
        // Naming it means a dump distinguishes "the driver is waiting for tasks"
        // from "the driver is stuck preparing the next source", which the log
        // could not tell apart.
        if let Some(probe) = self.ctx.op_probe.as_ref() {
            probe.set_driver_phase(super::super::transfer_probe::DriverPhase::AwaitingTasks, "");
        }
        let operation_id = self.ctx.operation_id;
        let next = match self.drain_deadline {
            // Already draining: bounded, so one task that never returns can't
            // hold the operation (and the user's dialog) open.
            //
            // The window stays re-negotiable while it runs. A quit cancels first
            // and aborts what's left a beat later, and by then this await is
            // already sitting on the cooperative deadline — so tier 2 gets its own
            // `select!` arm to shorten it. Guarded on `drain_shortened` because a
            // fired token is ready forever, and an unguarded arm would spin.
            Some(deadline) => tokio::select! {
                biased;
                () = self.ctx.state.backend_abort.cancelled(), if !self.drain_shortened => {
                    self.drain_shortened = true;
                    let shortened = drain_deadline_for(true);
                    log::info!(
                        target: "copy",
                        "copy_volumes_with_progress: op={operation_id} is no longer waiting for its {} in-flight task(s); \
                         cutting the drain short (to {shortened:?})",
                        self.in_flight.len(),
                    );
                    self.drain_deadline = Some(tokio::time::Instant::now() + shortened);
                    return AwaitStep::Rearmed;
                }
                result = tokio::time::timeout_at(deadline, self.in_flight.join_next_with_id()) => match result {
                    Ok(next) => next,
                    Err(_) => {
                        let operation_id = operation_id.into();
                        let tasks = self.in_flight.len();
                        let deadline = drain_deadline_for(self.drain_shortened);
                        let probe_dump = self
                            .ctx
                            .op_probe
                            .as_ref()
                            .map(|p| p.render_dump("abandoning wedged tasks"));
                        // The intent stays `Running` through a failure's drain.
                        let report = match self.ctx.state.intent().load() {
                            OperationIntent::Running => OperationErrorReport::FailureDrainAbandoned {
                                operation_id,
                                tasks,
                                deadline,
                                probe_dump,
                            },
                            _ => OperationErrorReport::WindDownAbandoned {
                                operation_id,
                                tasks,
                                deadline,
                                probe_dump,
                            },
                        };
                        self.ctx.state.scope().host().events().error_report(report);
                        return AwaitStep::Finished;
                    }
                },
            },
            None => {
                tokio::select! {
                    // A stop wins the race: a task result that was also ready
                    // is re-polled on the next pass, so nothing is dropped.
                    biased;
                    () = self.ctx.state.stop_work.cancelled() => {
                        // An abort fires tier 1 first, so it can be what woke this
                        // arm; ask which window we're owed rather than assuming.
                        self.drain_shortened = self.ctx.state.backend_abort.is_cancelled();
                        let window = drain_deadline_for(self.drain_shortened);
                        let tasks = self.in_flight.len();
                        match self.ctx.state.intent().load() {
                            OperationIntent::Running => log::info!(
                                target: "copy",
                                "copy_volumes_with_progress: op={operation_id} had a task fail; draining the {tasks} \
                                 still in flight (up to {window:?})",
                            ),
                            intent => log::info!(
                                target: "copy",
                                "copy_volumes_with_progress: op={operation_id} observed {intent:?} while awaiting \
                                 {tasks} in-flight task(s); winding them down (up to {window:?})",
                            ),
                        }
                        self.drain_deadline = Some(tokio::time::Instant::now() + window);
                        return AwaitStep::Rearmed;
                    }
                    next = self.in_flight.join_next_with_id() => next,
                }
            }
        };
        match next {
            Some(joined) => AwaitStep::Settled(Box::new(joined)),
            None => AwaitStep::Finished,
        }
    }

    /// Conflict resolution failed while preparing a source, which is not one
    /// task's failure but the whole batch's. Recorded the same way so the
    /// post-loop can't tell them apart.
    fn record_resolver_failure(&mut self, failure: WriteFailure) {
        log::warn!(
            target: "copy",
            "copy_volumes_with_progress: op={} couldn't resolve a conflict with {} task(s) in flight: detail={:?}",
            self.ctx.operation_id,
            self.in_flight.len(),
            failure.error.log_detail(),
        );
        self.settled.keep_the_first_failure(&self.ctx, failure);
    }

    /// Hand the post-loop what it needs, after letting go of whatever is left in
    /// the window.
    async fn finish(mut self) -> ConcurrentOutcome {
        // Empty unless the drain's deadline expired: aborting what's left is
        // what lets go of a wedged task.
        if let Some(probe) = self.ctx.op_probe.as_ref() {
            probe.set_driver_phase(
                super::super::transfer_probe::DriverPhase::PostLoop,
                "aborting what the drain abandoned",
            );
        }
        self.abort_in_flight().await;

        // An aborted DIRECTORY source already landed some of its children, and
        // they're only in its own ledger. Hand them over exactly as
        // `record_failure` does for one that came back, or a Rollback can't see
        // them and a reversal from history has no row for them.
        for (_, abandoned) in std::mem::take(&mut self.settled.unsettled) {
            abandoned.hand_over(&self.ctx);
        }

        ConcurrentOutcome {
            copy_error: self.settled.copy_error,
        }
    }

    /// Abort every task still in the window and wait (briefly) for them to stop.
    ///
    /// Waiting is what keeps the post-loop's sweep after the tasks' own
    /// teardown, as it was when a dropped future tore down on the spot. And a
    /// task can finish in the instant before its abort lands: its result then
    /// carries the only copy of its ledger (`run_copy_task` took it out of the
    /// shared one), so it's recorded like any other, ❌ never discarded.
    async fn abort_in_flight(&mut self) {
        if self.in_flight.is_empty() {
            return;
        }
        self.in_flight.abort_all();
        let grace = tokio::time::Instant::now() + ABORTED_TASKS_GRACE;
        loop {
            match tokio::time::timeout_at(grace, self.in_flight.join_next_with_id()).await {
                Ok(Some(joined)) => self.settled.record_joined(&self.ctx, joined),
                Ok(None) => return,
                Err(_) => {
                    // Dropping the set at the end of `finish` aborts them again;
                    // their ledgers hand over from `unsettled` like any other
                    // aborted task's.
                    log::warn!(
                        target: "copy",
                        "copy_volumes_with_progress: op={} let go of {} task(s) that didn't stop within {ABORTED_TASKS_GRACE:?} of their abort",
                        self.ctx.operation_id,
                        self.in_flight.len(),
                    );
                    return;
                }
            }
        }
    }
}

impl UnsettledSource {
    /// Hands what an aborted or panicked DIRECTORY source already landed to the
    /// operation's ledger. A FILE source's is empty; its temp is the staged-write
    /// cleanup's.
    fn hand_over(self, ctx: &ConcurrentCopy<'_>) {
        if !self.source_is_dir {
            return;
        }
        let files = std::mem::take(&mut *self.created.files.lock_ignore_poison());
        let dirs = std::mem::take(&mut *self.created.dirs.lock_ignore_poison());
        if let Some((src_vol, dst_vol)) = ctx.journal_volumes.as_ref() {
            journal::record_volume_transfer_source(
                ctx.state.scope(),
                src_vol,
                &self.source_path,
                dst_vol,
                &self.dest_path,
                true,
                &files,
                None,
                &journal::LandedTimes::default(),
                self.created.any_overwrote(),
            );
        }
        ctx.copied_paths.lock_ignore_poison().extend(files);
        ctx.created_dirs.lock_ignore_poison().extend(dirs);
    }
}

impl Settlements {
    /// One task came back out of the window, however it ended.
    fn record_joined(&mut self, ctx: &ConcurrentCopy<'_>, joined: Joined) {
        match joined {
            Ok((id, Ok(success))) => {
                self.settle(id);
                self.record_success(ctx, success);
            }
            Ok((id, Err(failure))) => {
                self.settle(id);
                self.record_failure(ctx, *failure);
            }
            Err(ended) if ended.is_panic() => {
                if let Some(unsettled) = self.settle(ended.id()) {
                    self.record_panic(ctx, unsettled, &ended);
                }
            }
            // Only the driver aborts a task, and only in `finish`: its ledger
            // stays in `unsettled` and hands over there.
            Err(_) => {}
        }
    }

    /// One source landed: fold its tallies in, drop its in-flight partial,
    /// journal it, record what it wrote for rollback, and emit the milestone.
    fn record_success(&mut self, ctx: &ConcurrentCopy<'_>, success: CopyTaskSuccess) {
        let CopyTaskSuccess {
            recorded_path,
            source_is_dir,
            bytes,
            overwrote,
            created_files,
            created_dirs: task_created_dirs,
            landed,
            source_path: done_source,
            skipped_count: task_skipped_count,
            skipped_bytes: task_skipped_bytes,
        } = success;
        // A folder whose every file was declined landed nothing: a skip, the
        // way the local engine reports one. A copy leaves its source in place.
        let verdict = if source_is_dir && task_skipped_count > 0 && created_files.is_empty() {
            SourceItemOutcome::Skipped
        } else {
            SourceItemOutcome::Done
        };
        emit_source_outcome(&*ctx.events, ctx.operation_id, &done_source, verdict, false);

        // Fold this source's deep-merge skips into the op-wide tally, and — when
        // the source landed with ZERO skips — record it as fully extracted so
        // the out-of-zip move op may drop it.
        ctx.deep_skipped_files.fetch_add(task_skipped_count, Ordering::Relaxed);
        ctx.deep_skipped_bytes.fetch_add(task_skipped_bytes, Ordering::Relaxed);
        if task_skipped_count == 0 {
            ctx.events.note_source_landed_clean(&done_source);
        }
        // Record what the op wrote for rollback.
        // Whatever this source set aside is now fully replaced.
        ctx.displaced.landed_under(&recorded_path);
        let file_name_done = recorded_path.file_name().map(|n| n.to_string_lossy().to_string());
        // Journal the per-leaf `rollback_unit` rows under the REAL volume ids (a
        // file source → one leaf at `recorded_path`; a dir source → one leaf per
        // `created_files` entry, source rebased from `done_source`). Created dirs
        // journal post-loop (after all files, so their `seq` follows the
        // contents). No-ops for the both-local shortcut (that path journals via
        // `copy_files_start`) and in tests that don't set a journal target.
        if let Some((src_vol, dst_vol)) = ctx.journal_volumes.as_ref() {
            journal::record_volume_transfer_source(
                ctx.state.scope(),
                src_vol,
                &done_source,
                dst_vol,
                &recorded_path,
                source_is_dir,
                &created_files,
                (!source_is_dir).then_some(bytes as i64),
                &landed,
                overwrote,
            );
        }
        // For a DIRECTORY source, record the individual files and the
        // newly-created subdirs the op wrote — never the directory root — so
        // rollback can't recursively delete a merged directory and destroy
        // dest-only files. For a FILE source, record the landed path;
        // `created_*` are empty.
        if source_is_dir {
            ctx.copied_paths.lock_ignore_poison().extend(created_files);
            ctx.created_dirs.lock_ignore_poison().extend(task_created_dirs);
        } else {
            ctx.copied_paths
                .lock_ignore_poison()
                .push(WrittenFile::volume(recorded_path, bytes));
        }
        // Per-file milestone emit. The task's `on_file_complete` bumped
        // the ledger's `files_done`; the FE's files-axis needs a Copying event that
        // observes the bumped value, but no chunked emit fires after
        // `on_file_complete` (the file's transfer is over). Mirrors the serial
        // path's milestone in `transfer_driver.rs::drive_transfer_serial_async`.
        // Through the ledger, ❌ never a hand-built event: the other tasks are
        // still streaming, so only the ledger's ordered emit keeps this one from
        // landing behind (or under) theirs.
        *ctx.last_progress_mutex.lock_ignore_poison() = Instant::now();
        ctx.leaf_ledger.emit(file_name_done, ctx.leaf_ledger.current());
    }

    /// One source failed: thread its rollback ledger out.
    fn record_failure(&mut self, ctx: &ConcurrentCopy<'_>, failure: CopyTaskFailure) {
        let CopyTaskFailure {
            failed_path: failed_dest,
            failure: reported,
            source_path: done_source,
            source_is_dir,
            overwrote,
            created_files,
            created_dirs: task_created_dirs,
        } = failure;
        // A cancel decided nothing about the source, so it gets no verdict.
        if !matches!(reported.error, VolumeError::Cancelled(_)) {
            emit_source_outcome(
                &*ctx.events,
                ctx.operation_id,
                &done_source,
                SourceItemOutcome::Failed,
                false,
            );
        }

        if source_is_dir {
            // DIRECTORY source interrupted mid-stream. Record the per-file
            // partials and newly-created subdirs the op wrote, NOT the dir root
            // `failed_dest`. The post-loop then cleans/rolls back per-file (and
            // prunes created dirs empty-only), so a merged dir holding a
            // pre-existing dest-only file survives — recursively deleting the
            // root would be silent data loss.
            //
            // Journal those same children: a cancel keeps every one this task
            // finished, and the journal is the only record a later reversal from
            // history has. `failed_dest` is the dest dir ROOT for a directory
            // source (safe-replace is file→file only), which is the root the leaf
            // sources rebase off.
            if let Some((src_vol, dst_vol)) = ctx.journal_volumes.as_ref() {
                journal::record_volume_transfer_source(
                    ctx.state.scope(),
                    src_vol,
                    &done_source,
                    dst_vol,
                    &failed_dest,
                    true,
                    &created_files,
                    None,
                    &journal::LandedTimes::default(),
                    overwrote,
                );
            }
            ctx.copied_paths.lock_ignore_poison().extend(created_files);
            ctx.created_dirs.lock_ignore_poison().extend(task_created_dirs);
        }
        // `PathedVolumeError`'s conversion owns the branch: the side it carries
        // picks the wording, and `new_data_at` says where the user's new file is.
        self.keep_the_first_failure(ctx, WriteFailure::from(reported));
    }

    /// The FIRST failure is the one that stopped the operation, so it stays its
    /// error: a sibling's later one, the stop the drain itself caused included,
    /// is logged and dropped. The first also starts the drain, unless a person's
    /// stop already stopped the work (its own result is then this failure).
    fn keep_the_first_failure(&mut self, ctx: &ConcurrentCopy<'_>, failure: WriteFailure) {
        if self.copy_error.is_some() {
            log::debug!(
                target: "copy",
                "copy_volumes_with_progress: op={} keeps its first failure; a draining sibling ended with detail={:?}",
                ctx.operation_id,
                failure.error.log_detail(),
            );
            return;
        }
        self.copy_error = Some(failure);
        if !ctx.state.work_stopped() {
            ctx.state.stop_work_after_failure();
        }
    }

    /// A task panicked: a failure like any other, ❌ never lost. What it had
    /// landed hands over like an aborted task's.
    fn record_panic(&mut self, ctx: &ConcurrentCopy<'_>, unsettled: UnsettledSource, ended: &JoinError) {
        log::error!(
            target: "copy",
            "copy_volumes_with_progress: op={} copy task for {} panicked: {ended}",
            ctx.operation_id,
            LogPrivate::path(&unsettled.source_path),
        );
        emit_source_outcome(
            &*ctx.events,
            ctx.operation_id,
            &unsettled.source_path,
            SourceItemOutcome::Failed,
            false,
        );
        let path = unsettled.source_path.display().to_string();
        unsettled.hand_over(ctx);
        self.keep_the_first_failure(
            ctx,
            WriteFailure::synthetic(WriteOperationError::IoError {
                path,
                message: ended.to_string().into(),
            }),
        );
    }

    /// The task `id` came back: stop holding its ledger. Its own result carries
    /// the ledger, unless it panicked, which is why the handle is returned.
    fn settle(&mut self, id: TaskId) -> Option<UnsettledSource> {
        let at = self.unsettled.iter().position(|(task, _)| *task == id)?;
        Some(self.unsettled.remove(at).1)
    }
}

/// The driver's own contract, asserted on what it hands back rather than on the
/// files a finished operation left: the post-loop's delete-capability split is a
/// second defense of the same data, so an end-to-end assertion can't tell a
/// right rollback ledger from a wrong one.
#[cfg(test)]
#[path = "copy_concurrent_driver_tests.rs"]
mod driver_tests;
