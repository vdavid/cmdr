//! Single-writer thread for all SQLite index writes.
//!
//! All writes go through a dedicated `std::thread` that owns the write connection.
//! This eliminates contention between the full scan, subtree scans, and watcher updates.
//! Reads happen on separate connections (WAL mode allows concurrent reads).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::indexing::aggregator::AggregationPhase;
use crate::indexing::store::{EntryRow, IndexStore, IndexStoreError};
use crate::pluralize::{pluralize, pluralize_with};

mod aggregation;
mod delta;
mod entries;
mod maintenance;

use aggregation::{
    handle_backfill_missing_dir_stats, handle_compute_all_aggregates, handle_compute_partial_aggregates,
    handle_compute_subtree_aggregates,
};
use delta::propagate_delta_by_id;
use entries::{
    handle_delete_entry_by_id, handle_delete_subtree_by_id, handle_insert_entries_v2, handle_move_entry_v2,
    handle_truncate_data, handle_upsert_entry_v2,
};
use maintenance::{handle_incremental_vacuum, handle_wal_checkpoint};

// ── Aggregation progress events ──────────────────────────────────────

/// Tauri event payload for aggregation progress updates.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "index-aggregation-progress")]
#[serde(rename_all = "camelCase")]
pub struct AggregationProgressEvent {
    /// One of `phase_to_str`'s outputs: `saving_entries` | `loading` | `sorting` | `computing` | `writing`.
    pub phase: String,
    pub current: u64,
    pub total: u64,
}

pub(super) fn phase_to_str(phase: AggregationPhase) -> &'static str {
    match phase {
        AggregationPhase::SavingEntries => "saving_entries",
        AggregationPhase::LoadingDirectories => "loading",
        AggregationPhase::Sorting => "sorting",
        AggregationPhase::Computing => "computing",
        AggregationPhase::Writing => "writing",
    }
}

// ── Writer generation (for search index staleness detection) ─────────

/// Monotonically increasing generation counter, bumped on every mutation
/// (`InsertEntriesV2`, `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`,
/// `TruncateData`). The search index stores the generation it was loaded at;
/// a mismatch triggers a background reload. Initialized to 1 (not 0) to avoid
/// ambiguity with a freshly constructed search index.
pub(crate) static WRITER_GENERATION: AtomicU64 = AtomicU64::new(1);

// ── Messages ─────────────────────────────────────────────────────────

/// Capacity of the bounded writer channel. When full, senders block,
/// providing natural backpressure instead of unbounded memory growth.
const WRITER_CHANNEL_CAPACITY: usize = 20_000;

/// Messages sent to the writer thread via a bounded mpsc channel.
pub enum WriteMessage {
    /// Full scan: batch of entries with pre-assigned integer IDs.
    InsertEntriesV2(Vec<EntryRow>),
    /// Watcher/reconciler: upsert a single entry by parent_id + name.
    /// The writer resolves or inserts using integer keys.
    UpsertEntryV2 {
        parent_id: i64,
        name: String,
        is_directory: bool,
        is_symlink: bool,
        logical_size: Option<u64>,
        physical_size: Option<u64>,
        modified_at: Option<u64>,
        inode: Option<u64>,
        nlink: Option<u64>,
    },
    /// Live event loop's rename pre-pass: move an existing entry to a new
    /// `(parent_id, name)`, preserving its `entry_id` (and therefore any
    /// `dir_stats` for directories). Detected by inode match against the
    /// post-rename path. Cross-parent moves propagate the entry's contribution
    /// down the old ancestor chain and up the new one. Same-parent renames
    /// don't change ancestor totals so no propagation is needed.
    MoveEntryV2 {
        entry_id: i64,
        new_parent_id: i64,
        new_name: String,
    },
    /// Watcher: delete a single entry and its dir_stats by entry ID.
    DeleteEntryById(i64),
    /// Watcher: delete a subtree (directory removed with all children) by entry ID.
    DeleteSubtreeById(i64),
    /// Scanner: delete all descendants of an entry before a subtree rescan.
    /// Prevents orphaned entries when re-scanning an already-indexed subtree.
    DeleteDescendantsById(i64),
    /// Watcher: incremental delta propagation walking the parent_id chain.
    PropagateDeltaById {
        entry_id: i64,
        logical_size_delta: i64,
        physical_size_delta: i64,
        file_count_delta: i32,
        dir_count_delta: i32,
    },
    /// Full scan complete: trigger bottom-up aggregation for all directories.
    ComputeAllAggregates,
    /// Mid-scan: compute partial recursive sizes from the accumulator maps as
    /// they stand, and write a bounded subset of dir_stats rows so visible
    /// listings can show growing sizes during the scan. Borrows the maps
    /// read-only; MUST NOT clear or mutate them (the final ComputeAllAggregates
    /// depends on them). No SQL fallback when maps are empty: empty maps mid-scan
    /// mean "nothing scanned yet", so the correct action is a no-op (unlike
    /// ComputeAllAggregates, whose SQL fallback exists for the maps-lost edge case).
    ComputePartialAggregates {
        /// Directories whose children should be written regardless of depth,
        /// because a pane is currently showing them ("hot" paths). Already
        /// firmlink-normalized by the sender.
        hot_paths: Vec<String>,
    },
    /// Subtree scan complete: trigger aggregation for a subtree only.
    ComputeSubtreeAggregates { root: String },
    /// Store the last processed FSEvents event ID.
    UpdateLastEventId(u64),
    /// Update a meta key.
    UpdateMeta { key: String, value: String },
    /// Delete a meta key (no-op if absent). Used at scan start to clear the
    /// previous `scan_completed_at` so a killed rescan heals to a fresh scan
    /// instead of replaying on top of a gutted index. Not search-relevant, so
    /// (like `UpdateMeta`) it does NOT bump the writer generation.
    DeleteMeta(String),
    /// Request current entry count (for progress reporting).
    #[cfg(test)]
    GetEntryCount(oneshot::Sender<Result<u64, IndexStoreError>>),
    /// Flush: confirms all prior messages have been committed.
    /// The writer responds through the channel after processing this message.
    Flush(oneshot::Sender<()>),
    /// Truncate `entries` and `dir_stats` tables, preserving `meta`.
    /// Used before a full rescan so the scan starts from a clean slate.
    TruncateData,
    /// Begin an explicit SQLite transaction.
    /// All subsequent writes are batched until `CommitTransaction`.
    /// Dramatically reduces fsync overhead for bulk operations (replay).
    BeginTransaction,
    /// Commit the current explicit transaction.
    CommitTransaction,
    /// Backfill dir_stats for directories that have entries but no stats row.
    /// Happens after reconciler replay or cold-start replay to catch dirs
    /// created by events that ran after the last full aggregation.
    BackfillMissingDirStats,
    /// Periodic housekeeping: reclaim free pages from deletes/rescans.
    /// Sent by a background timer, not counted in WriterStats.
    IncrementalVacuum,
    /// Periodic housekeeping: TRUNCATE the WAL file once readers permit, so the
    /// post-scan high-water mark doesn't sit on disk indefinitely. Sent by the
    /// same background timer as `IncrementalVacuum`, and also fired explicitly
    /// after a full scan's `ComputeAllAggregates` so the scan-time spike doesn't
    /// wait up to 30 s before being trimmed. Not counted in WriterStats.
    WalCheckpoint,
    /// Emit `index-dir-updated` for the given paths. Enqueued after a batch
    /// of writes so the UI notification fires only after all prior messages
    /// (deletes, upserts, deltas) are committed.
    EmitDirUpdated(Vec<String>),
    /// Shut down the writer thread.
    Shutdown,
}

