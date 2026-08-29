//! The concurrent driver behind a volume-to-volume copy.
//!
//! A `FuturesUnordered` sliding window, one task per top-level source item,
//! sized by `transfer_concurrency`. It is the only driver in the transfer
//! subsystem that runs several sources at once (moves are serial, local-FS copy
//! is serial), which is why it lives here rather than inside
//! `transfer_driver/`: sharing one abstraction with the serial driver would mean
//! reconciling `Fn + Send + Sync` (what `FuturesUnordered` polling needs) with
//! the serial driver's per-call `FnMut`, for a 1-of-4 win.
//!
//! Three files, one per stage of a source's life:
//!
//! - **This one** owns the window and the state that changes as it turns:
//!   `ConcurrentDriver::run` fills, waits, records.
//! - **`copy_concurrent_source.rs`** gets one source ready to spawn, including
//!   conflict resolution — which runs synchronously HERE, on the driver, so the
//!   whole batch blocks on a single Stop prompt instead of racing per-task
//!   prompts.
//! - **`copy_concurrent_task.rs`** streams one source end to end.
//!
//! `copy_volumes_with_progress` owns everything around all three: the phases
//! before (dest-dir creation, temp reap, preflight, space check), the shared
//! counters and rollback ledgers, and all post-loop bookkeeping. This module
//! only drives the window and reports what happened.
//!
//! Semantics, the cancel-drain contract, and the merge invariant: `DETAILS.md`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::super::super::conflict::ApplyToAll;
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::journal;
use super::super::super::state::{WriteOperationState, is_cancelled, load_intent, update_operation_status};
use super::super::super::types::{VolumeCopyConfig, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::super::dest_name_index::DestNameIndex;
use super::super::transfer_probe::OperationProbe;
use super::copy::drain_deadline as drain_deadline_for;
use super::copy_concurrent_task::{CopyTaskFailure, CopyTaskSuccess, run_copy_task};
use super::preflight::SourceHint;
use super::transfer_error::{PathRole, WriteFailure};
use crate::file_system::volume::Volume;
use crate::ignore_poison::IgnorePoison;

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
    pub(super) file_window: super::strategy::FileWindow,
    /// The destination directory was created by THIS operation (Phase 0.5), so
    /// nothing the user already had can be inside it and every pre-check is a
    /// guaranteed miss.
    pub(super) dest_dir_is_ours: bool,
    /// The one destination listing Phase 0.6 already paid for, indexed for name
    /// lookups. `None` ⇒ probe per file.
    pub(super) dest_index: &'a Option<DestNameIndex>,
    pub(super) pre_skip_paths: &'a HashSet<PathBuf>,
    pub(super) source_hints: &'a HashMap<PathBuf, SourceHint>,
    pub(super) total_files: usize,
    pub(super) total_bytes: u64,
    pub(super) progress_interval: Duration,
    /// Real (source, dest) volume ids for the operation-log journal. `None` for
    /// the both-local shortcut and in tests that install no journal.
    pub(super) journal_volumes: &'a Option<(String, String)>,
    pub(super) op_probe: &'a Option<Arc<OperationProbe>>,
    pub(super) files_done_atomic: Arc<AtomicUsize>,
    pub(super) atomic_bytes_done: Arc<AtomicU64>,
    pub(super) files_skipped_atomic: Arc<AtomicUsize>,
    pub(super) bytes_skipped_atomic: Arc<AtomicU64>,
    pub(super) last_progress_mutex: Arc<std::sync::Mutex<Instant>>,
    pub(super) apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>>,
    pub(super) copied_paths: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) created_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) in_flight_partials: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) deep_skipped_files: Arc<AtomicUsize>,
    pub(super) deep_skipped_bytes: Arc<AtomicU64>,
}

/// What the driver leaves for the caller's post-loop bookkeeping. The counters
/// aren't in here: they live in the shared atomics the caller already holds.
pub(super) struct ConcurrentOutcome {
    /// The single half-written partial of a FILE source whose stream failed, for
    /// the post-loop cleanup sweep. Stays `None` for a directory source, whose
    /// cleanup runs per-file off the `copied_paths` / `created_dirs` ledgers.
    pub(super) last_dest_path: Option<PathBuf>,
    /// The first task failure, if any. The driver stops pushing new work and
    /// drains on one.
    pub(super) copy_error: Option<WriteFailure>,
}

