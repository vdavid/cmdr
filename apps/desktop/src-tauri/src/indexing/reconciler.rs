//! Event reconciler: buffers FSEvents during scan, replays after scan completes.
//!
//! During the initial full scan, the watcher runs concurrently and buffers events.
//! Once the scan finishes, the reconciler replays only events that arrived *after*
//! the scanner read their affected path (using monotonically increasing event IDs).
//! Events with `event_id <= scan_start_event_id` are discarded because the scan data
//! is newer.
//!
//! After replay, the reconciler switches to live mode where events are processed
//! immediately.
//!
//! ## Integer-keyed resolution (milestone 4)
//!
//! All path resolution uses `store::resolve_path(conn, path)` to convert filesystem
//! paths to integer entry IDs. Write messages use integer-keyed variants:
//! `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`.
//! The reconciler holds a read connection (`rusqlite::Connection`) for path resolution.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rusqlite::Connection;
use tauri::AppHandle;

use crate::ignore_poison::IgnorePoison;
use crate::indexing::DEBUG_STATS;
use crate::indexing::firmlinks;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore, IndexStoreError};
use crate::indexing::watcher::FsChangeEvent;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use crate::pluralize::pluralize;

mod throttle;
use throttle::{Throttle, ThrottleOutcome};

/// The live-path throttle, carrying the exact upsert to replay on the trailing
/// flush (see [`PendingUpsert`]). Only the live reconciler holds one; the replay
/// path threads `None` so journal catch-up stays unthrottled. Visible to
/// `indexing` because it rides `process_fs_event`'s signature.
pub(super) type LiveThrottle = Throttle<PendingUpsert>;

/// The suppressed upsert a throttled file's trailing flush replays. Built from
/// the already-stat'd suppressed event, so the sweep never re-stats (which would
/// risk blocking the live loop on a dead mount, and add a phantom-apply-on-
/// deleted-file case). Only regular files are throttled, so `is_directory` /
/// `is_symlink` are always false at flush time.
pub(super) struct PendingUpsert {
    parent_id: i64,
    name: String,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    nlink: Option<u64>,
}

// ── EventReconciler ──────────────────────────────────────────────────

/// Maximum number of events the reconciler will buffer during a scan.
/// Beyond this, buffering stops and a full rescan is forced after the
/// current scan completes. The index is a disposable cache, so dropping
/// events is always safe.
const MAX_BUFFER_CAPACITY: usize = 500_000;

/// Aggregator for high-volume reconciler skip events. Each per-event line is at TRACE
/// (off by default; file chain captures Debug+ only); this aggregator emits a single
/// DEBUG summary every ~5 s, so error report bundles still carry the existence-of-drift
/// signal without the per-event noise. Two instances cover the two skip classes that
/// dominate normal log volume: [`UNKNOWN_PATH_SKIPS`] (removal for a path not in the DB)
/// and [`STALE_PARENT_SKIPS`] (create/modify whose parent dir isn't in the DB).
///
/// Most skips are harmless build-output churn, but a sustained rate (or a sample path
/// in an unexpected tree) can flag real reconciler/index drift. Triagers: if you need
/// the per-event detail, run with `RUST_LOG=cmdr_lib::indexing::reconciler=trace,debug`.
mod skip_aggregator {
    use std::sync::Mutex;
    use std::time::Instant;

    use crate::pluralize::pluralize;

    const FLUSH_INTERVAL_SECS: u64 = 5;
    const SAMPLE_LEN: usize = 80;

    struct State {
        count_since_last_flush: u64,
        total: u64,
        last_flush: Instant,
        /// One representative path from the current window. Truncated to SAMPLE_LEN
        /// chars so a verbose log dir doesn't blow up the line. Cleared on flush.
        sample: Option<String>,
    }

    /// One skip category: its own rolling state plus the words for the summary line
    /// (`"skipped {unit}s {reason} in …"`).
    pub(super) struct SkipAggregator {
        state: Mutex<Option<State>>,
        /// Counted noun, e.g. `"removal"` → "skipped 3 removals".
        unit: &'static str,
        /// Reason phrase, e.g. `"for unknown paths"`.
        reason: &'static str,
    }

    impl SkipAggregator {
        const fn new(unit: &'static str, reason: &'static str) -> Self {
            Self {
                state: Mutex::new(None),
                unit,
                reason,
            }
        }

        /// Increment the counter and emit a summary if the flush interval has elapsed.
        /// Called on every skip (cheap: one mutex acquisition, one branch).
        pub(super) fn record(&self, path: &str) {
            let mut guard = match self.state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let state = guard.get_or_insert_with(|| State {
                count_since_last_flush: 0,
                total: 0,
                last_flush: Instant::now(),
                sample: None,
            });
            state.count_since_last_flush += 1;
            state.total += 1;
            if state.sample.is_none() {
                // Truncate on a CHAR boundary, not a byte index: NAS paths carry accented
                // names (e.g. "Külkeres síelés"), and `&path[..SAMPLE_LEN]` would panic
                // when byte SAMPLE_LEN lands mid-codepoint.
                let s = if path.chars().count() > SAMPLE_LEN {
                    format!("{}…", path.chars().take(SAMPLE_LEN).collect::<String>())
                } else {
                    path.to_string()
                };
                state.sample = Some(s);
            }
            if state.last_flush.elapsed().as_secs() >= FLUSH_INTERVAL_SECS {
                let count = state.count_since_last_flush;
                let total = state.total;
                let sample = state.sample.clone().unwrap_or_default();
                let secs = state.last_flush.elapsed().as_secs_f64();
                state.count_since_last_flush = 0;
                state.last_flush = Instant::now();
                state.sample = None;
                let (unit, reason) = (self.unit, self.reason);
                // Drop the lock before logging so the message format won't reenter under it.
                drop(guard);
                log::debug!(
                    "Reconciler: skipped {} {reason} in {secs:.1}s [{total} total], sample: {sample}",
                    pluralize(count, unit)
                );
            }
        }
    }