// ── IndexWriter handle ───────────────────────────────────────────────

/// Handle for sending messages to the writer thread.
///
/// Cloneable; all clones share the same underlying channel.
#[derive(Clone)]
pub struct IndexWriter {
    sender: mpsc::SyncSender<WriteMessage>,
    /// Handle for the writer thread, shared so shutdown() can join it.
    thread_handle: Arc<std::sync::Mutex<Option<thread::JoinHandle<()>>>>,
    /// Path to the database file (needed by scanner for ScanContext init).
    db_path: PathBuf,
    /// Expected total entries from the scan, set by the caller when the scan
    /// completes. The writer thread reads this to report flushing progress as
    /// it processes remaining `InsertEntriesV2` batches.
    expected_total_entries: Arc<AtomicU64>,
    /// Shared ID counter for entry allocation. The scanner atomically increments
    /// this to get unique IDs, and the writer bumps it after `UpsertEntryV2` inserts
    /// (which let SQLite auto-assign). Reset to 2 on `TruncateData`.
    next_id: Arc<AtomicI64>,
    /// Per-writer mutation counter, ticked alongside the global `WRITER_GENERATION`
    /// at every mutating message. Tests use this instead of the global so an
    /// assertion of "did this writer mutate?" isn't disturbed by other concurrent
    /// writers (cargo test runs tests as threads in one process). Production code
    /// should keep using `WRITER_GENERATION`.
    #[cfg_attr(not(test), allow(dead_code, reason = "test-only observable"))]
    mutation_counter: Arc<AtomicU64>,
    /// Phase 1 instrumentation: best-effort estimate of channel depth.
    /// Incremented on each `send()`; the writer thread decrements it after each `recv()`.
    /// Used by the heartbeat (writer thread) to log queue pressure.
    queue_depth: Arc<AtomicUsize>,
}

impl IndexWriter {
    /// Spawn the writer thread with its own write connection.
    ///
    /// Opens a WAL-mode write connection to the DB at `db_path`, spawns a
    /// `std::thread` (blocking I/O, not tokio), and returns a handle.
    /// If `app_handle` is provided, the writer emits `index-aggregation-progress`
    /// events during `ComputeAllAggregates`.
    pub fn spawn(db_path: &Path, app_handle: Option<AppHandle>) -> Result<Self, IndexStoreError> {
        let conn = IndexStore::open_write_connection(db_path)?;
        // SQLite busy retry logger. Brief contention is routine (WAL checkpoints, long-lived
        // readers), so per-attempt logging stays at debug; sustained contention (>=20 attempts
        // = >100ms lock wait) is a genuine stall signal and logs at warn.
        conn.busy_handler(Some(|attempt: i32| {
            if attempt >= 20 {
                log::warn!(target: "stall_probe::sqlite_busy", "writer busy_handler attempt={attempt}");
            } else {
                log::debug!(target: "stall_probe::sqlite_busy", "writer busy_handler attempt={attempt}");
            }
            // Same back-off behaviour as default busy timeout (sleep up to ~100ms).
            if attempt > 50 {
                false
            } else {
                thread::sleep(Duration::from_millis(5));
                true
            }
        }))?;

        let initial_next_id = IndexStore::get_next_id(&conn)?;
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(WRITER_CHANNEL_CAPACITY);
        let expected_total_entries = Arc::new(AtomicU64::new(0));
        let expected_total_clone = Arc::clone(&expected_total_entries);
        let next_id = Arc::new(AtomicI64::new(initial_next_id));
        let next_id_clone = Arc::clone(&next_id);
        let mutation_counter = Arc::new(AtomicU64::new(0));
        let mutation_counter_clone = Arc::clone(&mutation_counter);
        let queue_depth = Arc::new(AtomicUsize::new(0));
        let queue_depth_clone = Arc::clone(&queue_depth);

        let handle = thread::Builder::new()
            .name("index-writer".into())
            .spawn(move || {
                writer_loop(
                    conn,
                    receiver,
                    app_handle,
                    expected_total_clone,
                    next_id_clone,
                    mutation_counter_clone,
                    queue_depth_clone,
                )
            })
            .map_err(IndexStoreError::Io)?;

        Ok(Self {
            sender,
            thread_handle: Arc::new(std::sync::Mutex::new(Some(handle))),
            db_path: db_path.to_path_buf(),
            expected_total_entries,
            next_id,
            mutation_counter,
            queue_depth,
        })
    }

