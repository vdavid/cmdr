//! The live event loop: real-time FSEvents/inotify processing after scan +
//! reconciliation completes. Holds `run_live_event_loop`, its per-batch worker
//! `process_live_batch` (three-phase: dir creations, inode rename pre-pass,
//! then everything else with removal-storm coalescing), and the live-path
//! helpers `detect_renames_by_inode`, `split_parent_and_name`, and
//! `mark_pending_and_drain`. Shared primitives (`merge_fs_events`,
//! `open_read_conn_with_retry`, the flush-interval constants) live in the parent
//! `event_loop` module.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::AppHandle;

use super::super::churn_monitor::ChurnObserver;
use super::super::watcher;
use super::{
    BacklogTracker, IngestionPressure, LIVE_FLUSH_INTERVAL_MS, THROTTLE_SWEEP_INTERVAL_MS, classify_ingestion_pressure,
    merge_fs_events, open_read_conn_with_retry, report_backlog, storm,
};
use crate::indexing::DEBUG_STATS;
use crate::indexing::IndexPathSpace;
use crate::indexing::events::{RescanReason, emit_rescan_notification};
use crate::indexing::metadata;
use crate::indexing::paths::path_prefix;
use crate::indexing::reconcile::reconciler::EventReconciler;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use crate::pluralize::pluralize;

/// Mark every affected directory (and its ancestors) as having a recursive-size
/// update in flight, then drain the set for the `index-dir-updated` emit.
///
/// Marking rides the exact paths that drive the UI refresh, so the "size
/// updating" hourglass shows on precisely the directories whose sizes are about
/// to change. The flags clear wholesale once the writer drains (see
/// `writer::writer_loop` and `indexing/read/pending_sizes.rs`). Live-path only — the
/// shared `process_fs_event` is deliberately not instrumented, so replay doesn't
/// flag everything during startup (the global indexing flag covers scans).
///
/// Marks on the VOLUME's tracker (`get_pending_sizes_for`), so an external drive's
/// hourglass shows on its own rows, not root's.
pub(super) fn mark_pending_and_drain(volume_id: &str, pending_paths: &mut HashSet<String>) -> Vec<String> {
    if let Some(tracker) = crate::indexing::read::pending_sizes::get_pending_sizes_for(volume_id) {
        for path in pending_paths.iter() {
            tracker.mark(path);
        }
    }
    pending_paths.drain().collect()
}

