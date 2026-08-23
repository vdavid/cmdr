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

use super::super::activity_monitor::{BatchObservers, ChangeKind};
use super::super::branches::{Admission, WatchScope};
use super::super::watcher;
use super::{
    BacklogTracker, IngestionPressure, LIVE_FLUSH_INTERVAL_MS, THROTTLE_SWEEP_INTERVAL_MS, classify_ingestion_pressure,
    merge_fs_events, open_read_conn_with_retry, report_backlog, storm,
};
use crate::indexing::DEBUG_STATS;
use crate::indexing::IndexPathSpace;
use crate::indexing::events::{
    Diagnostic, EventSink, IndexErrorReport, IndexEvent, RescanReason, emit_rescan_notification,
};
use crate::indexing::lifecycle::{lifecycle_bus, manager};
use crate::indexing::metadata;
use crate::indexing::paths::path_prefix;
use crate::indexing::read::pending_sizes;
use crate::indexing::reconcile::reconciler::EventReconciler;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::pluralize::pluralize;

/// How many consecutive silent 5 s beats a quiet loop absorbs before it says
/// "still alive, still idle" anyway. Twelve is a minute, which keeps the
/// distinction between an idle loop and a dead one in an error-report bundle
/// without paying a line every five seconds for it.
const IDLE_HEARTBEAT_BEATS: u64 = 12;

/// A drained batch of live changes, split into the two facts the pending set used
/// to conflate.
///
/// Every consumer wants exactly one of them, and handing the wrong one out is
/// expensive in both directions: give the size set to a listing consumer and it
/// sees `/Users` in every batch, give the origins to the hourglass and ancestors
/// never show "size updating".
pub(super) struct ChangedDirs {
    /// The dirs whose OWN listing changed. What a consumer that expands DOWNWARD
    /// (the importance scheduler's incremental rescore, the media live tick) must
    /// use.
    pub(super) origins: Vec<String>,
    /// `origins` plus every ancestor up to `/`: the dirs whose recursive SIZE the
    /// writer is about to change. What the FE emit and the hourglass need.
    pub(super) with_ancestors: Vec<String>,
}

/// Mark every directory whose recursive size is about to change (the origins plus
/// their ancestors) as having an update in flight, then drain the pending set into
/// both views.
///
/// Marking rides the exact paths that drive the UI refresh, so the "size
/// updating" hourglass shows on precisely the directories whose sizes are about
/// to change. The flags clear wholesale once the writer drains (see
/// `writer::writer_loop` and `indexing/read/pending_sizes.rs`). Live-path only — the
/// shared `process_fs_event` is deliberately not instrumented, so replay doesn't
/// flag everything during startup (the global indexing flag covers scans).
///
/// The ancestor expansion happens HERE, once per batch over the deduplicated
/// origins, rather than per event inside `process_fs_event` — which both keeps the
/// narrow fact available to the bus and stops each event from allocating a chain of
/// ancestor `String`s.
///
/// Marks on the VOLUME's tracker (`get_pending_sizes_for`), so an external drive's
/// hourglass shows on its own rows, not root's.
pub(super) fn mark_pending_and_drain(volume_id: &str, pending_origins: &mut HashSet<String>) -> ChangedDirs {
    let origins: Vec<String> = pending_origins.drain().collect();
    let with_ancestors = path_prefix::with_ancestor_closure(&origins);
    if let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) {
        for path in &with_ancestors {
            tracker.mark(path);
        }
    }
    ChangedDirs {
        origins,
        with_ancestors,
    }
}

/// Add one admitted event to the pending batch, merging it onto whatever that
/// path already holds.
///
/// Shared by the receive arm and the promotion of events a finished walk
/// released, so a held event is merged by exactly the same rules a live one is.
pub(in crate::indexing) fn queue_admitted(
    event: watcher::FsChangeEvent,
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
) {
    pending_events
        .entry(event.path.clone())
        .and_modify(|existing| {
            *existing = merge_fs_events(existing, &event);
        })
        .or_insert(event);
}