    /// Removals for a path that isn't in the DB (mostly harmless build-output churn).
    pub(super) static UNKNOWN_PATH_SKIPS: SkipAggregator = SkipAggregator::new("removal", "for unknown paths");
    /// Create/modify events whose parent dir isn't in the DB (stale intermediate dir).
    pub(super) static STALE_PARENT_SKIPS: SkipAggregator = SkipAggregator::new("event", "for missing parents");
}

/// Buffers FSEvents during the initial scan and replays them after the scan completes.
pub struct EventReconciler {
    /// Events buffered during scan, in arrival order.
    buffer: Vec<FsChangeEvent>,
    /// Whether we're in buffering mode (scan in progress).
    buffering: bool,
    /// Set when the buffer cap is hit. Forces a full rescan after the
    /// current scan completes instead of replaying individual events.
    pub(super) buffer_overflow: bool,
    /// Paths pending MustScanSubDirs rescans, deduplicated. Shared with
    /// spawned rescan tasks so they can start the next rescan on completion.
    pending_rescans: Arc<Mutex<HashSet<PathBuf>>>,
    /// Whether a MustScanSubDirs rescan is currently running.
    rescan_active: Arc<AtomicBool>,
    /// Per-file throttle for live upserts (leading + trailing, 60 s window). Only
    /// consulted on the live path; the trailing flush runs off the event loop's
    /// sweep tick via [`EventReconciler::sweep_throttle`].
    throttle: LiveThrottle,
}

impl EventReconciler {
    /// Create a new reconciler in buffering mode.
    pub fn new() -> Self {
        Self::with_throttle(Throttle::new(resolve_downloads_prefix()))
    }

    /// Construct with a caller-supplied throttle (tests inject a short window).
    fn with_throttle(throttle: LiveThrottle) -> Self {
        Self {
            buffer: Vec::new(),
            buffering: true,
            buffer_overflow: false,
            pending_rescans: Arc::new(Mutex::new(HashSet::new())),
            rescan_active: Arc::new(AtomicBool::new(false)),
            throttle,
        }
    }

    /// Test constructor with an explicit throttle window, so the trailing flush is
    /// exercised without sleeping a real [`THROTTLE_WINDOW`].
    #[cfg(test)]
    pub(super) fn new_with_throttle_window(window: std::time::Duration) -> Self {
        Self::with_throttle(Throttle::with_window(window, None))
    }

    /// Buffer an event during scan. If the buffer cap is reached, stops
    /// buffering and sets `buffer_overflow` to force a full rescan.
    pub fn buffer_event(&mut self, event: FsChangeEvent) {
        if !self.buffering || self.buffer_overflow {
            return;
        }
        if self.buffer.len() >= MAX_BUFFER_CAPACITY {
            log::warn!(
                // allowed-pluralize-noun: MAX_BUFFER_CAPACITY is the const 500_000.
                "Reconciler: buffer cap reached ({MAX_BUFFER_CAPACITY} events). \
                 Dropping further events; a full rescan will run after the current scan."
            );
            self.buffer_overflow = true;
            self.buffer.clear();
            self.buffer.shrink_to_fit();
            return;
        }
        self.buffer.push(event);
    }

    /// Replay buffered events after scan completes.
    ///
    /// - Events with `event_id <= scan_start_event_id` are skipped (scan data is newer).
    /// - Events with `event_id > scan_start_event_id` are processed (filesystem changed after
    ///   scan).
    /// - Returns the last processed event ID.
    pub fn replay(
        &mut self,
        scan_start_event_id: u64,
        conn: &Connection,
        writer: &IndexWriter,
        on_dirs_affected: &mut dyn FnMut(Vec<String>),
    ) -> Result<u64, String> {
        // Sort by event_id to process in order
        self.buffer.sort_by_key(|e| e.event_id);

        let total = self.buffer.len();
        let mut processed = 0u64;
        let mut last_event_id = scan_start_event_id;
        let mut affected_paths: Vec<String> = Vec::new();

        log::info!(
            "Reconciler: replaying {} (scan_start_event_id={scan_start_event_id})",
            pluralize(total as u64, "buffered event")
        );

        for event in &self.buffer {
            // Skip events that the scan already covered
            if event.event_id <= scan_start_event_id {
                continue;
            }

            // Replay stays unthrottled (None): journal catch-up must converge
            // fully and fast; throttling is a live-steady-state concern.
            if let Some(paths) = process_fs_event(event, conn, writer, None) {
                affected_paths.extend(paths);
            }

            last_event_id = event.event_id;
            processed += 1;
        }

        // Notify caller of all affected paths
        if !affected_paths.is_empty() {
            on_dirs_affected(affected_paths);
        }

        // Store last event ID
        if last_event_id > scan_start_event_id {
            let _ = writer.send(WriteMessage::UpdateLastEventId(last_event_id));
        }

        log::info!(
            "Reconciler: replayed {processed}/{} (last_event_id={last_event_id})",
            pluralize(total as u64, "event")
        );
        Ok(last_event_id)
    }

    /// Switch from buffering to live mode. Clears the buffer.
    pub fn switch_to_live(&mut self) {
        self.buffering = false;
        self.buffer_overflow = false;
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        log::info!("Reconciler: switched to live mode");
    }