/// Process FSEvents in real time after scan + reconciliation completes.
///
/// Runs as a tokio task, reading events from the watcher channel and
/// deduplicating them by normalized path during each flush interval.
/// Only the deduplicated batch is processed through the reconciler, which
/// cuts allocations dramatically during event storms. Batches
/// `index-dir-updated` notifications with a 1s flush interval.
/// Exits when the channel closes (watcher stopped).
pub(in crate::indexing) async fn run_live_event_loop(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<watcher::FsChangeEvent>,
    mut reconciler: EventReconciler,
    writer: IndexWriter,
    app: AppHandle,
    volume_id: String,
    space: IndexPathSpace,
    watcher_overflow: Option<Arc<AtomicBool>>,
) {
    log::info!("Live event processing: started");
    log::info!(target: "stall_probe::reconciler", "live_event_loop_started");

    // Open a read-only connection for path resolution (integer-keyed lookups).
    // Read-only because nothing in this loop writes through this connection -
    // all writes go via `writer.send(...)`. Using `open_read_connection`
    // avoids running write-mode pragmas (auto_vacuum, journal_mode = WAL) that
    // can race the writer thread on startup. Retry once: with `busy_timeout`
    // set in `apply_pragmas` this should almost never fail, but a single
    // transient error here used to silently kill the FSEvents receiver and
    // stop live indexing for the rest of the session, so retry + error-log
    // is cheap insurance.
    let db_path = writer.db_path();
    let conn = match open_read_conn_with_retry(&db_path).await {
        Ok(c) => c,
        Err(e) => {
            crate::log_error!(
                "Live event loop: failed to open read connection after retries, live indexing disabled: {e}"
            );
            return;
        }
    };

    // Drain any rescans deferred during buffered replay (missing-parent
    // escalations defer into `pending_rescans` without live-queueing during
    // replay; `EventReconciler::replay` populates them, this starts them).
    reconciler.kick_pending_rescans(&writer);

    let mut event_count = 0u64;
    let mut pending_paths = HashSet::<String>::new();
    let mut pending_events = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut flush_interval = tokio::time::interval(Duration::from_millis(LIVE_FLUSH_INTERVAL_MS));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Trailing-flush sweep for the per-file throttle (no new thread).
    let mut throttle_sweep_interval = tokio::time::interval(Duration::from_millis(THROTTLE_SWEEP_INTERVAL_MS));
    throttle_sweep_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Phase 1 instrumentation: heartbeat every 5s with batch/event metrics.
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut batches_since_heartbeat: u64 = 0;
    let mut events_since_heartbeat: u64 = 0;
    let mut last_batch_duration_ms: u128 = 0;

    // Tripped by the writer on a fatal storage error. Polled each flush tick below
    // so a dead index stops this loop promptly, bounding the reconciler's
    // failing-resolve churn to one batch after the trip (the supervisor also tears
    // the watcher down, but this doesn't wait for that).
    let failure_signal = writer.failure_signal();

    // Backlog reporting: reports the TREND, so a backlog that's draining steadily
    // reads as progress and only a stuck queue warns.
    let mut backlog = BacklogTracker::new();

    // Per-subtree churn observability (`indexing/watch/churn_monitor.rs`): inert (and
    // free) unless `CMDR_CHURN_SPIKE` is set. `process_live_batch` does the
    // recording; this only owns the state and feeds it the raw-event counter the
    // loop already maintains.
    let mut churn = ChurnObserver::from_env(&volume_id, Instant::now());

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(event) => {
                        // Keep the dedup key + stored path ABSOLUTE (the FS reads and
                        // FE emit use it); the mount-relative strip happens only at
                        // the reconciler's `resolve_abs`. `absolute` firmlink-
                        // normalizes for the boot disk, passes through for a drive.
                        let canonical = space.absolute(&event.path);
                        let deduped_event = watcher::FsChangeEvent {
                            path: canonical.clone(),
                            event_id: event.event_id,
                            flags: event.flags,
                        };
                        pending_events
                            .entry(canonical)
                            .and_modify(|existing| {
                                *existing = merge_fs_events(existing, &deduped_event);
                            })
                            .or_insert(deduped_event);
                        event_count += 1;
                        DEBUG_STATS.live_event_count.store(event_count, Ordering::Relaxed);
                        if event_count.is_multiple_of(10_000) {
                            log::debug!(
                                "Live event processing: {} received ({} pending deduplicated)",
                                pluralize(event_count, "event"),
                                pending_events.len()
                            );
                        }
                    }
                    None => {
                        // Channel closed: process remaining events before exit
                        process_live_batch(
                            &mut pending_events, &mut reconciler, &space, &conn,
                            &writer, &mut pending_paths, churn.with_raw_total(event_count),
                        );
                        if !pending_paths.is_empty() {
                            let _ = writer.send(WriteMessage::EmitDirUpdated(
                                mark_pending_and_drain(&volume_id, &mut pending_paths),
                            ));
                        }
                        break;
                    }
                }
            }
            _ = flush_interval.tick() => {
                // Stop promptly if the index DB died with a fatal storage error: the
                // writer detected it and tripped the signal, so there is nothing to
                // process against a dead DB (the reconciler's resolves would just fail).
                // The supervisor also tears the watcher down; polling here bounds the
                // failing-resolve churn to at most one batch after the trip.
                if failure_signal.is_tripped() {
                    log::info!("Live event processing: stopping, the index storage failed");
                    break;
                }

                // Ingestion-pressure guard (Fix 2). The watcher→loop channel is
                // unbounded, so a slow drain grows the queue instead of dropping
                // events. Past the RAM-guard hard cap we DELIBERATELY fall back to a
                // full scan; a merely-high watermark just logs (rate-limited).
                match classify_ingestion_pressure(event_rx.len()) {
                    IngestionPressure::Overflowing => {
                        let queued = event_rx.len();
                        log::warn!(
                            "Live event processing: ingestion queue at {queued} (hard cap); falling back to a full scan"
                        );
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::IngestionBacklog,
                            format!(
                                "The live event queue reached {queued} pending events, past the ingestion hard cap. \
                                 Running a fresh scan to catch up."
                            ),
                        );
                        let vid = volume_id.clone();
                        tauri::async_runtime::spawn(async move {
                            crate::indexing::lifecycle::manager::perform_registry_rescan(&vid, "ingestion backlog").await;
                        });
                        // Drain and discard the backlog; the fresh scan supersedes it.
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        break;
                    }
                    IngestionPressure::FallingBehind => {
                        report_backlog(&mut backlog, "Live event processing", event_rx.len());
                    }
                    IngestionPressure::Healthy => backlog.reset(),
                }

                // Check if the FSEvents channel overflowed. Events were dropped
                // between FSEvents and our forward task. The only safe recovery is
                // a full rescan.
                if let Some(ref flag) = watcher_overflow
                    && flag.load(Ordering::Relaxed) {
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::WatcherChannelOverflow,
                            format!(
                                "The filesystem watcher's event channel overflowed after {}. \
                                 Some file changes were lost.",
                                pluralize(event_count, "live event")
                            ),
                        );
                        // Drain and discard remaining events: they're a partial
                        // picture and processing them before a rescan is pointless.
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        break;
                    }

                let batch_size = pending_events.len() as u64;
                let batch_start = Instant::now();
                process_live_batch(
                    &mut pending_events, &mut reconciler, &space, &conn,
                    &writer, &mut pending_paths, churn.with_raw_total(event_count),
                );
                let batch_ms = batch_start.elapsed().as_millis();
                batches_since_heartbeat += 1;
                events_since_heartbeat += batch_size;
                last_batch_duration_ms = batch_ms;
                if batch_ms > 200 {
                    log::info!(
                        target: "stall_probe::reconciler",
                        "process_live_batch_slow batch_size={batch_size} batch_ms={batch_ms}",
                    );
                }
                if !pending_paths.is_empty() {
                    // Enqueue the notification as a writer message so it fires
                    // after all prior writes (deletes, upserts, deltas) commit.
                    // Without this, multi-message operations (e.g. rename =
                    // delete + insert) show intermediate dir_stats to the UI.
                    let _ = writer.send(WriteMessage::EmitDirUpdated(
                        mark_pending_and_drain(&volume_id, &mut pending_paths),
                    ));
                }
            }
            _ = throttle_sweep_interval.tick() => {
                // Apply any throttled files whose 60 s window elapsed. The
                // resulting ancestor paths ride the next flush tick's emit.
                let affected = reconciler.sweep_throttle(&writer, Instant::now());
                pending_paths.extend(affected);
                // Trailing edge of the per-subtree rescan throttle: re-kick the
                // drain so a churny subtree whose window has now elapsed re-walks.
                reconciler.sweep_rescan_throttle(&writer);
            }
            _ = heartbeat_interval.tick() => {
                log::info!(
                    target: "stall_probe::reconciler",
                    "live_heartbeat batches={batches_since_heartbeat} events={events_since_heartbeat} last_batch_ms={last_batch_duration_ms} total_events={event_count}",
                );
                batches_since_heartbeat = 0;
                events_since_heartbeat = 0;
            }
        }
    }

    log::info!("Live event processing: stopped ({})", pluralize(event_count, "event"));
    log::info!(target: "stall_probe::reconciler", "live_event_loop_stopped events={event_count}");
}

