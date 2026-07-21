//! Cold-start journal replay: `run_replay_event_loop` processes FSEvents
//! replayed from the journal (boot disk only), enters live mode immediately
//! after flushing, and spawns background verification. Holds `ReplayConfig`,
//! the replay-only bounded-buffer constants, and the replay helpers
//! `defer_replay_rescan` and `flush_replay_batch`. Reuses the live machinery
//! (`process_live_batch`, `mark_pending_and_drain`) and the shared primitives
//! (`merge_fs_events`, `open_read_conn_with_retry`) from the parent module.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_specta::Event;

use super::super::ActivityPhase;
use super::super::DEBUG_STATS;
use super::super::IndexPathSpace;
use super::super::churn_monitor::ChurnObserver;
use super::super::events::{
    IndexReplayCompleteEvent, IndexReplayProgressEvent, RescanReason, emit_rescan_notification, set_phase_for,
};
use super::super::lifecycle_bus;
use super::super::reconciler::{self, EventReconciler};
use super::super::watcher;
use super::super::writer::{IndexWriter, WriteMessage};
use super::live::{mark_pending_and_drain, process_live_batch};
use super::verification::run_background_verification;
use super::{
    BacklogTracker, IngestionPressure, JOURNAL_GAP_THRESHOLD, LIVE_FLUSH_INTERVAL_MS, ReplayConfig,
    THROTTLE_SWEEP_INTERVAL_MS, classify_ingestion_pressure, merge_fs_events, open_read_conn_with_retry,
    report_backlog,
};
use crate::pluralize::pluralize;

/// Cap on `affected_paths` during replay. When exceeded, individual path
/// tracking stops and a single "full refresh" is emitted instead.
const MAX_AFFECTED_PATHS: usize = 50_000;

/// If the number of events processed during replay exceeds this threshold,
/// abort replay and fall back to a full scan. Safety net for scenarios where
/// FDA was toggled and the app suddenly sees millions of previously hidden paths.
const REPLAY_EVENT_COUNT_LIMIT: u64 = 10_000_000;

/// Replay events are deduplicated by normalized path in batches of this
/// size before processing. Dramatically reduces CPU when the FSEvents
/// journal contains many duplicate events for the same path (for example,
/// SQLite journal files, browser cache). Matches the `UpdateLastEventId`
/// batching cadence.
const REPLAY_DEDUP_BATCH_SIZE: u64 = 1_000;

// ── Replay event loop (cold start sinceWhen) ─────────────────────────

