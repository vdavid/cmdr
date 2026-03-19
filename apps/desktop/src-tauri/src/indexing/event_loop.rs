use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use super::DEBUG_STATS;
use super::enrichment::get_read_pool;
use super::events::{IndexReplayCompleteEvent, IndexReplayProgressEvent, RescanReason, emit_rescan_notification};
use super::firmlinks;
use super::reconciler::{self, EventReconciler};
use super::scanner;
use super::store::{self, IndexStore};
use super::watcher;
use super::writer::{IndexWriter, WriteMessage};

// ── Live event loop ──────────────────────────────────────────────────

/// Flush interval for live event batching. Events are deduplicated by
/// normalized path during this window before being processed. Longer
/// windows reduce allocations during event storms (for example, multiple
/// agents writing simultaneously) at the cost of slightly delayed UI
/// updates.
pub(super) const LIVE_FLUSH_INTERVAL_MS: u64 = 1000;

/// Threshold for detecting a journal gap. If the first event ID received is
/// more than this many IDs ahead of the stored `since_event_id`, we consider
/// the journal unavailable and fall back to a full scan.
pub(super) const JOURNAL_GAP_THRESHOLD: u64 = 10_000_000;

/// Capacity of the watcher→event loop channel. Provides backpressure to
/// FSEvents/inotify when the event loop can't keep up, preventing unbounded
/// memory growth. Each event is ~300 bytes, so 20K ≈ 6 MB worst case.
pub(super) const WATCHER_CHANNEL_CAPACITY: usize = 20_000;

/// Cap on `affected_paths` during replay. When exceeded, individual path
/// tracking stops and a single "full refresh" is emitted instead.
const MAX_AFFECTED_PATHS: usize = 50_000;

/// Cap on `pending_rescans` during replay. When exceeded, a full rescan
/// is triggered instead of queuing individual subtree rescans.
const MAX_PENDING_RESCANS: usize = 1_000;

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

/// Configuration for a replay event loop.
pub(super) struct ReplayConfig {
    pub(super) volume_id: String,
    pub(super) since_event_id: u64,
    pub(super) estimated_total: Option<u64>,
}

/// Merge two `FsChangeEvent`s for the same normalized path, keeping the
/// most significant flags. Priority: `must_scan_sub_dirs` always wins,
/// then `item_removed`, then `item_created`, then `item_modified`.
/// The higher `event_id` is always kept.
fn merge_fs_events(existing: &watcher::FsChangeEvent, incoming: &watcher::FsChangeEvent) -> watcher::FsChangeEvent {
    use watcher::FsEventFlags;

    let event_id = existing.event_id.max(incoming.event_id);

    // must_scan_sub_dirs always wins -- it triggers a subtree rescan
    if incoming.flags.must_scan_sub_dirs || existing.flags.must_scan_sub_dirs {
        return watcher::FsChangeEvent {
            path: existing.path.clone(),
            event_id,
            flags: FsEventFlags {
                must_scan_sub_dirs: true,
                ..existing.flags.clone()
            },
        };
    }

    // removed wins over everything else
    if incoming.flags.item_removed || existing.flags.item_removed {
        return watcher::FsChangeEvent {
            path: existing.path.clone(),
            event_id,
            flags: FsEventFlags {
                item_removed: true,
                item_is_file: incoming.flags.item_is_file || existing.flags.item_is_file,
                item_is_dir: incoming.flags.item_is_dir || existing.flags.item_is_dir,
                ..Default::default()
            },
        };
    }

    // created > modified
    if incoming.flags.item_created || existing.flags.item_created {
        return watcher::FsChangeEvent {
            path: existing.path.clone(),
            event_id,
            flags: FsEventFlags {
                item_created: true,
                item_is_file: incoming.flags.item_is_file || existing.flags.item_is_file,
                item_is_dir: incoming.flags.item_is_dir || existing.flags.item_is_dir,
                ..Default::default()
            },
        };
    }

    // Otherwise keep the incoming event (newer) with the higher event_id
    watcher::FsChangeEvent {
        path: existing.path.clone(),
        event_id,
        flags: incoming.flags.clone(),
    }
}

