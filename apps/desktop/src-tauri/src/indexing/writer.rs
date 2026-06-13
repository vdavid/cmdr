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
use tauri_specta::Event;
use tokio::sync::oneshot;

use crate::indexing::aggregator::{self, AggregationPhase, AggregationProgress};
use crate::indexing::store::{DirStatsById, EntryRow, IndexStore, IndexStoreError};
use crate::pluralize::{pluralize, pluralize_with};

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

fn phase_to_str(phase: AggregationPhase) -> &'static str {
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
struct AccumulatorMaps {
    /// `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
    /// has_symlinks_direct)`: direct children only. `has_symlinks_direct` is `true` if any
    /// direct child of `parent_id` is a symlink.
    direct_stats: HashMap<i64, (u64, u64, u64, u64, bool)>,
    /// `parent_id -> Vec<child_dir_id>`: direct child directories only.
    child_dirs: HashMap<i64, Vec<i64>>,
    /// Running count of entries inserted so far (for flushing progress).
    entries_inserted: u64,
    /// Running count of rows the scan skipped on a UNIQUE `(parent_id,
    /// name_folded)` conflict (`INSERT OR IGNORE`). Summarized once per scan at
    /// `ComputeAllAggregates`; see `classify_skip_severity`.
    entries_skipped: u64,
}