    /// Return the path to the DB file. Used by the scanner to open a
    /// temporary connection for `ScanContext` initialization.
    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    /// Shared ID counter for entry allocation. The scanner uses this to
    /// allocate unique IDs without reading from the DB (which can be stale).
    pub fn next_id(&self) -> &Arc<AtomicI64> {
        &self.next_id
    }

    /// Set the expected total entries from a completed scan. The writer thread
    /// reads this to report flushing progress as it drains `InsertEntriesV2`.
    pub fn set_expected_total_entries(&self, total: u64) {
        self.expected_total_entries.store(total, Ordering::Relaxed);
    }

    /// Per-writer mutation counter. Bumped alongside the global `WRITER_GENERATION`
    /// every time the writer thread processes a mutating message. Tests rely on
    /// this to assert "did THIS writer mutate?" without flaking under concurrent
    /// other-writer activity in the same test binary.
    #[cfg(test)]
    pub(crate) fn mutation_count(&self) -> u64 {
        self.mutation_counter.load(Ordering::Relaxed)
    }

    /// Send a message to the writer thread. Blocks if the channel is full
    /// (backpressure), which slows down event processing rather than
    /// consuming unlimited memory.
    pub fn send(&self, msg: WriteMessage) -> Result<(), IndexStoreError> {
        // Phase 1 instrumentation: track best-effort channel depth.
        self.queue_depth.fetch_add(1, Ordering::Relaxed);
        self.sender.send(msg).map_err(|e| {
            // Send failed. Undo the depth bump so the heartbeat doesn't drift.
            self.queue_depth.fetch_sub(1, Ordering::Relaxed);
            let _ = e;
            IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread has shut down",
            ))
        })
    }

    /// Best-effort estimate of the writer channel depth: messages sent but not
    /// yet processed. Read by the scan progress loop to skip partial-aggregation
    /// passes while the writer is catching up on an insert backlog.
    pub fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Relaxed)
    }

    /// Non-blocking send. Unlike `send`, never parks the caller when the channel
    /// is full — the message is dropped and `Ok(false)` is returned. This is what
    /// lets the partial-aggregation sender live on a tokio task without risking a
    /// parked worker: a full channel means the writer is busy with the real scan
    /// work, and a dropped partial pass is harmless (the next tick retries).
    ///
    /// Returns:
    /// - `Ok(true)`  — message enqueued.
    /// - `Ok(false)` — channel full, message dropped (not an error).
    /// - `Err(..)`   — writer thread gone (channel disconnected).
    pub fn try_send(&self, msg: WriteMessage) -> Result<bool, IndexStoreError> {
        try_send_with_depth(&self.sender, &self.queue_depth, msg)
    }

    /// Send a `Flush` and await the response, confirming all prior messages have been committed.
    pub async fn flush(&self) -> Result<(), IndexStoreError> {
        let (tx, rx) = oneshot::channel();
        self.send(WriteMessage::Flush(tx))?;
        rx.await.map_err(|_| {
            IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread dropped flush reply",
            ))
        })
    }

    /// Send a `Flush` and block until all prior messages have been committed.
    /// Safe to call from synchronous code (no async runtime needed).
    pub fn flush_blocking(&self) -> Result<(), IndexStoreError> {
        let (tx, rx) = oneshot::channel();
        self.send(WriteMessage::Flush(tx))?;
        rx.blocking_recv().map_err(|_| {
            IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread dropped flush reply",
            ))
        })
    }

    /// Send a `Shutdown` message and wait for the writer thread to finish.
    ///
    /// Joins the thread to ensure all buffered writes are flushed.
    /// After this call further sends will fail.
    pub fn shutdown(&self) {
        let _ = self.sender.send(WriteMessage::Shutdown);
        if let Ok(mut guard) = self.thread_handle.lock()
            && let Some(handle) = guard.take()
            && let Err(e) = handle.join()
        {
            log::warn!("Index writer thread panicked on shutdown: {e:?}");
        }
    }
}