    /// Process a single event in live mode.
    ///
    /// Collects affected directory paths into `pending_paths` for batched
    /// emission by the caller (1s flush interval). Returns the event ID
    /// on success, or `None` if still buffering.
    pub fn process_live_event(
        &mut self,
        event: &FsChangeEvent,
        conn: &Connection,
        writer: &IndexWriter,
        pending_paths: &mut HashSet<String>,
    ) -> Option<u64> {
        if self.buffering {
            self.buffer_event(event.clone());
            return None;
        }

        // Handle MustScanSubDirs
        if event.flags.must_scan_sub_dirs {
            let normalized = firmlinks::normalize_path(&event.path);
            self.queue_must_scan_sub_dirs(PathBuf::from(&normalized), writer);
            return Some(event.event_id);
        }

        if let Some(affected_paths) = process_fs_event(event, conn, writer, Some(&mut self.throttle)) {
            pending_paths.extend(affected_paths);
        }

        // UpdateLastEventId is sent once per batch by the caller (process_live_batch)
        // instead of per-event, to reduce writer channel pressure during event storms.

        Some(event.event_id)
    }

    /// Flush every throttled key whose 60 s window has elapsed, applying its
    /// last-seen size (never re-statting). Called on the event loop's ~1 s
    /// throttle-sweep tick. Returns the ancestor paths whose `dir_stats` the
    /// flushes changed, for the caller's batched `index-dir-updated` emit.
    pub(super) fn sweep_throttle(&mut self, writer: &IndexWriter, now: Instant) -> Vec<String> {
        let flushes = self.throttle.sweep(now);
        let mut affected: Vec<String> = Vec::new();
        for (path, upsert) in flushes {
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: upsert.parent_id,
                name: upsert.name,
                is_directory: false,
                is_symlink: false,
                logical_size: upsert.logical_size,
                physical_size: upsert.physical_size,
                modified_at: upsert.modified_at,
                inode: upsert.inode,
                nlink: upsert.nlink,
            });
            affected.extend(collect_ancestor_paths(&path));
        }
        affected
    }

    /// Queue a MustScanSubDirs rescan, throttled to max 1 concurrent.
    pub(super) fn queue_must_scan_sub_dirs(&mut self, path: PathBuf, writer: &IndexWriter) {
        DEBUG_STATS.record_must_scan(&path.to_string_lossy());
        self.pending_rescans.lock_ignore_poison().insert(path.clone());

        if self.rescan_active.load(Ordering::Relaxed) {
            log::debug!(
                "Reconciler: MustScanSubDirs for {} queued (rescan already active)",
                path.display()
            );
            return;
        }

        start_next_rescan(
            Arc::clone(&self.pending_rescans),
            Arc::clone(&self.rescan_active),
            writer,
        );
    }

    /// Whether the reconciler's event buffer overflowed during the scan.
    pub(super) fn did_buffer_overflow(&self) -> bool {
        self.buffer_overflow
    }

    /// Number of buffered events (for diagnostics).
    #[cfg(test)]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the reconciler is in buffering mode.
    #[cfg(test)]
    pub fn is_buffering(&self) -> bool {
        self.buffering
    }
}

/// Start the next pending MustScanSubDirs rescan, if any.
///
/// Standalone function (not a method) so the spawned task can call it
/// after completion to drain the pending queue automatically.
fn start_next_rescan(
    pending_rescans: Arc<Mutex<HashSet<PathBuf>>>,
    rescan_active: Arc<AtomicBool>,
    writer: &IndexWriter,
) {
    let path = {
        let mut pending = pending_rescans.lock_ignore_poison();
        match pending.iter().next().cloned() {
            Some(p) => {
                pending.remove(&p);
                p
            }
            None => return,
        }
    };
    rescan_active.store(true, Ordering::Relaxed);

    let writer = writer.clone();
    let pending_for_task = Arc::clone(&pending_rescans);
    let active_for_task = Arc::clone(&rescan_active);

    log::info!("MustScanSubDirs: reconcile starting for {}", path.display());

    tokio::task::spawn_blocking(move || {
        let cancelled = AtomicBool::new(false);
        let conn = match IndexStore::open_write_connection(&writer.db_path()) {
            Ok(c) => c,
            Err(e) => {
                log::warn!(
                    "MustScanSubDirs: couldn't open read connection for {}: {e}",
                    path.display()
                );
                active_for_task.store(false, Ordering::Relaxed);
                // Try the next pending rescan even if this one failed
                start_next_rescan(pending_for_task, active_for_task, &writer);
                return;
            }
        };

        match reconcile_subtree(&path, &conn, &writer, &cancelled) {
            Ok(summary) => {
                if summary.duration.as_secs() > 10 {
                    log::warn!(
                        "MustScanSubDirs: reconcile slow for {} (+{} -{} ~{}, {}s)",
                        path.display(),
                        summary.added,
                        summary.removed,
                        summary.updated,
                        summary.duration.as_secs(),
                    );
                } else {
                    log::info!(
                        "MustScanSubDirs: reconcile complete for {} (+{} -{} ~{}, {}ms)",
                        path.display(),
                        summary.added,
                        summary.removed,
                        summary.updated,
                        summary.duration.as_millis(),
                    );
                }
            }
            Err(e) => {
                log::warn!("MustScanSubDirs: reconcile failed for {}: {e}", path.display());
            }
        }

        DEBUG_STATS.record_rescan_completed();
        active_for_task.store(false, Ordering::Relaxed);

        // Automatically start the next queued rescan
        start_next_rescan(pending_for_task, active_for_task, &writer);
    });
}

// ── Subtree reconciliation ───────────────────────────────────────────

/// Summary of a subtree reconciliation.
pub(super) struct ReconcileSummary {
    pub added: u64,
    pub removed: u64,
    pub updated: u64,
    pub duration: std::time::Duration,
}