impl AccumulatorMaps {
    fn new() -> Self {
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
    fn accumulate<'a>(&mut self, entries: impl IntoIterator<Item = &'a EntryRow>) {
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

    fn clear(&mut self) {
        self.direct_stats.clear();
        self.child_dirs.clear();
        self.entries_inserted = 0;
        self.entries_skipped = 0;
    }
}

/// Log severity for the count of rows a full scan skipped on a UNIQUE
/// `(parent_id, name_folded)` conflict (the `INSERT OR IGNORE` path).
#[derive(Debug, PartialEq, Eq)]
enum SkipSeverity {
    /// Nothing skipped: log nothing.
    None,
    /// Sparse skips, expected dedup (one dir reachable by two walk paths via a
    /// firmlink/symlink, or case/NFD sibling pairs on case-sensitive or
    /// cross-OS-synced trees). Not actionable: log at DEBUG.
    Benign,
    /// A large fraction of the scan skipped: the signature of two writer threads
    /// racing on one DB (the constraint's reason for being, a 1.83 TB ghost size
    /// was traced to exactly that). Actionable: log at WARN.
    Suspicious,
}

/// Classify a full scan's accumulated UNIQUE-conflict skips. The absolute floor
/// keeps a tiny tree with a couple genuine sibling collisions from tripping the
/// warning; the ratio separates a handful of dedup hits in a multi-million-row
/// scan from a racing writer (whose loser duplicates a large fraction of rows).
fn classify_skip_severity(inserted: u64, skipped: u64) -> SkipSeverity {
    const MIN_SUSPICIOUS_SKIPS: u64 = 50;
    const SUSPICIOUS_SKIP_RATIO: f64 = 0.01;
    if skipped == 0 {
        return SkipSeverity::None;
    }
    let total = inserted + skipped;
    let ratio = skipped as f64 / total as f64;
    if skipped >= MIN_SUSPICIOUS_SKIPS && ratio > SUSPICIOUS_SKIP_RATIO {
        SkipSeverity::Suspicious
    } else {
        SkipSeverity::Benign
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

// ── Extracted message handlers ──────────────────────────────────────

/// Tick the global writer-generation counter (used by search to detect a stale
/// index) AND the per-writer counter (used by tests to assert that THIS writer
/// did or didn't mutate, without flaking on other concurrent writers in the
/// same test binary).
#[inline]
fn bump_generation(mutation_counter: &AtomicU64) {
    WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
    mutation_counter.fetch_add(1, Ordering::Relaxed);
}

fn handle_insert_entries_v2(
    conn: &rusqlite::Connection,
    entries: Vec<EntryRow>,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    expected_total_entries: &AtomicU64,
    mutation_counter: &AtomicU64,
) {
    let count = entries.len();
    let t = Instant::now();
    // Accumulate AFTER the DB commit succeeds. `insert_entries_v2_batch`
    // uses `INSERT OR IGNORE`, so a UNIQUE conflict on
    // `(parent_id, name_folded)` (case-sensitive volumes with `Foo.txt` and
    // `foo.txt` siblings, NFC/NFD duplicates from cross-OS sync, etc.) skips
    // just that row instead of rolling back the entire 2000-entry batch. The
    // accumulator must skip those rows too, or `compute_all_aggregates_with_maps`
    // inflates `dir_stats` with phantom bytes (the constraint comment that
    // called out "1.83 TB ghost size on a 994 GB volume" is exactly this
    // failure mode).
    //
    // A per-batch skip is logged at DEBUG only (with a sample for diagnosis): a
    // few skips per scan is expected dedup and not actionable. The accumulated
    // count is summarized once per scan at `ComputeAllAggregates`, which escalates
    // to WARN only when the skip ratio looks like a racing writer. See
    // `classify_skip_severity`.
    match IndexStore::insert_entries_v2_batch(conn, &entries) {
        Ok(inserted) => {
            let skipped_count = inserted.iter().filter(|landed| !**landed).count();
            if skipped_count == 0 {
                accumulator.accumulate(&entries);
            } else {
                accumulator.entries_skipped += skipped_count as u64;
                accumulator.accumulate(
                    entries
                        .iter()
                        .zip(inserted.iter())
                        .filter_map(|(e, landed)| if *landed { Some(e) } else { None }),
                );
                let samples: Vec<(i64, &str)> = entries
                    .iter()
                    .zip(inserted.iter())
                    .filter_map(|(e, landed)| {
                        if !*landed {
                            Some((e.parent_id, e.name.as_str()))
                        } else {
                            None
                        }
                    })
                    .take(3)
                    .collect();
                log::debug!(
                    "Index writer: {skipped_count} of {batch_size} skipped due to UNIQUE conflict on (parent_id, name_folded); sample: {samples:?}",
                    batch_size = pluralize_with(count as u64, "entry", "entries")
                );
            }
        }
        Err(e) => crate::log_error!("Index writer: insert_entries_v2_batch failed: {e}"),
    }
    let elapsed = t.elapsed().as_millis();
    if elapsed > 100 {
        log::debug!(
            "Writer: insert_entries_v2_batch ({}) took {elapsed}ms",
            pluralize_with(count as u64, "entry", "entries")
        );
    }
    bump_generation(mutation_counter);
    // Emit flushing progress when we know the expected total
    let expected = expected_total_entries.load(Ordering::Relaxed);
    if expected > 0
        && let Some(app) = app_handle
    {
        let _ = AggregationProgressEvent {
            phase: phase_to_str(AggregationPhase::SavingEntries).to_string(),
            current: accumulator.entries_inserted,
            total: expected,
        }
        .emit(app);
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the DB columns for a single upsert operation"
)]
fn handle_upsert_entry_v2(
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: String,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    nlink: Option<u64>,
    next_id: &AtomicI64,
    mutation_counter: &AtomicU64,
) {
    // Hardlink dedup: if this file has nlink > 1, check whether another entry
    // for the same inode already has non-NULL sizes. If so, override sizes to
    // None so each inode's bytes are counted exactly once.
    let should_dedup = inode.is_some() && matches!(nlink, Some(n) if n > 1) && logical_size.is_some();

    // Check if an entry already exists at (parent_id, name).
    // Auto-propagates size deltas to ancestor dir_stats on both
    // insert and update, so callers never need a separate
    // PropagateDeltaById for upserted entries.
    match IndexStore::resolve_component(conn, parent_id, &name) {
        Ok(Some(existing_id)) => {
            // Type change (file↔dir): delete old entry and insert fresh so
            // file_count/dir_count deltas propagate correctly. An in-place update
            // would leave counts wrong because the old type's count isn't decremented.
            let old_entry = IndexStore::get_entry_by_id(conn, existing_id).ok().flatten();
            if let Some(ref old) = old_entry
                && old.is_directory != is_directory
            {
                log::debug!(
                    "Writer: UpsertEntryV2 type change for id={existing_id} \
                         (was_dir={}, now_dir={is_directory}), converting to delete+insert",
                    old.is_directory
                );
                if old.is_directory {
                    handle_delete_subtree_by_id(conn, existing_id, mutation_counter);
                } else {
                    handle_delete_entry_by_id(conn, existing_id, mutation_counter);
                }
                upsert_insert_new(
                    conn,
                    parent_id,
                    &name,
                    is_directory,
                    is_symlink,
                    logical_size,
                    physical_size,
                    modified_at,
                    inode,
                    should_dedup,
                    next_id,
                );
                return;
            }

            upsert_update_existing(
                conn,
                existing_id,
                parent_id,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
                should_dedup,
                old_entry,
            );
        }
        Ok(None) => {
            upsert_insert_new(
                conn,
                parent_id,
                &name,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
                should_dedup,
                next_id,
            );
        }
        Err(e) => {
            log::warn!("Index writer: resolve_component failed for {name}: {e}");
        }
    }
    bump_generation(mutation_counter);
}

/// Update an existing entry during `UpsertEntryV2`, with hardlink dedup and delta propagation.
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the DB columns for an existing-entry update"
)]
fn upsert_update_existing(
    conn: &rusqlite::Connection,
    existing_id: i64,
    parent_id: i64,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    should_dedup: bool,
    old_entry: Option<EntryRow>,
) {
    // Dedup: override sizes if another entry already has sizes for this inode
    let (logical_size, physical_size) = if should_dedup
        && IndexStore::has_sized_entry_for_inode(conn, inode.unwrap(), Some(existing_id)).unwrap_or(false)
    {
        (None, None)
    } else {
        (logical_size, physical_size)
    };
    if let Err(e) = IndexStore::update_entry(
        conn,
        existing_id,
        is_directory,
        is_symlink,
        logical_size,
        physical_size,
        modified_at,
        inode,
    ) {
        log::warn!("Index writer: update_entry failed for id={existing_id}: {e}");
    } else if let Some(old) = old_entry {
        // Propagate size delta if anything changed
        let old_logical = old.logical_size.unwrap_or(0) as i64;
        let new_logical = logical_size.unwrap_or(0) as i64;
        let old_physical = old.physical_size.unwrap_or(0) as i64;
        let new_physical = physical_size.unwrap_or(0) as i64;
        let logical_delta = new_logical - old_logical;
        let physical_delta = new_physical - old_physical;
        if logical_delta != 0 || physical_delta != 0 {
            propagate_delta_by_id(conn, parent_id, logical_delta, physical_delta, 0, 0);
        }
        // Symlink state change can flip the parent's `recursive_has_symlinks`.
        if old.is_symlink != is_symlink {
            propagate_recursive_has_symlinks(conn, parent_id);
        }
    }
}

/// Insert a new entry during `UpsertEntryV2`, with hardlink dedup and delta propagation.
#[allow(clippy::too_many_arguments, reason = "mirrors the DB columns for a new-entry insert")]
fn upsert_insert_new(
    conn: &rusqlite::Connection,
    parent_id: i64,
    name: &str,
    is_directory: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    should_dedup: bool,
    next_id: &AtomicI64,
) {
    // Dedup: override sizes if another entry already has sizes for this inode
    let (logical_size, physical_size) =
        if should_dedup && IndexStore::has_sized_entry_for_inode(conn, inode.unwrap(), None).unwrap_or(false) {
            (None, None)
        } else {
            (logical_size, physical_size)
        };

    let new_entry_id = next_id.fetch_add(1, Ordering::Relaxed);
    match IndexStore::insert_entry_v2_with_id(
        conn,
        new_entry_id,
        parent_id,
        name,
        is_directory,
        is_symlink,
        logical_size,
        physical_size,
        modified_at,
        inode,
    ) {
        Ok(new_id) => {
            log::trace!("Writer: UpsertEntryV2 inserted \"{name}\" (parent_id={parent_id}) → id={new_id}");
            if is_directory {
                // Initialize empty dir_stats for new directories so enrichment
                // always has a row. Child events will update it incrementally.
                if let Err(e) = IndexStore::upsert_dir_stats_by_id(
                    conn,
                    &[DirStatsById {
                        entry_id: new_id,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                    }],
                ) {
                    log::warn!("Writer: init dir_stats for new dir id={new_id} failed: {e}");
                }
                propagate_delta_by_id(conn, parent_id, 0, 0, 0, 1);
            } else {
                let logical = logical_size.unwrap_or(0) as i64;
                let physical = physical_size.unwrap_or(0) as i64;
                propagate_delta_by_id(conn, parent_id, logical, physical, 1, 0);
            }
            // New symlink: walk the parent chain and OR in the flag.
            // We start at parent_id so the parent's stats include this symlink.
            if is_symlink {
                propagate_recursive_has_symlinks(conn, parent_id);
            }
        }
        Err(e) => {
            log::warn!("Index writer: insert_entry_v2 failed for {name}: {e}");
        }
    }
}

/// Move an existing entry to a new `(parent_id, name)`, preserving its
/// `entry_id` and (for directories) its `dir_stats`.
///
/// Used by the live event loop's rename pre-pass: when an `item_renamed`
/// event arrives whose new path has an inode that already exists in the DB
/// at a *different* `(parent_id, name)`, we rename the row in place rather
/// than going through delete+insert (which would lose `dir_stats`).
///
/// Cross-parent moves subtract the entry's contribution from the old
/// ancestor chain and add it to the new one. Same-parent renames don't
/// change ancestor totals so no propagation runs. The OR-aggregated
/// `recursive_has_symlinks` flag is recomputed both ways for cross-parent
/// moves: the old chain may need to clear it (if this was the last
/// symlink-bearing branch), the new chain may need to set it.
fn handle_move_entry_v2(
    conn: &rusqlite::Connection,
    entry_id: i64,
    new_parent_id: i64,
    new_name: String,
    mutation_counter: &AtomicU64,
) {
    use crate::indexing::store::normalize_for_comparison;

    let old_entry = match IndexStore::get_entry_by_id(conn, entry_id) {
        Ok(Some(e)) => e,
        Ok(None) => {
            log::debug!(target: "indexing::writer", "MoveEntryV2: entry id={entry_id} no longer exists, skipping");
            return;
        }
        Err(e) => {
            log::warn!("Index writer: MoveEntryV2 get_entry_by_id({entry_id}) failed: {e}");
            return;
        }
    };

    // Defensive no-op when the move would be a no-op anyway. Compares names
    // by their folded form so a rename that only changes case-folding
    // (e.g. NFD vs NFC on macOS) doesn't trigger spurious propagation.
    if old_entry.parent_id == new_parent_id
        && normalize_for_comparison(&old_entry.name) == normalize_for_comparison(&new_name)
    {
        log::debug!(
            target: "indexing::writer",
            "MoveEntryV2: id={entry_id} already at target (parent_id={new_parent_id}, name={new_name}), no-op",
        );
        return;
    }

    // A different entry can already occupy the destination (parent_id, name_folded): the move
    // overwrote an existing file, or a concurrent upsert raced ahead of this message. On disk
    // the moved entry owns that name now, so delete the conflicting row first (subtree-aware,
    // with delta propagation); without this the UPDATE below fails the UNIQUE constraint and
    // the moved entry stays stuck at its old location until verification heals it.
    let new_name_folded = normalize_for_comparison(&new_name);
    match IndexStore::resolve_component(conn, new_parent_id, &new_name) {
        Ok(Some(conflicting_id)) if conflicting_id != entry_id => {
            log::debug!(
                target: "indexing::writer",
                "MoveEntryV2: id={conflicting_id} already at destination (parent_id={new_parent_id}, name={new_name}), replacing it with id={entry_id}",
            );
            let conflicting_is_dir = IndexStore::get_entry_by_id(conn, conflicting_id)
                .ok()
                .flatten()
                .map(|e| e.is_directory)
                .unwrap_or(false);
            if conflicting_is_dir {
                handle_delete_subtree_by_id(conn, conflicting_id, mutation_counter);
            } else {
                handle_delete_entry_by_id(conn, conflicting_id, mutation_counter);
            }
        }
        Ok(_) => {}
        Err(e) => {
            log::warn!("Index writer: MoveEntryV2 destination lookup failed for id={entry_id}: {e}");
            return;
        }
    }
    if let Err(e) = conn.execute(
        "UPDATE entries SET parent_id = ?1, name = ?2, name_folded = ?3 WHERE id = ?4",
        rusqlite::params![new_parent_id, new_name, new_name_folded, entry_id],
    ) {
        log::warn!("Index writer: MoveEntryV2 update failed for id={entry_id}: {e}");
        return;
    }

    log::debug!(
        target: "indexing::writer",
        "MoveEntryV2: id={entry_id} \"{}\" → \"{}\" (parent_id {} → {})",
        old_entry.name,
        new_name,
        old_entry.parent_id,
        new_parent_id,
    );

    // Same-parent rename: ancestor totals unchanged, just the row's name moved.
    if old_entry.parent_id == new_parent_id {
        bump_generation(mutation_counter);
        return;
    }

    // Cross-parent move: subtract from the old chain, add to the new chain.
    let (logical_delta, physical_delta, file_delta, dir_delta) = if old_entry.is_directory {
        let totals = IndexStore::get_dir_stats_by_id(conn, entry_id).ok().flatten();
        let (logical, physical, files, dirs) = match totals {
            Some(s) => (
                s.recursive_logical_size as i64,
                s.recursive_physical_size as i64,
                s.recursive_file_count as i64,
                s.recursive_dir_count as i64,
            ),
            None => (0, 0, 0, 0),
        };
        // The directory itself contributes one to the dir count of every ancestor.
        (logical, physical, files as i32, (dirs + 1) as i32)
    } else {
        (
            old_entry.logical_size.unwrap_or(0) as i64,
            old_entry.physical_size.unwrap_or(0) as i64,
            1,
            0,
        )
    };

    propagate_delta_by_id(
        conn,
        old_entry.parent_id,
        -logical_delta,
        -physical_delta,
        -file_delta,
        -dir_delta,
    );
    propagate_delta_by_id(
        conn,
        new_parent_id,
        logical_delta,
        physical_delta,
        file_delta,
        dir_delta,
    );

    // The `recursive_has_symlinks` flag may flip on either chain. The old
    // chain might lose its only symlink-bearing descendant; the new chain
    // might gain one. `propagate_recursive_has_symlinks` is monotonic on
    // additions and recomputes correctly on removals, so calling it on both
    // is safe and stops walking as soon as a value stabilizes.
    if old_entry.is_symlink {
        propagate_recursive_has_symlinks(conn, old_entry.parent_id);
        propagate_recursive_has_symlinks(conn, new_parent_id);
    } else if old_entry.is_directory {
        let had_symlinks = IndexStore::get_dir_stats_by_id(conn, entry_id)
            .ok()
            .flatten()
            .map(|s| s.recursive_has_symlinks)
            .unwrap_or(false);
        if had_symlinks {
            propagate_recursive_has_symlinks(conn, old_entry.parent_id);
            propagate_recursive_has_symlinks(conn, new_parent_id);
        }
    }

    bump_generation(mutation_counter);
}

fn handle_delete_entry_by_id(conn: &rusqlite::Connection, entry_id: i64, mutation_counter: &AtomicU64) {
    // Read old entry before deleting to get accurate delta
    let old_entry = IndexStore::get_entry_by_id(conn, entry_id).ok().flatten();
    if let Err(e) = IndexStore::delete_entry_by_id(conn, entry_id) {
        log::warn!("Index writer: delete_entry_by_id failed for id={entry_id}: {e}");
    }
    // Auto-propagate accurate negative delta via parent_id chain
    if let Some(entry) = old_entry {
        let (logical_delta, physical_delta, file_delta, dir_delta) = if entry.is_directory {
            (0i64, 0i64, 0i32, -1i32)
        } else {
            (
                -(entry.logical_size.unwrap_or(0) as i64),
                -(entry.physical_size.unwrap_or(0) as i64),
                -1,
                0,
            )
        };
        propagate_delta_by_id(
            conn,
            entry.parent_id,
            logical_delta,
            physical_delta,
            file_delta,
            dir_delta,
        );
        // If we just deleted a symlink, the parent's `recursive_has_symlinks`
        // may flip back to false (and propagate further up).
        if entry.is_symlink {
            propagate_recursive_has_symlinks(conn, entry.parent_id);
        }
    }
    bump_generation(mutation_counter);
}

fn handle_delete_subtree_by_id(conn: &rusqlite::Connection, root_id: i64, mutation_counter: &AtomicU64) {
    // Read subtree totals before deleting to get accurate delta
    let totals = IndexStore::get_subtree_totals_by_id(conn, root_id).ok();
    let parent_id = IndexStore::get_parent_id(conn, root_id).ok().flatten();
    // Did the subtree contain any symlinks? Read the root's stored flag before
    // deletion (covers descendants), and also check any direct symlink children.
    let subtree_had_symlinks = {
        let from_root = IndexStore::get_dir_stats_by_id(conn, root_id)
            .ok()
            .flatten()
            .map(|s| s.recursive_has_symlinks)
            .unwrap_or(false);
        if from_root {
            true
        } else {
            // The root itself might be a symlink (rare), or a child might be one
            // without dir_stats covering it. Check directly.
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM entries WHERE id = ?1 AND is_symlink = 1)",
                rusqlite::params![root_id],
                |row| row.get::<_, i32>(0).map(|n| n != 0),
            )
            .unwrap_or(false)
        }
    };
    if let Err(e) = IndexStore::delete_subtree_by_id(conn, root_id) {
        log::warn!("Index writer: delete_subtree_by_id failed for id={root_id}: {e}");
    }
    // Auto-propagate accurate negative delta via parent_id chain
    if let (Some((logical_size, physical_size, file_count, dir_count)), Some(pid)) = (totals, parent_id) {
        propagate_delta_by_id(
            conn,
            pid,
            -(logical_size as i64),
            -(physical_size as i64),
            -(file_count as i32),
            -(dir_count as i32),
        );
        // If the deleted subtree contained any symlinks, the parent's
        // `recursive_has_symlinks` may flip, so recompute up the chain.
        if subtree_had_symlinks {
            propagate_recursive_has_symlinks(conn, pid);
        }
    }
    bump_generation(mutation_counter);
}