/// Process FSEvents in real time after scan + reconciliation completes.
///
/// Runs as a tokio task, reading events from the watcher channel and
/// deduplicating them by normalized path during each flush interval.
/// Only the deduplicated batch is processed through the reconciler, which
/// cuts allocations dramatically during event storms. Batches
/// `index-dir-updated` notifications with a 1s flush interval.
/// Exits when the channel closes (watcher stopped).
pub(super) async fn run_live_event_loop(
    mut event_rx: tokio::sync::mpsc::Receiver<watcher::FsChangeEvent>,
    mut reconciler: EventReconciler,
    writer: IndexWriter,
    app: AppHandle,
    volume_id: String,
    watcher_overflow: Option<Arc<AtomicBool>>,
) {
    log::info!("Live event processing: started");

    // Open a read connection for path resolution (integer-keyed lookups)
    let db_path = writer.db_path();
    let conn = match IndexStore::open_write_connection(&db_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Live event loop: failed to open read connection: {e}");
            return;
        }
    };

    let mut event_count = 0u64;
    let mut pending_paths = HashSet::<String>::new();
    let mut pending_events = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(LIVE_FLUSH_INTERVAL_MS));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        let normalized = firmlinks::normalize_path(&event.path);
                        let deduped_event = watcher::FsChangeEvent {
                            path: normalized.clone(),
                            event_id: event.event_id,
                            flags: event.flags,
                        };
                        pending_events
                            .entry(normalized)
                            .and_modify(|existing| {
                                *existing = merge_fs_events(existing, &deduped_event);
                            })
                            .or_insert(deduped_event);
                        event_count += 1;
                        DEBUG_STATS.live_event_count.store(event_count, Ordering::Relaxed);
                        if event_count.is_multiple_of(10_000) {
                            log::debug!(
                                "Live event processing: {event_count} events received \
                                 ({} pending deduplicated)",
                                pending_events.len()
                            );
                        }
                    }
                    None => {
                        // Channel closed: process remaining events before exit
                        process_live_batch(
                            &mut pending_events, &mut reconciler, &conn,
                            &writer, &mut pending_paths,
                        );
                        if !pending_paths.is_empty() {
                            reconciler::emit_dir_updated(&app, pending_paths.drain().collect());
                        }
                        break;
                    }
                }
            }
            _ = flush_interval.tick() => {
                // Check if the FSEvents channel overflowed — events were dropped
                // between FSEvents and our forward task. The only safe recovery is
                // a full rescan.
                if let Some(ref flag) = watcher_overflow
                    && flag.load(Ordering::Relaxed) {
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::WatcherChannelOverflow,
                            format!(
                                "The filesystem watcher's event channel overflowed after \
                                 {event_count} live events. Some file changes were lost."
                            ),
                        );
                        // Drain and discard remaining events — they're a partial
                        // picture and processing them before a rescan is pointless.
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        break;
                    }

                process_live_batch(
                    &mut pending_events, &mut reconciler, &conn,
                    &writer, &mut pending_paths,
                );
                if !pending_paths.is_empty() {
                    reconciler::emit_dir_updated(&app, pending_paths.drain().collect());
                }
            }
        }
    }

    log::info!("Live event processing: stopped ({event_count} events)");
}

