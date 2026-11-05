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

use super::super::activity_monitor::BatchObservers;
use super::super::branches::{self, Admission, WatchScope};
use super::super::watcher;
use super::live::{drain_promoted, mark_pending_and_drain, process_live_batch, queue_admitted};
use super::replay_chunks::{JournalPosition, ReplayCommits, load_deferred_rescans};
use super::replay_cost::{ReplayBudget, ReplayWindow};
use super::verification::run_background_verification;
use super::{
    BacklogTracker, IngestionPressure, JOURNAL_GAP_THRESHOLD, LIVE_FLUSH_INTERVAL_MS, ReplayConfig,
    THROTTLE_SWEEP_INTERVAL_MS, classify_ingestion_pressure, history_was_purged, open_read_conn_with_retry,
    report_backlog,
};
use crate::ActivityPhase;
use crate::indexing::DEBUG_STATS;
use crate::indexing::IndexPathSpace;
use crate::indexing::deletes;
use crate::indexing::events::emit_dir_updated;
use crate::indexing::events::{EventSink, IndexEvent, RescanReason, emit_rescan_notification, set_phase_for};
use crate::indexing::hold::VolumeWork;
use crate::indexing::lifecycle::lifecycle_bus;
use crate::indexing::paths::path_prefix;
use crate::indexing::reconcile::reconciler::{self, EventReconciler};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::pluralize::pluralize;

/// Cap on the replay's ORIGIN dirs. When exceeded, individual path tracking
/// stops and a single "full refresh" is emitted instead.
const MAX_ORIGIN_DIRS: usize = 50_000;

/// How often (in raw events) phase 1 samples the ingestion queue for the RAM
/// guard. Independent of the dedup window, which drains by distinct paths.
const REPLAY_PRESSURE_CHECK_EVERY: u64 = 1_000;

// ── Replay event loop (cold start sinceWhen) ─────────────────────────