/// A live directory child, normalized to the fields the per-dir diff needs.
/// The two walk sources build this identically: the local `read_dir` path from a
/// `std::fs::Metadata`, the network path from a `Volume` listing's `FileEntry`.
/// The diff is then source-agnostic — the same add/remove/modify/type-change
/// logic for both.
pub(super) struct LiveChild {
    pub name: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub snap: crate::indexing::metadata::MetadataSnapshot,
}

/// Outcome of diffing ONE directory's live children against its DB rows.
pub(super) struct DirDiff {
    pub added: u64,
    pub removed: u64,
    pub updated: u64,
    /// `(child_dir_id, child_name)` for every EXISTING child dir that matched a
    /// live dir — the caller recurses into these (their id is already known).
    /// This is UNCONDITIONAL of whether the dir changed: an unchanged dir still
    /// recurses, because "unchanged at the parent's level" says nothing about
    /// whether its own subtree was ever listed. Gating this on `changed` is the
    /// reconcile-stops-at-the-root bug (see the fn doc).
    pub matched_child_dirs: Vec<(i64, String)>,
    /// Names of NEW child dirs created this pass — the caller flushes the writer,
    /// resolves their ids, then recurses (the id isn't known until the insert
    /// commits).
    pub new_child_dir_names: Vec<String>,
}

/// Diff one directory's live listing against its DB children and emit only the
/// differences (`UpsertEntryV2` for adds + changes, `DeleteEntryById` /
/// `DeleteSubtreeById` for vanished rows). Shared by the local `read_dir` walk
/// and the network `Volume`-trait walk so the diff logic lives in ONE place.
///
/// Writes NOTHING for an unchanged row (the no-op-cheap property the perf bench
/// relied on): a matched row is re-UPSERTed only when its size/mtime (file) or
/// mtime (dir/symlink) actually differs, so a rescan over an unchanged tree
/// issues zero entry-row writes and never touches the catastrophic
/// `INSERT OR REPLACE`/`platform_case` path.
///
/// The recursion set (`matched_child_dirs`) is DECOUPLED from that write
/// decision: every matched child dir is returned for the caller to descend into,
/// changed or not. The walk must re-list each existing child dir's subtree on a
/// reconcile — a child being unchanged at THIS dir's level proves nothing about
/// whether its subtree was ever scanned. (Re-gating recursion on `changed` is the
/// reconcile-stops-at-the-root prod bug: a share with only its top dirs indexed
/// would match them, write nothing, recurse nowhere, and "complete" instantly
/// over an unscanned tree.)
pub(super) fn diff_dir_against_db(
    dir_id: i64,
    live_children: &[LiveChild],
    db_children: &[store::EntryRow],
    writer: &IndexWriter,
) -> DirDiff {
    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;
    let mut matched_child_dirs: Vec<(i64, String)> = Vec::new();
    let mut new_child_dir_names: Vec<String> = Vec::new();

    let mut db_by_name: std::collections::HashMap<String, &store::EntryRow> =
        std::collections::HashMap::with_capacity(db_children.len());
    for row in db_children {
        db_by_name.insert(store::normalize_for_comparison(&row.name), row);
    }

    let mut matched_db_keys: HashSet<String> = HashSet::with_capacity(live_children.len());

    for child in live_children {
        let norm_name = store::normalize_for_comparison(&child.name);
        let is_dir = child.is_directory;
        let is_symlink = child.is_symlink;
        let snap = &child.snap;

        if let Some(db_row) = db_by_name.get(&norm_name) {
            matched_db_keys.insert(norm_name);

            let changed = if is_dir || is_symlink {
                snap.modified_at != db_row.modified_at
            } else {
                snap.logical_size != db_row.logical_size || snap.modified_at != db_row.modified_at
            };

            if changed {
                // Type change (file↔dir): delete first so counts propagate correctly.
                // Dir→file: DeleteSubtreeById removes children + propagates negative deltas.
                // File→dir: DeleteEntryById propagates negative file_count delta.
                // Then UpsertEntryV2 inserts fresh with the correct type and positive deltas.
                if db_row.is_directory != is_dir {
                    if db_row.is_directory {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(db_row.id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(db_row.id));
                    }
                }

                let _ = writer.send(WriteMessage::UpsertEntryV2 {
                    parent_id: dir_id,
                    name: child.name.clone(),
                    is_directory: is_dir,
                    is_symlink,
                    logical_size: snap.logical_size,
                    physical_size: snap.physical_size,
                    modified_at: snap.modified_at,
                    inode: snap.inode,
                    nlink: snap.nlink,
                });
                updated += 1;
            }

            // Recurse into this child if it's a (non-symlink) dir on disk:
            // - was already a dir in the DB → recurse the existing id;
            // - was a file, now a dir (type change) → the old row was deleted and
            //   a fresh dir inserted above, so treat it like a new dir: resolve the
            //   new id after a flush, then recurse (its children must be walked).
            if is_dir && !is_symlink {
                if db_row.is_directory {
                    matched_child_dirs.push((db_row.id, child.name.clone()));
                } else {
                    new_child_dir_names.push(child.name.clone());
                }
            }
        } else {
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: dir_id,
                name: child.name.clone(),
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });
            // UpsertEntryV2 auto-propagates deltas in the writer.
            added += 1;

            if is_dir && !is_symlink {
                new_child_dir_names.push(child.name.clone());
            }
        }
    }

    for row in db_children {
        let norm_name = store::normalize_for_comparison(&row.name);
        if !matched_db_keys.contains(&norm_name) {
            if row.is_directory {
                let _ = writer.send(WriteMessage::DeleteSubtreeById(row.id));
            } else {
                let _ = writer.send(WriteMessage::DeleteEntryById(row.id));
            }
            removed += 1;
        }
    }

    DirDiff {
        added,
        removed,
        updated,
        matched_child_dirs,
        new_child_dir_names,
    }
}