/// Drain the pending events map, process each through the reconciler, and
/// send a single `UpdateLastEventId` for the batch.
pub(super) fn process_live_batch(
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
    reconciler: &mut EventReconciler,
    conn: &Connection,
    writer: &IndexWriter,
    pending_paths: &mut HashSet<String>,
) {
    if pending_events.is_empty() {
        return;
    }

    let mut max_event_id = 0u64;
    for (_path, event) in pending_events.drain() {
        max_event_id = max_event_id.max(event.event_id);
        reconciler.process_live_event(&event, conn, writer, pending_paths);
    }

    if max_event_id > 0 {
        let _ = writer.send(WriteMessage::UpdateLastEventId(max_event_id));
    }
}

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
pub(super) async fn run_replay_event_loop(
    mut event_rx: tokio::sync::mpsc::Receiver<watcher::FsChangeEvent>,
    writer: IndexWriter,
    app: AppHandle,
    config: ReplayConfig,
    fallback_tx: tokio::sync::oneshot::Sender<()>,
    watcher_overflow: Option<Arc<AtomicBool>>,
    scanning: Arc<AtomicBool>,
) -> Result<(), String> {
    let ReplayConfig {
        volume_id,
        since_event_id,
        estimated_total,
    } = config;

    log::info!("Replay: started (since_event_id={since_event_id}, estimated_total={estimated_total:?})");

    // Open a read connection for path resolution (integer-keyed lookups)
    let db_path = writer.db_path();
    let conn = match IndexStore::open_write_connection(&db_path) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("Failed to open read connection for replay: {e}"));
        }
    };

    let mut event_count = 0u64;
    let mut first_event_checked = false;
    let mut fallback_tx = Some(fallback_tx);
    let mut last_event_id = since_event_id;

    // Collect all affected parent paths during replay (deduplicated).
    // Capped at MAX_AFFECTED_PATHS; beyond that we emit a full refresh.
    let mut affected_paths = HashSet::<String>::new();
    let mut affected_paths_overflow = false;

    // MustScanSubDirs paths to queue after replay.
    // Capped at MAX_PENDING_RESCANS; beyond that a full rescan is triggered.
    let mut pending_rescans = Vec::<String>::new();
    let mut pending_rescans_overflow = false;

    // Progress reporting interval
    let mut last_progress = std::time::Instant::now();
    let replay_start = std::time::Instant::now();

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
                    let _ = tx.send(());
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
            log::info!("Replay: HistoryDone received after {event_count} events");

            // Flush remaining deduplicated events before leaving Phase 1
            deduped_total += flush_replay_batch(
                &mut replay_pending,
                &conn,
                &writer,
                &mut affected_paths,
                &mut affected_paths_overflow,
            ) as u64;

            // Process the HistoryDone event itself (it may carry other flags)
            if let Some(paths) = reconciler::process_fs_event(&event, &conn, &writer)
                && !affected_paths_overflow
            {
                affected_paths.extend(paths);
            }
            last_event_id = event.event_id;
            event_count += 1;

            break; // Exit Phase 1, enter Phase 2
        }

        // Handle MustScanSubDirs: queue for after replay (don't start during replay)
        if event.flags.must_scan_sub_dirs {
            if !pending_rescans_overflow {
                if pending_rescans.len() >= MAX_PENDING_RESCANS {
                    log::warn!(
                        "Replay: pending rescans cap reached ({MAX_PENDING_RESCANS}). \
                         Will trigger a full rescan instead of individual subtree rescans."
                    );
                    pending_rescans_overflow = true;
                    pending_rescans.clear();
                } else {
                    let normalized = firmlinks::normalize_path(&event.path);
                    pending_rescans.push(normalized);
                }
            }
            last_event_id = event.event_id;
            event_count += 1;
            continue;
        }

        // Accumulate into dedup buffer instead of processing immediately.
        // Same pattern as the live event loop: normalize path, merge flags.
        let normalized = firmlinks::normalize_path(&event.path);
        let deduped_event = watcher::FsChangeEvent {
            path: normalized.clone(),
            event_id: event.event_id,
            flags: event.flags.clone(),
        };
        replay_pending
            .entry(normalized)
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
                    "Replay processed {event_count} events, exceeding the safety limit of \
                     {REPLAY_EVENT_COUNT_LIMIT}. This can happen when Full Disk Access was \
                     toggled."
                ),
            );
            if let Some(tx) = fallback_tx.take() {
                let _ = tx.send(());
            }
            return Ok(());
        }

        // Flush dedup buffer and batch UpdateLastEventId
        if event_count.is_multiple_of(REPLAY_DEDUP_BATCH_SIZE) {
            deduped_total += flush_replay_batch(
                &mut replay_pending,
                &conn,
                &writer,
                &mut affected_paths,
                &mut affected_paths_overflow,
            ) as u64;
            if last_event_id > since_event_id
                && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(last_event_id))
            {
                log::warn!("Replay: UpdateLastEventId send failed: {e}");
            }
        }

        // Emit progress every 500ms during replay
        if last_progress.elapsed() >= Duration::from_millis(500) {
            let _ = app.emit(
                "index-replay-progress",
                IndexReplayProgressEvent {
                    volume_id: volume_id.clone(),
                    events_processed: event_count,
                    estimated_total,
                },
            );
            last_progress = std::time::Instant::now();
        }

        // Log milestone counts
        if event_count.is_multiple_of(10_000) {
            log::debug!("Replay: {event_count} events processed so far");
        }
    }

    // ── Phase 2: After HistoryDone ───────────────────────────────────

    if deduped_total < event_count {
        log::info!(
            "Replay: deduplicated {event_count} raw events to {deduped_total} unique \
             ({:.0}% reduction)",
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
                "Replay: complete ({event_count} events, {} affected dirs, {:.1}s)",
                affected_paths.len(),
                replay_start.elapsed().as_secs_f64(),
            );
        }
        Err(e) => {
            log::warn!("Replay: flush failed (writer may have shut down): {e}");
        }
    }

    // Emit final progress
    let _ = app.emit(
        "index-replay-progress",
        IndexReplayProgressEvent {
            volume_id: volume_id.clone(),
            events_processed: event_count,
            estimated_total,
        },
    );

    // Emit replay complete
    let _ = app.emit(
        "index-replay-complete",
        IndexReplayCompleteEvent {
            volume_id: volume_id.clone(),
            duration_ms: replay_start.elapsed().as_millis() as u64,
        },
    );

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
    DEBUG_STATS.set_phase(super::ActivityPhase::Live, "post-replay");

    // Replay done — allow verifier to run and report scanning=false to frontend.
    scanning.store(false, Ordering::Relaxed);

    log::info!("Replay: switching to live mode");
    let mut reconciler = EventReconciler::new();
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

    // Queue any MustScanSubDirs rescans that were deferred during replay.
    // If pending_rescans overflowed, trigger a full rescan via fallback.
    if pending_rescans_overflow {
        emit_rescan_notification(
            &app,
            &volume_id,
            RescanReason::TooManySubdirRescans,
            format!(
                "Replay accumulated more than {MAX_PENDING_RESCANS} directories needing full \
                 rescans. This typically means a major filesystem reorganization happened."
            ),
        );
        if let Some(tx) = fallback_tx.take() {
            let _ = tx.send(());
        }
        return Ok(());
    }
    for path in pending_rescans {
        reconciler.queue_must_scan_sub_dirs(std::path::PathBuf::from(path), &writer);
    }

    let mut live_count = 0u64;
    let mut live_pending_paths = HashSet::<String>::new();
    let mut live_pending_events = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(LIVE_FLUSH_INTERVAL_MS));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        let normalized = firmlinks::normalize_path(&event.path);
                        let deduped_event = watcher::FsChangeEvent {
                            path: normalized.clone(),
                            event_id: event.event_id,
                            flags: event.flags,
                        };
                        live_pending_events
                            .entry(normalized)
                            .and_modify(|existing| {
                                *existing = merge_fs_events(existing, &deduped_event);
                            })
                            .or_insert(deduped_event);
                        live_count += 1;
                        if live_count.is_multiple_of(10_000) {
                            log::debug!(
                                "Live event processing (post-replay): {live_count} events \
                                 ({} pending deduplicated)",
                                live_pending_events.len()
                            );
                        }
                    }
                    None => {
                        process_live_batch(
                            &mut live_pending_events, &mut reconciler, &conn,
                            &writer, &mut live_pending_paths,
                        );
                        if !live_pending_paths.is_empty() {
                            reconciler::emit_dir_updated(&app, live_pending_paths.drain().collect());
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
                                "The filesystem watcher's event channel overflowed after \
                                 {event_count} replay + {live_count} live events. Some file \
                                 changes were lost."
                            ),
                        );
                        if let Some(tx) = fallback_tx.take() {
                            let _ = tx.send(());
                        }
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        return Ok(());
                    }

                process_live_batch(
                    &mut live_pending_events, &mut reconciler, &conn,
                    &writer, &mut live_pending_paths,
                );
                if !live_pending_paths.is_empty() {
                    reconciler::emit_dir_updated(&app, live_pending_paths.drain().collect());
                }
            }
        }
    }

    log::info!("Replay event loop: stopped ({event_count} replay + {live_count} live events)");
    Ok(())
}

