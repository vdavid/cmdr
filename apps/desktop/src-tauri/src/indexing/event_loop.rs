use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_specta::Event;

use super::DEBUG_STATS;
use super::IndexPathSpace;
use super::enrichment::get_read_pool;
use super::events::{
    IndexReplayCompleteEvent, IndexReplayProgressEvent, RescanReason, emit_rescan_notification, set_phase_for,
};
use super::firmlinks;
use super::reconciler::{self, EventReconciler};
use super::scanner;
use super::store::{self, IndexStore};
use super::watcher;
use super::writer::{IndexWriter, WriteMessage};
use crate::pluralize::pluralize;

// ── Live event loop ──────────────────────────────────────────────────

/// Flush interval for live event batching. Events are deduplicated by
/// normalized path during this window before being processed. Longer
/// windows reduce allocations during event storms (for example, multiple
/// agents writing simultaneously) at the cost of slightly delayed UI
/// updates.
pub(super) const LIVE_FLUSH_INTERVAL_MS: u64 = 1000;

/// How often the live loop sweeps the reconciler's per-file throttle for keys
/// whose window elapsed, applying their last-seen size. Runs on the existing
/// `tokio::select!` (no new thread). ~1 s keeps trailing flushes prompt relative
/// to the 60 s window while staying negligible work when idle.
pub(super) const THROTTLE_SWEEP_INTERVAL_MS: u64 = 1000;

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
    /// The volume's path space. Journal replay only runs for a `has_event_journal()`
    /// volume (the boot disk today), so this is `root` in practice; it's threaded
    /// rather than hardcoded so the replay resolves in the same space as the live
    /// loop that follows it, and it's ready if a journaled mount-rooted kind appears.
    pub(super) space: IndexPathSpace,
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

/// Open a read-only DB connection with a bounded retry. Used by the live
/// and replay event loops at startup. With `busy_timeout` already set in
/// `apply_pragmas` the first attempt almost always succeeds; the retry is
/// here so a one-off open error can't permanently disable live indexing.
async fn open_read_conn_with_retry(db_path: &Path) -> Result<Connection, store::IndexStoreError> {
    match IndexStore::open_read_connection(db_path) {
        Ok(c) => Ok(c),
        Err(e) => {
            log::warn!("Open read connection failed, retrying in 100ms: {e}");
            tokio::time::sleep(Duration::from_millis(100)).await;
            IndexStore::open_read_connection(db_path)
        }
    }
}