// ── Full-rescan finish (shared) ──────────────────────────────────────

/// Number of dir ids per `MarkDirsListed` message. Bounds each message's size;
/// the writer-side `IndexStore::mark_dirs_listed` chunks the SQL `UPDATE` further
/// (at 900, under SQLite's bound-parameter ceiling) inside a savepoint, so this is
/// only message-level batching — never load-bearing for SQL correctness.
const MARK_CHUNK: usize = 10_000;

/// Emit `MarkDirsListed` for every successfully-listed dir id, chunked. A no-op
/// when empty. The only failure is a writer send (the writer thread is gone); the
/// caller maps that into its own error type.
pub(super) fn send_marks(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) -> Result<(), IndexStoreError> {
    for chunk in listed_ids.chunks(MARK_CHUNK) {
        writer.send(WriteMessage::MarkDirsListed {
            ids: chunk.to_vec(),
            epoch,
        })?;
    }
    Ok(())
}

/// The full-rescan FINISH, in ONE place so the network and (future) local
/// full-tree reconcile can't drift on the ordering invariant: stamp every
/// successfully-listed dir's `listed_epoch` FIRST, then run a SINGLE
/// `ComputeAllAggregates`.
///
/// The mark-before-aggregate order is load-bearing: aggregating
/// before the marks rolls the whole tree to `min_subtree_epoch = 0` (incomplete);
/// a mark queued after the aggregate drags that dir's ancestors to incomplete. The
/// single in-order writer guarantees the order once sequenced here.
///
/// This is the single-aggregate coverage refresh the full-rescan path uses, NOT
/// the per-dir `PropagateMinSubtreeEpoch` propagation `reconcile_subtree` runs (a
/// ~2.4x regression at full scale; that stays the small-scope live path). A no-op
/// reconcile still runs the aggregate (cheap O(dirs) bulk SQL since no
/// `InsertEntriesV2` ran) so coverage re-stamps to the new epoch; it writes no
/// entry rows. The only failure is a writer send; the caller maps it.
pub(super) fn finish_reconcile(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) -> Result<(), IndexStoreError> {
    send_marks(listed_ids, epoch, writer)?;
    writer.send(WriteMessage::ComputeAllAggregates)?;
    Ok(())
}

/// RAII bracket for a FULL reconcile's bulk walk: tells the writer to STOP
/// per-entry ancestor `dir_stats` propagation for the walk's duration, then
/// RESTORES it on EVERY scope exit (clean finish, cancel, empty-root, error, or
/// panic) so the shared, long-lived writer is never left non-propagating for the
/// LIVE event loop that runs afterwards.
///
/// Why suppress: the full reconcile emits thousands of `UpsertEntryV2` / `Delete*`;
/// letting each one walk the ancestor chain
/// (`propagate_delta_by_id` / `propagate_min_subtree_epoch` /
/// `propagate_recursive_has_symlinks`) is O(entries × tree-depth) and wedges the
/// writer for hours on a large delta. It's also pure waste: `finish_reconcile`'s
/// single `ComputeAllAggregates` recomputes every dir's stats from the entries
/// table, overwriting whatever per-entry propagation produced. The LIVE path
/// (`reconcile_subtree`, FSEvents) has NO final aggregate, so it MUST keep
/// propagating — which is exactly why this guard restores the default on exit.
///
/// Both sends are best-effort: on a hard writer-gone error the restore send fails
/// and is ignored, matching how the surrounding walk already treats writer sends.
pub(super) struct BulkReconcileGuard {
    writer: IndexWriter,
}

impl BulkReconcileGuard {
    /// Begin the bracket: disable per-entry propagation on the writer.
    pub(super) fn begin(writer: &IndexWriter) -> Self {
        let _ = writer.send(WriteMessage::SetDeltaPropagation(false));
        Self { writer: writer.clone() }
    }
}

impl Drop for BulkReconcileGuard {
    fn drop(&mut self) {
        // Re-enable per-entry propagation for the subsequent live path.
        let _ = self.writer.send(WriteMessage::SetDeltaPropagation(true));
    }
}

