//! Post-scan completion orchestration for a local full scan.
//!
//! `IndexManager::start_scan` spawns [`run_scan_completion`] right after
//! kicking off the walk, so control flow is identical to an inline spawn. The
//! task waits for the walk to finish, then does the whole post-scan handoff:
//! drain buffered watcher events, handle overflow, emit scan-complete, write
//! completion meta, open the replay connection, replay buffered events,
//! backfill dir_stats, switch the reconciler to live, fire freshness, and
//! start the live event loop.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri_specta::Event;

use super::IndexPathSpace;
use super::event_loop::run_live_event_loop;
use super::events::{
    ActivityPhase, DEBUG_STATS, IndexAggregationCompleteEvent, IndexDirUpdatedEvent, IndexScanAbortedEvent,
    IndexScanCompleteEvent, RescanReason, emit_rescan_notification, set_phase_for,
};
use super::reconciler::{self, EventReconciler};
use super::scanner::{ScanError, ScanSummary};
use super::store::IndexStore;
use super::watcher::FsChangeEvent;
use super::writer::{IndexWriter, WriteMessage};
use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;

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
    /// The manager's "a scan is running" flag; reset to false on completion.
    pub scanning: Arc<AtomicBool>,
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
    /// Tauri app handle for emitting events.
    pub app: AppHandle,
    /// Writer handle for meta writes, flushing, and backfill.
    pub writer: IndexWriter,
    /// This volume's freshness signal (the same `Arc` the registry holds).
    /// Fired through `apply_freshness_event_on`, never a registry re-lock.
    pub freshness: Arc<std::sync::Mutex<Option<super::freshness::Freshness>>>,
    /// Slot the live event loop's `JoinHandle` is stored into so `shutdown()`
    /// can wait for it to drain.
    pub live_event_task_slot: Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// The watcher event id captured at scan start; the replay baseline.
    pub scan_start_event_id: u64,
}

/// Whether a failed local scan should emit `index-scan-aborted`: only when the
/// volume VANISHED (its root became unlistable), never for a legitimately empty
/// root or a walk panic. The abort event clears the frontend's stuck "scanning"
/// row; an empty root and a panic keep the prior index visible-stale without an
/// abort. Pure so the decision is unit-testable without an `AppHandle`.
fn scan_failure_is_vanished_volume(err: &ScanError) -> bool {
    matches!(err, ScanError::RootUnlistable)
}