/// Drain the pending events map, process each through the reconciler, and
/// send a single `UpdateLastEventId` for the batch.
///
/// Three-phase approach:
///
/// **Phase 1: Directory creations:** Sort by path depth and process parents
/// before children, then flush so the read connection sees the newly created
/// rows when later phases resolve children.
///
/// **Phase 1.5: Rename detection by inode:** For every event flagged
/// `item_renamed` whose path still exists on disk, stat the path and look
/// up its inode. If the DB already has an entry with that inode at a
/// *different* `(parent_id, name)`, send `MoveEntryV2` to reuse the existing
/// row, preserving its `entry_id` and (for directories) its `dir_stats`.
/// The matched event is removed from the batch so Phase 2 doesn't re-process
/// it. Then we flush again so Phase 2's `resolve_path` sees the moved row;
/// the OLD-path event of the same rename will then silently no-op.
///
/// **Phase 2: Everything else:** Files, modifications, removals, and any
/// rename events that didn't match by inode (the OLD-path side of a successful
/// match, or both sides of an inode-unstable rename on exFAT/FAT-family
/// volumes. The latter falls through to today's create/delete behaviour.
///
/// Without Phase 1, child file events in the same 1s batch as their parent
/// directory's creation event would fail `resolve_path()` and be silently
/// skipped ("parent not in DB"). Without Phase 1.5, renames are processed as
/// delete+insert, which clears the renamed dir's `dir_stats`.
pub(in crate::indexing) fn process_live_batch(
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
    reconciler: &mut EventReconciler,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    pending_paths: &mut HashSet<String>,
    churn: &mut ChurnObserver,
) {
    // Churn observability, BEFORE the early return and before the drain: an
    // idle period must still close and emit, or the time series grows holes
    // exactly where "this subtree went quiet" is the answer we're after.
    // Read-only — it writes nothing and decides nothing.
    //
    // This lives INSIDE `process_live_batch`, not at a loop's flush tick, on
    // purpose: there is more than one live loop (`live.rs` and `replay.rs`
    // Phase 3), and hooking one of them silently measured nothing on the
    // cold-start replay path. Every live batch funnels through here, so this is
    // the only site that cannot be forgotten.
    churn.observe(pending_events.keys().map(String::as_str), Instant::now());

    if pending_events.is_empty() {
        return;
    }

    // Partition into directory creations and everything else
    let mut dir_creations: Vec<(String, watcher::FsChangeEvent)> = Vec::new();
    let mut other_events: Vec<(String, watcher::FsChangeEvent)> = Vec::new();

    for (path, event) in pending_events.drain() {
        if event.flags.item_created && event.flags.item_is_dir && !event.flags.must_scan_sub_dirs {
            dir_creations.push((path, event));
        } else {
            other_events.push((path, event));
        }
    }

    let mut max_event_id = 0u64;

    // Pass 1: process directory creations (shorter paths first = parents before children)
    if !dir_creations.is_empty() {
        dir_creations.sort_by_key(|(path, _)| path.len());
        for (_path, event) in &dir_creations {
            max_event_id = max_event_id.max(event.event_id);
            reconciler.process_live_event(event, conn, writer, pending_paths);
        }
        // Flush so the read connection can resolve the newly created directories
        // when processing child events in pass 2. Uses block_in_place because
        // flush_blocking() panics inside a tokio runtime, and the Connection
        // borrow prevents making this function async.
        tokio::task::block_in_place(|| {
            let _ = writer.flush_blocking();
        });
    }

    // Pass 1.5: rename detection by inode. Removes matched events from
    // `other_events` and replaces the create/delete dance with a single
    // `MoveEntryV2`, preserving the entry's `dir_stats`.
    let rename_handled =
        detect_renames_by_inode(&mut other_events, space, conn, writer, pending_paths, &mut max_event_id);
    if rename_handled > 0 {
        // Flush so Phase 2's `resolve_path` calls see the moved rows. Without
        // this, the OLD-path event of a matched rename could see the row at
        // its original `(parent_id, name)` and try to delete it.
        tokio::task::block_in_place(|| {
            let _ = writer.flush_blocking();
        });
    }

    // Pass 2: removals (with removal-storm coalescing) and everything else.
    //
    // Removals get storm coalescing (root cause 7): a per-batch burst under one
    // prefix escalates to a single subtree rescan, and the storm's
    // strict-descendant per-file removals are dropped (the rescan re-lists the
    // survivors). Non-removals (files, modifications) keep flowing per-event —
    // the drop rule keys strictly on `item_removed`, so a mixed create+delete
    // storm still converges (the reconcile sees final disk state).
    let (removals, non_removals): (Vec<_>, Vec<_>) = other_events.into_iter().partition(|(_p, e)| e.flags.item_removed);

    for (_path, event) in &non_removals {
        max_event_id = max_event_id.max(event.event_id);
        reconciler.process_live_event(event, conn, writer, pending_paths);
    }

    if !removals.is_empty() {
        // Escalate over-threshold removal groups to subtree rescans FIRST, so the
        // freshly-queued anchors are visible in `rescan_scopes()` for the drop
        // filter below.
        let removal_paths: Vec<&str> = removals.iter().map(|(p, _)| p.as_str()).collect();
        for anchor in storm::detect_storm_anchors(&removal_paths) {
            log::info!(
                "Removal storm: coalescing {} removals into a subtree rescan of {}",
                removals.len(),
                anchor.display(),
            );
            reconciler.queue_must_scan_sub_dirs(anchor, writer);
        }

        // Snapshot the queued-or-active rescan scopes once (owned paths, so the
        // per-event `requeue_rescan` below doesn't conflict with the borrow).
        let scopes = reconciler.rescan_scopes();
        let mut kept: Vec<(String, watcher::FsChangeEvent)> = Vec::with_capacity(removals.len());
        for (path, event) in removals {
            // Every removal advances the journal position — a dropped one WAS
            // handled (by the coalescing rescan), just not per-file.
            max_event_id = max_event_id.max(event.event_id);
            // Drop STRICT descendants of a rescan scope and re-queue that scope
            // (set-dedup makes it idempotent; also recovers a sub-threshold tail
            // batch that lands after the walk already listed those dirs). Never
            // the scope's own removal event — it must take the cheap
            // `DeleteSubtreeById` path (`reconcile_subtree` on a vanished root
            // deletes nothing and would strand the subtree).
            if let Some(scope) = storm::scope_to_requeue(&path, &scopes) {
                let scope = scope.clone();
                reconciler.requeue_rescan(scope, writer);
                continue;
            }
            kept.push((path, event));
        }

        // Parent-first ordering (dirs before files, shallow-first): `rm -rf`
        // emits a dir's rmdir AFTER its children's unlinks but usually in the
        // SAME batch. `item_is_dir` rides FSEvents flags (macOS-solid; a harmless
        // no-op on Linux, where removals default it false).
        kept.sort_by_key(|(path, event)| (!event.flags.item_is_dir, path_prefix::depth(path)));

        // Process dir removals first, then FLUSH before the file removals so each
        // dir's `DeleteSubtreeById` is visible to the read connection — its
        // file-siblings then resolve to nothing and become cheap unknown-path
        // skips (one subtree delete instead of N per-file deletes, the ~3-5x
        // saver the incident log shows working across batches, engaged early).
        let mut processed_any_dir = false;
        let mut flushed_dirs = false;
        for (_path, event) in &kept {
            if event.flags.item_is_dir {
                processed_any_dir = true;
            } else if processed_any_dir && !flushed_dirs {
                // Reached the first file after ≥1 dir removal: commit the dir
                // removals so the file-siblings resolve to nothing and skip.
                tokio::task::block_in_place(|| {
                    let _ = writer.flush_blocking();
                });
                flushed_dirs = true;
            }
            reconciler.process_live_event(event, conn, writer, pending_paths);
        }
    }

    if max_event_id > 0 {
        let _ = writer.send(WriteMessage::UpdateLastEventId(max_event_id));
    }
}