/// Reconcile a subtree by diffing the filesystem against the DB directory-by-directory.
///
/// Unlike `scanner::scan_subtree` which deletes all descendants then re-inserts,
/// this function walks each directory, compares children by name, and only writes
/// the differences. Safe to interrupt at any point: the DB is never in a
/// partially-deleted state.
///
/// This is the LIVE small-scope fill path (per-navigation verifier,
/// `MustScanSubDirs`, SMB-overflow `FullRefresh`): it propagates coverage per
/// listed dir. The full-rescan path does NOT use this — the network rescan walks
/// via `volume_scanner::reconcile_volume_via_trait`, which reuses the shared
/// [`diff_dir_against_db`] but stamps + runs ONE `ComputeAllAggregates` (the
/// single-aggregate constraint the perf bench measured), never per-dir propagation.
pub(super) fn reconcile_subtree(
    root: &Path,
    conn: &Connection,
    writer: &IndexWriter,
    cancelled: &AtomicBool,
) -> Result<ReconcileSummary, String> {
    let start = Instant::now();
    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;

    // The epoch every dir we successfully list this pass is stamped with. A
    // reconcile *stamps* with the current epoch; it never bumps it. Read once.
    let epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
    // Every dir whose direct contents we successfully list (incl. empty), so we
    // can `MarkDirsListed` them after the walk and lift ancestor coverage.
    // Without this, a reconcile-discovered subtree stays `listed_epoch = 0`
    // forever and drags every ancestor to incomplete — the exact local-live-path
    // regression this milestone guards against.
    let mut listed_dir_ids: Vec<i64> = Vec::new();

    let root_str = firmlinks::normalize_path(&root.to_string_lossy());
    let root_id = match store::resolve_path(conn, &root_str) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Root not in DB. This happens when must_scan_sub_dirs fires for a
            // newly created/copied directory. Try to create it: resolve the parent,
            // stat the root, and upsert it via the writer.
            let parent_path = compute_parent_path(&root_str);
            let parent_id = match store::resolve_path(conn, &parent_path) {
                Ok(Some(id)) => id,
                Ok(None) => {
                    log::debug!("reconcile_subtree: neither root nor parent in DB, skipping: {root_str}");
                    return Ok(ReconcileSummary {
                        added: 0,
                        removed: 0,
                        updated: 0,
                        duration: start.elapsed(),
                    });
                }
                Err(e) => return Err(format!("resolve_path for parent: {e}")),
            };

            // Stat the root directory and upsert it
            let metadata = match std::fs::symlink_metadata(root) {
                Ok(m) => m,
                Err(e) => {
                    log::debug!("reconcile_subtree: can't stat root {root_str}: {e}");
                    return Ok(ReconcileSummary {
                        added: 0,
                        removed: 0,
                        updated: 0,
                        duration: start.elapsed(),
                    });
                }
            };

            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let snap = extract_metadata(&metadata, metadata.is_dir(), metadata.is_symlink());
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });

            // Flush so the read connection can see the new entry
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush after root upsert failed: {e}");
            }
            added += 1;

            match store::resolve_path(conn, &root_str) {
                Ok(Some(id)) => id,
                Ok(None) => {
                    log::warn!("reconcile_subtree: root still not in DB after upsert, skipping: {root_str}");
                    return Ok(ReconcileSummary {
                        added,
                        removed: 0,
                        updated: 0,
                        duration: start.elapsed(),
                    });
                }
                Err(e) => return Err(format!("resolve_path for root after upsert: {e}")),
            }
        }
        Err(e) => return Err(format!("resolve_path for root: {e}")),
    };

    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), root_id));

    // Collect newly-created directories so we can flush the writer, resolve their IDs,
    // and then queue them for recursive processing.
    let mut new_dir_paths: Vec<PathBuf> = Vec::new();

    while let Some((dir_path, dir_id)) = queue.pop_front() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let fs_children = match read_fs_children(&dir_path) {
            Some(c) => c,
            None => continue,
        };
        // We successfully listed this dir's direct contents (an empty listing
        // still counts). Stamp it at the current epoch after the walk.
        listed_dir_ids.push(dir_id);

        let db_children =
            IndexStore::list_children_on(dir_id, conn).map_err(|e| format!("list_children_on({dir_id}): {e}"))?;

        // Normalize the local listing into source-agnostic `LiveChild`s and run
        // the shared per-dir diff (same logic the network walk uses).
        let live_children: Vec<LiveChild> = fs_children
            .iter()
            .map(|(name, meta, is_symlink)| {
                let is_dir = meta.is_dir();
                LiveChild {
                    name: name.clone(),
                    is_directory: is_dir,
                    is_symlink: *is_symlink,
                    snap: extract_metadata(meta, is_dir, *is_symlink),
                }
            })
            .collect();

        let diff = diff_dir_against_db(dir_id, &live_children, &db_children, writer);
        added += diff.added;
        removed += diff.removed;
        updated += diff.updated;
        for (child_id, child_name) in diff.matched_child_dirs {
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            new_dir_paths.push(dir_path.join(child_name));
        }

        // If we found new directories and the queue is empty (current level done),
        // flush the writer so the read connection can resolve the new IDs.
        if !new_dir_paths.is_empty() && queue.is_empty() {
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush failed: {e}");
            }
            for new_dir in new_dir_paths.drain(..) {
                let path_str = firmlinks::normalize_path(&new_dir.to_string_lossy());
                if let Ok(Some(id)) = store::resolve_path(conn, &path_str) {
                    queue.push_back((new_dir, id));
                }
            }
        }
    }

    // Stamp every dir we listed at the current epoch, then lift ancestor
    // coverage. The walk collected ids shallow→deep (BFS), so recompute
    // deepest-first: a parent's `min_subtree_epoch` reads its children's stored
    // values, which must already reflect this pass. `propagate_min_subtree_epoch`
    // short-circuits once a value stabilizes, so the repeated up-walks are cheap.
    // (This is the SMALL-SCOPE live path; a full rescan uses the single-aggregate
    // path in `volume_scanner`, not per-dir propagation — see the fn doc.)
    if !listed_dir_ids.is_empty() {
        // Chunk under SQLite's bound-parameter ceiling (+1 for the epoch param).
        const MARK_CHUNK: usize = 900;
        for chunk in listed_dir_ids.chunks(MARK_CHUNK) {
            if let Err(e) = writer.send(WriteMessage::MarkDirsListed {
                ids: chunk.to_vec(),
                epoch,
            }) {
                log::warn!("reconcile_subtree: failed to send MarkDirsListed: {e}");
            }
        }
        for dir_id in listed_dir_ids.iter().rev() {
            let _ = writer.send(WriteMessage::PropagateMinSubtreeEpoch(*dir_id));
        }
    }

    Ok(ReconcileSummary {
        added,
        removed,
        updated,
        duration: start.elapsed(),
    })
}