/// Mark every affected directory (and its ancestors) as having a recursive-size
/// update in flight, then drain the set for the `index-dir-updated` emit.
///
/// Marking rides the exact paths that drive the UI refresh, so the "size
/// updating" hourglass shows on precisely the directories whose sizes are about
/// to change. The flags clear wholesale once the writer drains (see
/// `writer::writer_loop` and `indexing/pending_sizes.rs`). Live-path only — the
/// shared `process_fs_event` is deliberately not instrumented, so replay doesn't
/// flag everything during startup (the global indexing flag covers scans).
///
/// Marks on the VOLUME's tracker (`get_pending_sizes_for`), so an external drive's
/// hourglass shows on its own rows, not root's.
fn mark_pending_and_drain(volume_id: &str, pending_paths: &mut HashSet<String>) -> Vec<String> {
    if let Some(tracker) = crate::indexing::pending_sizes::get_pending_sizes_for(volume_id) {
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
pub(super) async fn run_live_event_loop(
    mut event_rx: tokio::sync::mpsc::Receiver<watcher::FsChangeEvent>,
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
                            &writer, &mut pending_paths,
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
                    &writer, &mut pending_paths,
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
pub(super) fn process_live_batch(
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
    reconciler: &mut EventReconciler,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    pending_paths: &mut HashSet<String>,
) {
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

    // Pass 2: process everything else
    for (_path, event) in &other_events {
        max_event_id = max_event_id.max(event.event_id);
        reconciler.process_live_event(event, conn, writer, pending_paths);
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
        let snap = super::metadata::extract_metadata(&metadata, is_dir, is_symlink);

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
fn split_parent_and_name(path: &str) -> Option<(String, String)> {
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
        space,
        since_event_id,
        estimated_total,
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

    // Collect all affected parent paths during replay (deduplicated).
    // Capped at MAX_AFFECTED_PATHS; beyond that we emit a full refresh.
    let mut affected_paths = HashSet::<String>::new();
    let mut affected_paths_overflow = false;

    // MustScanSubDirs paths to queue after replay.
    // Capped at MAX_PENDING_RESCANS; beyond that a full rescan is triggered.
    let mut pending_rescans = Vec::<String>::new();
    let mut pending_rescans_overflow = false;

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
            log::info!("Replay: HistoryDone received after {}", pluralize(event_count, "event"));

            // Flush remaining deduplicated events before leaving Phase 1
            deduped_total += flush_replay_batch(
                &mut replay_pending,
                &space,
                &conn,
                &writer,
                &mut affected_paths,
                &mut affected_paths_overflow,
            ) as u64;

            // Process the HistoryDone event itself (it may carry other flags)
            if let Some(paths) = reconciler::process_fs_event(&event, &space, &conn, &writer, None)
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
                    // Keep absolute; the reconcile strips at its resolve.
                    pending_rescans.push(space.absolute(&event.path));
                }
            }
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
                let _ = tx.send(());
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
            ) as u64;
            if last_event_id > since_event_id
                && let Err(e) = writer.send(WriteMessage::UpdateLastEventId(last_event_id))
            {
                log::warn!("Replay: UpdateLastEventId send failed: {e}");
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
    set_phase_for(&app, &volume_id, super::ActivityPhase::Live, "post-replay");

    // Replay done. Allow verifier to run and report scanning=false to frontend.
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
                // allowed-pluralize-noun: MAX_PENDING_RESCANS is the const 1_000.
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
                            &writer, &mut live_pending_paths,
                        );
                        if !live_pending_paths.is_empty() {
                            let changed = mark_pending_and_drain(&volume_id, &mut live_pending_paths);
                            super::lifecycle_bus::publish_dirs_changed(&volume_id, &changed);
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
                            let _ = tx.send(());
                        }
                        event_rx.close();
                        while event_rx.recv().await.is_some() {}
                        return Ok(());
                    }

                process_live_batch(
                    &mut live_pending_events, &mut reconciler, &space, &conn,
                    &writer, &mut live_pending_paths,
                );
                if !live_pending_paths.is_empty() {
                    let changed = mark_pending_and_drain(&volume_id, &mut live_pending_paths);
                    super::lifecycle_bus::publish_dirs_changed(&volume_id, &changed);
                    reconciler::emit_dir_updated(&app, changed);
                }
            }
            _ = throttle_sweep_interval.tick() => {
                // Apply any throttled files whose 60 s window elapsed; the
                // resulting ancestor paths ride the next flush tick's emit.
                let affected = reconciler.sweep_throttle(&writer, Instant::now());
                live_pending_paths.extend(affected);
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

// ── Background verification ──────────────────────────────────────────

/// Run post-replay verification in the background.
///
/// Called after live mode starts so the app is responsive immediately.
/// Corrections found by verification go through the writer channel,
/// which serializes them with live writes.
pub(super) async fn run_background_verification(affected_paths: HashSet<String>, writer: IndexWriter, app: AppHandle) {
    DEBUG_STATS.verifying.store(true, Ordering::Relaxed);
    let verify_start = Instant::now();
    log::debug!(
        "Background verification started ({} affected dirs)",
        affected_paths.len(),
    );

    // Verify affected directories: FSEvents journal replay coalesces events,
    // so child deletions may only show as "parent dir modified," and new
    // children may not get individual creation events. Readdir each affected
    // parent and reconcile with DB.
    //
    // Run on the blocking pool: `verify_affected_dirs` is sync (Phase 1 SQLite
    // reads via `ReadPool`, Phase 2 `read_dir`/`symlink_metadata` per child).
    // On a typical home folder it takes seconds. Doing it inline on an async
    // worker pins that worker for the full duration; on macOS it also feeds
    // a burst of writer messages and event emits through the main thread,
    // which competes with user-initiated IPCs like `plugin:window|close`.
    // The blocking pool absorbs the sync work; the async runtime stays free
    // to serve UI requests responsively (top-5 principle #3 — UI must always
    // be responsive).
    let verify_writer = writer.clone();
    let verify_affected_paths = affected_paths.clone();
    let verify_result = match tauri::async_runtime::spawn_blocking(move || {
        verify_affected_dirs(&verify_affected_paths, &verify_writer)
    })
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Background verification: verify_affected_dirs join failed: {e}");
            VerifyResult {
                stale_count: 0,
                new_file_count: 0,
                new_dir_paths: Vec::new(),
            }
        }
    };

    // Scan newly discovered directories (inserts children + computes subtree aggregates).
    // Skip excluded paths (system dirs like /System, /dev) that aren't in the index.
    if !verify_result.new_dir_paths.is_empty() {
        // Flush first: verify_affected_dirs sent UpsertEntryV2 for each new dir, but those
        // writes are still queued. scan_subtree opens a read connection to resolve the dir's
        // path → entry_id, which fails if the entry isn't committed yet.
        if let Err(e) = writer.flush().await {
            log::warn!("Background verification pre-scan flush failed: {e}");
        }

        // Guarded-walker-based parallel walk + sync writer-channel sends — same blocking-pool
        // reasoning as `verify_affected_dirs` above. A subtree scan can take many
        // seconds and saturates multiple rayon threads; keeping it off the async
        // pool is essential.
        let scan_writer = writer.clone();
        let scan_dirs = verify_result.new_dir_paths.clone();
        if let Err(e) = tauri::async_runtime::spawn_blocking(move || {
            let cancelled = AtomicBool::new(false);
            for dir_path in &scan_dirs {
                // Background verification is root-scoped (boot disk), so `BootDisk`.
                if scanner::should_exclude(dir_path, scanner::ExclusionScope::BootDisk) {
                    continue;
                }
                match scanner::scan_subtree(Path::new(dir_path), &scan_writer, &cancelled) {
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
        })
        .await
        {
            log::warn!("Background verification: scan_subtree batch join failed: {e}");
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

        // Tell the UI about the newly-scanned subtrees so open listings can
        // refresh them. Coalesced into a single emit: the scan loop above
        // already finished all subtrees before we get here (the loop is
        // synchronous), so emitting per-path here only paid the per-emit
        // macOS main-thread cost N times without giving the FE any new info.
        // The FE handler is throttled at 2 s per pane anyway, so N separate
        // emits and one batched emit produce the same UX. This keeps the main
        // thread free for user-initiated IPCs like `plugin:window|close`.
        // (Was the post-commit-66712c2d "1.83 TB ghost-size" fix; the
        // `affected_paths` problem it solved persists — we just batch the
        // emit instead of looping it.)
        let visible_new_dirs: Vec<String> = verify_result
            .new_dir_paths
            .iter()
            .filter(|p| !scanner::should_exclude(p, scanner::ExclusionScope::BootDisk))
            .cloned()
            .collect();
        if !visible_new_dirs.is_empty() {
            // Background verification is root-scoped (uses the root read pool), so
            // its live corrections publish under the local root for the importance
            // scheduler's incremental rescore (plan Decision 5).
            super::lifecycle_bus::publish_dirs_changed(super::ROOT_VOLUME_ID, &visible_new_dirs);
            reconciler::emit_dir_updated(&app, visible_new_dirs);
        }

        // No off-writer ancestor compensation for the new dirs: each `scan_subtree`
        // above sent `ComputeSubtreeAggregates`, whose handler repairs the ancestor
        // chain (sizes, counts, symlinks, AND coverage — which this path never
        // corrected before) on the writer thread, race-free and without the 2×
        // credit a read-then-`PropagateDeltaById` here caused (Leak A). The
        // repairs already committed under the `has_changes` flush above.

        // Final emit for the replay-affected paths whose stats were corrected
        // (stale-row deletions and new-file additions in the affected_paths set).
        // `new_dir_paths` are not included here — they were already emitted
        // progressively above as each subtree's scan finished.
        if !affected_paths.is_empty() {
            let changed: Vec<String> = affected_paths.into_iter().collect();
            super::lifecycle_bus::publish_dirs_changed(super::ROOT_VOLUME_ID, &changed);
            reconciler::emit_dir_updated(&app, changed);
        }
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
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    affected_paths: &mut HashSet<String>,
    affected_paths_overflow: &mut bool,
) -> usize {
    let count = pending.len();
    for (_path, event) in pending.drain() {
        if let Some(paths) = reconciler::process_fs_event(&event, space, conn, writer, None)
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
/// Two-phase approach, no `INDEXING` lock needed:
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
/// 2. **Missing entries**: Disk children not in DB get `UpsertEntryV2`. New files also get
///    `PropagateDeltaById`. New directories are collected in `new_dir_paths` for the caller to scan
///    via `scan_subtree`.
fn verify_affected_dirs(affected_paths: &HashSet<String>, writer: &IndexWriter) -> VerifyResult {
    // ── Phase 1: Bulk-read DB state via ReadPool (no lifecycle/registry lock) ──
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

            // Skip excluded system paths (e.g. /System, /dev, /Volumes).
            // Root-scoped background verification (boot disk), so `BootDisk`.
            if scanner::should_exclude(&normalized, scanner::ExclusionScope::BootDisk) {
                continue;
            }

            let metadata = match std::fs::symlink_metadata(&child_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();
            let snap = super::metadata::extract_metadata(&metadata, is_dir, is_symlink);

            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: *parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });

            // UpsertEntryV2 auto-propagates deltas in the writer.
            if is_dir {
                log::debug!("verify_affected_dirs: new dir on disk: {normalized} (parent_id={parent_id})");
                new_dir_paths.push(normalized);
            } else {
                new_file_count += 1;
            }
        }
    }

    if stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() {
        log::debug!(
            "Replay verification: {stale_count} stale, {}, {} across {}",
            pluralize(new_file_count, "new file"),
            pluralize(new_dir_paths.len() as u64, "new dir"),
            pluralize(affected_paths.len() as u64, "affected dir"),
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
mod tests;