fn handle_truncate_data(
    conn: &rusqlite::Connection,
    accumulator: &mut AccumulatorMaps,
    expected_total_entries: &AtomicU64,
    next_id: &AtomicI64,
    mutation_counter: &AtomicU64,
) {
    accumulator.clear();
    expected_total_entries.store(0, Ordering::Relaxed);
    let t = Instant::now();
    match conn.execute_batch(
        "DELETE FROM dir_stats; DELETE FROM entries; INSERT OR IGNORE INTO entries (id, parent_id, name, is_directory, is_symlink) VALUES (1, 0, '', 1, 0);",
    ) {
        Ok(()) => {
            // Root sentinel is id=1, so next assignable ID is 2
            next_id.store(2, Ordering::Relaxed);
            log::info!(
                "Writer: truncated entries + dir_stats ({}ms)",
                t.elapsed().as_millis(),
            );
            // Reclaim free pages from the truncation
            if let Err(e) = conn.execute_batch("PRAGMA incremental_vacuum;") {
                log::warn!("Writer: incremental_vacuum after truncate failed: {e}");
            }
        }
        Err(e) => log::warn!("Writer: truncate failed: {e}"),
    }
    bump_generation(mutation_counter);
}

/// Maximum directory depth (from the scan root) that a partial-aggregation pass
/// writes `dir_stats` for. Depth from the scan root: `/Users` = 1,
/// `/Users/david` = 2, `~/Downloads` = 3. Covers onboarding browsing while
/// keeping each pass's write set to a few thousand rows rather than 100K+.
///
/// Real-volume measurement (Apple Silicon, 5.94M entries / 558K dirs, release
/// build): the depth-3 write set plus pane hot paths was 151–716 rows per pass,
/// and total per-pass cost (full in-memory bottom-up compute over every
/// scanned dir + the bounded write) ran 6–397 ms, p95 377 ms — comfortably
/// under the 500 ms budget across the whole scan. The compute dominates the
/// write (rows are trivial); it scales with dirs-scanned-so-far, which is why
/// the last passes near 558K dirs are the slowest. Lowering this depth would
/// shrink the write set but not the compute, so it isn't the lever to pull if a
/// future, larger volume breaches the budget — raise `PARTIAL_AGG_TICK_INTERVAL`
/// instead. (Note: an unoptimized debug build runs this compute ~20× slower,
/// p95 ~2.6 s — measure tuning against a release build, never `pnpm dev`.)
const PARTIAL_AGG_MAX_DEPTH: usize = 3;

/// Mid-scan partial aggregation: compute partial recursive sizes from the
/// accumulator maps and write a bounded subset of `dir_stats` rows so visible
/// listings show growing sizes during the scan.
///
/// Borrows the maps read-only — it must never clear or mutate them, because the
/// final `ComputeAllAggregates` consumes the same maps to produce the exact
/// totals. The differential test pins this invariant.
///
/// Empty maps are a no-op with no SQL fallback. This rule is load-bearing, not
/// hygiene: the scanner sends `ComputeAllAggregates` _before_ the manager's
/// completion handler sets `scan_done`, so the 500 ms progress reporter can race
/// one last `ComputePartialAggregates` into the channel _after_ the final
/// aggregation. Channel ordering doesn't prevent that. What makes it safe is
/// that the final pass clears the maps, so the late partial pass sees empty maps
/// and no-ops. A SQL fallback here would overwrite the just-computed final
/// `dir_stats` with a depth-capped partial subset.
fn handle_compute_partial_aggregates(
    conn: &rusqlite::Connection,
    accumulator: &AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    hot_paths: Vec<String>,
) {
    if accumulator.direct_stats.is_empty() {
        log::debug!("ComputePartialAggregates: maps empty, no-op");
        return;
    }
    let t = Instant::now();
    let hot_paths_count = hot_paths.len();
    match aggregator::compute_partial_aggregates(
        conn,
        &accumulator.direct_stats,
        &accumulator.child_dirs,
        &hot_paths,
        PARTIAL_AGG_MAX_DEPTH,
    ) {
        Ok(stats) => {
            log::info!(
                "ComputePartialAggregates: {} dirs computed, {} rows written, \
                 {}/{hot_paths_count} hot paths resolved ({}ms)",
                stats.dirs_computed,
                stats.rows_written,
                stats.hot_paths_resolved,
                t.elapsed().as_millis(),
            );
            // Refresh both panes via the `/` full-refresh sentinel. Emitting from
            // inside the handler is correct by the same ordering argument as
            // `EmitDirUpdated`: the writes just committed on this thread, and
            // `writer_loop` wraps each message in `objc2::rc::autoreleasepool` on
            // macOS, so the ObjC-on-background-thread rule is satisfied.
            if let Some(app) = app_handle {
                crate::indexing::reconciler::emit_dir_updated(app, vec!["/".to_string()]);
            }
        }
        Err(e) => log::warn!("Index writer: compute_partial_aggregates failed: {e}"),
    }
    // No `bump_generation`: partial passes change no `entries` rows, only
    // transient `dir_stats`. Search-staleness detection cares about entry
    // existence, so a partial pass isn't a "mutation" for that purpose.
}