/// Bump the depth counter, attempt a non-blocking `try_send`, and undo the bump
/// on any failure. Extracted as a free function (taking the raw channel + atomic)
/// so the bump/undo accounting can be tested against a bare `sync_channel`
/// without standing up a draining writer thread.
///
/// The undo on **both** `Full` and `Disconnected` is load-bearing: `queue_depth`
/// is only ever incremented on a successful enqueue, so a failed `try_send` that
/// left the bump in place would drift the depth upward forever — breaking both
/// the `PARTIAL_AGG_MAX_QUEUE_DEPTH` backpressure skip and the `queue_depth == 0`
/// pending-sizes wholesale clear in `writer_loop`. This mirrors `send`'s
/// undo-on-error pattern.
fn try_send_with_depth(
    sender: &mpsc::SyncSender<WriteMessage>,
    queue_depth: &AtomicUsize,
    msg: WriteMessage,
) -> Result<bool, IndexStoreError> {
    queue_depth.fetch_add(1, Ordering::Relaxed);
    match sender.try_send(msg) {
        Ok(()) => Ok(true),
        Err(mpsc::TrySendError::Full(_)) => {
            queue_depth.fetch_sub(1, Ordering::Relaxed);
            Ok(false)
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            queue_depth.fetch_sub(1, Ordering::Relaxed);
            Err(IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread has shut down",
            )))
        }
    }
}

// ── Writer thread loop ───────────────────────────────────────────────

/// Snapshot of cumulative counters, used to compute per-interval deltas.
#[derive(Clone, Default)]
struct StatsSnapshot {
    total: u64,
    insert_entries: u64,
    upsert_entry: u64,
    move_entry: u64,
    delete_entry: u64,
    delete_subtree: u64,
    propagate_delta: u64,
    compute_aggregates: u64,
    compute_partial: u64,
    flush: u64,
    other: u64,
}

/// Diagnostic counters for writer thread logging.
struct WriterStats {
    current: StatsSnapshot,
    previous: StatsSnapshot,
    last_summary: Instant,
}

impl WriterStats {
    fn new() -> Self {
        Self {
            current: StatsSnapshot::default(),
            previous: StatsSnapshot::default(),
            last_summary: Instant::now(),
        }
    }

    fn record(&mut self, msg: &WriteMessage) {
        self.current.total += 1;
        match msg {
            WriteMessage::InsertEntriesV2(_) => self.current.insert_entries += 1,
            WriteMessage::UpsertEntryV2 { .. } => self.current.upsert_entry += 1,
            WriteMessage::MoveEntryV2 { .. } => self.current.move_entry += 1,
            WriteMessage::DeleteEntryById(_) => self.current.delete_entry += 1,
            WriteMessage::DeleteSubtreeById(_) | WriteMessage::DeleteDescendantsById(_) => {
                self.current.delete_subtree += 1;
            }
            WriteMessage::PropagateDeltaById { .. } => self.current.propagate_delta += 1,
            WriteMessage::ComputeAllAggregates | WriteMessage::ComputeSubtreeAggregates { .. } => {
                self.current.compute_aggregates += 1;
            }
            WriteMessage::ComputePartialAggregates { .. } => self.current.compute_partial += 1,
            WriteMessage::Flush(_) => self.current.flush += 1,
            _ => self.current.other += 1,
        }
    }

    /// Log a summary if at least 5 seconds have passed since the last one.
    ///
    /// Shows per-interval deltas as the primary info, with cumulative total in brackets.
    /// Only non-zero delta categories are included to keep the message concise.
    fn maybe_log_summary(&mut self) {
        let elapsed = self.last_summary.elapsed();
        if elapsed.as_secs() < 5 || self.current.total == 0 {
            return;
        }

        let delta_total = self.current.total - self.previous.total;
        if delta_total == 0 {
            self.last_summary = Instant::now();
            return;
        }

        // (singular, plural, count). Pluralizing per row keeps the "+1 insert"
        // / "+5 inserts" form natural; baking `+s` everywhere reads as "+1 inserts".
        let deltas: &[(&str, &str, u64)] = &[
            (
                "insert",
                "inserts",
                self.current.insert_entries - self.previous.insert_entries,
            ),
            (
                "upsert",
                "upserts",
                self.current.upsert_entry - self.previous.upsert_entry,
            ),
            ("move", "moves", self.current.move_entry - self.previous.move_entry),
            (
                "delete",
                "deletes",
                self.current.delete_entry - self.previous.delete_entry,
            ),
            (
                "delete_subtree",
                "delete_subtrees",
                self.current.delete_subtree - self.previous.delete_subtree,
            ),
            (
                "propagation",
                "propagations",
                self.current.propagate_delta - self.previous.propagate_delta,
            ),
            (
                "aggregate",
                "aggregates",
                self.current.compute_aggregates - self.previous.compute_aggregates,
            ),
            (
                "partial aggregate",
                "partial aggregates",
                self.current.compute_partial - self.previous.compute_partial,
            ),
            ("flush", "flushes", self.current.flush - self.previous.flush),
            ("other", "others", self.current.other - self.previous.other),
        ];

        let parts: Vec<String> = deltas
            .iter()
            .filter(|(_, _, count)| *count > 0)
            .map(|(singular, plural, count)| pluralize_with(*count, singular, plural))
            .collect();

        let breakdown = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        };

        log::debug!(
            "Writer: +{}{breakdown} in {:.1}s [{} total]",
            pluralize(delta_total, "msg"),
            elapsed.as_secs_f64(),
            self.current.total,
        );

        self.previous = self.current.clone();
        self.last_summary = Instant::now();
    }
}