/// Fold whatever a finished walk released into the batch that's about to run,
/// and queue a re-list for any branch whose buffer stopped being a complete
/// record.
///
/// Runs before the batch rather than after, so a held event lands in the very
/// first batch after its walk ends instead of waiting another flush interval.
pub(in crate::indexing) fn drain_promoted(
    scope: &WatchScope,
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
    reconciler: &mut EventReconciler,
    writer: &IndexWriter,
) {
    let promoted = scope.branches().take_promoted();
    if !promoted.events.is_empty() || !promoted.relist.is_empty() {
        log::info!(
            "Branch watch: releasing {} held by a walk that just ended{}",
            pluralize(promoted.events.len() as u64, "event"),
            if promoted.relist.is_empty() {
                String::new()
            } else {
                format!(", and re-listing {}", pluralize(promoted.relist.len() as u64, "branch"))
            },
        );
    }
    for event in promoted.events {
        queue_admitted(event, pending_events);
    }
    for branch in promoted.relist {
        reconciler.queue_must_scan_sub_dirs(std::path::PathBuf::from(branch), writer);
    }
}

/// What the live loop needs to know about the volume it serves, beside the
/// channel, the reconciler, the writer, and the sink.
///
/// One value rather than four arguments, matching [`ReplayConfig`](super::ReplayConfig)
/// next door: the two loops take the same facts and a caller that mismatched
/// them would resolve paths in one volume's space against another's index.
pub(in crate::indexing) struct LiveConfig {
    /// The volume this loop serves.
    pub(in crate::indexing) volume_id: String,
    /// Its path space: pass-through for the boot disk, mount-relative strip for
    /// a mount-rooted drive.
    pub(in crate::indexing) space: IndexPathSpace,
    /// The watcher's overflow flag, or `None` when no watcher started. Set means
    /// events were dropped before reaching us, and only a rescan recovers.
    pub(in crate::indexing) watcher_overflow: Option<Arc<AtomicBool>>,
    /// How much of the volume this loop answers for.
    pub(in crate::indexing) scope: WatchScope,
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
    events: Arc<dyn EventSink>,
    config: LiveConfig,
) {
    let LiveConfig {
        volume_id,
        space,
        watcher_overflow,
        scope,
    } = config;
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
            events.emit(IndexEvent::Error {
                report: IndexErrorReport::LiveEventLoopUnavailable {
                    detail: Diagnostic(e.to_string()),
                },
            });
            return;
        }
    };

    // Drain any rescans deferred during buffered replay (missing-parent
    // escalations defer into `pending_rescans` without live-queueing during
    // replay; `EventReconciler::replay` populates them, this starts them).
    reconciler.kick_pending_rescans(&writer);

    let mut event_count = 0u64;
    let mut pending_origins = HashSet::<String>::new();
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
    // Consecutive beats that saw nothing, so a quiet loop says "still alive" once a
    // minute instead of twelve times (see the heartbeat arm).
    let mut idle_beats: u64 = 0;

    // Tripped by the writer on a fatal storage error. Polled each flush tick below
    // so a dead index stops this loop promptly, bounding the reconciler's
    // failing-resolve churn to one batch after the trip (the supervisor also tears
    // the watcher down, but this doesn't wait for that).
    let failure_signal = writer.failure_signal();

    // Backlog reporting: reports the TREND, so a backlog that's draining steadily
    // reads as progress and only a stuck queue warns.
    let mut backlog = BacklogTracker::new();

    // What watches this loop's batches: the per-subtree churn monitor (inert, and
    // free, unless `CMDR_CHURN_SPIKE` is set) and the per-folder activity tap,
    // which reports through this loop's own sink. `process_live_batch` does all
    // the recording; this only owns the state and feeds the churn side the
    // raw-event counter the loop already maintains.
    let mut observers = BatchObservers::from_env(&volume_id, Arc::clone(&events), Instant::now());

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
                            path: canonical,
                            event_id: event.event_id,
                            flags: event.flags,
                        };
                        // On a whole-volume loop every event is ours. On a
                        // branch-watched volume the scope decides: inside covered
                        // ground it flows, inside ground a walk is covering right
                        // now it waits, and anywhere else it's dropped.
                        if let Admission::Process(admitted) = scope.admit(deduped_event) {
                            for event in admitted {
                                queue_admitted(event, &mut pending_events);
                            }
                        }
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
                        drain_promoted(&scope, &mut pending_events, &mut reconciler, &writer);
                        process_live_batch(
                            &mut pending_events, &mut reconciler, &space, &conn,
                            &writer, &mut pending_origins, observers.with_raw_total(event_count),
                        );
                        if !pending_origins.is_empty() {
                            let changed = mark_pending_and_drain(&volume_id, &mut pending_origins);
                            lifecycle_bus::publish_dirs_changed(&volume_id, &changed.origins);
                            let _ = writer.send(WriteMessage::EmitDirUpdated(changed.with_ancestors));
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
                            events.as_ref(),
                            &volume_id,
                            RescanReason::IngestionBacklog,
                            format!(
                                "The live event queue reached {queued} pending events, past the ingestion hard cap. \
                                 Running a fresh scan to catch up."
                            ),
                        );
                        let vid = volume_id.clone();
                        crate::indexing::host::runtime::spawn(async move {
                            manager::perform_registry_rescan(&vid, "ingestion backlog").await;
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
                            events.as_ref(),
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

                drain_promoted(&scope, &mut pending_events, &mut reconciler, &writer);
                let batch_size = pending_events.len() as u64;
                let batch_start = Instant::now();
                process_live_batch(
                    &mut pending_events, &mut reconciler, &space, &conn,
                    &writer, &mut pending_origins, observers.with_raw_total(event_count),
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
                // A branch-watched loop discards most of the stream, and
                // `process_live_batch` only advances the journal position for what
                // it PROCESSED. Left there, a volume whose branches are quiet would
                // let its stored position age until the next launch's replay gap is
                // too wide to be worth replaying — so the position follows the
                // stream instead, except while something is buffered (see
                // `safe_event_id`).
                if let Some(id) = scope.branches().safe_event_id() {
                    let _ = writer.send(WriteMessage::UpdateLastEventId(id));
                }

                if !pending_origins.is_empty() {
                    let changed = mark_pending_and_drain(&volume_id, &mut pending_origins);
                    // Both live loops publish the origins, so a volume that took the
                    // post-scan route feeds importance and media exactly like one that
                    // took the cold-start replay route.
                    lifecycle_bus::publish_dirs_changed(&volume_id, &changed.origins);
                    // Enqueue the FE notification as a writer message so it fires
                    // after all prior writes (deletes, upserts, deltas) commit.
                    // Without this, multi-message operations (e.g. rename =
                    // delete + insert) show intermediate dir_stats to the UI.
                    let _ = writer.send(WriteMessage::EmitDirUpdated(changed.with_ancestors));
                }
            }
            _ = throttle_sweep_interval.tick() => {
                // Apply any throttled files whose 60 s window elapsed. The
                // origin dirs they changed ride the next flush tick's emit.
                pending_origins.extend(reconciler.sweep_throttle(&writer, Instant::now()));
                // Trailing edge of the per-subtree rescan throttle: re-kick the
                // drain so a churny subtree whose window has now elapsed re-walks.
                reconciler.sweep_rescan_throttle(&writer);
            }
            _ = heartbeat_interval.tick() => {
                // DEBUG, not INFO: a heartbeat is not a noteworthy lifecycle event,
                // and the file sink is Debug, so a bundle keeps it either way.
                // A beat with no batches and no events proves the loop is alive and
                // says nothing else, so it only speaks once per
                // `IDLE_HEARTBEAT_BEATS` beats. Any beat that SAW work still logs at
                // full 5 s resolution, which is what the stall probe is for.
                let idle = batches_since_heartbeat == 0 && events_since_heartbeat == 0;
                idle_beats = if idle { idle_beats + 1 } else { 0 };
                if !idle || idle_beats >= IDLE_HEARTBEAT_BEATS {
                    log::debug!(
                        target: "stall_probe::reconciler",
                        "live_heartbeat batches={batches_since_heartbeat} events={events_since_heartbeat} last_batch_ms={last_batch_duration_ms} total_events={event_count} idle_beats={idle_beats}",
                    );
                    idle_beats = 0;
                }
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
    pending_origins: &mut HashSet<String>,
    observers: &mut BatchObservers,
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
    // the only site that cannot be forgotten. The activity tap below rides the
    // same guarantee, one stage later in the batch.
    observers.observe_churn(pending_events.keys().map(String::as_str), Instant::now());

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
        for (path, event) in &dir_creations {
            max_event_id = max_event_id.max(event.event_id);
            // ⚠️ Folded in EXPLICITLY. `pending_events.drain()` above empties the
            // input map into two vectors, and this is the one the later passes
            // never see, so a tap reading only Pass 2 would miss every new
            // directory — the single most intent-bearing thing a batch can hold.
            observers.activity().record_event(path, &event.flags);
            reconciler.process_live_event(event, conn, writer, pending_origins);
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
    let renamed_paths = detect_renames_by_inode(
        &mut other_events,
        space,
        conn,
        writer,
        pending_origins,
        &mut max_event_id,
    );
    // ⚠️ The MATCHED renames, which the pre-pass has just taken out of
    // `other_events`. Only the FAILED matches survive into Pass 2, so a tap
    // reading the corrected stream alone would count the noise and drop every
    // real rename — and a rename-only batch would report nothing at all.
    for path in &renamed_paths {
        observers.activity().record(path, ChangeKind::Renamed);
    }
    if !renamed_paths.is_empty() {
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

    for (path, event) in &non_removals {
        max_event_id = max_event_id.max(event.event_id);
        observers.activity().record_event(path, &event.flags);
        reconciler.process_live_event(event, conn, writer, pending_origins);
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
            // ⚠️ Surfaced to the tap BEFORE the drop filter below throws the
            // strict-descendant removals away. Without this a sixty-thousand-file
            // delete inside a surviving folder contributes nothing at all, because
            // every per-file event it produced is about to be dropped in favour of
            // the rescan.
            observers.activity().record_storm_anchor(&anchor.to_string_lossy());
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
        for (path, event) in &kept {
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
            observers.activity().record_event(path, &event.flags);
            reconciler.process_live_event(event, conn, writer, pending_origins);
        }
    }

    if max_event_id > 0 {
        let _ = writer.send(WriteMessage::UpdateLastEventId(max_event_id));
    }

    // Close the batch: the tap reports its per-folder rollups through the loop's
    // sink. Last, so everything the batch corrected is already folded in.
    observers.finish_batch();
}

/// Inspect every `item_renamed` event in `events`. For each path that still
/// exists on disk and has an inode that already maps to a DB entry at a
/// *different* `(parent_id, name)`, send `MoveEntryV2` and remove the event.
///
/// Returns the NEW paths of the renames it handled, so the caller can decide
/// whether to flush before Phase 2 and can report them as renames.
///
/// ⚠️ **The paths, ❌ not a bare count.** A matched event is `retain`ed out of
/// `events`, so after this call only the FAILED matches are still in the batch.
/// Anything downstream reading the corrected stream alone would therefore see
/// the noise and none of the signal, and a rename-only batch would look empty.
/// These are the successes, and they are only available here.
///
/// Events whose stat fails are *not* removed (they're either the OLD-path
/// side of a successful match, which silently no-ops in Phase 2 once the row
/// has moved, or true removals/unrelated noise that Phase 2 needs to see).
pub(super) fn detect_renames_by_inode(
    events: &mut Vec<(String, watcher::FsChangeEvent)>,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    pending_origins: &mut HashSet<String>,
    max_event_id: &mut u64,
) -> Vec<String> {
    let mut handled: Vec<String> = Vec::new();

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
        // a mount-rooted drive at the resolve. `pending_origins.insert` below keeps it
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

        // The new parent's listing gained an entry, so it is an origin. The old
        // parent is already covered by the OLD-path event still in `pending_events`
        // (the reconciler reports it from `process_live_event` when its
        // `resolve_path` no-ops). A consumer that expands downward reaches the moved
        // directory itself through this parent, which is what makes a rename INTO a
        // `node_modules` still flip its whole subtree's floor status.
        pending_origins.insert(new_parent_path);
        *max_event_id = (*max_event_id).max(event.event_id);
        handled.push(path.clone());
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