/// Process FSEvents replayed from the journal on cold start.
///
/// Two-phase approach to avoid a race condition where `index-dir-updated`
/// notifications fire before the writer commits replay data to SQLite:
///
/// **Phase 1 (replay):** Process events via `process_fs_event` directly,
/// collecting affected parent paths in a `HashSet`. No per-event UI
/// notification. `UpdateLastEventId` sent every 1000 events to reduce
/// writer load.
///
/// **Phase 2 (after HistoryDone):** Send final `UpdateLastEventId`, flush
/// the writer (wait for all prior messages to commit), then emit a single
/// batched `index-dir-updated`. After that, continue processing live events
/// with per-event emit (live events arrive slowly enough for the writer to
/// keep up).
///
/// If a journal gap is detected (first event ID >> stored last_event_id),
/// sends a signal via `fallback_tx` to trigger a full scan.
pub(in crate::indexing) async fn run_replay_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<watcher::FsChangeEvent>,
    writer: IndexWriter,
    app: AppHandle,
    config: ReplayConfig,
    fallback_tx: tokio::sync::oneshot::Sender<RescanReason>,
    watcher_overflow: Option<Arc<AtomicBool>>,
    scanning: Arc<AtomicBool>,
) -> Result<(), String> {
    let ReplayConfig {
        volume_id,
        space,
        since_event_id,
        estimated_total,
        heal_after_replay,
    } = config;

    log::info!("Replay: started (since_event_id={since_event_id}, estimated_total={estimated_total:?})");
    log::info!(target: "stall_probe::reconciler", "replay_event_loop_started since_event_id={since_event_id}");

    // Open a read-only connection for path resolution (integer-keyed lookups).
    // See `run_live_event_loop` for the rationale on read-only + retry.
    let db_path = writer.db_path();
    let conn = match open_read_conn_with_retry(&db_path).await {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("Failed to open read connection for replay: {e}"));
        }
    };

    let mut event_count = 0u64;
    let mut first_event_checked = false;
    let mut fallback_tx = Some(fallback_tx);
    let mut last_event_id = since_event_id;
    // Backlog reporting for both replay phases: reports the TREND, so a large
    // cold-start backlog draining steadily reads as progress, not as a warning.
    let mut backlog = BacklogTracker::new();

    // Collect all affected parent paths during replay (deduplicated).
    // Capped at MAX_AFFECTED_PATHS; beyond that we emit a full refresh.
    let mut affected_paths = HashSet::<String>::new();
    let mut affected_paths_overflow = false;

    // MustScanSubDirs paths to queue after replay. A `HashSet` dedups the anchor
    // churn a long gap produces (the same dir re-flagged thousands of times); the
    // live drain they hand off to ancestor-collapses and per-subtree-throttles
    // them, so there's no cap here and no full-rescan escalation on volume.
    let mut pending_rescans = HashSet::<String>::new();

    // Progress reporting interval
    let mut last_progress = Instant::now();
    let replay_start = Instant::now();

    // Wrap all replay writes in a single SQLite transaction.
    // Without this, each write is auto-committed (separate fsync), making
    // 50K+ writes take minutes. With a transaction, it takes seconds.
    if let Err(e) = writer.send(WriteMessage::BeginTransaction) {
        log::warn!("Replay: BeginTransaction send failed: {e}");
    }

    // ── Phase 1: Replay (before HistoryDone) ─────────────────────────

    // Deduplicate events by normalized path before processing, same as
    // the live event loop. Flushed every REPLAY_DEDUP_BATCH_SIZE events.
    let mut replay_pending = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut deduped_total = 0u64;

    while let Some(event) = event_rx.recv().await {
        // Check for journal gap on the first event
        if !first_event_checked {
            first_event_checked = true;
            if event.event_id > since_event_id + JOURNAL_GAP_THRESHOLD {
                emit_rescan_notification(
                    &app,
                    &volume_id,
                    RescanReason::JournalGap,
                    format!(
                        "Stored last_event_id={since_event_id}, first received event_id={}, \
                         gap={} (threshold={JOURNAL_GAP_THRESHOLD}). FSEvents journal may \
                         have been purged.",
                        event.event_id,
                        event.event_id - since_event_id,
                    ),
                );
                if let Some(tx) = fallback_tx.take() {
                    let _ = tx.send(RescanReason::JournalGap);
                }
                return Ok(());
            }
            log::debug!(
                "Replay: first event_id={}, gap from stored={}, journal appears available",
                event.event_id,
                event.event_id.saturating_sub(since_event_id),
            );
        }

        // HistoryDone marks end of replay phase
        if event.flags.history_done {
            log::info!("Replay: HistoryDone received after {}", pluralize(event_count, "event"));

            // Flush remaining deduplicated events before leaving Phase 1
            deduped_total += flush_replay_batch(
                &mut replay_pending,
                &space,
                &conn,
                &writer,
                &mut affected_paths,
                &mut affected_paths_overflow,
                &mut pending_rescans,
            ) as u64;

            // Process the HistoryDone event itself (it may carry other flags).
            // A missing-parent escalation here DEFERS into the pending list (no
            // live queueing during replay), same as a must_scan_sub_dirs event.
            let mut escalation: Option<std::path::PathBuf> = None;
            if let Some(paths) = reconciler::process_fs_event(&event, &space, &conn, &writer, None, &mut escalation)
                && !affected_paths_overflow
            {
                affected_paths.extend(paths);
            }
            if let Some(anchor) = escalation {
                defer_replay_rescan(&mut pending_rescans, anchor.to_string_lossy().to_string());
            }
            last_event_id = event.event_id;
            event_count += 1;

            break; // Exit Phase 1, enter Phase 2
        }

        // Handle MustScanSubDirs: queue for after replay (don't start during replay)
        if event.flags.must_scan_sub_dirs {
            // Keep absolute; the reconcile strips at its resolve.
            defer_replay_rescan(&mut pending_rescans, space.absolute(&event.path));
            last_event_id = event.event_id;
            event_count += 1;
            continue;
        }

        // Accumulate into dedup buffer instead of processing immediately.
        // Same pattern as the live event loop: canonicalize path, merge flags.
        let canonical = space.absolute(&event.path);
        let deduped_event = watcher::FsChangeEvent {
            path: canonical.clone(),
            event_id: event.event_id,
            flags: event.flags.clone(),
        };
        replay_pending
            .entry(canonical)
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &deduped_event);
            })
            .or_insert(deduped_event);

        last_event_id = event.event_id;
        event_count += 1;

        // Safety net: if replay event count exceeds the limit, abort and
        // fall back to a full scan. Handles the FDA-toggle scenario where
        // the app suddenly sees millions of previously hidden paths.
        if event_count >= REPLAY_EVENT_COUNT_LIMIT {
            emit_rescan_notification(
                &app,
                &volume_id,
                RescanReason::ReplayOverflow,
                format!(
                    "Replay processed {}, exceeding the safety limit of \
                     {REPLAY_EVENT_COUNT_LIMIT}. This can happen when Full Disk Access was \
                     toggled.",
                    pluralize(event_count, "event")
                ),
            );
            if let Some(tx) = fallback_tx.take() {
                let _ = tx.send(RescanReason::ReplayOverflow);
            }
            return Ok(());
        }

        // Flush dedup buffer and batch UpdateLastEventId
        if event_count.is_multiple_of(REPLAY_DEDUP_BATCH_SIZE) {
            deduped_total += flush_replay_batch(
                &mut replay_pending,
                &space,
                &conn,
                &writer,
                &mut affected_paths,
                &mut affected_paths_overflow,
                &mut pending_rescans,
            ) as u64;
            if last_event_id > since_event_id
                && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(last_event_id))
            {
                log::warn!("Replay: UpdateLastEventId send failed: {e}");
            }

            // Ingestion-pressure guard (Fix 2). The channel is unbounded, so a slow
            // replay drain grows the queue instead of backpressuring FSEvents into
            // dropping events (which used to force a full scan). Past the RAM-guard
            // hard cap we DELIBERATELY fall back; a merely-high watermark just logs.
            match classify_ingestion_pressure(event_rx.len()) {
                IngestionPressure::Overflowing => {
                    let queued = event_rx.len();
                    log::warn!("Replay: ingestion queue at {queued} (hard cap); falling back to a full scan");
                    emit_rescan_notification(
                        &app,
                        &volume_id,
                        RescanReason::IngestionBacklog,
                        format!(
                            "The replay event queue reached {queued} pending events, past the ingestion hard cap. \
                             Running a fresh scan to catch up."
                        ),
                    );
                    if let Some(tx) = fallback_tx.take() {
                        let _ = tx.send(RescanReason::IngestionBacklog);
                    }
                    return Ok(());
                }
                IngestionPressure::FallingBehind => {
                    report_backlog(&mut backlog, "Replay", event_rx.len());
                }
                IngestionPressure::Healthy => backlog.reset(),
            }
        }

        // Emit progress every 500ms during replay
        if last_progress.elapsed() >= Duration::from_millis(500) {
            let _ = IndexReplayProgressEvent {
                volume_id: volume_id.clone(),
                events_processed: event_count,
                estimated_total,
            }
            .emit(&app);
            last_progress = Instant::now();
        }

        // Log milestone counts
        if event_count.is_multiple_of(10_000) {
            log::debug!("Replay: {} processed so far", pluralize(event_count, "event"));
        }
    }

    // ── Phase 2: After HistoryDone ───────────────────────────────────

    if deduped_total < event_count {
        // allowed-pluralize-noun: dedup only kicks in when event_count >= 2.
        log::info!(
            "Replay: deduplicated {event_count} raw events to {deduped_total} unique ({:.0}% reduction)",
            (1.0 - deduped_total as f64 / event_count.max(1) as f64) * 100.0,
        );
    }

    // Send final UpdateLastEventId
    if last_event_id > since_event_id
        && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(last_event_id))
    {
        log::warn!("Replay: final UpdateLastEventId send failed: {e}");
    }

    // Commit the replay transaction (all writes become visible in one fsync)
    if let Err(e) = writer.send(WriteMessage::CommitTransaction) {
        log::warn!("Replay: CommitTransaction send failed: {e}");
    }

    // Flush: wait for the writer to commit all replay data
    match writer.flush().await {
        Ok(()) => {
            log::info!(
                "Replay: complete ({}, {}, {:.1}s)",
                pluralize(event_count, "event"),
                pluralize(affected_paths.len() as u64, "affected dir"),
                replay_start.elapsed().as_secs_f64(),
            );
        }
        Err(e) => {
            log::warn!("Replay: flush failed (writer may have shut down): {e}");
        }
    }

    // Emit final progress
    let _ = IndexReplayProgressEvent {
        volume_id: volume_id.clone(),
        events_processed: event_count,
        estimated_total,
    }
    .emit(&app);

    // Emit replay complete
    let _ = IndexReplayCompleteEvent {
        volume_id: volume_id.clone(),
        duration_ms: replay_start.elapsed().as_millis() as u64,
    }
    .emit(&app);

    // Emit a single batched index-dir-updated with all collected paths.
    // If affected_paths overflowed, emit a full refresh notification with
    // just "/" so the frontend refreshes everything.
    if affected_paths_overflow {
        reconciler::emit_dir_updated(&app, vec!["/".to_string()]);
    } else if !affected_paths.is_empty() {
        reconciler::emit_dir_updated(&app, affected_paths.iter().cloned().collect());
    }

    // Backfill dir_stats for any directories created by the replay
    // that didn't go through a full aggregation pass.
    let _ = writer.send(WriteMessage::BackfillMissingDirStats);

    // One-shot ledger heal: replay runs no full aggregate of its own, so on a
    // never-healed DB it enqueues one here — AFTER the entries table is fully
    // replayed (past `CommitTransaction` + backfill above), so the `Sql`
    // recompute sees final state. The writer's armed latch (set at launch)
    // consumes it and persists the healed marker. See `indexing/DETAILS.md`
    // § "The dir_stats ledger".
    if heal_after_replay {
        let _ = writer.send(WriteMessage::ComputeAllAggregates {
            source: crate::indexing::writer::AggSource::Sql,
        });
    }

    // ── Switch to live mode immediately (before verification) ────────

    DEBUG_STATS.close_phase_with_stats(vec![
        ("raw_events", event_count.to_string()),
        ("unique_events", deduped_total.to_string()),
        (
            "dedup_pct",
            format!(
                "{:.0}",
                (1.0 - deduped_total as f64 / event_count.max(1) as f64) * 100.0
            ),
        ),
        ("affected_dirs", affected_paths.len().to_string()),
    ]);
    set_phase_for(&app, &volume_id, ActivityPhase::Live, "post-replay");

    // Replay done. Allow verifier to run and report scanning=false to frontend.
    scanning.store(false, Ordering::Relaxed);

    log::info!("Replay: switching to live mode");
    let mut reconciler = EventReconciler::new_for(volume_id.clone(), space.clone());
    reconciler.switch_to_live();

    // Spawn background verification: runs concurrently with live events.
    // The writer serializes all writes, so this is safe.
    // Skip verification if affected_paths overflowed (no paths to verify).
    if !affected_paths_overflow {
        let verify_writer = writer.clone();
        let verify_app = app.clone();
        tauri::async_runtime::spawn(async move {
            run_background_verification(affected_paths, verify_writer, verify_app).await;
        });
    }

    // Queue any MustScanSubDirs rescans that were deferred during replay. Route
    // each by depth (see `reconciler/rescan_route.rs`): a shallow/root-scale anchor
    // — the case our replay-unification can collapse to one invisible reconcile-of-`/`
    // with a stuck hourglass — takes the VISIBLE scanner path instead; a deep/narrow
    // anchor stays on the live drain, which dedups, ancestor-collapses, and
    // per-subtree-throttles them at background QoS, so a churn-heavy gap catches up
    // subtree by subtree. The genuine full-scan fallbacks (journal purge, >10M
    // events, watcher overflow) remain.
    for path in pending_rescans {
        reconciler.route_must_scan_sub_dirs(std::path::PathBuf::from(path), &writer);
    }

    let mut live_count = 0u64;
    // Per-subtree churn observability, same as `run_live_event_loop`: this loop
    // drives live batches too (the cold-start replay path never reaches
    // `run_live_event_loop`), so it needs its own observer or the whole
    // journal-replay route measures nothing.
    let mut churn = ChurnObserver::from_env(&volume_id, Instant::now());
    let mut live_pending_paths = HashSet::<String>::new();
    let mut live_pending_events = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(LIVE_FLUSH_INTERVAL_MS));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Trailing-flush sweep for the per-file throttle (no new thread). Both live
    // loops run it so a volume that took the post-replay path is throttled too.
    let mut throttle_sweep_interval = tokio::time::interval(Duration::from_millis(THROTTLE_SWEEP_INTERVAL_MS));
    throttle_sweep_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        let canonical = space.absolute(&event.path);
                        let deduped_event = watcher::FsChangeEvent {
                            path: canonical.clone(),
                            event_id: event.event_id,
                            flags: event.flags,
                        };
                        live_pending_events
                            .entry(canonical)
                            .and_modify(|existing| {
                                *existing = merge_fs_events(existing, &deduped_event);
                            })
                            .or_insert(deduped_event);
                        live_count += 1;
                        if live_count.is_multiple_of(10_000) {
                            log::debug!(
                                "Live event processing (post-replay): {} ({} pending deduplicated)",
                                pluralize(live_count, "event"),
                                live_pending_events.len()
                            );
                        }
                    }
                    None => {
                        process_live_batch(
                            &mut live_pending_events, &mut reconciler, &space, &conn,
                            &writer, &mut live_pending_paths, churn.with_raw_total(live_count),
                        );
                        if !live_pending_paths.is_empty() {
                            let changed = mark_pending_and_drain(&volume_id, &mut live_pending_paths);
                            lifecycle_bus::publish_dirs_changed(&volume_id, &changed);
                            reconciler::emit_dir_updated(&app, changed);
                        }
                        break;
                    }
                }
            }
            _ = flush_interval.tick() => {
                // Check if the FSEvents channel overflowed
                if let Some(ref flag) = watcher_overflow
                    && flag.load(Ordering::Relaxed) {
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::WatcherChannelOverflow,
                            format!(
                                "The filesystem watcher's event channel overflowed after {} + {}. \
                                 Some file changes were lost.",
                                pluralize(event_count, "replay event"),
                                pluralize(live_count, "live event"),
                            ),
                        );
                        if let Some(tx) = fallback_tx.take() {
                            let _ = tx.send(RescanReason::WatcherChannelOverflow);
                        }
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        return Ok(());
                    }

                // Ingestion-pressure guard (Fix 2), same as Phase 1 and the live loop.
                match classify_ingestion_pressure(event_rx.len()) {
                    IngestionPressure::Overflowing => {
                        let queued = event_rx.len();
                        log::warn!("Replay (live): ingestion queue at {queued} (hard cap); falling back to a full scan");
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::IngestionBacklog,
                            format!(
                                "The live event queue reached {queued} pending events, past the ingestion hard cap. \
                                 Running a fresh scan to catch up."
                            ),
                        );
                        if let Some(tx) = fallback_tx.take() {
                            let _ = tx.send(RescanReason::IngestionBacklog);
                        }
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        return Ok(());
                    }
                    IngestionPressure::FallingBehind => {
                        report_backlog(&mut backlog, "Replay (live)", event_rx.len());
                    }
                    IngestionPressure::Healthy => backlog.reset(),
                }

                process_live_batch(
                    &mut live_pending_events, &mut reconciler, &space, &conn,
                    &writer, &mut live_pending_paths, churn.with_raw_total(live_count),
                );
                if !live_pending_paths.is_empty() {
                    let changed = mark_pending_and_drain(&volume_id, &mut live_pending_paths);
                    lifecycle_bus::publish_dirs_changed(&volume_id, &changed);
                    reconciler::emit_dir_updated(&app, changed);
                }
            }
            _ = throttle_sweep_interval.tick() => {
                // Apply any throttled files whose 60 s window elapsed; the
                // resulting ancestor paths ride the next flush tick's emit.
                let affected = reconciler.sweep_throttle(&writer, Instant::now());
                live_pending_paths.extend(affected);
                // Trailing edge of the per-subtree rescan throttle: re-kick the
                // drain so a churny subtree whose window has now elapsed re-walks.
                reconciler.sweep_rescan_throttle(&writer);
            }
        }
    }

    log::info!(
        "Replay event loop: stopped ({} + {})",
        pluralize(event_count, "replay event"),
        pluralize(live_count, "live event"),
    );
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Defer a rescan anchor into the replay-phase pending set. Shared by
/// must_scan_sub_dirs events and missing-parent escalations: both are the same
/// "go look here" signal, deferred identically during replay (no live queueing
/// then; the post-replay live loop hands them to `queue_must_scan_sub_dirs`,
/// which dedups, ancestor-collapses, and per-subtree-throttles them). The
/// `HashSet` dedups a churny dir re-flagged many times across the gap so it
/// consumes one entry, not thousands.
fn defer_replay_rescan(pending_rescans: &mut HashSet<String>, path: String) {
    pending_rescans.insert(path);
}