/// Read and filter filesystem children of a directory.
///
/// Shared by both local reconcile walks — the small-scope live `reconcile_subtree`
/// and the full-tree [`local_reconcile`](crate::indexing::local_reconcile) rescan.
/// Returns `None` when the directory itself can't be listed (a permission wall or a
/// vanished dir), distinct from `Some(vec![])` for an empty-but-readable dir.
///
/// Applies the SAME two filters the jwalk fresh scan uses, so a reconcile converges
/// to the identical DB the fresh scan would build:
/// - `scanner::should_exclude` (system/virtual prefixes, `/Volumes/`, the E2E
///   allowlist), and
/// - `scanner::is_canonicalization_alias` — the macOS root symlinks `/tmp`, `/var`,
///   `/etc` normalize to `/private/...`, so they're aliases of a real directory the
///   fresh scan stored under the canonical path. The fresh scan skips the alias
///   (`scanner::run_scan`); a reconcile that DIDN'T would find them absent from the
///   DB and re-add them every pass, diverging from the fresh-scan tree.
///   Skipping here keeps fresh and reconcile in lock-step.
pub(super) fn read_fs_children(dir_path: &Path) -> Option<Vec<(String, std::fs::Metadata, bool)>> {
    let read_dir = match std::fs::read_dir(dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            log::debug!("reconcile: can't read {}: {e}", dir_path.display());
            return None;
        }
    };

    let mut children = Vec::new();
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let child_path = dir_path.join(&name);
        let child_path_str = child_path.to_string_lossy();
        let normalized_child = firmlinks::normalize_path(&child_path_str);
        // The local reconcile runs only on the boot disk today, so `BootDisk`;
        // the mount-rooted reconcile threads the kind-derived scope here.
        if scanner::should_exclude(&normalized_child, scanner::ExclusionScope::BootDisk) {
            continue;
        }
        // Skip the canonicalization-alias symlinks (/tmp, /var, /etc) so we don't
        // re-add what the fresh scan deliberately stored under the canonical
        // /private/... path.
        if scanner::is_canonicalization_alias(&child_path_str, &normalized_child) {
            continue;
        }
        if let Ok(meta) = std::fs::symlink_metadata(&child_path) {
            let is_symlink = meta.is_symlink();
            children.push((name, meta, is_symlink));
        }
    }
    Some(children)
}

// ── Event processing ─────────────────────────────────────────────────

/// Process a single filesystem event. Returns affected parent paths for UI notification.
///
/// Shared between replay and live mode. Normalizes paths, checks exclusions,
/// stats the file, resolves paths to integer entry IDs, and sends appropriate
/// integer-keyed write messages (`UpsertEntryV2`, `DeleteEntryById`, etc.).
///
/// `throttle` is `Some` ONLY on the live path (`process_live_event`). Replay and
/// cold-start pass `None` so journal catch-up applies every event immediately.
/// When present, a regular file's in-place rewrite may be suppressed here (its
/// last-seen size flushed later by [`EventReconciler::sweep_throttle`]).
pub(super) fn process_fs_event(
    event: &FsChangeEvent,
    conn: &Connection,
    writer: &IndexWriter,
    throttle: Option<&mut LiveThrottle>,
) -> Option<Vec<String>> {
    let normalized = firmlinks::normalize_path(&event.path);

    // Skip excluded paths. The live local event loop runs only on the boot disk
    // today, so `BootDisk`; the mount-relative live pipeline threads the
    // kind-derived scope here (a mount-rooted drive uses `MountRooted`).
    if scanner::should_exclude(&normalized, scanner::ExclusionScope::BootDisk) {
        return None;
    }

    // Skip HistoryDone marker events
    if event.flags.history_done {
        return None;
    }

    let parent_path = compute_parent_path(&normalized);
    let mut affected = collect_ancestor_paths(&normalized);

    if event.flags.item_removed {
        return handle_removal(&normalized, conn, event, writer, affected, throttle);
    }

    if event.flags.item_created || event.flags.item_modified || event.flags.item_renamed {
        return handle_creation_or_modification(
            &normalized,
            &parent_path,
            conn,
            event,
            writer,
            &mut affected,
            throttle,
        );
    }

    // For other flag combinations (xattr, owner change, etc.), just stat and update
    if event.flags.item_is_file || event.flags.item_is_dir {
        return handle_creation_or_modification(
            &normalized,
            &parent_path,
            conn,
            event,
            writer,
            &mut affected,
            throttle,
        );
    }

    None
}

/// Handle a file/directory removal event.
///
/// FSEvents can deliver `item_removed` for paths that still exist on disk
/// (e.g., atomic file swaps, coalesced events with OR'd flags). To avoid
/// deleting live entries, we stat the path first: if it exists, delegate to
/// `handle_creation_or_modification` (which upserts). Only delete from the DB
/// when the path is truly gone from the filesystem.
fn handle_removal(
    normalized: &str,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    mut affected: Vec<String>,
    throttle: Option<&mut LiveThrottle>,
) -> Option<Vec<String>> {
    // Check if the path actually exists on disk before deleting from the DB.
    if Path::new(normalized).symlink_metadata().is_ok() {
        // Path still exists, so treat as a modification, not a removal (throttled
        // like any other in-place rewrite). Deletes themselves are never throttled.
        let parent_path = compute_parent_path(normalized);
        return handle_creation_or_modification(normalized, &parent_path, conn, event, writer, &mut affected, throttle);
    }

    // Path is truly gone; resolve and delete from DB
    let entry_id = match store::resolve_path(conn, normalized) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Per-event line at TRACE: useful when actively debugging reconciler/index
            // drift, but ~90% of normal log volume comes from build-output churn that's
            // genuinely harmless. The aggregate at DEBUG (below) gives the existence-of-
            // drift signal without flooding the file.
            log::trace!("Reconciler: removal for unknown path, skipping: {normalized}");
            skip_aggregator::UNKNOWN_PATH_SKIPS.record(normalized);
            return Some(affected);
        }
        Err(e) => {
            log::warn!("Reconciler: resolve_path failed for removal {normalized}: {e}");
            return Some(affected);
        }
    };

    if event.flags.item_is_dir {
        let _ = writer.send(WriteMessage::DeleteSubtreeById(entry_id));
    } else {
        let _ = writer.send(WriteMessage::DeleteEntryById(entry_id));
    }

    Some(affected)
}