// ── Background verification ──────────────────────────────────────────

/// Run post-replay verification in the background.
///
/// Called after live mode starts so the app is responsive immediately.
/// Corrections found by verification go through the writer channel,
/// which serializes them with live writes.
pub(super) async fn run_background_verification(affected_paths: HashSet<String>, writer: IndexWriter, app: AppHandle) {
    DEBUG_STATS.verifying.store(true, Ordering::Relaxed);
    let verify_start = std::time::Instant::now();
    log::debug!(
        "Background verification started ({} affected dirs)",
        affected_paths.len(),
    );

    // Verify affected directories: FSEvents journal replay coalesces events,
    // so child deletions may only show as "parent dir modified," and new
    // children may not get individual creation events. Readdir each affected
    // parent and reconcile with DB.
    let verify_result = verify_affected_dirs(&affected_paths, &writer);

    // Scan newly discovered directories (inserts children + computes subtree aggregates).
    // Skip excluded paths (system dirs like /System, /dev) that aren't in the index.
    if !verify_result.new_dir_paths.is_empty() {
        // Flush first: verify_affected_dirs sent UpsertEntryV2 for each new dir, but those
        // writes are still queued. scan_subtree opens a read connection to resolve the dir's
        // path → entry_id, which fails if the entry isn't committed yet.
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification pre-scan flush failed: {e}");
        }

        let cancelled = AtomicBool::new(false);
        for dir_path in &verify_result.new_dir_paths {
            if scanner::should_exclude(dir_path) {
                continue;
            }
            match scanner::scan_subtree(Path::new(dir_path), &writer, &cancelled) {
                Ok(summary) => {
                    log::debug!(
                        "Background verification: scanned new dir {dir_path} ({} entries, {}ms)",
                        summary.total_entries,
                        summary.duration_ms,
                    );
                }
                Err(e) => {
                    log::warn!("Background verification: scan_subtree({dir_path}) failed: {e}");
                }
            }
        }
    }

    let has_changes =
        verify_result.stale_count > 0 || verify_result.new_file_count > 0 || !verify_result.new_dir_paths.is_empty();

    if has_changes {
        log::debug!(
            "Background verification found {} stale, {} new files, {} new dirs; flushing",
            verify_result.stale_count,
            verify_result.new_file_count,
            verify_result.new_dir_paths.len(),
        );
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification flush failed: {e}");
        }

        // For new directories, propagate their subtree totals up the ancestor chain.
        // scan_subtree computes aggregates within the subtree but doesn't propagate
        // upward. Resolve each new dir path to its entry ID, read the computed
        // dir_stats, and send PropagateDeltaById to the parent.
        if !verify_result.new_dir_paths.is_empty() {
            // Resolve paths → IDs and batch-read dir_stats via ReadPool.
            // Note: although `run_background_verification` is async, `pool.with_conn()`
            // is safe here because the closure contains no `.await` points — the task
            // cannot migrate threads mid-closure, so thread-local storage is reliable.
            let dir_deltas: Vec<(i64, store::DirStatsById)> = get_read_pool()
                .and_then(|pool| {
                    pool.with_conn(|conn| {
                        let mut deltas = Vec::new();
                        for dir_path in &verify_result.new_dir_paths {
                            let entry_id = match store::resolve_path(conn, dir_path) {
                                Ok(Some(id)) => id,
                                _ => continue,
                            };
                            let parent_id = match IndexStore::get_parent_id(conn, entry_id) {
                                Ok(Some(pid)) => pid,
                                _ => continue,
                            };
                            let stats = IndexStore::get_dir_stats_by_id(conn, entry_id)
                                .ok()
                                .flatten()
                                .unwrap_or(store::DirStatsById {
                                    entry_id,
                                    recursive_logical_size: 0,
                                    recursive_physical_size: 0,
                                    recursive_file_count: 0,
                                    recursive_dir_count: 0,
                                });
                            deltas.push((parent_id, stats));
                        }
                        deltas
                    })
                    .ok()
                })
                .unwrap_or_default();

            for (parent_id, stats) in &dir_deltas {
                let _ = writer.send(WriteMessage::PropagateDeltaById {
                    entry_id: *parent_id,
                    logical_size_delta: stats.recursive_logical_size as i64,
                    physical_size_delta: stats.recursive_physical_size as i64,
                    file_count_delta: stats.recursive_file_count as i32,
                    dir_count_delta: (stats.recursive_dir_count as i32) + 1,
                });
            }

            if let Err(e) = writer.flush().await {
                log::warn!("Background verification propagation flush failed: {e}");
            }
        }

        // Emit index-dir-updated for any corrected paths so the UI refreshes
        let mut corrected_paths: Vec<String> = affected_paths.into_iter().collect();
        corrected_paths.extend(verify_result.new_dir_paths.iter().cloned());
        reconciler::emit_dir_updated(&app, corrected_paths);
    }

    DEBUG_STATS.verifying.store(false, Ordering::Relaxed);
    log::debug!(
        "Background verification completed in {}ms",
        verify_start.elapsed().as_millis(),
    );
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Drain the replay dedup buffer, process each event through the
/// reconciler, and collect affected paths. Returns the number of
/// deduplicated events processed.
fn flush_replay_batch(
    pending: &mut HashMap<String, watcher::FsChangeEvent>,
    conn: &Connection,
    writer: &IndexWriter,
    affected_paths: &mut HashSet<String>,
    affected_paths_overflow: &mut bool,
) -> usize {
    let count = pending.len();
    for (_path, event) in pending.drain() {
        if let Some(paths) = reconciler::process_fs_event(&event, conn, writer)
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
    }
    count
}

