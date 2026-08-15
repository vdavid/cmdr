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

use crate::indexing::IndexPathSpace;
use crate::indexing::events::emit_dir_updated;
use crate::indexing::events::{
    ActivityPhase, DEBUG_STATS, EventSink, IndexEvent, RescanReason, emit_rescan_notification, set_phase_for,
};
use crate::indexing::reconcile::reconciler::{self, EventReconciler};
use crate::indexing::scanner::{ScanError, ScanSummary};
use crate::indexing::store::{IndexStore, ScanCalibrationKind};
use crate::indexing::watch::branches::{self, WatchScope};
use crate::indexing::watch::event_loop::{LiveConfig, run_live_event_loop};
use crate::indexing::watch::watcher::FsChangeEvent;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::pluralize;
use tokio_util::sync::CancellationToken;

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
    /// This volume's stop signal (the same one the manager holds). Handed to the
    /// post-scan reconciler so its detached subtree walks stop when the volume
    /// does; see `EventReconciler::new_for`.
    pub cancel: CancellationToken,
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
        events,
        writer,
        freshness,
        live_event_task_slot,
        scan_start_event_id,
        calibration_kind,
        cancel,
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
    let mut reconciler = EventReconciler::new_for(volume_id.clone(), space.clone(), cancel.clone());
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
    if let Err(e) = writer.flush().await {
        log::warn!("Reconciler: writer flush before replay failed: {e}");
    }

    // Signal that aggregation (and entry flushing) is complete.
    // The flush above drains all queued writes including
    // ComputeAllAggregates, so by this point the UI can dismiss
    // the progress overlay.
    events.emit(IndexEvent::AggregationComplete {
        volume_id: volume_id.clone(),
    });

    DEBUG_STATS.close_phase_with_stats(vec![]);
    set_phase_for(events.as_ref(), &volume_id, ActivityPhase::Reconciling, "post-scan");

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
    // Gate ALL meta writes behind `was_completed`: a user-stopped scan holds
    // only partial totals, and writing `scan_completed_at` for it would mark a
    // partial index as complete — the next startup would skip the
    // `IncompletePreviousScan` fresh rescan. With the clear-at-start above, a
    // cancelled scan leaves NO completion marker, so it heals on restart. The
    // reconcile/live transition below is intentionally NOT gated; only the meta
    // writes are.
    if was_completed {
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
        // The calibration numbers go into TWO buckets: this walk kind's own
        // keys (so the next run of the same kind gets an ETA from a
        // comparable run) and the unsuffixed keys (the last-completed-scan
        // facts the badge tooltip and the any-kind fallback read).
        for (key, value) in [
            ("scan_duration_ms", summary.duration_ms.to_string()),
            ("total_entries", summary.total_entries.to_string()),
            ("total_physical_bytes", summary.total_physical_bytes.to_string()),
        ] {
            let _ = writer.send(WriteMessage::UpdateMeta {
                key: calibration_kind.meta_key(key),
                value: value.clone(),
            });
            let _ = writer.send(WriteMessage::UpdateMeta {
                key: key.to_string(),
                value,
            });
        }
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

    DEBUG_STATS.close_phase_with_stats(vec![("buffered_events", buffered_count.to_string())]);
    set_phase_for(
        events.as_ref(),
        &volume_id,
        ActivityPhase::Live,
        "post-scan reconciliation complete",
    );

    // Step 5: Start live event processing loop
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
    {
        let mut guard = live_event_task_slot.lock_ignore_poison();
        *guard = Some(handle);
    }
}

