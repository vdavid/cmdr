//! Post-scan completion orchestration for a local full scan.
//!
//! `IndexManager::start_scan` spawns [`run_scan_completion`] right after
//! kicking off the walk, so control flow is identical to an inline spawn. The
//! task waits for the walk to finish, then does the whole post-scan handoff:
//! drain buffered watcher events, handle overflow, emit scan-complete, write
//! completion meta, open the replay connection, replay buffered events,
//! backfill dir_stats, switch the reconciler to live, fire freshness, and
//! start the live event loop.
//!
//! The two arms that aren't that flow live beside it: [`stamps`] is everything a
//! walk that ran to the END claims about itself, and [`unfinished`] is a walk that
//! neither finished nor was cancelled.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::indexing::IndexPathSpace;
use crate::indexing::events::emit_dir_updated;
use crate::indexing::events::{
    ActivityPhase, DEBUG_STATS, EventSink, IndexEvent, RescanReason, emit_rescan_notification, set_phase_for,
};
use crate::indexing::lifecycle::cover;
use crate::indexing::reconcile::reconciler::EventReconciler;
use crate::indexing::scanner::{ScanError, ScanSummary};
use crate::indexing::store::{IndexStore, ScanCalibrationKind, StepDurations};
use crate::indexing::watch::branches::{self, WatchScope};
use crate::indexing::watch::event_loop::{LiveConfig, run_live_event_loop};
use crate::indexing::watch::watcher::FsChangeEvent;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::pluralize;

use crate::indexing::hold::{HoldKind, VolumeWork};

pub(in crate::indexing::lifecycle) mod stamps;
mod unfinished;

use stamps::{split_save_and_compute, stamp_a_completed_walk, stamp_step_durations};
use unfinished::{report_a_vanished_drive, report_unfinished_scan};

/// Everything the post-scan completion task takes ownership of from
/// `start_scan`. These are exactly the variables the former inline closure
/// captured: the scanner join handle, the shared flags/handles, the watcher
/// channel, and the scan-start event id.
pub(super) struct ScanCompletion {
    /// The scanner/reconcile-walk thread handle. Joined (off a blocking task)
    /// to await scan completion. Both `scan_volume` and `start_local_reconcile`
    /// return this same shape.
    pub join_handle: std::thread::JoinHandle<Result<ScanSummary, ScanError>>,
    /// Set to true when the scan finishes so the progress reporter loop exits.
    pub scan_done: Arc<AtomicBool>,
    /// The manager's "a bulk producer owns this ground" flag; cleared on completion.
    pub ground_in_flux: Arc<AtomicBool>,
    /// The whole-volume claim `start_scan` took, held here for the rest of the
    /// scan's life and dropped when the walk is provably over.
    ///
    /// ⚠️ It travels rather than staying with the manager because the scan
    /// outlives the call that started it: `start_scan` returns while the walk runs,
    /// so a claim scoped to that frame would free the ground before the first row
    /// landed. Owned by this task, the ground frees on the completion path, the
    /// cancel path, and a panic alike — including the early returns below.
    pub ground: cover::Claim,
    /// Buffered watcher events; drained into the reconciler, then handed to the
    /// live event loop. Unbounded (Fix 2): the forward task never backpressures.
    pub event_rx: tokio::sync::mpsc::UnboundedReceiver<FsChangeEvent>,
    /// `None` if the watcher failed to start; otherwise the FSEvents overflow
    /// flag, checked here and passed to the live loop.
    pub watcher_overflow_flag: Option<Arc<AtomicBool>>,
    /// Volume id (for events, phases, and freshness).
    pub volume_id: String,
    /// The volume's path space (pass-through for the boot disk, mount-relative strip
    /// for a mount-rooted external drive). Threaded to the reconciler's post-scan
    /// buffered replay and the live event loop so both resolve in the right space.
    pub space: IndexPathSpace,
    /// Where this scan's completion reports go.
    pub events: Arc<dyn EventSink>,
    /// Writer handle for meta writes, flushing, and backfill.
    pub writer: IndexWriter,
    /// This volume's freshness signal (the same `Arc` the registry holds).
    /// Fired through `apply_freshness_event_on`, never a registry re-lock.
    pub freshness: Arc<std::sync::Mutex<Option<super::freshness::Freshness>>>,
    /// Slot the live event loop's `JoinHandle` is stored into so `shutdown()`
    /// can wait for it to drain.
    pub live_event_task_slot: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// The watcher event id captured at scan start; the replay baseline.
    pub scan_start_event_id: u64,
    /// Which calibration bucket this run's totals and duration belong in. The
    /// two walks differ ~5x in wall clock, so writing them into one slot makes
    /// the next run of the OTHER kind predict a wildly wrong ETA.
    pub calibration_kind: ScanCalibrationKind,
    /// This task's work (`ScanCompletion`), a child of the volume's: the task holds
    /// the drive for as long as it runs, the reconciler it hands to the live loop
    /// takes a `LiveLoop` child, and a stopped volume gets no replay and no loop.
    pub work: VolumeWork,
}