/// Result of `verify_affected_dirs`.
struct VerifyResult {
    /// Entries in DB but not on disk (deleted).
    stale_count: u64,
    /// Files on disk but not in DB (inserted with delta propagation).
    new_file_count: u64,
    /// Directories on disk but not in DB (inserted, need subtree scan by caller).
    new_dir_paths: Vec<String>,
}

/// Verify that DB entries for affected directories match what's on disk.
///
/// FSEvents journal replay coalesces events: child deletions may appear as
/// "parent directory modified" without individual removal events. Similarly,
/// new children may not get individual creation events.
///
/// Two-phase approach — no `INDEXING` lock needed:
///
/// **Phase 1 (ReadPool, no lock):** Resolve each affected path to its entry ID,
/// list children as `EntryRow` (integer-keyed), and snapshot into a `HashMap`.
/// Uses `get_read_pool()` + `pool.with_conn()` for lock-free DB reads.
///
/// **Phase 2 (no lock):** Walk the snapshot, check the filesystem
/// (`Path::exists`, `read_dir`, `symlink_metadata`), and send corrections to
/// the writer channel using integer-keyed write messages:
/// 1. **Stale entries**: DB children that no longer exist on disk get
///    `DeleteEntryById`/`DeleteSubtreeById` (auto-propagates deltas).
/// 2. **Missing entries**: Disk children not in DB get `UpsertEntryV2`.
///    New files also get `PropagateDeltaById`. New directories are collected
///    in `new_dir_paths` for the caller to scan via `scan_subtree`.
fn verify_affected_dirs(affected_paths: &HashSet<String>, writer: &IndexWriter) -> VerifyResult {
    // ── Phase 1: Bulk-read DB state via ReadPool (no INDEXING lock) ──
    // Snapshot: parent_path → (parent_id, Vec<EntryRow>)
    let pool = match get_read_pool() {
        Some(p) => p,
        None => {
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    let db_snapshot: HashMap<String, (i64, Vec<store::EntryRow>)> = match pool.with_conn(|conn| {
        let mut snapshot = HashMap::with_capacity(affected_paths.len());
        for parent_path in affected_paths {
            let parent_id = match store::resolve_path(conn, parent_path) {
                Ok(Some(id)) => id,
                _ => continue, // Path not in index, skip
            };
            match IndexStore::list_children_on(parent_id, conn) {
                Ok(entries) => {
                    snapshot.insert(parent_path.clone(), (parent_id, entries));
                }
                Err(_) => {
                    // Insert empty vec so Phase 2 still checks disk for new entries
                    snapshot.insert(parent_path.clone(), (parent_id, Vec::new()));
                }
            }
        }
        snapshot
    }) {
        Ok(snapshot) => snapshot,
        Err(e) => {
            log::warn!("verify_affected_dirs: ReadPool error: {e}");
            return VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            };
        }
    };

    // ── Phase 2: Filesystem checks without the lock ──────────────────
    let mut stale_count = 0u64;
    let mut new_file_count = 0u64;
    let mut new_dir_paths = Vec::<String>::new();

    for (parent_path, (parent_id, db_children)) in &db_snapshot {
        // Build a set of normalized DB child names for fast lookup
        let db_child_names: HashSet<String> = db_children
            .iter()
            .map(|c| store::normalize_for_comparison(&c.name))
            .collect();

        // Build child path from parent_path + name for filesystem checks
        let parent_prefix = if parent_path == "/" {
            String::new()
        } else {
            parent_path.clone()
        };

        // Detect stale entries (in DB but not on disk)
        for child in db_children {
            let child_path = format!("{}/{}", parent_prefix, child.name);
            if !Path::new(&child_path).exists() {
                if child.is_directory {
                    let _ = writer.send(WriteMessage::DeleteSubtreeById(child.id));
                } else {
                    let _ = writer.send(WriteMessage::DeleteEntryById(child.id));
                }
                stale_count += 1;
            }
        }

        // Detect missing entries (on disk but not in DB)
        let read_dir = match std::fs::read_dir(parent_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for dir_entry in read_dir.flatten() {
            let child_path = dir_entry.path();
            let child_path_str = child_path.to_string_lossy().to_string();
            let normalized = firmlinks::normalize_path(&child_path_str);

            let name = dir_entry.file_name().to_string_lossy().to_string();
            if db_child_names.contains(&store::normalize_for_comparison(&name)) {
                continue;
            }

            // Skip excluded system paths (e.g. /System, /dev, /Volumes)
            if scanner::should_exclude(&normalized) {
                continue;
            }

            let metadata = match std::fs::symlink_metadata(&child_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();

            let (logical_size, physical_size, modified_at) = if is_dir || is_symlink {
                (None, None, reconciler::entry_modified_at(&metadata))
            } else {
                reconciler::entry_size_and_mtime(&metadata)
            };

            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: *parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
            });

            if is_dir {
                log::debug!("verify_affected_dirs: new dir on disk: {normalized} (parent_id={parent_id})");
                new_dir_paths.push(normalized);
            } else if let Some(sz) = logical_size {
                // UpsertEntryV2 inserts the entry; propagate its size delta up the
                // ancestor chain starting from the parent.
                let _ = writer.send(WriteMessage::PropagateDeltaById {
                    entry_id: *parent_id,
                    logical_size_delta: sz as i64,
                    physical_size_delta: physical_size.unwrap_or(0) as i64,
                    file_count_delta: 1,
                    dir_count_delta: 0,
                });
                new_file_count += 1;
            }
        }
    }

    if stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() {
        log::debug!(
            "Replay verification: {stale_count} stale, {new_file_count} new files, \
             {} new dirs across {} affected dirs",
            new_dir_paths.len(),
            affected_paths.len(),
        );
    }

    VerifyResult {
        stale_count,
        new_file_count,
        new_dir_paths,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(path: &str, event_id: u64, flags: watcher::FsEventFlags) -> watcher::FsChangeEvent {
        watcher::FsChangeEvent {
            path: path.to_string(),
            event_id,
            flags,
        }
    }

    /// Merging created+removed: `item_removed` wins (priority-based merge),
    /// dropping `item_created`. The reconciler's stat-before-delete in
    /// `handle_removal` compensates: if the file still exists on disk, it
    /// upserts instead of deleting. Regression coverage for f0c225f.
    #[test]
    fn merge_created_then_removed_prioritizes_removed() {
        let created = make_event(
            "/test/file.txt",
            100,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        let removed = make_event(
            "/test/file.txt",
            200,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        );

        let merged = merge_fs_events(&created, &removed);

        assert!(merged.flags.item_removed, "item_removed should be set");
        assert!(!merged.flags.item_created, "item_created is dropped — removed wins");
        assert!(merged.flags.item_is_file, "item_is_file should be preserved");
        assert_eq!(merged.event_id, 200, "higher event_id should be kept");
    }

    /// Same as above but with events in reverse order: removed first, then
    /// created. `item_removed` still wins.
    #[test]
    fn merge_removed_then_created_prioritizes_removed() {
        let removed = make_event(
            "/test/file.txt",
            100,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        let created = make_event(
            "/test/file.txt",
            200,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        );

        let merged = merge_fs_events(&removed, &created);

        assert!(merged.flags.item_removed, "item_removed should be set");
        assert!(!merged.flags.item_created, "item_created is dropped — removed wins");
        assert!(merged.flags.item_is_file, "item_is_file should be preserved");
        assert_eq!(merged.event_id, 200, "higher event_id should be kept");
    }

    /// When merging, the higher event_id should always win regardless of
    /// which event is "existing" vs "incoming".
    #[test]
    fn merge_keeps_higher_event_id() {
        let older = make_event(
            "/test/file.txt",
            300,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        let newer = make_event(
            "/test/file.txt",
            100,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );

        // existing=older (300), incoming=newer (100)
        let merged = merge_fs_events(&older, &newer);
        assert_eq!(merged.event_id, 300, "higher event_id should be kept");

        // existing=newer (100), incoming=older (300)
        let merged = merge_fs_events(&newer, &older);
        assert_eq!(merged.event_id, 300, "higher event_id should be kept");
    }

    // ── merge_fs_events dedup/flag tests ─────────────────────────────

    /// Three events for the same path merge into one with the highest
    /// priority flag and the highest event_id.
    #[test]
    fn merge_three_events_same_path_keeps_highest_priority() {
        let modified = make_event(
            "/test/file.txt",
            10,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        let created = make_event(
            "/test/file.txt",
            20,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        let modified2 = make_event(
            "/test/file.txt",
            30,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );

        // Simulate HashMap-style dedup: fold sequentially
        let merged = merge_fs_events(&modified, &created);
        let merged = merge_fs_events(&merged, &modified2);

        assert!(
            merged.flags.item_created,
            "item_created should survive (higher priority than modified)"
        );
        assert!(!merged.flags.item_modified, "item_modified is subsumed by item_created");
        assert_eq!(merged.event_id, 30, "highest event_id should be kept");
    }

    /// Events for different paths are preserved independently when stored
    /// in a HashMap keyed by path (the live event loop's dedup strategy).
    #[test]
    fn distinct_paths_are_all_preserved() {
        let paths = ["/a.txt", "/b.txt", "/c.txt", "/d/e.txt", "/f/g/h.txt"];
        let mut map = HashMap::<String, watcher::FsChangeEvent>::new();

        for (i, path) in paths.iter().enumerate() {
            let event = make_event(
                path,
                (i + 1) as u64,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            map.entry(path.to_string())
                .and_modify(|existing| {
                    *existing = merge_fs_events(existing, &event);
                })
                .or_insert(event);
        }

        assert_eq!(map.len(), paths.len(), "each distinct path should have its own entry");
        for path in &paths {
            assert!(map.contains_key(*path), "map should contain {path}");
        }
    }

    /// `must_scan_sub_dirs` always wins when merged with other flags.
    #[test]
    fn merge_must_scan_sub_dirs_wins_over_modified() {
        let modified = make_event(
            "/test/dir",
            10,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_dir: true,
                ..Default::default()
            },
        );
        let must_scan = make_event(
            "/test/dir",
            20,
            watcher::FsEventFlags {
                must_scan_sub_dirs: true,
                item_is_dir: true,
                ..Default::default()
            },
        );

        // must_scan_sub_dirs incoming
        let merged = merge_fs_events(&modified, &must_scan);
        assert!(merged.flags.must_scan_sub_dirs, "must_scan_sub_dirs should win");
        assert_eq!(merged.event_id, 20);

        // must_scan_sub_dirs existing
        let merged = merge_fs_events(&must_scan, &modified);
        assert!(
            merged.flags.must_scan_sub_dirs,
            "must_scan_sub_dirs should win regardless of order"
        );
        assert_eq!(merged.event_id, 20);
    }

    /// `must_scan_sub_dirs` wins even when the other event has `item_removed`.
    #[test]
    fn merge_must_scan_sub_dirs_wins_over_removed() {
        let removed = make_event(
            "/test/dir",
            10,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_dir: true,
                ..Default::default()
            },
        );
        let must_scan = make_event(
            "/test/dir",
            20,
            watcher::FsEventFlags {
                must_scan_sub_dirs: true,
                item_is_dir: true,
                ..Default::default()
            },
        );

        let merged = merge_fs_events(&removed, &must_scan);
        assert!(
            merged.flags.must_scan_sub_dirs,
            "must_scan_sub_dirs should win over item_removed"
        );
    }

    // ── EventReconciler buffer overflow tests ────────────────────────

    /// Buffering exactly MAX_BUFFER_CAPACITY (500K) events does NOT
    /// trigger overflow. Adding one more does.
    #[test]
    fn buffer_capacity_boundary() {
        // MAX_BUFFER_CAPACITY is 500_000 (private to reconciler.rs)
        let cap = 500_000usize;
        let mut reconciler = EventReconciler::new();

        for i in 0..cap {
            reconciler.buffer_event(make_event(
                "/test/file.txt",
                i as u64,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ));
        }

        assert_eq!(
            reconciler.buffer_len(),
            cap,
            "buffer should hold exactly MAX_BUFFER_CAPACITY events"
        );
        assert!(
            !reconciler.did_buffer_overflow(),
            "should not overflow at exactly MAX_BUFFER_CAPACITY"
        );

        // One more triggers overflow
        reconciler.buffer_event(make_event(
            "/test/overflow.txt",
            cap as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));

        assert!(
            reconciler.did_buffer_overflow(),
            "should overflow after exceeding MAX_BUFFER_CAPACITY"
        );
        assert_eq!(reconciler.buffer_len(), 0, "buffer should be cleared on overflow");
    }

    /// After overflow, subsequent buffer_event calls are no-ops.
    #[test]
    fn buffer_overflow_drops_further_events() {
        let cap = 500_000usize;
        let mut reconciler = EventReconciler::new();

        // Fill to capacity + 1 to trigger overflow
        for i in 0..=cap {
            reconciler.buffer_event(make_event(
                "/test/file.txt",
                i as u64,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ));
        }
        assert!(reconciler.did_buffer_overflow());

        // Further events are silently dropped
        reconciler.buffer_event(make_event(
            "/test/new.txt",
            999_999,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
        assert_eq!(reconciler.buffer_len(), 0, "buffer should remain empty after overflow");
    }

    /// `did_buffer_overflow()` returns true after overflow, but
    /// `switch_to_live()` resets it. This matches the production flow:
    /// overflow is checked (and acted on) BEFORE `switch_to_live()`.
    #[test]
    fn overflow_flag_is_readable_before_switch_to_live() {
        let cap = 500_000usize;
        let mut reconciler = EventReconciler::new();

        for i in 0..=cap {
            reconciler.buffer_event(make_event(
                "/test/file.txt",
                i as u64,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ));
        }

        // Overflow is observable before switch_to_live
        assert!(reconciler.did_buffer_overflow(), "overflow flag should be set");

        // switch_to_live resets it (by design: the caller already consumed the flag)
        reconciler.switch_to_live();
        assert!(
            !reconciler.did_buffer_overflow(),
            "switch_to_live should reset overflow flag"
        );
        assert!(!reconciler.is_buffering(), "should be in live mode");
    }

    /// Buffering mode transitions: new -> buffering, switch_to_live ->
    /// live, buffer_event is no-op in live mode.
    #[test]
    fn buffering_mode_transitions() {
        let mut reconciler = EventReconciler::new();

        // Starts in buffering mode
        assert!(reconciler.is_buffering());

        // Buffer works
        reconciler.buffer_event(make_event(
            "/a.txt",
            1,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
        assert_eq!(reconciler.buffer_len(), 1);

        // Switch to live
        reconciler.switch_to_live();
        assert!(!reconciler.is_buffering());
        assert_eq!(reconciler.buffer_len(), 0, "buffer cleared on switch");

        // buffer_event is no-op in live mode
        reconciler.buffer_event(make_event(
            "/b.txt",
            2,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
        assert_eq!(reconciler.buffer_len(), 0, "buffer_event should be no-op in live mode");
    }

    // ── Replay dedup tests ───────────────────────────────────────────

    /// Replay dedup: 500 removal events for the same path (like a SQLite
    /// journal file) collapse to a single merged event.
    #[test]
    fn replay_dedup_collapses_duplicate_events() {
        let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();

        for i in 0..500 {
            let path = "/Users/test/Library/peewee-sqlite.db-journal".to_string();
            let event = make_event(
                &path,
                1000 + i,
                watcher::FsEventFlags {
                    item_removed: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|existing| {
                    *existing = merge_fs_events(existing, &event);
                })
                .or_insert(event);
        }

        assert_eq!(pending.len(), 1, "500 events for same path should collapse to 1");
        let merged = pending.values().next().unwrap();
        assert_eq!(merged.event_id, 1499, "highest event_id should be kept");
        assert!(merged.flags.item_removed, "item_removed flag should be preserved");
    }

    /// Replay dedup: events for different paths are all preserved while
    /// duplicates within each path are merged.
    #[test]
    fn replay_dedup_preserves_distinct_paths_merges_duplicates() {
        let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();

        // 100 events: 10 paths x 10 events each
        for path_idx in 0..10u64 {
            for event_idx in 0..10u64 {
                let path = format!("/path/{path_idx}/file.txt");
                let event = make_event(
                    &path,
                    path_idx * 10 + event_idx,
                    watcher::FsEventFlags {
                        item_modified: true,
                        item_is_file: true,
                        ..Default::default()
                    },
                );
                pending
                    .entry(path)
                    .and_modify(|existing| {
                        *existing = merge_fs_events(existing, &event);
                    })
                    .or_insert(event);
            }
        }

        assert_eq!(pending.len(), 10, "10 unique paths should be preserved");
        for path_idx in 0..10u64 {
            let path = format!("/path/{path_idx}/file.txt");
            let event = &pending[&path];
            assert_eq!(
                event.event_id,
                path_idx * 10 + 9,
                "each path should keep its highest event_id"
            );
        }
    }

    /// Replay dedup: mixed create/modify/remove events for the same path
    /// merge with correct flag priority (removed wins).
    #[test]
    fn replay_dedup_mixed_events_merge_correctly() {
        let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();
        let path = "/test/file.txt".to_string();

        let events = [
            (
                1,
                watcher::FsEventFlags {
                    item_created: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ),
            (
                2,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ),
            (
                3,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ),
            (
                4,
                watcher::FsEventFlags {
                    item_removed: true,
                    item_is_file: true,
                    ..Default::default()
                },
            ),
        ];

        for (id, flags) in events {
            let event = make_event(&path, id, flags);
            pending
                .entry(path.clone())
                .and_modify(|existing| {
                    *existing = merge_fs_events(existing, &event);
                })
                .or_insert(event);
        }

        assert_eq!(pending.len(), 1);
        let merged = &pending[&path];
        assert!(merged.flags.item_removed, "removed should win over created+modified");
        assert!(
            !merged.flags.item_created,
            "created should be dropped when removed wins"
        );
        assert_eq!(merged.event_id, 4, "highest event_id should be kept");
    }

    /// Replay dedup: simulates realistic event storm with a mix of high-churn
    /// paths (SQLite journals, Chrome cache) and unique paths. Verifies the
    /// dedup ratio matches expectations.
    #[test]
    fn replay_dedup_realistic_event_storm() {
        let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();
        let mut raw_count = 0u64;

        // 500 events for a SQLite journal (same path, rapid create/delete)
        for i in 0..500 {
            let path = "/Users/test/Library/aw-server/peewee-sqlite.db-journal".to_string();
            let event = make_event(
                &path,
                i,
                watcher::FsEventFlags {
                    item_removed: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|e| *e = merge_fs_events(e, &event))
                .or_insert(event);
            raw_count += 1;
        }

        // 200 events for Chrome cache (20 different todelete_ files, 10 events each)
        for file_idx in 0..20 {
            for event_idx in 0..10 {
                let path = format!("/Users/test/Library/Chrome/todelete_{file_idx:04x}");
                let event = make_event(
                    &path,
                    500 + file_idx * 10 + event_idx,
                    watcher::FsEventFlags {
                        item_removed: true,
                        item_is_file: true,
                        ..Default::default()
                    },
                );
                pending
                    .entry(path)
                    .and_modify(|e| *e = merge_fs_events(e, &event))
                    .or_insert(event);
                raw_count += 1;
            }
        }

        // 50 unique file modifications (no duplicates)
        for i in 0..50 {
            let path = format!("/Users/test/projects/file_{i}.rs");
            let event = make_event(
                &path,
                700 + i,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|e| *e = merge_fs_events(e, &event))
                .or_insert(event);
            raw_count += 1;
        }

        assert_eq!(raw_count, 750, "should have 750 raw events");
        // 1 (journal) + 20 (chrome) + 50 (unique) = 71 unique paths
        assert_eq!(pending.len(), 71, "should deduplicate to 71 unique paths");
    }
}