/// Report a scan that neither finished nor was cancelled: a typed failure, or a
/// walker thread that panicked outright. Both reset freshness to Stale, and a
/// vanished root also clears the frontend's stuck "scanning" row.
///
/// Split out so the completion path above reads as one flow, and so a cancelled
/// walk can't slip in here: it arrives as an `Err` but must NOT be reported as a
/// failure, and the caller peels it off before this is ever reached.
fn report_unfinished_scan(
    result: &std::thread::Result<Result<ScanSummary, ScanError>>,
    events: &dyn EventSink,
    volume_id: &str,
    freshness: &std::sync::Mutex<Option<super::freshness::Freshness>>,
) {
    match result {
        Ok(Err(e)) => {
            log::warn!("Volume scan failed: {e}");
            // The scan/reconcile bailed (e.g. `EmptyRoot`, `RootUnlistable`, or a
            // `catch_unwind`-converted reconcile-walk `Panicked`). The prior index
            // is untouched and stays visible, but `ScanStarted` already moved
            // freshness to Scanning, so reset it to Stale — honest "rescan
            // available" instead of a stuck spinner. Fire through the cloned handle,
            // never the registry (no re-lock).
            super::state::apply_freshness_event_on(
                freshness,
                events,
                volume_id,
                super::freshness::FreshnessEvent::ScanFailed,
            );

            // If the failure is a VANISHED volume (its root went unlistable —
            // a yanked external drive), the scan will never complete on its own, so
            // clear the frontend's live activity and go Idle — mirroring the network
            // disconnect arm (`lifecycle/network_scan.rs`). A legitimately empty root
            // (`EmptyRoot`) or a panic is NOT a vanished volume, so it does not
            // abort. No `scan_completed_at` was written (the meta writes live in the
            // clean-completion arm only), so the index heals to a rescan on remount.
            if scan_failure_is_vanished_volume(e) {
                set_phase_for(
                    events,
                    volume_id,
                    ActivityPhase::Idle,
                    "local scan aborted (volume vanished)",
                );
                events.emit(IndexEvent::ScanAborted {
                    volume_id: volume_id.to_string(),
                });
            }
        }
        Err(_) => {
            log::warn!("Volume scan thread panicked");
            // The walker thread itself panicked (the reconcile walk is
            // `catch_unwind`-wrapped, so this is the residual guarded-walker/thread
            // case). Same honest reset as the `Ok(Err(_))` arm above.
            super::state::apply_freshness_event_on(
                freshness,
                events,
                volume_id,
                super::freshness::FreshnessEvent::ScanFailed,
            );
        }
        // The caller routes finished and cancelled walks itself and never gets
        // here; matching exhaustively keeps that split visible rather than
        // silently absorbing a future outcome into "failed".
        Ok(Ok(_)) => debug_assert!(false, "a completed scan must not reach the failure reporter"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::events::{IndexEventKind, RecordingSink};
    use crate::indexing::lifecycle::freshness::Freshness;

    /// Everything `run_scan_completion` needs, with a real writer over a real DB
    /// and a `RecordingSink` in place of the app, so the assertions can read the
    /// meta table the handler actually wrote.
    struct Fixture {
        writer: IndexWriter,
        db_path: std::path::PathBuf,
        events: Arc<RecordingSink>,
        freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
        /// Held so the watcher channel stays open for the duration of the test.
        _event_tx: tokio::sync::mpsc::UnboundedSender<FsChangeEvent>,
        event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<FsChangeEvent>>,
        _dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new(volume_id: &str) -> Self {
            let dir = tempfile::tempdir().expect("temp dir");
            let db_path = dir.path().join(format!("{volume_id}.db"));
            IndexStore::open(&db_path).expect("open store");
            let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn the writer");
            let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
            Self {
                writer,
                db_path,
                events: Arc::new(RecordingSink::new()),
                // A scan is in flight when the handler runs, so start from `Scanning`
                // (what `ScanStarted` left behind).
                freshness: Arc::new(std::sync::Mutex::new(Some(Freshness::Scanning))),
                _event_tx: event_tx,
                event_rx: Some(event_rx),
                _dir: dir,
            }
        }

        /// Build the handler's inputs around a walk thread that resolves to `result`.
        fn completion(&mut self, volume_id: &str, result: Result<ScanSummary, ScanError>) -> ScanCompletion {
            ScanCompletion {
                join_handle: std::thread::spawn(move || result),
                scan_done: Arc::new(AtomicBool::new(false)),
                scanning: Arc::new(AtomicBool::new(true)),
                event_rx: self.event_rx.take().expect("one completion per fixture"),
                watcher_overflow_flag: None,
                volume_id: volume_id.to_string(),
                space: IndexPathSpace::root(),
                events: Arc::clone(&self.events) as Arc<dyn EventSink>,
                writer: self.writer.clone(),
                freshness: Arc::clone(&self.freshness),
                live_event_task_slot: Arc::new(std::sync::Mutex::new(None)),
                scan_start_event_id: 0,
                calibration_kind: ScanCalibrationKind::FullWalk,
                cancel: CancellationToken::new(),
            }
        }

        /// The value of `meta.scan_completed_at` once the writer has drained.
        async fn completion_marker(&self) -> Option<String> {
            self.writer.flush().await.expect("flush the writer");
            let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
            IndexStore::get_meta(&conn, "scan_completed_at").expect("read the meta table")
        }

        fn freshness_now(&self) -> Option<Freshness> {
            *self.freshness.lock_ignore_poison()
        }
    }

    fn summary(entries: u64) -> ScanSummary {
        ScanSummary {
            total_entries: entries,
            total_dirs: 1,
            total_physical_bytes: 4096,
            duration_ms: 12,
        }
    }

    /// A walk that ran to the end stamps the completion marker, so the next launch
    /// loads the index instead of rescanning the whole volume.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_completed_scan_writes_the_completion_marker() {
        let mut fx = Fixture::new("done-clean");
        let params = fx.completion("done-clean", Ok(summary(42)));
        run_scan_completion(params).await;

        assert!(
            fx.completion_marker().await.is_some(),
            "a clean completion must stamp `scan_completed_at`"
        );
        assert_eq!(
            fx.freshness_now(),
            Some(Freshness::Fresh),
            "a clean completion is authoritative"
        );
    }

    /// The one that strands an index if it ever goes wrong: a stopped scan holds
    /// only partial totals, so marking it complete would make the next launch skip
    /// the healing rescan and serve a permanently half-built index.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancelled_scan_writes_no_completion_marker() {
        let mut fx = Fixture::new("done-cancelled");
        let params = fx.completion("done-cancelled", Err(ScanError::Cancelled(summary(7))));
        run_scan_completion(params).await;

        assert_eq!(
            fx.completion_marker().await,
            None,
            "a cancelled scan must leave `scan_completed_at` absent so it heals on restart"
        );
    }

    /// Cancelled is its own outcome, distinguishable from BOTH neighbours. It
    /// isn't a completion (never `Fresh`, no marker) and it isn't a failure
    /// (freshness untouched, no `ScanAborted`), and the post-scan handoff still
    /// runs so the rows the walk did write are reconciled and served.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancelled_scan_is_neither_a_completion_nor_a_failure() {
        let mut fx = Fixture::new("cancel-not-fail");
        let params = fx.completion("cancel-not-fail", Err(ScanError::Cancelled(summary(7))));
        run_scan_completion(params).await;

        let kinds = fx.events.kinds_for("cancel-not-fail");
        assert!(
            !kinds.contains(&IndexEventKind::ScanAborted),
            "a user-stopped scan must not look like a vanished volume: {kinds:?}"
        );
        assert!(
            kinds.contains(&IndexEventKind::ScanComplete),
            "the post-scan handoff runs for a cancelled walk too (a failure's arm skips it): {kinds:?}"
        );
        // Not `Fresh` (that's a completion) and not `Stale` (that's a failure):
        // a stop leaves freshness exactly where `ScanStarted` put it.
        assert_eq!(
            fx.freshness_now(),
            Some(Freshness::Scanning),
            "a cancelled scan neither claims authority nor reports a failure"
        );
    }

    /// Failed is not cancelled and not complete: no marker, and freshness drops to
    /// Stale so the badge offers a rescan instead of spinning forever.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_scan_writes_no_marker_and_reports_stale() {
        let mut fx = Fixture::new("done-failed");
        let params = fx.completion("done-failed", Err(ScanError::RootUnlistable));
        run_scan_completion(params).await;

        assert_eq!(
            fx.completion_marker().await,
            None,
            "a failed scan must leave `scan_completed_at` absent"
        );
        assert_eq!(
            fx.freshness_now(),
            Some(Freshness::Stale),
            "a failed scan is honest about being stale"
        );
        assert!(
            fx.events
                .kinds_for("done-failed")
                .contains(&IndexEventKind::ScanAborted),
            "a vanished root clears the stuck scanning row"
        );
    }

    /// The heal: a scan that runs to the end after a cancelled one stamps the
    /// marker, so a stop is recoverable rather than a permanent state.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_rescan_after_a_cancelled_scan_writes_the_marker() {
        let mut fx = Fixture::new("cancel-then-rescan");
        let cancelled = fx.completion("cancel-then-rescan", Err(ScanError::Cancelled(summary(7))));
        run_scan_completion(cancelled).await;
        assert_eq!(
            fx.completion_marker().await,
            None,
            "precondition: the cancelled scan left no marker"
        );

        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        fx.event_rx = Some(rx);
        let rescan = fx.completion("cancel-then-rescan", Ok(summary(42)));
        run_scan_completion(rescan).await;

        assert!(
            fx.completion_marker().await.is_some(),
            "a completed rescan heals the index that was left unmarked"
        );
    }

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