/// Runs the sliding window to completion, cancellation, or the first failure.
///
/// Returns `Err` only when conflict resolution itself fails; a failed transfer
/// comes back as `ConcurrentOutcome::copy_error` so the caller's post-loop still
/// runs its rollback and cleanup.
pub(super) async fn drive_transfer_concurrent(ctx: ConcurrentCopy<'_>) -> Result<ConcurrentOutcome, WriteFailure> {
    let mut driver = ConcurrentDriver::new(ctx);
    driver.run().await?;
    Ok(driver.finish())
}

/// One task per top-level source item, streaming end to end. The future owns
/// everything it touches (`copy_concurrent_task.rs`), so the window carries no
/// lifetime and the phases around it stay free to borrow the driver.
type CopyTaskFuture = Pin<Box<dyn Future<Output = Result<CopyTaskSuccess, CopyTaskFailure>> + Send>>;

/// The sliding window and the state that changes as it runs.
///
/// The borrowed context stays whole in `ctx` — it is what the per-source
/// preparation in `copy_concurrent_source.rs` reads, and none of it changes
/// during a run. Everything that DOES change lives here, which is what makes
/// each phase a method you can read (and drive from a test) on its own.
struct ConcurrentDriver<'a> {
    ctx: ConcurrentCopy<'a>,
    in_flight: FuturesUnordered<CopyTaskFuture>,
    /// The sources still to prepare, in the order the user selected them.
    sources: std::iter::Enumerate<std::slice::Iter<'a, PathBuf>>,
    /// Set once the driver has SEEN the user's cancel / rollback: from then on
    /// it stops waiting indefinitely for its tasks and gives them a bounded
    /// window to wind down. See `copy.rs::CANCEL_DRAIN_DEADLINE`.
    drain_deadline: Option<tokio::time::Instant>,
    /// Whether that window is the hard-abort tier's short one rather than the
    /// cooperative cancel's. Latched, so the abort re-arms the deadline once.
    drain_shortened: bool,
    /// Handed back to the caller's post-loop; see `ConcurrentOutcome`.
    last_dest_path: Option<PathBuf>,
    copy_error: Option<WriteFailure>,
}

/// How one trip through the driver's await ended.
enum AwaitStep {
    /// The wind-down window was armed or shortened. Nothing settled; go round
    /// again so the new deadline is the one being waited on.
    Rearmed,
    /// A task came back.
    Settled(Result<CopyTaskSuccess, CopyTaskFailure>),
    /// The window emptied, or the wind-down deadline expired with tasks still
    /// in it — those are abandoned, and their staged partials are cleaned up by
    /// the caller's post-loop.
    Finished,
}

impl<'a> ConcurrentDriver<'a> {
    fn new(ctx: ConcurrentCopy<'a>) -> Self {
        Self {
            sources: ctx.source_paths.iter().enumerate(),
            ctx,
            in_flight: FuturesUnordered::new(),
            drain_deadline: None,
            drain_shortened: false,
            last_dest_path: None,
            copy_error: None,
        }
    }

    /// Fill the window, wait for something, record it. Ends when the sources and
    /// the window are both empty, on the first task failure, or when a
    /// wind-down deadline expires.
    async fn run(&mut self) -> Result<(), WriteFailure> {
        loop {
            self.spawn_ready_tasks().await?;
            if self.in_flight.is_empty() {
                break;
            }
            match self.await_next().await {
                AwaitStep::Rearmed => continue,
                AwaitStep::Finished => break,
                AwaitStep::Settled(Ok(success)) => self.record_success(success),
                AwaitStep::Settled(Err(failure)) => {
                    self.record_failure(failure);
                    // Drop remaining in-flight tasks; their streams close, temp
                    // files get cleaned up by the per-backend write abort +
                    // delete path. Partial cleanup is the caller's post-loop.
                    break;
                }
            }
        }
        Ok(())
    }

    /// Keep preparing and pushing sources until either they run out or the
    /// window is full.
    ///
    /// Preparing a source is where conflict resolution happens, synchronously,
    /// on this driver — so while a person is answering a Stop prompt the window
    /// is neither filled further nor drained, and nothing is streaming into a
    /// folder they are still being asked about.
    async fn spawn_ready_tasks(&mut self) -> Result<(), WriteFailure> {
        while self.in_flight.len() < self.ctx.concurrency {
            if is_cancelled(&self.ctx.state.intent) {
                break;
            }
            let Some((source_index, source_path)) = self.sources.next() else {
                break;
            };
            if let Some(task) = self.ctx.prepare_source(source_index, source_path).await? {
                self.in_flight.push(Box::pin(run_copy_task(task)));
            }
        }
        Ok(())
    }