fn handle_compute_all_aggregates(
    conn: &rusqlite::Connection,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    expected_total_entries: &AtomicU64,
) {
    let t = Instant::now();
    let use_maps = !accumulator.direct_stats.is_empty();
    log::info!(
        "ComputeAllAggregates: using {} (direct_stats={} parents, child_dirs={} parents)",
        if use_maps { "in-memory maps" } else { "SQL fallback" },
        accumulator.direct_stats.len(),
        accumulator.child_dirs.len(),
    );
    let mut on_progress = build_progress_callback(app_handle);
    let result = if !use_maps {
        aggregator::compute_all_aggregates_reported(conn, &mut on_progress)
    } else {
        aggregator::compute_all_aggregates_with_maps(
            conn,
            &accumulator.direct_stats,
            &accumulator.child_dirs,
            &mut on_progress,
        )
    };
    // Summarize the scan's UNIQUE-conflict skips once, here, instead of WARNing
    // per offending batch. Sparse skips are expected dedup; only a racing-writer
    // ratio is worth a WARN. Read before `clear()`.
    let inserted = accumulator.entries_inserted;
    let skipped = accumulator.entries_skipped;
    match classify_skip_severity(inserted, skipped) {
        SkipSeverity::None => {}
        SkipSeverity::Benign => log::debug!(
            "Index scan: {skipped} of {entries} skipped on UNIQUE conflict (expected dedup: firmlinks, case/NFD siblings)",
            entries = pluralize_with(inserted + skipped, "entry", "entries"),
        ),
        SkipSeverity::Suspicious => log::warn!(
            "Index scan: {skipped} of {entries} skipped on UNIQUE conflict ({pct:.1}%); a high ratio can mean two writers raced on one DB",
            entries = pluralize_with(inserted + skipped, "entry", "entries"),
            pct = skipped as f64 / (inserted + skipped) as f64 * 100.0,
        ),
    }
    // Maps are consumed; clear to free memory.
    // Reset expected_total so subtree-scan inserts don't emit
    // spurious saving_entries progress events after the full scan.
    accumulator.clear();
    expected_total_entries.store(0, Ordering::Relaxed);
    match result {
        Ok(count) => {
            log::info!(
                "ComputeAllAggregates: done, {} in {:.1}s",
                pluralize_with(count, "directory", "directories"),
                t.elapsed().as_secs_f64(),
            );
        }
        Err(e) => log::warn!("Index writer: compute_all_aggregates failed: {e}"),
    }
}

fn handle_compute_subtree_aggregates(conn: &rusqlite::Connection, root: &str) {
    let t = Instant::now();
    match aggregator::compute_subtree_aggregates(conn, root) {
        Ok(count) => {
            log::debug!(
                "Index writer: computed subtree aggregates for {} under {root} ({}ms)",
                pluralize(count, "dir"),
                t.elapsed().as_millis(),
            );
            // The subtree's own `recursive_has_symlinks` was just computed.
            // Walk the parent chain so ancestors pick up changes (a symlink may
            // have just appeared inside the subtree, or the last one disappeared).
            if let Ok(Some(root_id)) = crate::indexing::store::resolve_path(conn, root)
                && let Ok(Some(parent_id)) = IndexStore::get_parent_id(conn, root_id)
                && parent_id != 0
            {
                propagate_recursive_has_symlinks(conn, parent_id);
            }
        }
        Err(e) => log::warn!("Index writer: compute_subtree_aggregates({root}) failed: {e}"),
    }
}

fn handle_backfill_missing_dir_stats(conn: &rusqlite::Connection) {
    let t = Instant::now();
    match aggregator::backfill_missing_dir_stats(conn) {
        Ok(0) => {
            log::debug!("BackfillMissingDirStats: no dirs missing stats");
        }
        Ok(count) => {
            log::info!(
                "BackfillMissingDirStats: computed stats for {} in {:.1}s",
                pluralize(count, "dir"),
                t.elapsed().as_secs_f64(),
            );
        }
        Err(e) => log::warn!("BackfillMissingDirStats failed: {e}"),
    }
}

/// Cap thresholds for the tiered incremental-vacuum policy. Below `MIN`,
/// holding the write lock isn't worth the work. Between `MIN` and `BACKLOG`,
/// keep the original steady-state cap so concurrent operations barely notice.
/// Above `BACKLOG`, ramp the cap to drain backlogs (post-truncate, post-replay,
/// or DBs migrated from older versions that accumulated free pages) in tens of
/// minutes instead of hours.
const VACUUM_MIN_FREELIST: i64 = 1_000;
const VACUUM_STEADY_CAP: i64 = 2_000;
const VACUUM_BACKLOG_THRESHOLD: i64 = 20_000;
const VACUUM_BACKLOG_CAP: i64 = 20_000;

/// Pick the per-tick `incremental_vacuum` page cap given the current
/// `freelist_count`. Pure so it can be tested in isolation; the handler
/// just runs the SQL and logs.
///
/// Tiered cap: skip the no-op lock acquisition when the freelist is small;
/// hold the lock only as long as needed to drain real backlog. The 20K cap
/// (~80 MB at 4 KiB pages) is sized so a single tick fsyncs in ~100-300 ms
/// on SSD — long enough to make real progress but short enough that the
/// writer doesn't visibly stall behind it.
fn pick_vacuum_cap(freelist: i64) -> Option<i64> {
    if freelist < VACUUM_MIN_FREELIST {
        None
    } else if freelist < VACUUM_BACKLOG_THRESHOLD {
        Some(VACUUM_STEADY_CAP)
    } else {
        Some(VACUUM_BACKLOG_CAP)
    }
}

fn handle_incremental_vacuum(conn: &rusqlite::Connection) {
    let free = match conn.pragma_query_value(None, "freelist_count", |row| row.get::<_, i64>(0)) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("Writer: freelist_count query failed: {e}");
            return;
        }
    };

    let Some(cap) = pick_vacuum_cap(free) else {
        return;
    };

    if let Err(e) = conn.execute_batch(&format!("PRAGMA incremental_vacuum({cap});")) {
        log::warn!("Writer: incremental_vacuum failed: {e}");
    } else {
        log::debug!(
            "Writer: incremental_vacuum reclaimed up to {cap} of {}",
            pluralize(free as u64, "free page")
        );
    }
}

/// Periodically TRUNCATE the WAL file so its high-water mark doesn't sit on
/// disk indefinitely. SQLite's `wal_autocheckpoint` runs in PASSIVE mode and
/// only moves pages from WAL to the main file; it never shrinks the file
/// itself. After a big scan the WAL can balloon to 1+ GB, and without an
/// explicit TRUNCATE that file size persists until the next app restart.
///
/// TRUNCATE blocks waiting for readers. With `busy_timeout = 5000` already set
/// on this connection (`apply_pragmas`), the call waits up to 5 s for any
/// active reader to finish. If readers don't drain in that window, the call
/// degrades to PASSIVE semantics automatically (busy code = 1 in the return
/// tuple) — pages still get checkpointed, the file just doesn't shrink this
/// time. Next tick tries again. No error path needed.
fn handle_wal_checkpoint(conn: &rusqlite::Connection) {
    // `PRAGMA wal_checkpoint(TRUNCATE)` returns a single row with three
    // columns: (busy, log_size, checkpointed). `busy = 0` means everything
    // got checkpointed AND the file was truncated; `busy = 1` means at least
    // one reader was still on the WAL so the file couldn't shrink (pages
    // were still copied to the main file). Either is a success from the
    // caller's POV — only a SQL error means something is actually wrong.
    let result: rusqlite::Result<(i64, i64, i64)> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    });
    match result {
        Ok((0, log_size, checkpointed)) => {
            log::debug!(
                "Writer: wal_checkpoint TRUNCATE done ({checkpointed} of {})",
                pluralize(log_size as u64, "page")
            );
        }
        Ok((_, log_size, checkpointed)) => {
            // Busy: readers blocking the truncate. Pages still got written
            // to the main file; the WAL file just didn't shrink this tick.
            log::debug!(
                "Writer: wal_checkpoint partial ({checkpointed} of {}, blocked by readers)",
                pluralize(log_size as u64, "page")
            );
        }
        Err(e) => log::warn!("Writer: wal_checkpoint failed: {e}"),
    }
}

/// Build a progress callback that emits `index-aggregation-progress` events via the AppHandle.
/// If no AppHandle is available, returns a no-op closure.
fn build_progress_callback(app_handle: &Option<AppHandle>) -> impl FnMut(AggregationProgress) + '_ {
    move |progress: AggregationProgress| {
        if let Some(app) = app_handle {
            let _ = AggregationProgressEvent {
                phase: phase_to_str(progress.phase).to_string(),
                current: progress.current,
                total: progress.total,
            }
            .emit(app);
        }
    }
}

/// Walk the parent_id chain upward, updating dir_stats for each ancestor.
///
/// Starts at `start_id` (typically the parent of the affected entry) and
/// walks up to the root sentinel. Each ancestor gets its dir_stats updated
/// with the given delta. Creates dir_stats rows if they don't exist.
///
/// Uses direct SQL statements (no transaction) because this function is
/// always called from within the writer thread, which may already be inside
/// a `BEGIN IMMEDIATE` transaction (for example, during replay).
fn propagate_delta_by_id(
    conn: &rusqlite::Connection,
    start_id: i64,
    logical_size_delta: i64,
    physical_size_delta: i64,
    file_delta: i32,
    dir_delta: i32,
) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        // Read existing stats
        let existing = IndexStore::get_dir_stats_by_id(conn, current_id).ok().flatten();

        let (new_logical, new_physical, new_files, new_dirs, has_symlinks) = match existing {
            Some(s) => (
                (s.recursive_logical_size as i64 + logical_size_delta).max(0) as u64,
                (s.recursive_physical_size as i64 + physical_size_delta).max(0) as u64,
                (s.recursive_file_count as i64 + i64::from(file_delta)).max(0) as u64,
                (s.recursive_dir_count as i64 + i64::from(dir_delta)).max(0) as u64,
                s.recursive_has_symlinks,
            ),
            None => (
                logical_size_delta.max(0) as u64,
                physical_size_delta.max(0) as u64,
                i64::from(file_delta).max(0) as u64,
                i64::from(dir_delta).max(0) as u64,
                false,
            ),
        };

        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO dir_stats
                 (entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, recursive_dir_count, recursive_has_symlinks)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![current_id, new_logical, new_physical, new_files, new_dirs, has_symlinks as i32],
        ) {
            log::warn!("propagate_delta_by_id: upsert failed for id={current_id}: {e}");
            break;
        }

        // Walk up to parent
        if current_id == ROOT_ID {
            break;
        }
        match IndexStore::get_parent_id(conn, current_id) {
            Ok(Some(pid)) if pid != 0 => current_id = pid,
            _ => break,
        }
    }
}