/// Wait for the scan to finish, then run post-scan reconciliation and switch to
/// live mode. Spawned by `start_scan`; see [`ScanCompletion`] for the inputs.
///
/// Ends by running the rescan somebody queued behind this scan, if there is one.
/// ⚠️ **Here rather than beside the claim release below**, and the gap is the
/// point: the ground comes back when the walk THREAD is done, but this task keeps
/// writing after that — buffered events, then `scan_completed_at`. A truncating
/// rescan starting in that window would take the old scan's completion marker onto
/// its own half-built index, the one state that makes a launch skip the heal.
pub(super) async fn run_scan_completion(params: ScanCompletion) {
    let volume_id = params.volume_id.clone();
    finish_the_scan(params).await;
    super::rescan_request::run_if_owed(&volume_id);
}

/// Everything the completion task does with the scan's result, join to live loop.
async fn finish_the_scan(params: ScanCompletion) {
    let ScanCompletion {
        join_handle,
        scan_done,
        ground_in_flux,
        ground,
        event_rx,
        watcher_overflow_flag,
        volume_id,
        space,
        events,
        writer,
        freshness,
        live_event_task_slot,
        scan_start_event_id,
        calibration_kind,
        work,
    } = params;

    // Wait for scan to complete
    let join_result = tokio::task::spawn_blocking(move || join_handle.join()).await;

    // Signal the progress reporter to stop regardless of outcome
    scan_done.store(true, Ordering::Relaxed);
    // Clear the in-flux flag so get_status() reports correctly
    ground_in_flux.store(false, Ordering::Relaxed);
    // And let the volume's ground go, which is what lets a search walk start.
    // Released HERE, right after the join: the walk thread is finished, so nothing
    // is reading the disk any more, and a search that waited through the scan gets
    // its answer without also waiting out the handoff below. What the handoff still
    // writes is arbitrated by the branch set, exactly as on any live volume.
    //
    // ⚠️ The rescan someone may have queued behind this scan does NOT run here; see
    // `run_scan_completion` for why it waits for the end of the handoff.
    drop(ground);

    // Flatten the outer Result (from spawn_blocking) and inner Result (from thread join)
    let result = match join_result {
        Ok(thread_result) => thread_result,
        Err(e) => {
            log::warn!("Completion handler task failed: {e}");
            return;
        }
    };

    // The three outcomes stay THREE, split exactly here, once.
    //
    // A cancelled walk is neither a completion nor a failure. It takes the same
    // post-scan handoff as a clean one (the rows it wrote are real and want
    // reconciling), but writes NO completion meta and touches NO freshness.
    // `was_completed` gates both, so those two can never drift apart. Collapsing
    // cancelled into either neighbour is the bug to watch for: folded into the
    // `Ok` arm it stamps `scan_completed_at` on a partial and strands the index
    // permanently; folded into the failure arm it fires `ScanFailed` and can
    // raise a spurious abort for a volume that never went anywhere.
    let (summary, was_completed) = match result {
        Ok(Ok(summary)) => (summary, true),
        Ok(Err(ScanError::Cancelled(partial))) => (partial, false),
        unfinished => {
            report_unfinished_scan(&unfinished, events.as_ref(), &volume_id, &freshness);
            return;
        }
    };

    // ⚠️ **The completion gate.** ONE presence read decides everything this task
    // still claims about the volume: the stamp, its calibration and sweep keys,
    // the freshness flip, the phase, and the live loop it would start. A walk can
    // end `Ok` and its drive still be gone by the time any of that is written —
    // the walk's own gate answers for the reads it made, and this answers for the
    // claims those reads would support.
    //
    // ❌ It gates the CANCELLED outcome too: a cancelled walk on a drive that left
    // is owed no live loop either, because nothing would ever drain one and it
    // would read a drive that isn't there.
    if !work.drive_is_listed() {
        log::warn!("Scan completion: '{volume_id}' stopped being listed, so its scan claims nothing");
        // Whatever it deleted on the way out is what the rebuild marker is for.
        crate::indexing::deletes::note_the_drive_left(&volume_id, &writer);
        super::state::apply_freshness_event_on(
            &freshness,
            events.as_ref(),
            &volume_id,
            super::freshness::FreshnessEvent::ScanFailed,
        );
        report_a_vanished_drive(events.as_ref(), &volume_id);
        return;
    }
    crate::indexing::deletes::drive_seen(work.volume_id());

    log::info!(
        "Scan: {} ({} entries, {} dirs, {:.1}s)",
        if was_completed { "complete" } else { "cancelled" },
        summary.total_entries,
        summary.total_dirs,
        summary.duration_ms as f64 / 1000.0,
    );

    DEBUG_STATS.close_phase_with_stats(vec![
        ("entries", summary.total_entries.to_string()),
        ("dirs", summary.total_dirs.to_string()),
        ("duration_s", format!("{:.1}", summary.duration_ms as f64 / 1000.0)),
    ]);
    set_phase_for(events.as_ref(), &volume_id, ActivityPhase::Aggregating, "post-scan");

    // Step 4: Reconcile buffered watcher events, in this volume's path space
    // (a mount-rooted drive strips its mount root before `resolve_path`).
    //
    // A scanned volume is watched WHOLE: the scan covered every path its stream
    // can carry. It still holds a branch set, because a search can walk a hole in
    // an indexed drive, and those events have to wait for that walk exactly as
    // they would on an unindexed one.
    let scope = WatchScope::WholeVolume(branches::live_for(&volume_id));
    let mut reconciler = EventReconciler::new_for(volume_id.clone(), space.clone(), work.child(HoldKind::LiveLoop));
    reconciler.within(scope.clone());

    // Drain all buffered events from the channel into the reconciler
    let mut event_rx = event_rx;
    let mut buffered_count = 0u64;
    while let Ok(event) = event_rx.try_recv() {
        reconciler.buffer_event(event);
        buffered_count += 1;
    }
    log::info!(
        "Reconciler: {} buffered during scan",
        pluralize(buffered_count, "event")
    );

    if reconciler.did_buffer_overflow() {
        emit_rescan_notification(
            events.as_ref(),
            &volume_id,
            RescanReason::ReconcilerBufferOverflow,
            "The filesystem watcher buffered over 500,000 events during the \
             scan, exceeding the reconciler's capacity. A lot of filesystem \
             activity was happening during the scan."
                .to_string(),
        );
    }

    // Check if the FSEvents channel overflowed (events dropped
    // before reaching the forward task). If so, our buffered events
    // are incomplete. The reconciler replay will miss changes.
    // We still proceed (the scan data itself is fine), but log a
    // warning. The live event loop will detect the overflow flag
    // and trigger a rescan at that point, since a fresh scan is
    // the only way to recover from dropped events.
    if let Some(ref flag) = watcher_overflow_flag
        && flag.load(Ordering::Relaxed)
    {
        log::info!(
            "FSEvents channel overflowed during scan. Some watcher \
                 events were dropped. Live event loop will trigger a rescan."
        );
    }

    // Emit scan-complete first: it says the WALK is over, and the flushing
    // progress below belongs to the step after it.
    //
    // ⚠️ The aggregation terminal is the mirror image, and the flush between
    // them is load-bearing: a progress tick arriving after it reopens a status
    // step nothing would ever close again. Same rule, same reason, in
    // `phases/completion.rs`.
    events.emit(IndexEvent::ScanComplete {
        volume_id: volume_id.clone(),
        total_entries: summary.total_entries,
        total_dirs: summary.total_dirs,
        duration_ms: summary.duration_ms,
    });

    // Tell the writer how many entries the scan produced, so it
    // can report flushing progress as it drains remaining
    // InsertEntriesV2 batches from the channel.
    writer.set_expected_total_entries(summary.total_entries);

    // Flush the writer to ensure all scan batches are committed
    // before opening the read connection. Without this, the WAL
    // snapshot may not include the latest InsertEntriesV2 batches,
    // causing resolve_path to fail for recently-scanned parents.
    //
    // This wait IS the save and compute steps, so it's timed: the writer
    // timed the aggregate itself, and the rest of the wait is the save.
    let flush_started = Instant::now();
    if let Err(e) = writer.flush().await {
        log::warn!("Reconciler: writer flush before replay failed: {e}");
    }
    let save_and_compute = split_save_and_compute(elapsed_ms(flush_started), writer.take_last_full_aggregate_ms());

    // Signal that aggregation (and entry flushing) is complete.
    // The flush above drains all queued writes including
    // ComputeAllAggregates, so by this point the UI can dismiss
    // the progress overlay.
    events.emit(IndexEvent::AggregationComplete {
        volume_id: volume_id.clone(),
    });

    DEBUG_STATS.close_phase_with_stats(vec![]);
    set_phase_for(events.as_ref(), &volume_id, ActivityPhase::Reconciling, "post-scan");
    let catch_up_started = Instant::now();

    // Tell the frontend to refresh all visible listings. Directory
    // sizes are now available for the first time after a full scan.
    events.emit(IndexEvent::DirsUpdated {
        paths: vec!["/".to_string()],
    });

    // Store scan metadata now, before the reconciler replay which
    // can fail (e.g. "database is locked") and cause an early return.
    // Without this, scan_completed_at is never persisted and the next
    // startup triggers a full rescan of the entire volume.
    //
    // Gate ALL meta writes behind `was_completed` (see [`stamps`]): a user-stopped
    // scan holds only partial totals. The reconcile/live transition below is
    // intentionally NOT gated; only the meta writes are.
    if was_completed {
        stamp_a_completed_walk(&volume_id, &summary, &space, calibration_kind, &writer);
        stamp_step_durations(save_and_compute, calibration_kind, &writer);
    }

    // A volume stopped while its scan was finishing gets no replay: the replay reads
    // the drive, and the manager that would drain what follows is already gone.
    if work.cancel.is_cancelled() {
        log::debug!("Scan completion: '{volume_id}' was stopped, so no replay and no live loop");
        return;
    }

    // Open a read connection for path resolution during replay
    let replay_conn = match IndexStore::open_read_connection(&writer.db_path()) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Reconciler: failed to open read connection for replay: {e}");
            return;
        }
    };

    // Set a baseline last_event_id so there's always a valid
    // event ID even if no live events were buffered during the scan.
    // The reconciler will overwrite this with a higher ID if any
    // post-scan events exist.
    if scan_start_event_id > 0 {
        let _ = writer.send(WriteMessage::UpdateLastEventId(scan_start_event_id));
    }

    // Replay events that arrived after the scan read their paths
    match reconciler.replay(scan_start_event_id, &replay_conn, &writer, &mut |paths| {
        emit_dir_updated(events.as_ref(), paths)
    }) {
        Ok(last_id) => {
            log::info!("Reconciler: post-scan replay complete (last_event_id={last_id})");
        }
        Err(e) => {
            log::warn!("Reconciler: replay failed: {e}");
        }
    }

    // Backfill dir_stats for any directories created by the replay
    // that didn't go through the full aggregation pass.
    let _ = writer.send(WriteMessage::BackfillMissingDirStats);

    // Switch to live mode
    reconciler.switch_to_live();

    // Freshness ⇒ Fresh (green) on a clean completion. A cancelled
    // local scan keeps its prior freshness (root stays browsable);
    // it isn't reset to gray the way an interrupted SMB scan is,
    // because local data isn't tied to a connection that vanished.
    if was_completed {
        super::state::apply_freshness_event_on(
            &freshness,
            events.as_ref(),
            &volume_id,
            super::freshness::FreshnessEvent::ScanCompleted,
        );
    }

    // The catch-up step ends here, so a completed walk remembers how long it took.
    if was_completed {
        let catch_up = StepDurations {
            catch_up_ms: Some(elapsed_ms(catch_up_started)),
            ..StepDurations::default()
        };
        stamp_step_durations(catch_up, calibration_kind, &writer);
    }

    DEBUG_STATS.close_phase_with_stats(vec![("buffered_events", buffered_count.to_string())]);
    set_phase_for(
        events.as_ref(),
        &volume_id,
        ActivityPhase::Live,
        "post-scan reconciliation complete",
    );

    // Step 5: Start live event processing loop, under the slot's lock. A stop cancels
    // the volume's work BEFORE `shutdown` takes the slot, so a loop started after this
    // check is always one `shutdown` sees and waits on, and a stop that landed first
    // gets no loop at all: nothing would ever drain it, and it would read the drive.
    let mut slot = live_event_task_slot.lock_ignore_poison();
    if work.cancel.is_cancelled() {
        log::debug!("Scan completion: '{volume_id}' was stopped before its live loop started");
        return;
    }
    let writer_live = writer.clone();
    let events_live = Arc::clone(&events);
    let volume_id_live = volume_id.clone();
    let overflow_live = watcher_overflow_flag.clone();
    let space_live = space.clone();
    let handle = crate::indexing::host::runtime::spawn(async move {
        run_live_event_loop(
            event_rx,
            reconciler,
            writer_live,
            events_live,
            LiveConfig {
                volume_id: volume_id_live,
                space: space_live,
                watcher_overflow: overflow_live,
                scope,
            },
        )
        .await;
    });

    // Store the handle so shutdown() can wait for it to drain
    *slot = Some(handle);
}

/// Whole milliseconds since `started`, saturating rather than truncating.
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