    /// Wait for the next task, for the user's cancel, or for the wind-down
    /// deadline — whichever comes first.
    ///
    /// Observing the user's intent HERE is what makes Cancel and Rollback work
    /// at all. `spawn_ready_tasks` checks `is_cancelled` too, but a driver whose
    /// window is full — or whose sources have run out — never gets back to it
    /// while its tasks are parked, which is why Rollback did nothing in the
    /// 2026-07-31 incident. `backend_cancel` fires on every transition out of
    /// `Running`, so both Cancel and Rollback land here.
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
            // Already winding down: bounded, so one task that never returns
            // can't hold the operation (and the user's dialog) open.
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
                         cutting the wind-down short (to {shortened:?})",
                        self.in_flight.len(),
                    );
                    self.drain_deadline = Some(tokio::time::Instant::now() + shortened);
                    return AwaitStep::Rearmed;
                }
                result = tokio::time::timeout_at(deadline, self.in_flight.next()) => match result {
                    Ok(next) => next,
                    Err(_) => {
                        crate::log_error!(
                            target: "copy",
                            "copy_volumes_with_progress: op={} abandoning {} task(s) that did not wind down within {:?} of the cancel. \
                             Their handles are left for the backend to reap; their staged partials are cleaned up below.{}",
                            operation_id,
                            self.in_flight.len(),
                            drain_deadline_for(self.drain_shortened),
                            self.ctx.op_probe
                                .as_ref()
                                .map(|p| format!("\n{}", p.render_dump("abandoning wedged tasks")))
                                .unwrap_or_default(),
                        );
                        return AwaitStep::Finished;
                    }
                },
            },
            None => {
                tokio::select! {
                    // Cancel wins the race: a task result that was also ready
                    // is re-polled on the next pass, so nothing is dropped.
                    biased;
                    () = self.ctx.state.backend_cancel.cancelled() => {
                        // An abort fires tier 1 first, so it can be what woke this
                        // arm; ask which window we're owed rather than assuming.
                        self.drain_shortened = self.ctx.state.backend_abort.is_cancelled();
                        let window = drain_deadline_for(self.drain_shortened);
                        log::info!(
                            target: "copy",
                            "copy_volumes_with_progress: op={} observed {:?} while awaiting {} in-flight task(s); \
                             winding them down (up to {window:?})",
                            operation_id,
                            load_intent(&self.ctx.state.intent),
                            self.in_flight.len(),
                        );
                        self.drain_deadline = Some(tokio::time::Instant::now() + window);
                        return AwaitStep::Rearmed;
                    }
                    next = self.in_flight.next() => next,
                }
            }
        };
        match next {
            Some(settled) => AwaitStep::Settled(settled),
            None => AwaitStep::Finished,
        }
    }

    /// One source landed: fold its tallies in, drop its in-flight partial,
    /// journal it, record what it wrote for rollback, and emit the milestone.
    fn record_success(&mut self, success: CopyTaskSuccess) {
        let CopyTaskSuccess {
            partial_path,
            recorded_path,
            source_is_dir,
            bytes,
            overwrote,
            created_files,
            created_dirs: task_created_dirs,
            source_path: done_source,
            skipped_count: task_skipped_count,
            skipped_bytes: task_skipped_bytes,
        } = success;
        let ctx = &self.ctx;

        // Fold this source's deep-merge skips into the op-wide tally, and — when
        // the source landed with ZERO skips — record it as fully extracted so
        // the out-of-zip move op may drop it.
        ctx.deep_skipped_files.fetch_add(task_skipped_count, Ordering::Relaxed);
        ctx.deep_skipped_bytes.fetch_add(task_skipped_bytes, Ordering::Relaxed);
        if task_skipped_count == 0 {
            ctx.events.note_source_landed_clean(&done_source);
        }
        // Remove the in-flight partial (the temp under safe-replace, else the
        // dest) and record what the op wrote for rollback.
        ctx.forget_in_flight_partial(&partial_path);
        let file_name_done = recorded_path.file_name().map(|n| n.to_string_lossy().to_string());
        // Journal the per-leaf `rollback_unit` rows under the REAL volume ids (a
        // file source → one leaf at `recorded_path`; a dir source → one leaf per
        // `created_files` entry, source rebased from `done_source`). Created dirs
        // journal post-loop (after all files, so their `seq` follows the
        // contents). No-ops for the both-local shortcut (that path journals via
        // `copy_files_start`) and in tests that don't set a journal target.
        if let Some((src_vol, dst_vol)) = ctx.journal_volumes.as_ref() {
            journal::record_volume_transfer_source(
                ctx.operation_id,
                src_vol,
                &done_source,
                dst_vol,
                &recorded_path,
                source_is_dir,
                &created_files,
                (!source_is_dir).then_some(bytes as i64),
                overwrote,
            );
        }
        // For a DIRECTORY source, record the individual files and the
        // newly-created subdirs the op wrote — never the directory root — so
        // rollback can't recursively delete a merged directory and destroy
        // dest-only files. For a FILE source, record the landed path (the
        // original after a safe-replace finalize, else the dest); `created_*`
        // are empty.
        if source_is_dir {
            ctx.copied_paths
                .lock_ignore_poison()
                .extend(created_files.into_iter().map(|f| f.path));
            ctx.created_dirs.lock_ignore_poison().extend(task_created_dirs);
        } else {
            ctx.copied_paths.lock_ignore_poison().push(recorded_path);
        }
        // Per-file milestone emit. The task's `on_file_complete` bumped
        // `files_done_atomic`; the FE's files-axis needs a Copying event that
        // observes the bumped value, but no chunked emit fires after
        // `on_file_complete` (the file's transfer is over). Mirrors the serial
        // path's milestone in `transfer_driver.rs::drive_transfer_serial_async`.
        *ctx.last_progress_mutex.lock_ignore_poison() = Instant::now();
        let current_files = ctx.files_done_atomic.load(Ordering::Relaxed);
        let current_bytes = ctx.atomic_bytes_done.load(Ordering::Relaxed);
        ctx.state.emit_progress_via_sink(
            &*ctx.events,
            WriteProgressEvent::new(
                ctx.operation_id.to_string(),
                WriteOperationType::Copy,
                WriteOperationPhase::Copying,
                file_name_done.clone(),
                current_files,
                ctx.total_files,
                current_bytes,
                ctx.total_bytes,
            ),
        );
        update_operation_status(
            ctx.operation_id,
            WriteOperationPhase::Copying,
            file_name_done,
            current_files,
            ctx.total_files,
            current_bytes,
            ctx.total_bytes,
        );
    }

    /// One source failed: drop its in-flight partial, thread its rollback ledger
    /// out, and decide whether what it left behind is a partial to clean or
    /// committed data to keep.
    fn record_failure(&mut self, failure: CopyTaskFailure) {
        let CopyTaskFailure {
            failed_path: failed_dest,
            reported_path,
            source_path: done_source,
            error: e,
            cleanup_temp,
            source_is_dir,
            overwrote,
            created_files,
            created_dirs: task_created_dirs,
        } = failure;
        let ctx = &self.ctx;

        // Remove from in-flight partials; this one's own partial cleanup (if
        // any) the post-loop logic will do.
        ctx.forget_in_flight_partial(&failed_dest);
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
                    ctx.operation_id,
                    src_vol,
                    &done_source,
                    dst_vol,
                    &failed_dest,
                    true,
                    &created_files,
                    None,
                    overwrote,
                );
            }
            ctx.copied_paths
                .lock_ignore_poison()
                .extend(created_files.into_iter().map(|f| f.path));
            ctx.created_dirs.lock_ignore_poison().extend(task_created_dirs);
        } else if cleanup_temp {
            // FILE source stream failure: `failed_dest` is the single
            // half-written partial. Clean it.
            //
            // `cleanup_temp == false` ⇒ finalize failed AFTER a successful
            // write: `failed_dest` is the temp holding the ONLY complete copy of
            // the new data (finalize already deleted the original). Do NOT
            // designate it for cleanup — leaving it on disk as a `.cmdr-tmp-*`
            // artifact is the correct, safe outcome. Cleaning it would be total
            // data loss.
            self.last_dest_path = Some(failed_dest);
        }
        self.copy_error = Some(WriteFailure::from_volume(&reported_path, PathRole::Source, e));
    }

    /// Hand the post-loop what it needs, after letting go of whatever is left in
    /// the window.
    fn finish(self) -> ConcurrentOutcome {
        // Drain whatever's left on cancel/error. On success, `in_flight` is
        // already empty. On abort, drop cancels the remaining futures (F10).
        if let Some(probe) = self.ctx.op_probe.as_ref() {
            probe.set_driver_phase(
                super::super::transfer_probe::DriverPhase::PostLoop,
                "draining in-flight",
            );
        }
        drop(self.in_flight);

        ConcurrentOutcome {
            last_dest_path: self.last_dest_path,
            copy_error: self.copy_error,
        }
    }
}

/// The driver's own contract, asserted on what it hands back rather than on the
/// files a finished operation left: the post-loop's delete-capability split is a
/// second defense of the same data, so an end-to-end assertion can't tell a
/// right rollback ledger from a wrong one.
#[cfg(test)]
#[path = "copy_concurrent_driver_tests.rs"]
mod driver_tests;