/// Handle file/directory creation, modification, or rename.
///
/// Resolves the parent path to an integer ID and sends `UpsertEntryV2`.
/// For new entries (create), also sends `PropagateDeltaById` starting
/// from the parent so dir_stats are updated along the ancestor chain.
fn handle_creation_or_modification(
    normalized: &str,
    parent_path: &str,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    affected: &mut Vec<String>,
    throttle: Option<&mut LiveThrottle>,
) -> Option<Vec<String>> {
    // Stat the file to get current metadata
    let path = Path::new(normalized);
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            // Path doesn't exist (deleted since event was generated).
            // Treat as a removal: resolve to entry ID and send integer-keyed delete.
            // Use DeleteSubtreeById for directories to also remove child entries;
            // journal replay may coalesce child events into a parent dir event,
            // leaving orphaned children without a subtree delete.
            match store::resolve_path(conn, normalized) {
                Ok(Some(id)) => {
                    if event.flags.item_is_dir {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(id));
                    }
                }
                Ok(None) => {
                    // Not in DB either -- nothing to do
                }
                Err(e) => {
                    log::warn!("Reconciler: resolve_path failed for gone path {normalized}: {e}");
                }
            }
            return Some(affected.clone());
        }
    };

    // Resolve parent path to integer ID
    let parent_id = match store::resolve_path(conn, parent_path) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Parent not in DB -- stale event (intermediate directory missing), skip.
            // Per-event at TRACE; a DEBUG aggregate every ~5 s keeps the drift signal in
            // error reports without the per-event flood (this was ~13% of normal log
            // volume, almost all harmless build-output churn).
            log::trace!("Reconciler: parent path not in DB, skipping event for {normalized} (parent: {parent_path})");
            skip_aggregator::STALE_PARENT_SKIPS.record(normalized);
            return Some(affected.clone());
        }
        Err(e) => {
            log::warn!("Reconciler: resolve_path failed for parent {parent_path}: {e}");
            return Some(affected.clone());
        }
    };

    let is_dir = metadata.is_dir();
    let is_symlink = metadata.is_symlink();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let snap = extract_metadata(&metadata, is_dir, is_symlink);

    // Live-path throttle: a regular file rewritten in place may be suppressed so
    // rapid rewrites collapse to ≤1 index write per THROTTLE_WINDOW. Only files
    // (dirs/symlinks carry no size), never on replay (`throttle` is None), and
    // never under the user's Downloads (active downloads want a live size). The
    // trailing flush that applies the suppressed size runs from the sweep tick.
    let is_regular_file = !is_dir && !is_symlink;
    let suppress = match throttle {
        Some(t) if is_regular_file && !t.is_exempt(normalized) => {
            let payload = PendingUpsert {
                parent_id,
                name: name.clone(),
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            };
            matches!(
                t.on_change(normalized, snap.logical_size.unwrap_or(0), payload, Instant::now()),
                ThrottleOutcome::Suppress
            )
        }
        _ => false,
    };

    if suppress {
        // Nothing written, so no dir_stats changed: don't notify ancestors. The
        // last-seen size is applied by the trailing-flush sweep.
        return Some(Vec::new());
    }

    let _ = writer.send(WriteMessage::UpsertEntryV2 {
        parent_id,
        name,
        is_directory: is_dir,
        is_symlink,
        logical_size: snap.logical_size,
        physical_size: snap.physical_size,
        modified_at: snap.modified_at,
        inode: snap.inode,
        nlink: snap.nlink,
    });

    // UpsertEntryV2 auto-propagates deltas in the writer, so no separate
    // PropagateDeltaById needed here.

    // For new directories, add them to affected paths for downstream processing
    if event.flags.item_created && is_dir {
        affected.push(normalized.to_string());
    }

    Some(affected.clone())
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Resolve the user's Downloads directory to a normalized prefix, so the live
/// throttle can exempt it (active downloads want a live size). Resolved once at
/// reconciler construction via the OS dir API, not a hardcoded string; `None`
/// when the OS reports no Downloads dir (rare). Normalized the same way live
/// event paths are, so the prefix comparison lines up. This is purely a
/// "don't throttle" flag — it reads no new metadata, so adds no TCC surface.
fn resolve_downloads_prefix() -> Option<String> {
    dirs::download_dir().map(|p| firmlinks::normalize_path(&p.to_string_lossy()))
}

/// Compute parent path from a normalized path.
fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Collect all ancestor paths from the immediate parent up to "/".
/// Used to notify the frontend that dir_stats changed along the entire ancestor chain
/// (since propagate_delta updates all ancestors, not just the direct parent).
fn collect_ancestor_paths(path: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut current = path.to_string();
    loop {
        let parent = compute_parent_path(&current);
        if parent.is_empty() || parent == current {
            break;
        }
        ancestors.push(parent.clone());
        if parent == "/" {
            break;
        }
        current = parent;
    }
    ancestors
}

/// Emit an `index-dir-updated` event to the frontend.
pub(super) fn emit_dir_updated(app: &AppHandle, paths: Vec<String>) {
    use tauri_specta::Event;
    let _ = crate::indexing::IndexDirUpdatedEvent { paths }.emit(app);
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