/// Inspect every `item_renamed` event in `events`. For each path that still
/// exists on disk and has an inode that already maps to a DB entry at a
/// *different* `(parent_id, name)`, send `MoveEntryV2` and remove the event.
///
/// Returns the number of renames handled so the caller can decide whether to
/// flush before Phase 2.
///
/// Events whose stat fails are *not* removed (they're either the OLD-path
/// side of a successful match, which silently no-ops in Phase 2 once the row
/// has moved, or true removals/unrelated noise that Phase 2 needs to see).
pub(super) fn detect_renames_by_inode(
    events: &mut Vec<(String, watcher::FsChangeEvent)>,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    pending_paths: &mut HashSet<String>,
    max_event_id: &mut u64,
) -> usize {
    let mut handled = 0usize;

    events.retain(|(path, event)| {
        if !event.flags.item_renamed {
            return true;
        }

        // A volume whose inodes aren't trustworthy (FAT/exFAT) stores `inode: None`
        // for every entry, so `find_entry_by_inode` below can never match — the
        // pre-pass is inert there and renames fall back to the safe create/delete
        // path. Short-circuit up front so a FAT volume skips the per-event stat +
        // query entirely (the raw `symlink_metadata` inode here is the unstable
        // derived-cluster value, so it must NOT drive a match).
        if !space.inodes_trustworthy() {
            return true;
        }

        let metadata = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            // Path doesn't exist (or is unreadable). Could be the OLD-path
            // event of a successful rename, or a true removal. Phase 2
            // handles both.
            Err(_) => return true,
        };

        let is_dir = metadata.is_dir();
        let is_symlink = metadata.is_symlink();
        let snap = metadata::extract_metadata(&metadata, is_dir, is_symlink);

        // Symlinks carry no inode. Fall through to the create/delete path.
        let inode = match snap.inode {
            Some(i) => i,
            None => return true,
        };

        let existing_id = match IndexStore::find_entry_by_inode(conn, inode) {
            Ok(Some(id)) => id,
            // No DB row for this inode. Phase 2 will create one.
            Ok(None) => return true,
            Err(e) => {
                log::warn!(target: "indexing::event_loop", "rename pre-pass: find_entry_by_inode({inode}) failed: {e}");
                return true;
            }
        };

        let (new_parent_path, new_name) = match split_parent_and_name(path) {
            Some(p) => p,
            None => return true,
        };

        // `new_parent_path` is FS-event-derived (absolute); strip the mount root for
        // a mount-rooted drive at the resolve. `pending_paths.insert` below keeps it
        // absolute (it drives the FE emit).
        let new_parent_id = match space.resolve_abs(conn, &new_parent_path) {
            Ok(Some(id)) => id,
            // New parent isn't in the DB yet; let Phase 2 handle it via the
            // existing create/modify path. Without a parent ID we can't move.
            Ok(None) => return true,
            Err(e) => {
                log::warn!(
                    target: "indexing::event_loop",
                    "rename pre-pass: resolve_path({new_parent_path}) failed: {e}",
                );
                return true;
            }
        };

        // Defensive no-op: if the entry is already at the target location
        // (e.g. an inode collision on a non-rename event), skip.
        if let Ok(Some(old_entry)) = IndexStore::get_entry_by_id(conn, existing_id)
            && old_entry.parent_id == new_parent_id
                && store::normalize_for_comparison(&old_entry.name) == store::normalize_for_comparison(&new_name)
            {
                return true;
            }

        if let Err(e) = writer.send(WriteMessage::MoveEntryV2 {
            entry_id: existing_id,
            new_parent_id,
            new_name: new_name.clone(),
        }) {
            log::warn!(target: "indexing::event_loop", "rename pre-pass: MoveEntryV2 send failed: {e}");
            return true;
        }

        log::debug!(
            target: "indexing::event_loop",
            "rename pre-pass: matched inode={inode} → MoveEntryV2 id={existing_id} new_parent={new_parent_id} name={new_name}",
        );

        // Surface the new parent path to the UI. The old parent's dir-updated
        // event is already covered by the OLD-path event still in
        // `pending_events` (the reconciler emits it from `process_live_event`
        // when its `resolve_path` no-ops, via `emit_dir_updated`).
        pending_paths.insert(new_parent_path);
        *max_event_id = (*max_event_id).max(event.event_id);
        handled += 1;
        false
    });

    handled
}

/// Split `/a/b/c` into (`/a/b`, `c`). Returns `None` for paths whose trailing
/// component is empty (the root `/`).
pub(super) fn split_parent_and_name(path: &str) -> Option<(String, String)> {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    if trimmed.is_empty() {
        return None;
    }
    let idx = trimmed.rfind('/')?;
    let name = &trimmed[idx + 1..];
    if name.is_empty() {
        return None;
    }
    let parent = if idx == 0 {
        "/".to_string()
    } else {
        trimmed[..idx].to_string()
    };
    Some((parent, name.to_string()))
}