/// Process FSEvents replayed from the journal on cold start.
///
/// Two-phase approach to avoid a race condition where `DirsUpdated`
/// notifications fire before the writer commits replay data to SQLite:
///
/// **Phase 1 (replay):** Merge events by path into a [`ReplayWindow`] and drain
/// it through `process_fs_event_into` each time it fills, collecting affected
/// parent paths in a `HashSet`. No per-event UI notification. The writes commit
/// in chunks ([`ReplayCommits`]): every few thousand paths, and at each drain
/// together with `UpdateLastEventId` and the rescans deferred so far, so the WAL
/// stays bounded and a crash resumes from the last drain (`replay_chunks.rs`). A
/// [`ReplayBudget`] caps the events delivered and paths processed; past it, a
/// full reconcile is cheaper and the loop falls back.
///
/// **Phase 2 (after HistoryDone):** Commit the final `UpdateLastEventId`, flush
/// the writer (wait for all prior messages to commit), then emit a single
/// batched `DirsUpdated`. After that, continue processing live events
/// with per-event emit (live events arrive slowly enough for the writer to
/// keep up).
///
/// If a journal gap is detected (first event ID >> stored last_event_id),
/// sends a signal via `fallback_tx` to trigger a full scan.
pub(in crate::indexing) async fn run_replay_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<watcher::FsChangeEvent>,
    writer: IndexWriter,
    events: Arc<dyn EventSink>,
    config: ReplayConfig,
    fallback_tx: tokio::sync::oneshot::Sender<RescanReason>,
    watcher_overflow: Option<Arc<AtomicBool>>,
    ground_in_flux: Arc<AtomicBool>,
) -> Result<(), String> {
    let ReplayConfig {
        volume_id,
        space,
        since_event_id,
        estimated_total,
        heal_after_replay,
        work,
        ground,
        mut budget,
        chunks,
    } = config;

    log::info!(
        "Replay: started (since_event_id={since_event_id}, estimated_total={estimated_total:?}, budget={} walked entries)",
        budget.limit()
    );
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
    // Capped at MAX_ORIGIN_DIRS; beyond that we emit a full refresh.
    let mut origin_dirs = HashSet::<String>::new();
    let mut origins_overflow = false;

    // MustScanSubDirs paths to queue after replay. A `HashSet` dedups the anchor
    // churn a long gap produces (the same dir re-flagged thousands of times); the
    // live drain they hand off to ancestor-collapses and per-subtree-throttles
    // them, so there's no cap here and no full-rescan escalation on volume.
    // It starts with what an earlier replay deferred and never handed off: a crash
    // after a chunk moved the stored position past their events leaves them only
    // on disk (`replay_chunks.rs`).
    let mut pending_rescans = load_deferred_rescans(&conn, &space);
    if !pending_rescans.is_empty() {
        log::info!(
            "Replay: picking up {} an earlier replay deferred",
            pluralize(pending_rescans.len() as u64, "rescan")
        );
    }

    // Progress reporting interval
    let mut last_progress = Instant::now();
    let replay_start = Instant::now();

    // Batch the replay's writes into explicit transactions: autocommitted, 50K+
    // writes take minutes, batched they take seconds. Batched in CHUNKS, though,
    // never one transaction for the whole replay: that parked every WAL checkpoint
    // until the end and grew the WAL until a nearly full disk ran out
    // (cmdr-reports#100). The guard inside commits on every exit, the early
    // fallbacks below included: a replay that returned past a hand-sent commit once
    // left the writer inside its transaction for the rest of the process.
    let mut commits = ReplayCommits::open(&writer, chunks, pending_rescans.len());

    // ── Phase 1: Replay (before HistoryDone) ─────────────────────────

    // Deduplicate events by normalized path before processing, same as the live
    // event loop, in a window bounded by distinct paths (`replay_cost.rs`).
    let mut window = ReplayWindow::new(chunks.window_paths);
    let mut deduped_total = 0u64;
    let mut history_done = false;

    while let Some(event) = event_rx.recv().await {
        // Check for journal gap on the first event
        if !first_event_checked {
            first_event_checked = true;
            if history_was_purged(since_event_id, event.event_id) {
                emit_rescan_notification(
                    events.as_ref(),
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
                commits.abandon(&writer);
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

            // Flush remaining deduplicated events before leaving Phase 1, if the
            // budget still covers them.
            if budget.would_exceed(window.len() as u64) {
                fall_back_over_budget(events.as_ref(), &volume_id, &budget, event_count, &mut fallback_tx);
                commits.abandon(&writer);
                return Ok(());
            }
            budget.spend(window.len() as u64);
            deduped_total += flush_replay_batch(
                window.pending_mut(),
                &space,
                &conn,
                &writer,
                &work,
                &mut origin_dirs,
                &mut origins_overflow,
                &mut pending_rescans,
                Some(&mut commits),
            ) as u64;

            // Process the HistoryDone event itself (it may carry other flags).
            // A missing-parent escalation here DEFERS into the pending list (no
            // live queueing during replay), same as a must_scan_sub_dirs event.
            let mut escalation: Option<std::path::PathBuf> = None;
            let mut event_deletes = Vec::new();
            if let Some(paths) = reconciler::process_fs_event_into(
                &event,
                &space,
                &conn,
                &writer,
                None,
                &mut escalation,
                &mut event_deletes,
            ) && !origins_overflow
            {
                origin_dirs.extend(paths);
            }
            send_replay_deletes(event_deletes, &work, &writer);
            if let Some(anchor) = escalation {
                defer_replay_rescan(&mut pending_rescans, anchor.to_string_lossy().to_string());
            }
            last_event_id = event.event_id;
            event_count += 1;

            history_done = true;
            break; // Exit Phase 1, enter Phase 2
        }

        // Every delivered event costs fseventsd's time whether or not it dedups
        // away, and delivery is most of a replay's cost (`replay_cost.rs`).
        budget.note_event();
        if budget.exhausted() {
            fall_back_over_budget(events.as_ref(), &volume_id, &budget, event_count, &mut fallback_tx);
            commits.abandon(&writer);
            return Ok(());
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
        let window_full = window.add(watcher::FsChangeEvent {
            path: space.absolute(&event.path),
            event_id: event.event_id,
            flags: event.flags,
        });

        last_event_id = event.event_id;
        event_count += 1;

        // Drain the window once it holds its distinct paths, then record how far
        // the journal got. Cost is checked BEFORE the drain: a replay that would
        // process more paths than the index holds costs more than the reconcile
        // it replaces (this is also what catches the FDA-toggle flood of
        // previously hidden paths).
        if window_full {
            if budget.would_exceed(window.len() as u64) {
                fall_back_over_budget(events.as_ref(), &volume_id, &budget, event_count, &mut fallback_tx);
                commits.abandon(&writer);
                return Ok(());
            }
            budget.spend(window.len() as u64);
            deduped_total += flush_replay_batch(
                window.pending_mut(),
                &space,
                &conn,
                &writer,
                &work,
                &mut origin_dirs,
                &mut origins_overflow,
                &mut pending_rescans,
                Some(&mut commits),
            ) as u64;
            // The window drained whole, so every event up to `last_event_id` is
            // processed: the position and the rescans deferred so far commit together.
            commits.close_window(
                &writer,
                &space,
                JournalPosition {
                    last_event_id,
                    since_event_id,
                },
                &pending_rescans,
            );
            // A writer that stopped (out of disk space, or a fatal storage error)
            // drops everything sent to it, and its supervisor is already taking the
            // volume down. Reading on would only spend fseventsd's time.
            if writer.failure_signal().is_tripped() {
                log::info!("Replay: the writer stopped, so the replay stops at its last committed chunk");
                return Ok(());
            }
        }

        if event_count.is_multiple_of(REPLAY_PRESSURE_CHECK_EVERY) {
            // Ingestion-pressure guard (Fix 2). The channel is unbounded, so a slow
            // replay drain grows the queue instead of backpressuring FSEvents into
            // dropping events (which used to force a full scan). Past the RAM-guard
            // hard cap we DELIBERATELY fall back; a merely-high watermark just logs.
            match classify_ingestion_pressure(event_rx.len()) {
                IngestionPressure::Overflowing => {
                    let queued = event_rx.len();
                    log::warn!("Replay: ingestion queue at {queued} (hard cap); falling back to a full scan");
                    emit_rescan_notification(
                        events.as_ref(),
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
                    commits.abandon(&writer);
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
            events.emit(IndexEvent::ReplayProgress {
                volume_id: volume_id.clone(),
                events_processed: event_count,
                estimated_total,
            });
            last_progress = Instant::now();
        }

        // Log milestone counts
        if event_count.is_multiple_of(10_000) {
            log::debug!("Replay: {} processed so far", pluralize(event_count, "event"));
        }
    }

    // The stream closed before `HistoryDone`: the volume is stopping (a quit, a
    // pause for disk space, a disable). The window still holds events nothing
    // processed, so the stored position stays at the last drain and the deferred
    // rescans stay on disk; the next replay picks up from there. ❌ Don't fall
    // through to phase 2: its final position would skip the undrained window.
    if !history_done {
        log::info!(
            "Replay: stopped before HistoryDone after {}; the next replay resumes from the last committed chunk",
            pluralize(event_count, "event")
        );
        commits.interrupt();
        return Ok(());
    }

    // ── Phase 2: After HistoryDone ───────────────────────────────────

    if deduped_total < event_count {
        // allowed-pluralize-noun: dedup only kicks in when event_count >= 2.
        log::info!(
            "Replay: deduplicated {event_count} raw events to {deduped_total} unique ({:.0}% reduction)",
            (1.0 - deduped_total as f64 / event_count.max(1) as f64) * 100.0,
        );
    }

    // Commit the last chunk with the final position. The deferred rescans come off
    // the disk here: they go to the live drain below.
    commits.finish(
        &writer,
        JournalPosition {
            last_event_id,
            since_event_id,
        },
    );

    // Flush: wait for the writer to commit all replay data
    match writer.flush().await {
        Ok(()) => {
            log::info!(
                "Replay: complete ({}, {}, {:.1}s)",
                pluralize(event_count, "event"),
                pluralize(origin_dirs.len() as u64, "affected dir"),
                replay_start.elapsed().as_secs_f64(),
            );
        }
        Err(e) => {
            log::warn!("Replay: flush failed (writer may have shut down): {e}");
        }
    }

    // Emit final progress
    events.emit(IndexEvent::ReplayProgress {
        volume_id: volume_id.clone(),
        events_processed: event_count,
        estimated_total,
    });

    // Emit replay complete
    events.emit(IndexEvent::ReplayComplete {
        volume_id: volume_id.clone(),
        duration_ms: replay_start.elapsed().as_millis() as u64,
    });

    // Emit a single batched DirsUpdated with all collected paths.
    // If origin_dirs overflowed, emit a full refresh notification with
    // just "/" so the frontend refreshes everything.
    if origins_overflow {
        emit_dir_updated(events.as_ref(), vec!["/".to_string()]);
    } else if !origin_dirs.is_empty() {
        // The FE refresh needs the recursive-size fact, so expand the origins to
        // their ancestor closure here (the origins alone would leave every ancestor
        // showing a stale size).
        let origins: Vec<String> = origin_dirs.iter().cloned().collect();
        emit_dir_updated(events.as_ref(), path_prefix::with_ancestor_closure(&origins));
    }

    // Backfill dir_stats for any directories created by the replay
    // that didn't go through a full aggregation pass.
    let _ = writer.send(WriteMessage::BackfillMissingDirStats);

    // One-shot ledger heal: replay runs no full aggregate of its own, so on a
    // never-healed DB it enqueues one here — AFTER the entries table is fully
    // replayed (past the last chunk's commit + backfill above), so the `Sql`
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
        ("affected_dirs", origin_dirs.len().to_string()),
    ]);
    set_phase_for(events.as_ref(), &volume_id, ActivityPhase::Live, "post-replay");

    // Replay done. Release the ground, which lets the verifier run and reports
    // `scanning: false` to the frontend.
    ground_in_flux.store(false, Ordering::Relaxed);
    // And hand the volume's ground back, so a rescan or a search walk can have it.
    // From here this loop is a LIVE loop: it writes one event at a time against
    // committed rows, which is what `branches` arbitrates, not the claim table.
    // ⚠️ Everything before this point releases it too, by owning it: an early
    // return (a journal gap, an overflow), an abort, or a panic all drop it.
    drop(ground);
    // A "Rescan now" that landed mid-replay was queued behind it, and this is the
    // moment it can have the volume: the journal is committed, and from here the
    // loop writes one event at a time like any live one.
    crate::indexing::lifecycle::rescan_request::run_if_owed(&volume_id);

    log::info!("Replay: switching to live mode");
    // The volume that replays a journal is one that indexes whole, so this loop
    // answers for every path — with the one exception every live loop shares: a
    // search can walk a hole in it, and those events wait for that walk rather
    // than racing it (`branches`).
    let scope = WatchScope::WholeVolume(branches::live_for(&volume_id));
    // The verification below takes its work first, so the loop's own can move into
    // the reconciler, which carries it for the rest of the loop.
    let verification = work.child(crate::indexing::hold::HoldKind::Verifier);
    let mut reconciler = EventReconciler::new_for(volume_id.clone(), space.clone(), work);
    reconciler.within(scope.clone());
    reconciler.switch_to_live();

    // Spawn background verification: runs concurrently with live events.
    // The writer serializes all writes, so this is safe.
    // Skip verification if the origin set overflowed (no paths to verify).
    if !origins_overflow {
        let verify_writer = writer.clone();
        let verify_events = Arc::clone(&events);
        crate::indexing::host::runtime::spawn(async move {
            run_background_verification(origin_dirs, verify_writer, verify_events, verification).await;
        });
    }

    // Queue any MustScanSubDirs rescans that were deferred during replay. Route
    // each by depth (see `reconcile/reconciler/rescan/route.rs`): a shallow/root-scale anchor
    // — the case our replay-unification can collapse to one invisible reconcile-of-`/`
    // with a stuck hourglass — takes the VISIBLE scanner path instead; a deep/narrow
    // anchor stays on the live drain, which dedups, ancestor-collapses, and
    // per-subtree-throttles them at background QoS, so a churn-heavy gap catches up
    // subtree by subtree. The genuine full-scan fallbacks (journal purge, the
    // cost budget, the RAM guard, watcher overflow) remain.
    for path in pending_rescans {
        reconciler.route_must_scan_sub_dirs(std::path::PathBuf::from(path), &writer);
    }

    let mut live_count = 0u64;
    // Churn observability and the per-folder activity tap, same as
    // `run_live_event_loop`: this loop drives live batches too (the cold-start
    // replay path never reaches `run_live_event_loop`), so it needs its own
    // observers or the whole journal-replay route reports nothing.
    let mut observers = BatchObservers::from_env(&volume_id, Arc::clone(&events), Instant::now());
    let mut live_pending_origins = HashSet::<String>::new();
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
                        if let Admission::Process(admitted) = scope.admit(deduped_event) {
                            for event in admitted {
                                queue_admitted(event, &mut live_pending_events);
                            }
                        }
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
                        drain_promoted(&scope, &mut live_pending_events, &mut reconciler, &writer);
                        process_live_batch(
                            &mut live_pending_events, &mut reconciler, &space, &conn,
                            &writer, &mut live_pending_origins, observers.with_raw_total(live_count),
                        );
                        if !live_pending_origins.is_empty() {
                            let changed = mark_pending_and_drain(&volume_id, &mut live_pending_origins);
                            lifecycle_bus::publish_dirs_changed(&volume_id, &changed.origins);
                            emit_dir_updated(events.as_ref(), changed.with_ancestors);
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
                            events.as_ref(),
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
                            events.as_ref(),
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

                drain_promoted(&scope, &mut live_pending_events, &mut reconciler, &writer);
                process_live_batch(
                    &mut live_pending_events, &mut reconciler, &space, &conn,
                    &writer, &mut live_pending_origins, observers.with_raw_total(live_count),
                );
                if !live_pending_origins.is_empty() {
                    let changed = mark_pending_and_drain(&volume_id, &mut live_pending_origins);
                    lifecycle_bus::publish_dirs_changed(&volume_id, &changed.origins);
                    emit_dir_updated(events.as_ref(), changed.with_ancestors);
                }
            }
            _ = throttle_sweep_interval.tick() => {
                // Apply any throttled files whose 60 s window elapsed; the origin
                // dirs they changed ride the next flush tick's emit.
                live_pending_origins.extend(reconciler.sweep_throttle(&writer, Instant::now()));
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

/// Send a replay batch's gathered deletes, but only if the drive is still listed.
///
/// ⚠️ **The presence read comes AFTER the batch's events were stat'd.** A drive that
/// went away makes every stat fail, and those failures are indistinguishable from
/// real removals — a cold-start replay of a journal recorded before the drive left is
/// exactly the shape that would reap a whole index. One read per batch, ❌ never per
/// event. Nothing is lost when it drops a batch: no row is deleted, and the delete
/// generation is what marks the index for a rebuild instead.
fn send_replay_deletes(deletes_to_send: Vec<WriteMessage>, work: &VolumeWork, writer: &IndexWriter) {
    if deletes_to_send.is_empty() {
        return;
    }
    if !work.drive_is_listed() {
        log::info!(
            "Replay: '{}' stopped being listed, so {} went unsent rather than deleting rows against a drive that left",
            work.volume_id(),
            pluralize(deletes_to_send.len() as u64, "gathered delete"),
        );
        // The batches already sent are the ones nothing can take back.
        deletes::note_the_drive_left(work.volume_id(), writer);
        return;
    }
    // A `Some(true)` here is also the presence half of the delete generation's reset.
    deletes::drive_seen(work.volume_id());
    for message in deletes_to_send {
        let _ = writer.send(message);
    }
    deletes::batch_sent(work.volume_id());
}

/// Give up on a replay whose next event or window would go past its cost budget: a full
/// reconcile brings the index current for less. Nothing is lost by stopping
/// here, because the walk that follows reads the disk whole.
fn fall_back_over_budget(
    events: &dyn EventSink,
    volume_id: &str,
    budget: &ReplayBudget,
    event_count: u64,
    fallback_tx: &mut Option<tokio::sync::oneshot::Sender<RescanReason>>,
) {
    emit_rescan_notification(
        events,
        volume_id,
        RescanReason::ReplayOverflow,
        format!(
            "Replay would cost more than walking the index's {} after {} (spent {} walked entries' worth); \
             a full reconcile is cheaper.",
            pluralize(budget.limit(), "entry"),
            pluralize(event_count, "event"),
            budget.spent(),
        ),
    );
    if let Some(tx) = fallback_tx.take() {
        let _ = tx.send(RescanReason::ReplayOverflow);
    }
}

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
/// reconciler, and collect the ORIGIN dirs it changed. Returns the number of
/// deduplicated events processed. Missing-parent escalations DEFER into the
/// pending-rescan list (no live queueing during replay). `commits` counts each
/// processed path toward the next chunk commit (`None` for a caller holding its own
/// transaction, like the cost bench).
#[allow(
    clippy::too_many_arguments,
    reason = "the replay drain's state plus the work its delete gate reads and the chunk counter; a struct would add indirection without clarity"
)]
pub(super) fn flush_replay_batch(
    pending: &mut HashMap<String, watcher::FsChangeEvent>,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    work: &VolumeWork,
    origin_dirs: &mut HashSet<String>,
    origins_overflow: &mut bool,
    pending_rescans: &mut HashSet<String>,
    mut commits: Option<&mut ReplayCommits>,
) -> usize {
    let count = pending.len();
    // Gathered across the whole drain, so one presence read covers it.
    let mut batch_deletes = Vec::new();
    for (_path, event) in pending.drain() {
        let mut escalation: Option<std::path::PathBuf> = None;
        if let Some(paths) =
            reconciler::process_fs_event_into(&event, space, conn, writer, None, &mut escalation, &mut batch_deletes)
            && !*origins_overflow
        {
            origin_dirs.extend(paths);
            if origin_dirs.len() >= MAX_ORIGIN_DIRS {
                log::warn!(
                    "Replay: changed-dir cap reached ({MAX_ORIGIN_DIRS}). \
                     Will emit a full refresh notification instead of individual paths."
                );
                *origins_overflow = true;
                origin_dirs.clear();
            }
        }
        if let Some(anchor) = escalation {
            defer_replay_rescan(pending_rescans, anchor.to_string_lossy().to_string());
        }
        if let Some(commits) = commits.as_deref_mut() {
            commits.note_processed();
        }
    }
    send_replay_deletes(batch_deletes, work, writer);
    count
}