/// Drain the replay dedup buffer, process each event through the
/// reconciler, and collect affected paths. Returns the number of
/// deduplicated events processed. Missing-parent escalations DEFER into the
/// pending-rescan list (no live queueing during replay).
fn flush_replay_batch(
    pending: &mut HashMap<String, watcher::FsChangeEvent>,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    affected_paths: &mut HashSet<String>,
    affected_paths_overflow: &mut bool,
    pending_rescans: &mut HashSet<String>,
) -> usize {
    let count = pending.len();
    for (_path, event) in pending.drain() {
        let mut escalation: Option<std::path::PathBuf> = None;
        if let Some(paths) = reconciler::process_fs_event(&event, space, conn, writer, None, &mut escalation)
            && !*affected_paths_overflow
        {
            affected_paths.extend(paths);
            if affected_paths.len() >= MAX_AFFECTED_PATHS {
                log::warn!(
                    "Replay: affected paths cap reached ({MAX_AFFECTED_PATHS}). \
                     Will emit a full refresh notification instead of individual paths."
                );
                *affected_paths_overflow = true;
                affected_paths.clear();
            }
        }
        if let Some(anchor) = escalation {
            defer_replay_rescan(pending_rescans, anchor.to_string_lossy().to_string());
        }
    }
    count
}