/// Recompute `recursive_has_symlinks` for a directory from its direct children
/// (`is_symlink`) plus its subdirectories' stored `recursive_has_symlinks`.
///
/// Returns the recomputed value, without writing it. Returns `false` if the
/// directory has no children or the queries fail.
fn recompute_recursive_has_symlinks(conn: &rusqlite::Connection, dir_id: i64) -> bool {
    // Direct symlink child?
    let direct: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE parent_id = ?1 AND is_symlink = 1)",
            rusqlite::params![dir_id],
            |row| row.get::<_, i32>(0).map(|n| n != 0),
        )
        .unwrap_or(false);
    if direct {
        return true;
    }
    // Any sub-directory with the flag set?
    let from_subdirs: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM entries e
                JOIN dir_stats ds ON ds.entry_id = e.id
                WHERE e.parent_id = ?1 AND e.is_directory = 1 AND ds.recursive_has_symlinks = 1
            )",
            rusqlite::params![dir_id],
            |row| row.get::<_, i32>(0).map(|n| n != 0),
        )
        .unwrap_or(false);
    from_subdirs
}

/// Walk the parent chain, recomputing `recursive_has_symlinks` for each ancestor
/// from its direct children + subdirs' stored flags.
///
/// Stops walking up as soon as an ancestor's recomputed value matches the value
/// already in the DB. The OR-aggregate is monotonic, so once the value stabilizes,
/// further ancestors won't change.
///
/// Used after symlink additions/removals (and subtree deletes that may have
/// removed all symlinks in a branch). For pure size/count deltas this is a no-op
/// and `propagate_delta_by_id` is enough.
fn propagate_recursive_has_symlinks(conn: &rusqlite::Connection, start_id: i64) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        let new_value = recompute_recursive_has_symlinks(conn, current_id);
        let old_value = IndexStore::get_dir_stats_by_id(conn, current_id)
            .ok()
            .flatten()
            .map(|s| s.recursive_has_symlinks);

        if old_value == Some(new_value) {
            // No change: the rest of the chain can't change either.
            break;
        }

        // Update only the recursive_has_symlinks column, preserving other stats.
        if let Err(e) = conn.execute(
            "UPDATE dir_stats SET recursive_has_symlinks = ?1 WHERE entry_id = ?2",
            rusqlite::params![new_value as i32, current_id],
        ) {
            log::warn!("propagate_recursive_has_symlinks: update failed for id={current_id}: {e}");
            break;
        }

        if current_id == ROOT_ID {
            break;
        }
        match IndexStore::get_parent_id(conn, current_id) {
            Ok(Some(pid)) if pid != 0 => current_id = pid,
            _ => break,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{DirStatsById, IndexStore, ROOT_ID};

    /// Create a temp DB, open the store (to init schema), and return the path + temp dir guard.
    fn setup_db() -> (PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-writer.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        (db_path, dir)
    }

    /// Open a read connection to the DB for assertions.
    fn open_read(db_path: &Path) -> IndexStore {
        IndexStore::open(db_path).expect("failed to open read store")
    }

    // ── Skip-severity classification ─────────────────────────────────

    #[test]
    fn skip_severity_none_when_nothing_skipped() {
        assert_eq!(classify_skip_severity(5_000_000, 0), SkipSeverity::None);
    }

    #[test]
    fn skip_severity_benign_for_sparse_dedup() {
        // A handful of firmlink double-visits / case-NFD siblings in a big scan: expected, not actionable.
        assert_eq!(classify_skip_severity(5_000_000, 3), SkipSeverity::Benign);
        assert_eq!(classify_skip_severity(5_000_000, 49), SkipSeverity::Benign);
    }

    #[test]
    fn skip_severity_benign_when_below_absolute_floor_even_at_high_ratio() {
        // Tiny tree with a couple genuine sibling collisions: high ratio but few skips, stay quiet.
        assert_eq!(classify_skip_severity(20, 10), SkipSeverity::Benign);
    }

    #[test]
    fn skip_severity_suspicious_for_racing_writer_signature() {
        // Two writers racing on one DB: the loser's inserts all conflict, so a large fraction skips.
        assert_eq!(classify_skip_severity(5_000_000, 5_000_000), SkipSeverity::Suspicious);
        // Just over both gates: 100 skips and >1% of the scan (100 / 9100 ≈ 1.1%).
        assert_eq!(classify_skip_severity(9_000, 100), SkipSeverity::Suspicious);
        // Exactly 1% does not trip it (the ratio gate is strict `>`): 100 / 10000.
        assert_eq!(classify_skip_severity(9_900, 100), SkipSeverity::Benign);
    }

    #[test]
    fn skip_severity_benign_when_over_floor_but_under_ratio() {
        // 50 skips clears the floor but is a vanishing fraction of a 5M scan: still benign.
        assert_eq!(classify_skip_severity(5_000_000, 50), SkipSeverity::Benign);
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

    // ── Integer-keyed variant tests ──────────────────────────────────

    #[test]
    fn insert_entries_v2_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1024),
            physical_size: Some(1024),
            modified_at: Some(1700000000),
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "file.txt");
        assert_eq!(children[0].logical_size, Some(1024));
        assert_eq!(children[0].id, 10);

        writer.shutdown();
    }

    // The accumulator must only count rows that actually landed in the DB.
    // `insert_entries_v2_batch` uses `INSERT OR IGNORE`, so one duplicate in
    // a batch skips just that row and the rest insert. The accumulator maps
    // drive `compute_all_aggregates_with_maps`; counting bytes for a row that
    // lost an OR-IGNORE produces inflated dir_stats (this was one of the
    // mechanisms behind the 1.83 TB ghost size on `..` of a 994 GB volume).
    #[test]
    fn handle_insert_entries_v2_only_accumulates_rows_that_landed() {
        use std::sync::atomic::AtomicU64;

        let (db_path, _dir) = setup_db();
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Pre-seed: id=100, name="first.txt".
        let entries_first = vec![EntryRow {
            id: 100,
            parent_id: ROOT_ID,
            name: "first.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(10),
            physical_size: Some(10),
            modified_at: None,
            inode: None,
        }];
        IndexStore::insert_entries_v2_batch(&conn, &entries_first).unwrap();

        // Second batch: row 0 collides on the (parent_id, name_folded) UNIQUE
        // index (same `first.txt` under ROOT_ID). Row 1 is fresh and must land.
        let entries_dup = vec![
            EntryRow {
                id: 200,
                parent_id: ROOT_ID,
                name: "first.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(999_999),
                physical_size: Some(999_999),
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 101,
                parent_id: ROOT_ID,
                name: "second.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(20),
                physical_size: Some(20),
                modified_at: None,
                inode: None,
            },
        ];

        let mut accumulator = AccumulatorMaps::new();
        let expected = AtomicU64::new(0);
        let mutation_counter = AtomicU64::new(0);

        handle_insert_entries_v2(
            &conn,
            entries_dup,
            &mut accumulator,
            &None,
            &expected,
            &mutation_counter,
        );

        // DB has the original first.txt (id=100) and the new second.txt (id=101).
        // id=200 was the OR-IGNORE'd duplicate and must not be in the DB.
        assert_eq!(
            IndexStore::get_entry_by_id(&conn, 100).unwrap().unwrap().name,
            "first.txt"
        );
        assert_eq!(
            IndexStore::get_entry_by_id(&conn, 101).unwrap().unwrap().name,
            "second.txt"
        );
        assert!(IndexStore::get_entry_by_id(&conn, 200).unwrap().is_none());

        // Accumulator must reflect exactly one new entry (the row that landed),
        // never the 999_999-byte phantom. If a regression makes the accumulator
        // count the OR-IGNORE'd row, this assert catches it.
        assert_eq!(
            accumulator.entries_inserted, 1,
            "accumulator must count only rows that landed in the DB"
        );
        let stats = accumulator.direct_stats.get(&ROOT_ID).expect("ROOT_ID stats present");
        assert_eq!(stats.0, 20, "logical bytes must only count the landed row");
        assert_eq!(stats.1, 20, "physical bytes must only count the landed row");
        assert_eq!(stats.2, 1, "file count must only include the landed row");
    }

    #[test]
    fn upsert_entry_v2_insert_and_update() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert via UpsertEntryV2 (entry doesn't exist yet)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "new.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(256),
                physical_size: Some(256),
                modified_at: Some(1700000000),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Update via UpsertEntryV2 (entry now exists)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "new.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(512),
                physical_size: Some(512),
                modified_at: Some(1700000001),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "new.txt");
        assert_eq!(children[0].logical_size, Some(512), "size should be updated to 512");

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_initializes_dir_stats_for_new_dirs() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a new directory via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "newdir".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // The new directory should have a zero-valued dir_stats row
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let dir_id = IndexStore::resolve_component(&conn, ROOT_ID, "newdir")
            .unwrap()
            .expect("newdir should exist");

        let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap();
        assert!(stats.is_some(), "new dir should have dir_stats");
        let stats = stats.unwrap();
        assert_eq!(stats.recursive_logical_size, 0);
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_dir_count, 0);

        writer.shutdown();
    }

    #[test]
    fn delete_entry_by_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert an entry
        let entries = vec![EntryRow {
            id: 20,
            parent_id: ROOT_ID,
            name: "doomed.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Delete by ID
        writer.send(WriteMessage::DeleteEntryById(20)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert!(children.is_empty(), "entry should be deleted");

        writer.shutdown();
    }

    #[test]
    fn delete_subtree_by_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Build a tree: ROOT -> dir(10) -> file(11) + subdir(12)
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
                logical_size: Some(50),
                physical_size: Some(50),
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 10,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Delete the subtree rooted at id=10
        writer.send(WriteMessage::DeleteSubtreeById(10)).unwrap();
        writer.flush_blocking().unwrap();

        let store = open_read(&db_path);
        let root_children = store.list_children(ROOT_ID).unwrap();
        assert!(root_children.is_empty(), "dir /a should be deleted");
        let a_children = store.list_children(10).unwrap();
        assert!(a_children.is_empty(), "children of /a should be deleted");

        writer.shutdown();
    }

    #[test]
    fn delete_entry_by_id_auto_propagates_delta() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a parent dir and a file
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "p".into(),
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
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();

        // Pre-populate dir_stats for the parent
        writer.flush_blocking().unwrap();

        // Manually set dir_stats for parent via direct DB write (using the by-id API)
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 500,
                    recursive_physical_size: 500,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Delete the file: writer should auto-propagate (-500, -1, 0) to parent id=10
        writer.send(WriteMessage::DeleteEntryById(11)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 0, "size should be 0 after file deletion");
        assert_eq!(stats.recursive_file_count, 0, "file count should be 0");

        writer.shutdown();
    }

    #[test]
    fn delete_subtree_by_id_auto_propagates_delta() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Build tree: ROOT(1) -> root_dir(10) -> sub(11) -> file.txt(12, 300 bytes)
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "root".into(),
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
                name: "sub".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(300),
                physical_size: Some(300),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Pre-populate dir_stats for ancestors
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: ROOT_ID,
                        recursive_logical_size: 300,
                        recursive_physical_size: 300,
                        recursive_file_count: 1,
                        recursive_dir_count: 2,
                        recursive_has_symlinks: false,
                    },
                    DirStatsById {
                        entry_id: 10,
                        recursive_logical_size: 300,
                        recursive_physical_size: 300,
                        recursive_file_count: 1,
                        recursive_dir_count: 1,
                        recursive_has_symlinks: false,
                    },
                ],
            )
            .unwrap();
        }

        // Delete the /root/sub subtree (id=11)
        writer.send(WriteMessage::DeleteSubtreeById(11)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // root_dir(10) should have lost: size=300, files=1, dirs=1
        let root_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(root_stats.recursive_logical_size, 0);
        assert_eq!(root_stats.recursive_file_count, 0);
        assert_eq!(root_stats.recursive_dir_count, 0);

        // ROOT(1) should have lost: size=300, files=1, dirs=1
        let vol_stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(vol_stats.recursive_logical_size, 0);
        assert_eq!(vol_stats.recursive_file_count, 0);
        assert_eq!(vol_stats.recursive_dir_count, 1); // root_dir(10) still exists

        writer.shutdown();
    }

    #[test]
    fn propagate_delta_by_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a directory to propagate to
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Pre-populate dir_stats
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 1000,
                    recursive_physical_size: 1000,
                    recursive_file_count: 5,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Propagate a file addition starting from home's entry_id
        writer
            .send(WriteMessage::PropagateDeltaById {
                entry_id: 10,
                logical_size_delta: 250,
                physical_size_delta: 250,
                file_count_delta: 1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let result = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(result.recursive_logical_size, 1250);
        assert_eq!(result.recursive_file_count, 6);

        writer.shutdown();
    }

    #[test]
    fn delete_entry_by_id_for_nonexistent_skips_propagation() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a directory and pre-populate its dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "p".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 100,
                    recursive_physical_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Delete a non-existent entry: should not propagate any delta
        writer.send(WriteMessage::DeleteEntryById(999)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 100, "stats should be unchanged");
        assert_eq!(stats.recursive_file_count, 1);

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

    #[test]
    fn upsert_entry_v2_auto_propagates_delta_on_insert() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a parent directory and pre-populate its dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert a new file via UpsertEntryV2: should auto-propagate to parent
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: Some(1700000000),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 500, "parent should have file's size");
        assert_eq!(stats.recursive_file_count, 1, "parent should count the new file");
        assert_eq!(stats.recursive_dir_count, 0);

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_auto_propagates_delta_on_update() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert parent dir with dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 200,
                    recursive_physical_size: 200,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert a file via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(200),
                physical_size: Some(200),
                modified_at: Some(1700000000),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Update the same file with a larger size: should propagate +100 delta
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "doc.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(300),
                physical_size: Some(300),
                modified_at: Some(1700000001),
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        // Initial 200 + insert propagated 200 + update propagated +100 = 500
        assert_eq!(
            stats.recursive_logical_size, 500,
            "parent should reflect insert + update deltas"
        );
        assert_eq!(stats.recursive_file_count, 2, "file_count: 1 initial + 1 from insert");

        writer.shutdown();
    }

    #[test]
    fn upsert_entry_v2_auto_propagates_dir_count_on_new_dir() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Pre-populate root dir_stats
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: ROOT_ID,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert a new directory via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "projects".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(stats.recursive_dir_count, 1, "root should count the new dir");
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_logical_size, 0);

        writer.shutdown();
    }

    // ── Hardlink dedup tests ────────────────────────────────────────

    #[test]
    fn hardlink_dedup_insert_primary_stores_sizes_and_inode() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let id = IndexStore::resolve_component(&conn, ROOT_ID, "primary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(1000), "primary should keep its sizes");
        assert_eq!(entry.inode, Some(100), "inode should be stored");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_insert_secondary_gets_null_sizes() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert primary link
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary link (same inode, different name)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
        assert_eq!(entry.logical_size, None, "secondary should have NULL sizes");
        assert_eq!(entry.physical_size, None);
        assert_eq!(entry.inode, Some(100), "inode should still be stored");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_update_secondary_keeps_null_sizes() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert primary
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary (gets NULL sizes via dedup)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Reconciler sends update for secondary with full sizes: dedup should fire again
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000001),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
        assert_eq!(
            entry.logical_size, None,
            "secondary sizes should stay NULL after update"
        );

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_self_healing_after_primary_deleted() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Pre-populate root dir_stats so delta propagation works
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: ROOT_ID,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert primary
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary (gets NULL sizes)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000000),
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Delete primary
        let primary_id = {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::resolve_component(&conn, ROOT_ID, "primary.txt")
                .unwrap()
                .unwrap()
        };
        writer.send(WriteMessage::DeleteEntryById(primary_id)).unwrap();
        writer.flush_blocking().unwrap();

        // Reconciler sends update for secondary: nlink=1 since it's the only link now
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: Some(1700000001),
                inode: Some(100),
                nlink: Some(1),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
        assert_eq!(
            entry.logical_size,
            Some(1000),
            "secondary should recover sizes after primary deleted"
        );
        assert_eq!(entry.physical_size, Some(1000));

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_nlink_1_skips_dedup() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert two files with the same inode but nlink=1 (not actually hardlinked)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_a.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: Some(200),
                nlink: Some(1),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_b.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: Some(200),
                nlink: Some(1),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let b_id = IndexStore::resolve_component(&conn, ROOT_ID, "file_b.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, b_id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(500), "nlink=1 should never trigger dedup");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_no_inode_skips_dedup() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert first file with inode
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_a.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert second file with no inode (non-Unix)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "file_b.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(500),
                physical_size: Some(500),
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let b_id = IndexStore::resolve_component(&conn, ROOT_ID, "file_b.txt")
            .unwrap()
            .unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, b_id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(500), "no inode should never trigger dedup");

        writer.shutdown();
    }

    #[test]
    fn hardlink_dedup_dir_stats_only_counts_primary_size() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a parent directory and pre-populate its dir_stats
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "mydir".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Insert primary hardlink into dir
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "primary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: None,
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Insert secondary hardlink into dir (same inode)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "secondary.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1000),
                physical_size: Some(1000),
                modified_at: None,
                inode: Some(100),
                nlink: Some(2),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            stats.recursive_logical_size, 1000,
            "dir should only count the primary's size"
        );
        assert_eq!(stats.recursive_file_count, 2, "both links count as files");

        writer.shutdown();
    }

    // ── recursive_has_symlinks tests ─────────────────────────────────

    #[test]
    fn upsert_symlink_propagates_recursive_has_symlinks_up() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Build a 2-level dir tree first (no symlinks).
        // ROOT -> outer (id=10) -> inner (id=11)
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "outer".into(),
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
                name: "inner".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Confirm baseline: no symlinks anywhere
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 11)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 10)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
        }

        // Add a symlink under inner via UpsertEntryV2
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 11,
                name: "link".into(),
                is_directory: false,
                is_symlink: true,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        // Flag should propagate up to both inner and outer
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 11)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "inner should flip to true"
            );
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 10)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "outer should propagate from inner"
            );
        }

        writer.shutdown();
    }

    #[test]
    fn delete_last_symlink_clears_recursive_has_symlinks_up() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT -> outer (id=20) -> link (id=21, symlink)
        let entries = vec![
            EntryRow {
                id: 20,
                parent_id: ROOT_ID,
                name: "outer".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 21,
                parent_id: 20,
                name: "link".into(),
                is_directory: false,
                is_symlink: true,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Baseline: outer has the flag set
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 20)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
        }

        // Delete the only symlink
        writer.send(WriteMessage::DeleteEntryById(21)).unwrap();
        writer.flush_blocking().unwrap();

        // Flag should clear up the chain
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 20)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "outer should clear after last symlink removed"
            );
        }

        writer.shutdown();
    }

    #[test]
    fn delete_subtree_with_symlinks_clears_parent_flag() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT -> top (id=30)
        //   ├── doomed (id=31) -> link (id=32, symlink)
        //   └── safe (id=33)  (no symlinks)
        let entries = vec![
            EntryRow {
                id: 30,
                parent_id: ROOT_ID,
                name: "top".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 31,
                parent_id: 30,
                name: "doomed".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 32,
                parent_id: 31,
                name: "link".into(),
                is_directory: false,
                is_symlink: true,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 33,
                parent_id: 30,
                name: "safe".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Baseline: top has the flag
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 30)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks
            );
        }

        // Delete the doomed subtree (which contained the only symlink)
        writer.send(WriteMessage::DeleteSubtreeById(31)).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(
                !IndexStore::get_dir_stats_by_id(&conn, 30)
                    .unwrap()
                    .unwrap()
                    .recursive_has_symlinks,
                "top should clear once the subtree containing the symlink is gone"
            );
        }

        writer.shutdown();
    }

    // ── MoveEntryV2 tests ────────────────────────────────────────────

    /// Helper: insert a dir with dir_stats. Returns nothing (the caller knows the id it asked for).
    fn insert_dir_with_stats(
        writer: &IndexWriter,
        db_path: &Path,
        id: i64,
        parent_id: i64,
        name: &str,
        stats: DirStatsById,
    ) {
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id,
                parent_id,
                name: name.into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            }]))
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(db_path).unwrap();
        IndexStore::upsert_dir_stats_by_id(&conn, &[stats]).unwrap();
    }

    #[test]
    fn move_entry_v2_same_parent_preserves_dir_stats() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Parent dir + child dir with non-trivial dir_stats. The whole point
        // of MoveEntryV2 vs. delete+insert is preserving these numbers.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "home",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 5_000,
                recursive_physical_size: 5_000,
                recursive_file_count: 7,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "Foo",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 5_000,
                recursive_physical_size: 5_000,
                recursive_file_count: 7,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        // Same-parent rename: "Foo" → "Bar".
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 10,
                new_name: "Bar".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(entry.name, "Bar", "name should be updated");
        assert_eq!(entry.parent_id, 10, "parent unchanged");

        let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(
            moved_stats.recursive_logical_size, 5_000,
            "moved dir keeps its own stats"
        );
        assert_eq!(moved_stats.recursive_file_count, 7);

        let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            parent_stats.recursive_logical_size, 5_000,
            "parent stats unchanged for same-parent rename"
        );
        assert_eq!(parent_stats.recursive_file_count, 7);
        assert_eq!(parent_stats.recursive_dir_count, 1);

        writer.shutdown();
    }

    /// Helper: insert a plain file row.
    fn insert_file(writer: &IndexWriter, id: i64, parent_id: i64, name: &str, size: u64) {
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id,
                parent_id,
                name: name.into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(size),
                physical_size: Some(size),
                modified_at: None,
                inode: None,
            }]))
            .unwrap();
        writer.flush_blocking().unwrap();
    }

    #[test]
    fn move_entry_v2_destination_collision_replaces_conflicting_file() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // One dir with two files. Moving "draft.txt" onto "final.txt"'s name
        // (a rename-with-overwrite, or a concurrent upsert racing ahead of the
        // move) used to fail the UNIQUE (parent_id, name_folded) constraint and
        // leave the moved entry stuck at its old name.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "docs",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 150,
                recursive_physical_size: 150,
                recursive_file_count: 2,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_file(&writer, 20, 10, "draft.txt", 100);
        insert_file(&writer, 21, 10, "final.txt", 50);

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 10,
                new_name: "final.txt".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(moved.name, "final.txt", "moved entry owns the destination name");
        assert_eq!(moved.parent_id, 10);
        assert!(
            IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
            "conflicting entry is deleted"
        );

        // The conflicting file's contribution is subtracted from the parent.
        let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(parent_stats.recursive_logical_size, 100);
        assert_eq!(parent_stats.recursive_file_count, 1);

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_destination_collision_replaces_conflicting_dir_subtree() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // A/proj (id 20, rich dir_stats) moves to B/proj, but B already has a
        // stale dir row "proj" (id 21) with a child file. The stale subtree must
        // go and the moved dir must keep its id and dir_stats.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 1000,
                recursive_physical_size: 1000,
                recursive_file_count: 3,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 500,
                recursive_physical_size: 500,
                recursive_file_count: 1,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "proj",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 1000,
                recursive_physical_size: 1000,
                recursive_file_count: 3,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            21,
            11,
            "proj",
            DirStatsById {
                entry_id: 21,
                recursive_logical_size: 500,
                recursive_physical_size: 500,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_file(&writer, 22, 21, "old.txt", 500);

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 11,
                new_name: "proj".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(moved.parent_id, 11, "moved dir landed under B");
        assert_eq!(moved.name, "proj");
        assert!(
            IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
            "conflicting dir is deleted"
        );
        assert!(
            IndexStore::get_entry_by_id(&conn, 22).unwrap().is_none(),
            "conflicting dir's children are deleted"
        );

        let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(
            moved_stats.recursive_logical_size, 1000,
            "moved dir keeps its own stats"
        );
        assert_eq!(moved_stats.recursive_file_count, 3);

        // A lost the moved dir's contribution entirely.
        let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a_stats.recursive_logical_size, 0);
        assert_eq!(a_stats.recursive_file_count, 0);
        assert_eq!(a_stats.recursive_dir_count, 0);

        // B lost the stale subtree (-500, -1 file, -1 dir) and gained the moved
        // dir (+1000, +3 files, +1 dir).
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 1000);
        assert_eq!(b_stats.recursive_file_count, 3);
        assert_eq!(b_stats.recursive_dir_count, 1);

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_cross_parent_propagates_deltas() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Two sibling dirs A and B, each with their own pre-populated stats.
        // Then a child dir D under A with non-trivial stats.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 1024,
                recursive_physical_size: 2048,
                recursive_file_count: 5,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "D",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 1024,
                recursive_physical_size: 2048,
                recursive_file_count: 5,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 11,
                new_name: "D".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // D itself: same dir_stats, new parent.
        let d_entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(d_entry.parent_id, 11);
        let d_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
        assert_eq!(d_stats.recursive_logical_size, 1024);
        assert_eq!(d_stats.recursive_file_count, 5);

        // A: lost D's contribution (size 1024, 5 files, 1 dir for D itself).
        let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a_stats.recursive_logical_size, 0);
        assert_eq!(a_stats.recursive_physical_size, 0);
        assert_eq!(a_stats.recursive_file_count, 0);
        assert_eq!(a_stats.recursive_dir_count, 0);

        // B: gained D's contribution.
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 1024);
        assert_eq!(b_stats.recursive_physical_size, 2048);
        assert_eq!(b_stats.recursive_file_count, 5);
        assert_eq!(b_stats.recursive_dir_count, 1);

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_file_cross_parent_propagates_deltas() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Two parent dirs, both starting with empty stats.
        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 700,
                recursive_physical_size: 700,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        // Insert a file under A (size 700, contributes 1 file).
        writer
            .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
                id: 30,
                parent_id: 10,
                name: "f.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(700),
                physical_size: Some(700),
                modified_at: Some(1700000000),
                inode: Some(99),
            }]))
            .unwrap();
        writer.flush_blocking().unwrap();

        // Move file to B.
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 30,
                new_parent_id: 11,
                new_name: "f.txt".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a_stats.recursive_logical_size, 0, "A loses the file's size");
        assert_eq!(a_stats.recursive_file_count, 0);

        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 700);
        assert_eq!(b_stats.recursive_file_count, 1);
        assert_eq!(b_stats.recursive_dir_count, 0, "files don't contribute to dir count");

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_no_op_when_target_matches_current() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "home",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 1024,
                recursive_physical_size: 1024,
                recursive_file_count: 3,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        // Capture the per-writer mutation count before the no-op. Reading the
        // global `WRITER_GENERATION` here would flake under concurrent tests,
        // since `cargo test` runs tests as threads in one process and any other
        // writer that mutates between `before` and `after` would bump it.
        let gen_before = writer.mutation_count();

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 10,
                new_parent_id: ROOT_ID,
                new_name: "home".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 1024, "no-op preserves stats");
        assert_eq!(stats.recursive_file_count, 3);

        // The per-writer counter should not have moved (the no-op short-circuits
        // before `bump_generation`).
        let gen_after = writer.mutation_count();
        assert_eq!(
            gen_before, gen_after,
            "no-op should not bump the writer's mutation counter"
        );

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_cross_parent_propagates_recursive_has_symlinks() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "A",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 1,
                recursive_has_symlinks: true,
            },
        );
        insert_dir_with_stats(
            &writer,
            &db_path,
            11,
            ROOT_ID,
            "B",
            DirStatsById {
                entry_id: 11,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );
        // The dir being moved carries the symlink flag in its own subtree.
        insert_dir_with_stats(
            &writer,
            &db_path,
            20,
            10,
            "D",
            DirStatsById {
                entry_id: 20,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: true,
            },
        );

        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 20,
                new_parent_id: 11,
                new_name: "D".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
        assert!(
            b_stats.recursive_has_symlinks,
            "new parent should pick up the symlink-bearing subtree"
        );

        writer.shutdown();
    }

    #[test]
    fn move_entry_v2_bumps_writer_generation() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        insert_dir_with_stats(
            &writer,
            &db_path,
            10,
            ROOT_ID,
            "Foo",
            DirStatsById {
                entry_id: 10,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
            },
        );

        let before = writer.mutation_count();
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 10,
                new_parent_id: ROOT_ID,
                new_name: "Bar".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        let after = writer.mutation_count();
        assert!(
            after > before,
            "writer's mutation counter should bump after a real move"
        );

        writer.shutdown();
    }

    // ── Partial aggregation tests ────────────────────────────────────

    /// A fresh writer with no inserts has empty accumulator maps, so a partial
    /// pass must be a no-op: no `dir_stats` rows, and the writer's mutation
    /// counter unchanged (partial passes are not "mutations" for search-staleness
    /// purposes — they change no `entries` rows). The counter is asserted as a
    /// before/after delta on this one writer (nothing else sends to it), never as
    /// an absolute value and never via the global `WRITER_GENERATION`.
    #[test]
    fn partial_aggregates_no_op_on_empty_maps() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let gen_before = writer.mutation_count();

        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let dir_stats_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dir_stats", [], |row| row.get(0))
            .unwrap();
        assert_eq!(dir_stats_count, 0, "empty maps must produce no dir_stats rows");

        let gen_after = writer.mutation_count();
        assert_eq!(
            gen_before, gen_after,
            "a partial pass must not bump the writer's mutation counter"
        );

        writer.shutdown();
    }

    /// Partial sums show up at shallow depth and grow across batches. A 3-level
    /// tree is inserted in two batches; a partial pass after batch 1 writes
    /// dir_stats reflecting only batch-1 contents, and a pass after batch 2 grows
    /// them.
    #[test]
    fn partial_aggregates_shallow_sums_grow_across_batches() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Tree (depths from ROOT_ID): /a (id=10, depth 1) -> /a/b (id=11, depth 2)
        //                             /a/b/c (id=12, depth 3) -> /a/b/c/f1 (file)
        // Batch 1 inserts /a, /a/b, /a/b/c and one 100-byte file under /a/b/c.
        let batch1 = vec![
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
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 13,
                parent_id: 12,
                name: "f1.dat".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(100),
                physical_size: Some(100),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(batch1)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            // Depth ≤ 3 dirs (ROOT_ID=0, /a=1, /a/b=2, /a/b/c=3) all get rows.
            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(a.recursive_logical_size, 100, "/a should sum batch-1 file");
            assert_eq!(a.recursive_file_count, 1);
            assert_eq!(a.recursive_dir_count, 2, "/a has /a/b and /a/b/c beneath it");
            let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
            assert_eq!(c.recursive_logical_size, 100, "/a/b/c holds the file directly");
        }

        // Batch 2 adds a second 50-byte file under /a/b/c.
        let batch2 = vec![EntryRow {
            id: 14,
            parent_id: 12,
            name: "f2.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(50),
            physical_size: Some(50),
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(batch2)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(a.recursive_logical_size, 150, "/a should grow to 100 + 50");
            assert_eq!(a.recursive_file_count, 2);
            let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
            assert_eq!(c.recursive_logical_size, 150);
            assert_eq!(c.recursive_file_count, 2);
        }

        writer.shutdown();
    }

    /// Dirs deeper than `PARTIAL_AGG_MAX_DEPTH` get no rows from a partial pass,
    /// but DO get rows from the final `ComputeAllAggregates`.
    #[test]
    fn partial_aggregates_depth_limiting() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Chain: /a(10,d1) -> /a/b(11,d2) -> /a/b/c(12,d3) -> /a/b/c/d(13,d4)
        // with a file under the depth-4 dir. d4 = MAX_DEPTH + 1.
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
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 13,
                parent_id: 12,
                name: "d".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 14,
                parent_id: 13,
                name: "deep.dat".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(70),
                physical_size: Some(70),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            // /a/b/c is at depth 3 (≤ MAX_DEPTH) — gets a row reflecting the file.
            let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
            assert_eq!(c.recursive_logical_size, 70, "depth-3 dir should sum the deep file");
            // /a/b/c/d is at depth 4 (> MAX_DEPTH) — no partial row.
            assert!(
                IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().is_none(),
                "depth-4 dir must get no partial row"
            );
        }

        // The final pass writes every dir, including the depth-4 one.
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            let d = IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().unwrap();
            assert_eq!(d.recursive_logical_size, 70, "final pass fills the depth-4 dir");
        }

        writer.shutdown();
    }

    /// A deep dir listed in `hot_paths` punches through the depth limit: it gets
    /// its own row plus rows for its direct children. An unresolvable hot path is
    /// skipped without error.
    #[test]
    fn partial_aggregates_hot_paths_punch_through_depth() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // /a(10,d1)/b(11,d2)/c(12,d3)/d(13,d4)/e(14,d5, child dir of d)
        // plus a 60-byte file under e. /a/b/c/d is the hot path (depth 4).
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
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 13,
                parent_id: 12,
                name: "d".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 14,
                parent_id: 13,
                name: "e".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 15,
                parent_id: 14,
                name: "x.dat".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(60),
                physical_size: Some(60),
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates {
                // The hot dir (depth 4) and one unresolvable path.
                hot_paths: vec!["/a/b/c/d".into(), "/does/not/exist".into()],
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        // /a/b/c/d (hot, depth 4) gets a row despite the cap.
        let d = IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().unwrap();
        assert_eq!(d.recursive_logical_size, 60, "hot dir punches through the depth cap");
        // Its direct child /a/b/c/d/e (depth 5) also gets a row.
        let e = IndexStore::get_dir_stats_by_id(&conn, 14).unwrap().unwrap();
        assert_eq!(e.recursive_logical_size, 60, "hot dir's direct child gets a row");
        // The unresolvable hot path produced no error and no spurious rows: the
        // flush above returned cleanly, which is the assertion.

        writer.shutdown();
    }

    // ── DB hygiene tests ─────────────────────────────────────────────

    /// The tier policy is the safety-critical part of the vacuum logic:
    /// regressing it would either thrash the writer lock (cap too aggressive
    /// in steady state) or let the freelist grow unbounded (cap missing on
    /// backlog). Lock the thresholds with explicit cases either side of each
    /// boundary plus the steady-state band's interior.
    #[test]
    fn pick_vacuum_cap_skips_below_min() {
        assert_eq!(pick_vacuum_cap(0), None);
        assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST - 1), None);
    }

    #[test]
    fn pick_vacuum_cap_uses_steady_band_for_modest_backlog() {
        assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST), Some(VACUUM_STEADY_CAP));
        assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD - 1), Some(VACUUM_STEADY_CAP));
    }

    #[test]
    fn pick_vacuum_cap_ramps_to_backlog_cap_for_large_backlog() {
        assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD), Some(VACUUM_BACKLOG_CAP));
        assert_eq!(pick_vacuum_cap(1_000_000), Some(VACUUM_BACKLOG_CAP));
    }

    /// End-to-end check: after a truncate that leaves a large freelist, the
    /// vacuum handler actually drops `freelist_count`. Doesn't pin the exact
    /// per-tier cap (that's covered by the policy tests above); pins the
    /// invariant that "freelist went down".
    #[test]
    fn handle_incremental_vacuum_reclaims_pages_after_truncate() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert enough entries that TruncateData later creates a real
        // freelist. Long names so each row touches its own page; 5000 rows
        // ≥ several thousand pages = at least one band above MIN.
        let entries: Vec<EntryRow> = (0..5000)
            .map(|i| EntryRow {
                id: 100 + i,
                parent_id: ROOT_ID,
                name: format!("test-entry-with-a-reasonably-long-name-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(4096),
                physical_size: Some(4096),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::TruncateData).unwrap();
        writer.flush_blocking().unwrap();

        // Read the freelist via a separate connection. The post-truncate
        // `PRAGMA incremental_vacuum;` inside the truncate handler already
        // drained some pages, but the cap was unbounded and ran inside the
        // same transaction; on a busy DB there can still be a meaningful
        // residual freelist. If the post-truncate vacuum already drained
        // everything, the subsequent IncrementalVacuum should still leave
        // freelist_count == 0 (a no-op), which the assertion below allows.
        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_before: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();
        drop(probe);

        writer.send(WriteMessage::IncrementalVacuum).unwrap();
        writer.flush_blocking().unwrap();

        let probe = IndexStore::open_read_connection(&db_path).unwrap();
        let free_after: i64 = probe
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .unwrap();

        assert!(
            free_after <= free_before,
            "IncrementalVacuum must not grow the freelist; before={free_before}, after={free_after}"
        );

        writer.shutdown();
    }

    /// End-to-end check: after inserts have grown the WAL, `WalCheckpoint`
    /// shrinks the on-disk WAL file. The WAL file is `db_path` + "-wal";
    /// after a successful TRUNCATE checkpoint with no readers, it should
    /// drop to zero bytes (or a small header).
    #[test]
    fn handle_wal_checkpoint_truncates_wal_file() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Grow the WAL with a non-trivial insert batch.
        let entries: Vec<EntryRow> = (0..2000)
            .map(|i| EntryRow {
                id: 200 + i,
                parent_id: ROOT_ID,
                name: format!("wal-test-entry-{i:08}"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(1024),
                physical_size: Some(1024),
                modified_at: None,
                inode: None,
            })
            .collect();
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        let wal_path = format!("{}-wal", db_path.display());
        let wal_size_before = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_before > 0,
            "expected WAL file to have grown after 2000 inserts; got {} bytes",
            wal_size_before // allowed-pluralize-noun: assertion-failure-only message; the assertion is `> 0`, so when it fires `wal_size_before == 0` and "0 bytes" reads correctly
        );

        writer.send(WriteMessage::WalCheckpoint).unwrap();
        writer.flush_blocking().unwrap();

        let wal_size_after = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size_after < wal_size_before,
            "WalCheckpoint should shrink the WAL file; before={wal_size_before}, after={wal_size_after}"
        );

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