/// Wait for the scan to finish, then run post-scan reconciliation and switch to
/// live mode. Spawned by `start_scan`; see [`ScanCompletion`] for the inputs.
pub(super) async fn run_scan_completion(params: ScanCompletion) {
    let ScanCompletion {
        join_handle,
        scan_done,
        scanning,
        event_rx,
        watcher_overflow_flag,
        volume_id,
        space,
        app,
        writer,
        freshness,
        live_event_task_slot,
        scan_start_event_id,
    } = params;

    // Wait for scan to complete
    let join_result = tokio::task::spawn_blocking(move || join_handle.join()).await;

    // Signal the progress reporter to stop regardless of outcome
    scan_done.store(true, Ordering::Relaxed);
    // Reset scanning flag so get_status() reports correctly and new scans can start
    scanning.store(false, Ordering::Relaxed);

    // Flatten the outer Result (from spawn_blocking) and inner Result (from thread join)
    let result = match join_result {
        Ok(thread_result) => thread_result,
        Err(e) => {
            log::warn!("Completion handler task failed: {e}");
            return;
        }
    };

    match result {
        Ok(Ok(summary)) => {
            log::info!(
                "Scan: complete ({} entries, {} dirs, {:.1}s)",
                summary.total_entries,
                summary.total_dirs,
                summary.duration_ms as f64 / 1000.0,
            );

            DEBUG_STATS.close_phase_with_stats(vec![
                ("entries", summary.total_entries.to_string()),
                ("dirs", summary.total_dirs.to_string()),
                ("duration_s", format!("{:.1}", summary.duration_ms as f64 / 1000.0)),
            ]);
            set_phase_for(&app, &volume_id, ActivityPhase::Aggregating, "post-scan");

            // Step 4: Reconcile buffered watcher events, in this volume's path space
            // (a mount-rooted drive strips its mount root before `resolve_path`).
            let mut reconciler = EventReconciler::new_for(volume_id.clone(), space.clone());

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
                    &app,
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

            // Emit scan-complete first, then start the flushing phase.
            // Order matters: the frontend's scan-complete handler calls
            // resetAggregation(), so the saving_entries event must come
            // after to avoid being immediately cleared.
            let _ = IndexScanCompleteEvent {
                volume_id: volume_id.clone(),
                total_entries: summary.total_entries,
                total_dirs: summary.total_dirs,
                duration_ms: summary.duration_ms,
            }
            .emit(&app);

            // Tell the writer how many entries the scan produced, so it
            // can report flushing progress as it drains remaining
            // InsertEntriesV2 batches from the channel.
            writer.set_expected_total_entries(summary.total_entries);

            // Flush the writer to ensure all scan batches are committed
            // before opening the read connection. Without this, the WAL
            // snapshot may not include the latest InsertEntriesV2 batches,
            // causing resolve_path to fail for recently-scanned parents.
            if let Err(e) = writer.flush().await {
                log::warn!("Reconciler: writer flush before replay failed: {e}");
            }

            // Signal that aggregation (and entry flushing) is complete.
            // The flush above drains all queued writes including
            // ComputeAllAggregates, so by this point the UI can dismiss
            // the progress overlay.
            let _ = IndexAggregationCompleteEvent {
                volume_id: volume_id.clone(),
            }
            .emit(&app);

            DEBUG_STATS.close_phase_with_stats(vec![]);
            set_phase_for(&app, &volume_id, ActivityPhase::Reconciling, "post-scan");

            // Tell the frontend to refresh all visible listings. Directory
            // sizes are now available for the first time after a full scan.
            let _ = IndexDirUpdatedEvent {
                paths: vec!["/".to_string()],
            }
            .emit(&app);

            // Store scan metadata now, before the reconciler replay which
            // can fail (e.g. "database is locked") and cause an early return.
            // Without this, scan_completed_at is never persisted and the next
            // startup triggers a full rescan of the entire volume.
            //
            // Gate ALL meta writes behind `!was_cancelled`: a user-stopped scan
            // holds only partial totals, and writing `scan_completed_at` for it
            // would mark a partial index as complete — the next startup would skip
            // the `IncompletePreviousScan` fresh rescan. With the clear-at-start
            // above, a cancelled scan leaves NO completion marker, so it heals on
            // restart. The reconcile/live transition below is intentionally NOT
            // gated; only the meta writes are.
            if !summary.was_cancelled {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs().to_string())
                    .unwrap_or_default();
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: "scan_completed_at".to_string(),
                    value: now,
                });
                // Any completed full walk restarts the shallow-`MustScanSubDirs`
                // sweep window and clears its coalesced count (the drift those
                // skipped signals stood for has now been repaired). Not only a
                // shallow-triggered sweep: the window means "a full walk happened
                // recently", so the user's own "Rescan now" counts too. See
                // `reconcile/reconciler/rescan_route.rs`.
                let sweep = reconciler::record_sweep_completed(&volume_id, reconciler::now_unix());
                if let Some(at) = sweep.last_sweep_unix {
                    let _ = writer.send(WriteMessage::UpdateMeta {
                        key: reconciler::SHALLOW_SWEEP_AT_KEY.to_string(),
                        value: at.to_string(),
                    });
                }
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: reconciler::SHALLOW_COALESCED_KEY.to_string(),
                    value: "0".to_string(),
                });
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: "scan_duration_ms".to_string(),
                    value: summary.duration_ms.to_string(),
                });
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: "total_entries".to_string(),
                    value: summary.total_entries.to_string(),
                });
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: "total_physical_bytes".to_string(),
                    value: summary.total_physical_bytes.to_string(),
                });
                let _ = writer.send(WriteMessage::UpdateMeta {
                    key: "volume_path".to_string(),
                    value: space.volume_root_string(),
                });
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
                reconciler::emit_dir_updated(&app, paths)
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
            if !summary.was_cancelled {
                super::state::apply_freshness_event_on(
                    &freshness,
                    &volume_id,
                    super::freshness::FreshnessEvent::ScanCompleted,
                );
            }

            DEBUG_STATS.close_phase_with_stats(vec![("buffered_events", buffered_count.to_string())]);
            set_phase_for(
                &app,
                &volume_id,
                ActivityPhase::Live,
                "post-scan reconciliation complete",
            );

            // Step 5: Start live event processing loop
            let writer_live = writer.clone();
            let app_live = app.clone();
            let volume_id_live = volume_id.clone();
            let overflow_live = watcher_overflow_flag.clone();
            let space_live = space.clone();
            let handle = tauri::async_runtime::spawn(async move {
                run_live_event_loop(
                    event_rx,
                    reconciler,
                    writer_live,
                    app_live,
                    volume_id_live,
                    space_live,
                    overflow_live,
                )
                .await;
            });

            // Store the handle so shutdown() can wait for it to drain
            {
                let mut guard = live_event_task_slot.lock_ignore_poison();
                *guard = Some(handle);
            }
        }
        Ok(Err(e)) => {
            log::warn!("Volume scan failed: {e}");
            // The scan/reconcile bailed (e.g. `EmptyRoot`, `RootUnlistable`, or a
            // `catch_unwind`-converted reconcile-walk `Panicked`). The prior index
            // is untouched and stays visible, but `ScanStarted` already moved
            // freshness to Scanning, so reset it to Stale — honest "rescan
            // available" instead of a stuck spinner. Fire through the cloned handle,
            // never the registry (no re-lock).
            super::state::apply_freshness_event_on(
                &freshness,
                &volume_id,
                super::freshness::FreshnessEvent::ScanFailed,
            );

            // If the failure is a VANISHED volume (its root went unlistable —
            // a yanked external drive), the scan will never complete on its own, so
            // clear the frontend's live activity and go Idle — mirroring the network
            // disconnect arm (`network_scan.rs`). A legitimately empty root
            // (`EmptyRoot`) or a panic is NOT a vanished volume, so it does not
            // abort. No `scan_completed_at` was written (the meta writes live in the
            // clean-completion arm only), so the index heals to a rescan on remount.
            if scan_failure_is_vanished_volume(&e) {
                set_phase_for(
                    &app,
                    &volume_id,
                    ActivityPhase::Idle,
                    "local scan aborted (volume vanished)",
                );
                let _ = IndexScanAbortedEvent {
                    volume_id: volume_id.clone(),
                }
                .emit(&app);
            }
        }
        Err(_) => {
            log::warn!("Volume scan thread panicked");
            // The walker thread itself panicked (the reconcile walk is
            // `catch_unwind`-wrapped, so this is the residual guarded-walker/thread
            // case). Same honest reset as the `Ok(Err(_))` arm above.
            super::state::apply_freshness_event_on(
                &freshness,
                &volume_id,
                super::freshness::FreshnessEvent::ScanFailed,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The abort decision fires ONLY for a vanished volume (`RootUnlistable`), so a
    /// yanked drive clears its stuck "scanning" row — but a legitimately empty root
    /// or a walk panic does NOT abort (the prior index stays visible-stale, no
    /// spurious activity clear). Pins the distinguisher the completion arm relies on.
    #[test]
    fn only_a_vanished_root_triggers_the_scan_abort() {
        assert!(
            scan_failure_is_vanished_volume(&ScanError::RootUnlistable),
            "a vanished (unlistable) root must abort"
        );
        assert!(
            !scan_failure_is_vanished_volume(&ScanError::EmptyRoot),
            "a legitimately empty root must NOT abort"
        );
        assert!(
            !scan_failure_is_vanished_volume(&ScanError::Panicked("boom".to_string())),
            "a walk panic must NOT abort"
        );
        assert!(
            !scan_failure_is_vanished_volume(&ScanError::WriterSend("gone".to_string())),
            "a writer-send failure must NOT abort"
        );
    }
}