/// In-memory accumulation of direct children stats, built during InsertEntriesV2.
///
/// Eliminates the two expensive full-table-scan SQL queries in the aggregator
/// (`bulk_get_children_stats_by_id` and `bulk_get_child_dir_ids`) by tracking
/// the same information incrementally as entries are inserted.
pub(super) struct AccumulatorMaps {
    /// `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
    /// has_symlinks_direct)`: direct children only. `has_symlinks_direct` is `true` if any
    /// direct child of `parent_id` is a symlink.
    pub(super) direct_stats: HashMap<i64, (u64, u64, u64, u64, bool)>,
    /// `parent_id -> Vec<child_dir_id>`: direct child directories only.
    pub(super) child_dirs: HashMap<i64, Vec<i64>>,
    /// Running count of entries inserted so far (for flushing progress).
    pub(super) entries_inserted: u64,
    /// Running count of rows the scan skipped on a UNIQUE `(parent_id,
    /// name_folded)` conflict (`INSERT OR IGNORE`). Summarized once per scan at
    /// `ComputeAllAggregates`; see `classify_skip_severity`.
    pub(super) entries_skipped: u64,
}

impl AccumulatorMaps {
    pub(super) fn new() -> Self {
        Self {
            direct_stats: HashMap::new(),
            child_dirs: HashMap::new(),
            entries_inserted: 0,
            entries_skipped: 0,
        }
    }

    /// Accumulate stats from a set of inserted entries. Accepts any iterator
    /// of `&EntryRow` so callers can pre-filter (skipping rows that lost a
    /// UNIQUE conflict during `INSERT OR IGNORE`) without an extra clone.
    pub(super) fn accumulate<'a>(&mut self, entries: impl IntoIterator<Item = &'a EntryRow>) {
        for entry in entries {
            self.entries_inserted += 1;
            let stats = self.direct_stats.entry(entry.parent_id).or_insert((0, 0, 0, 0, false));
            if entry.is_symlink {
                stats.4 = true;
            }
            if entry.is_directory {
                stats.3 += 1;
                self.child_dirs.entry(entry.parent_id).or_default().push(entry.id);
            } else {
                stats.0 += entry.logical_size.unwrap_or(0);
                stats.1 += entry.physical_size.unwrap_or(0);
                stats.2 += 1;
            }
        }
    }

    pub(super) fn clear(&mut self) {
        self.direct_stats.clear();
        self.child_dirs.clear();
        self.entries_inserted = 0;
        self.entries_skipped = 0;
    }
}

/// Main loop for the writer thread.
///
/// Processes messages sequentially from the mpsc channel. Each message is
/// handled in order, ensuring all writes are serialized. Maintains in-memory
/// accumulator maps during InsertEntriesV2 to skip expensive SQL queries
/// when ComputeAllAggregates arrives.
#[allow(clippy::too_many_arguments, reason = "writer-loop ambient state")]
fn writer_loop(
    conn: rusqlite::Connection,
    receiver: mpsc::Receiver<WriteMessage>,
    app_handle: Option<AppHandle>,
    expected_total_entries: Arc<AtomicU64>,
    next_id: Arc<AtomicI64>,
    mutation_counter: Arc<AtomicU64>,
    queue_depth: Arc<AtomicUsize>,
) {
    log::debug!("Writer: thread started");
    let mut stats = WriterStats::new();
    let mut accumulator = AccumulatorMaps::new();

    // Phase 1 instrumentation: time split between recv() (idle waiting),
    // processing (handlers), and commit (txn commits, tracked via wrapper).
    let mut probe = ProbeStats::new();
    // Heartbeat cadence
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    loop {
        let recv_start = Instant::now();
        // Use recv_timeout so we can emit heartbeats even when the channel
        // is idle (the 163s smoking gun should make this visible).
        let recv_result = receiver.recv_timeout(HEARTBEAT_INTERVAL);
        let recv_elapsed = recv_start.elapsed();
        probe.time_in_recv += recv_elapsed;

        let msg = match recv_result {
            Ok(m) => {
                // Decrement queue depth: the message has left the channel.
                queue_depth.fetch_sub(1, Ordering::Relaxed);
                m
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No message in this window. Emit heartbeat and loop.
                probe.maybe_emit_heartbeat(queue_depth.load(Ordering::Relaxed));
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if !matches!(msg, WriteMessage::IncrementalVacuum | WriteMessage::WalCheckpoint) {
            stats.record(&msg);
        }

        let proc_start = Instant::now();
        // macOS: drain autoreleased ObjC objects each iteration.
        #[cfg(target_os = "macos")]
        let should_exit = objc2::rc::autoreleasepool(|_| {
            process_message(
                &conn,
                msg,
                &stats,
                &mut accumulator,
                &app_handle,
                &expected_total_entries,
                &next_id,
                &mutation_counter,
                &mut probe,
            )
        });
        #[cfg(not(target_os = "macos"))]
        let should_exit = process_message(
            &conn,
            msg,
            &stats,
            &mut accumulator,
            &app_handle,
            &expected_total_entries,
            &next_id,
            &mutation_counter,
            &mut probe,
        );
        probe.time_in_processing += proc_start.elapsed();
        probe.messages_processed += 1;

        if should_exit {
            log::debug!("Writer: shutdown after processing {} messages", stats.current.total);
            return;
        }
        stats.maybe_log_summary();
        probe.maybe_emit_heartbeat(queue_depth.load(Ordering::Relaxed));

        // Pending-size hourglass: once the writer has fully caught up (no more
        // queued work), every directory's `dir_stats` reflects all known
        // changes, so the "size updating" flags are correct to clear wholesale.
        // Done here (end of iteration, after the message's DB effect is applied)
        // rather than at recv time — at recv the depth hits 0 *before* the
        // delete/propagate runs, which would briefly show a settled flag against
        // a not-yet-updated size. See `indexing/pending_sizes.rs`.
        if queue_depth.load(Ordering::Relaxed) == 0
            && let Some(tracker) = crate::indexing::pending_sizes::get_pending_sizes()
        {
            tracker.clear();
        }
    }

    log::debug!(
        "Writer: channel closed, thread exiting after processing {} messages",
        stats.current.total,
    );
}

/// Phase 1 instrumentation: rolling diagnostics for the writer thread.
struct ProbeStats {
    last_heartbeat: Instant,
    time_in_recv: Duration,
    time_in_processing: Duration,
    time_in_commit: Duration,
    messages_processed: u64,
    transaction_commits: u64,
}

impl ProbeStats {
    fn new() -> Self {
        Self {
            last_heartbeat: Instant::now(),
            time_in_recv: Duration::ZERO,
            time_in_processing: Duration::ZERO,
            time_in_commit: Duration::ZERO,
            messages_processed: 0,
            transaction_commits: 0,
        }
    }

    fn maybe_emit_heartbeat(&mut self, queue_depth: usize) {
        if self.last_heartbeat.elapsed() < Duration::from_secs(5) {
            return;
        }
        log::debug!(
            target: "stall_probe::writer",
            "heartbeat queue_depth={} messages_processed_since_last_heartbeat={} transaction_commits_since_last_heartbeat={} time_in_recv_ms={} time_in_processing_ms={} time_in_commit_ms={}",
            queue_depth,
            self.messages_processed,
            self.transaction_commits,
            self.time_in_recv.as_millis(),
            self.time_in_processing.as_millis(),
            self.time_in_commit.as_millis(),
        );
        self.last_heartbeat = Instant::now();
        self.time_in_recv = Duration::ZERO;
        self.time_in_processing = Duration::ZERO;
        self.time_in_commit = Duration::ZERO;
        self.messages_processed = 0;
        self.transaction_commits = 0;
    }
}

/// Process a single message. Returns `true` if the thread should exit.
#[allow(clippy::too_many_arguments, reason = "writer-loop ambient state")]
fn process_message(
    conn: &rusqlite::Connection,
    msg: WriteMessage,
    stats: &WriterStats,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    expected_total_entries: &AtomicU64,
    next_id: &AtomicI64,
    mutation_counter: &AtomicU64,
    probe: &mut ProbeStats,
) -> bool {
    match msg {
        // ── Integer-keyed variants ───────────────────────────────────
        WriteMessage::InsertEntriesV2(entries) => {
            handle_insert_entries_v2(
                conn,
                entries,
                accumulator,
                app_handle,
                expected_total_entries,
                mutation_counter,
            );
        }
        WriteMessage::UpsertEntryV2 {
            parent_id,
            name,
            is_directory,
            is_symlink,
            logical_size,
            physical_size,
            modified_at,
            inode,
            nlink,
        } => {
            handle_upsert_entry_v2(
                conn,
                parent_id,
                name,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
                nlink,
                next_id,
                mutation_counter,
            );
        }
        WriteMessage::MoveEntryV2 {
            entry_id,
            new_parent_id,
            new_name,
        } => {
            handle_move_entry_v2(conn, entry_id, new_parent_id, new_name, mutation_counter);
        }
        WriteMessage::DeleteEntryById(entry_id) => {
            handle_delete_entry_by_id(conn, entry_id, mutation_counter);
        }
        WriteMessage::DeleteSubtreeById(root_id) => {
            handle_delete_subtree_by_id(conn, root_id, mutation_counter);
        }
        WriteMessage::DeleteDescendantsById(root_id) => {
            // No delta propagation: the subtree will be immediately re-scanned and
            // ComputeSubtreeAggregates will recompute stats for the subtree root.
            if let Err(e) = IndexStore::delete_descendants_by_id(conn, root_id) {
                log::warn!("Index writer: delete_descendants_by_id failed for id={root_id}: {e}");
            }
        }
        WriteMessage::PropagateDeltaById {
            entry_id,
            logical_size_delta,
            physical_size_delta,
            file_count_delta,
            dir_count_delta,
        } => {
            propagate_delta_by_id(
                conn,
                entry_id,
                logical_size_delta,
                physical_size_delta,
                file_count_delta,
                dir_count_delta,
            );
        }
        WriteMessage::TruncateData => {
            handle_truncate_data(conn, accumulator, expected_total_entries, next_id, mutation_counter);
        }
        WriteMessage::ComputeAllAggregates => {
            handle_compute_all_aggregates(conn, accumulator, app_handle, expected_total_entries);
        }
        WriteMessage::ComputePartialAggregates { hot_paths } => {
            handle_compute_partial_aggregates(conn, accumulator, app_handle, hot_paths);
        }
        WriteMessage::ComputeSubtreeAggregates { root } => {
            handle_compute_subtree_aggregates(conn, &root);
        }
        WriteMessage::UpdateLastEventId(id) => {
            if let Err(e) = IndexStore::update_meta(conn, "last_event_id", &id.to_string()) {
                log::warn!("Index writer: update last_event_id failed: {e}");
            }
        }
        WriteMessage::UpdateMeta { key, value } => {
            if let Err(e) = IndexStore::update_meta(conn, &key, &value) {
                log::warn!("Index writer: update_meta({key}) failed: {e}");
            }
        }
        WriteMessage::DeleteMeta(key) => {
            if let Err(e) = IndexStore::delete_meta(conn, &key) {
                log::warn!("Index writer: delete_meta({key}) failed: {e}");
            }
        }
        #[cfg(test)]
        WriteMessage::GetEntryCount(reply) => {
            let result = IndexStore::get_entry_count(conn);
            // If the receiver dropped, that's fine; ignore the send error
            let _ = reply.send(result);
        }
        WriteMessage::Flush(reply) => {
            log::debug!(
                "Writer: processing flush (total msgs processed so far: {})",
                stats.current.total,
            );
            // All prior messages have been processed; signal the caller
            let _ = reply.send(());
        }
        WriteMessage::BeginTransaction => {
            log::debug!("Writer: BEGIN IMMEDIATE transaction");
            if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
                log::warn!("Index writer: BEGIN TRANSACTION failed: {e}");
            }
        }
        WriteMessage::CommitTransaction => {
            let t = Instant::now();
            if let Err(e) = conn.execute_batch("COMMIT") {
                log::warn!("Index writer: COMMIT failed: {e}");
            }
            let elapsed = t.elapsed();
            probe.time_in_commit += elapsed;
            probe.transaction_commits += 1;
            let elapsed_ms = elapsed.as_millis();
            log::debug!("Writer: COMMIT transaction ({elapsed_ms}ms)");
            if elapsed_ms > 50 {
                log::info!(
                    target: "stall_probe::writer",
                    "commit_slow ms={elapsed_ms}",
                );
            }
        }
        WriteMessage::BackfillMissingDirStats => {
            handle_backfill_missing_dir_stats(conn);
        }
        WriteMessage::IncrementalVacuum => {
            handle_incremental_vacuum(conn);
        }
        WriteMessage::WalCheckpoint => {
            handle_wal_checkpoint(conn);
        }
        WriteMessage::EmitDirUpdated(paths) => {
            if let Some(app) = app_handle {
                crate::indexing::reconciler::emit_dir_updated(app, paths);
            }
        }
        WriteMessage::Shutdown => return true,
    }
    false
}

/// Tick the global writer-generation counter (used by search to detect a stale
/// index) AND the per-writer counter (used by tests to assert that THIS writer
/// did or didn't mutate, without flaking on other concurrent writers in the
/// same test binary).
#[inline]
pub(super) fn bump_generation(mutation_counter: &AtomicU64) {
    WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
    mutation_counter.fetch_add(1, Ordering::Relaxed);
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};

    /// Create a temp DB, open the store (to init schema), and return the path + temp dir guard.
    pub(super) fn setup_db() -> (PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-writer.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        (db_path, dir)
    }

    /// Open a read connection to the DB for assertions.
    pub(super) fn open_read(db_path: &Path) -> IndexStore {
        IndexStore::open(db_path).expect("failed to open read store")
    }

    // ── Basic lifecycle tests ────────────────────────────────────────

    #[test]
    fn spawn_and_shutdown() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();
        writer.shutdown();
        // Further sends should fail
        let result = writer.send(WriteMessage::Shutdown);
        // Might succeed or fail depending on timing, but shouldn't panic
        let _ = result;
    }

    /// The writer clears the pending-size tracker once its queue drains to empty
    /// (the "size updating" hourglass turns off when the indexer catches up).
    ///
    /// Guarded by `PENDING_SIZES_TEST_MUTEX`: the tracker is a process-global,
    /// but it's `None` for every test that doesn't install it, so other writers
    /// no-op the clear. Only installers race, and they all hold this mutex.
    #[test]
    fn clears_pending_sizes_when_queue_drains() {
        use crate::indexing::pending_sizes::{
            PENDING_SIZES, PENDING_SIZES_TEST_MUTEX, PendingSizes, get_pending_sizes,
        };
        let _guard = PENDING_SIZES_TEST_MUTEX.lock().unwrap();

        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Install a tracker and mark a path. The writer is idle (no message
        // processed yet) so it hasn't cleared; the mark is observable.
        *PENDING_SIZES.lock().unwrap() = Some(Arc::new(PendingSizes::new()));
        let tracker = get_pending_sizes().expect("tracker installed");
        tracker.mark("/aaa/bbb/ccc");
        assert!(tracker.is_pending("/aaa/bbb"), "mark should register before any drain");

        // Send a message and let the writer drain. The end-of-iteration hook
        // clears the tracker once `queue_depth` hits 0. The clear runs a hair
        // after the flush reply is delivered, so poll for the result (it always
        // happens within microseconds on an idle writer).
        writer.flush_blocking().unwrap();
        let mut cleared = false;
        for _ in 0..200 {
            if !tracker.is_pending("/aaa/bbb") {
                cleared = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(cleared, "tracker should clear once the writer queue drains");

        *PENDING_SIZES.lock().unwrap() = None;
        writer.shutdown();
    }

    #[test]
    fn get_entry_count_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert using integer-keyed API (simpler, no path resolution needed)
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(100),
                physical_size: Some(100),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        let (tx, rx) = oneshot::channel();
        writer.send(WriteMessage::GetEntryCount(tx)).unwrap();

        let count = rx.blocking_recv().unwrap().unwrap();
        // 2 inserted + 1 root sentinel = 3
        assert_eq!(count, 3);

        writer.shutdown();
    }

    #[test]
    fn update_meta_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer
            .send(WriteMessage::UpdateMeta {
                key: "test_key".into(),
                value: "test_value".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let status = store.get_index_status().unwrap();
        // test_key is not in IndexStatus struct, read directly via connection
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let val = IndexStore::get_meta(&conn, "test_key").unwrap();
        assert_eq!(val.as_deref(), Some("test_value"));
        drop(store);
        drop(status);

        writer.shutdown();
    }

    #[test]
    fn update_meta_total_physical_bytes_round_trip() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer
            .send(WriteMessage::UpdateMeta {
                key: "total_physical_bytes".into(),
                value: "123456789".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let val = IndexStore::get_meta(&conn, "total_physical_bytes").unwrap();
        assert_eq!(val.as_deref(), Some("123456789"));

        writer.shutdown();
    }

    #[test]
    fn delete_meta_via_writer_clears_key() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Set, then delete, then expect the key to read back as None.
        writer
            .send(WriteMessage::UpdateMeta {
                key: "scan_completed_at".into(),
                value: "1700000000".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert_eq!(
            IndexStore::get_meta(&conn, "scan_completed_at").unwrap().as_deref(),
            Some("1700000000")
        );

        writer
            .send(WriteMessage::DeleteMeta("scan_completed_at".into()))
            .unwrap();
        writer.flush_blocking().unwrap();

        assert_eq!(
            IndexStore::get_meta(&conn, "scan_completed_at").unwrap(),
            None,
            "DeleteMeta must remove the key entirely"
        );

        // Deleting an absent key is a harmless no-op.
        writer.send(WriteMessage::DeleteMeta("never_set".into())).unwrap();
        writer.flush_blocking().unwrap();
        assert_eq!(IndexStore::get_meta(&conn, "never_set").unwrap(), None);

        writer.shutdown();
    }

    #[tokio::test]
    async fn flush_confirms_prior_writes() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert using integer-keyed API
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "test.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(512),
            physical_size: Some(512),
            modified_at: Some(1700000000),
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush().await.unwrap();

        // Data should be readable immediately after flush
        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "test.txt");
        assert_eq!(children[0].logical_size, Some(512));

        writer.shutdown();
    }

    #[test]
    fn update_last_event_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer.send(WriteMessage::UpdateLastEventId(12345)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let status = store.get_index_status().unwrap();
        assert_eq!(status.last_event_id.as_deref(), Some("12345"));

        writer.shutdown();
    }

    #[test]
    fn db_path_is_available() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();
        assert_eq!(writer.db_path(), db_path);
        writer.shutdown();
    }

    // ── try_send / queue_depth ───────────────────────────────────────

    /// Happy path on a live writer: `try_send` enqueues without blocking and
    /// bumps `queue_depth`; once the writer drains the message the depth returns
    /// to 0. Pins both the `Ok(true)` outcome and the depth accounting.
    #[test]
    fn try_send_enqueues_and_tracks_queue_depth() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let sent = writer
            .try_send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .expect("try_send on a live writer should not error");
        assert!(sent, "try_send into an empty channel should enqueue (Ok(true))");

        // After a flush barrier the writer has processed every prior message,
        // so the depth is back to 0.
        writer.flush_blocking().unwrap();
        let mut drained = false;
        for _ in 0..200 {
            if writer.queue_depth() == 0 {
                drained = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(drained, "queue_depth should return to 0 once the writer drains");

        writer.shutdown();
    }

    /// A `try_send` to a shut-down writer reports the disconnect as an error AND
    /// undoes its depth bump, so a dead channel can't leave `queue_depth` drifted.
    #[test]
    fn try_send_after_shutdown_errors_and_undoes_depth() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();
        writer.shutdown();

        let depth_before = writer.queue_depth();
        let result = writer.try_send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] });
        assert!(
            result.is_err(),
            "try_send to a disconnected writer should be Err, got {result:?}"
        );
        assert_eq!(
            writer.queue_depth(),
            depth_before,
            "the depth bump must be undone on a disconnected send"
        );
    }

    /// The bump/undo accounting against a raw `sync_channel(1)`: the first send
    /// fills the single slot (`Ok(true)`, depth +1), the second finds it full
    /// (`Ok(false)`, no error, depth unchanged — the bump is undone). This pins
    /// the Full path deterministically without a draining writer thread.
    #[test]
    fn try_send_with_depth_undoes_bump_on_full() {
        let (sender, _receiver) = mpsc::sync_channel::<WriteMessage>(1);
        let depth = AtomicUsize::new(0);

        let first = try_send_with_depth(
            &sender,
            &depth,
            WriteMessage::ComputePartialAggregates { hot_paths: vec![] },
        )
        .expect("first send into an open channel should not error");
        assert!(first, "first send fills the single slot (Ok(true))");
        assert_eq!(depth.load(Ordering::Relaxed), 1, "successful send bumps depth");

        let second = try_send_with_depth(
            &sender,
            &depth,
            WriteMessage::ComputePartialAggregates { hot_paths: vec![] },
        )
        .expect("a full channel is Ok(false), not Err");
        assert!(!second, "second send finds the channel full (Ok(false))");
        assert_eq!(
            depth.load(Ordering::Relaxed),
            1,
            "a dropped (full) send must leave depth unchanged — bump undone"
        );
    }
}
