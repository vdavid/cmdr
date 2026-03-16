//! Single-writer thread for all SQLite index writes.
//!
//! All writes go through a dedicated `std::thread` that owns the write connection.
//! This eliminates contention between the full scan, subtree scans, and watcher updates.
//! Reads happen on separate connections (WAL mode allows concurrent reads).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::indexing::aggregator::{self, AggregationPhase, AggregationProgress};
use crate::indexing::store::{DirStatsById, EntryRow, IndexStore, IndexStoreError};

// ── Aggregation progress events ──────────────────────────────────────

/// Tauri event payload for aggregation progress updates.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregationProgressEvent {
    phase: &'static str,
    current: u64,
    total: u64,
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
        size: Option<u64>,
        modified_at: Option<u64>,
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
        size_delta: i64,
        file_count_delta: i32,
        dir_count_delta: i32,
    },
    /// Full scan complete: trigger bottom-up aggregation for all directories.
    ComputeAllAggregates,
    /// Subtree scan complete: trigger aggregation for a subtree only.
    ComputeSubtreeAggregates { root: String },
    /// Store the last processed FSEvents event ID.
    UpdateLastEventId(u64),
    /// Update a meta key.
    UpdateMeta { key: String, value: String },
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
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(WRITER_CHANNEL_CAPACITY);
        let expected_total_entries = Arc::new(AtomicU64::new(0));
        let expected_total_clone = Arc::clone(&expected_total_entries);

        let handle = thread::Builder::new()
            .name("index-writer".into())
            .spawn(move || writer_loop(conn, receiver, app_handle, expected_total_clone))
            .map_err(IndexStoreError::Io)?;

        Ok(Self {
            sender,
            thread_handle: Arc::new(std::sync::Mutex::new(Some(handle))),
            db_path: db_path.to_path_buf(),
            expected_total_entries,
        })
    }

    /// Return the path to the DB file. Used by the scanner to open a
    /// temporary connection for `ScanContext` initialization.
    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    /// Set the expected total entries from a completed scan. The writer thread
    /// reads this to report flushing progress as it drains `InsertEntriesV2`.
    pub fn set_expected_total_entries(&self, total: u64) {
        self.expected_total_entries.store(total, Ordering::Relaxed);
    }

    /// Send a message to the writer thread. Blocks if the channel is full
    /// (backpressure), which slows down event processing rather than
    /// consuming unlimited memory.
    pub fn send(&self, msg: WriteMessage) -> Result<(), IndexStoreError> {
        self.sender.send(msg).map_err(|_| {
            IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread has shut down",
            ))
        })
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

// ── Writer thread loop ───────────────────────────────────────────────

/// Snapshot of cumulative counters, used to compute per-interval deltas.
#[derive(Clone, Default)]
struct StatsSnapshot {
    total: u64,
    insert_entries: u64,
    upsert_entry: u64,
    delete_entry: u64,
    delete_subtree: u64,
    propagate_delta: u64,
    compute_aggregates: u64,
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
            WriteMessage::DeleteEntryById(_) => self.current.delete_entry += 1,
            WriteMessage::DeleteSubtreeById(_) | WriteMessage::DeleteDescendantsById(_) => {
                self.current.delete_subtree += 1;
            }
            WriteMessage::PropagateDeltaById { .. } => self.current.propagate_delta += 1,
            WriteMessage::ComputeAllAggregates | WriteMessage::ComputeSubtreeAggregates { .. } => {
                self.current.compute_aggregates += 1;
            }
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

        let deltas: &[(&str, u64)] = &[
            ("inserts", self.current.insert_entries - self.previous.insert_entries),
            ("upserts", self.current.upsert_entry - self.previous.upsert_entry),
            ("deletes", self.current.delete_entry - self.previous.delete_entry),
            (
                "delete_subtrees",
                self.current.delete_subtree - self.previous.delete_subtree,
            ),
            (
                "propagate",
                self.current.propagate_delta - self.previous.propagate_delta,
            ),
            (
                "aggregates",
                self.current.compute_aggregates - self.previous.compute_aggregates,
            ),
            ("flushes", self.current.flush - self.previous.flush),
            ("other", self.current.other - self.previous.other),
        ];

        let parts: Vec<String> = deltas
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(name, count)| format!("{count} {name}"))
            .collect();

        let breakdown = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        };

        log::debug!(
            "Writer: +{delta_total} msgs{breakdown} in {:.1}s [{} total]",
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
    /// `parent_id -> (file_size_sum, file_count, dir_count)` — direct children only.
    direct_stats: HashMap<i64, (u64, u64, u64)>,
    /// `parent_id -> Vec<child_dir_id>` — direct child directories only.
    child_dirs: HashMap<i64, Vec<i64>>,
    /// Running count of entries inserted so far (for flushing progress).
    entries_inserted: u64,
}

impl AccumulatorMaps {
    fn new() -> Self {
        Self {
            direct_stats: HashMap::new(),
            child_dirs: HashMap::new(),
            entries_inserted: 0,
        }
    }

    /// Accumulate stats from a batch of inserted entries.
    fn accumulate(&mut self, entries: &[EntryRow]) {
        self.entries_inserted += entries.len() as u64;
        for entry in entries {
            let stats = self.direct_stats.entry(entry.parent_id).or_insert((0, 0, 0));
            if entry.is_directory {
                stats.2 += 1;
                self.child_dirs.entry(entry.parent_id).or_default().push(entry.id);
            } else {
                stats.0 += entry.size.unwrap_or(0);
                stats.1 += 1;
            }
        }
    }

    fn clear(&mut self) {
        self.direct_stats.clear();
        self.child_dirs.clear();
        self.entries_inserted = 0;
    }
}

/// Main loop for the writer thread.
///
/// Processes messages sequentially from the mpsc channel. Each message is
/// handled in order, ensuring all writes are serialized. Maintains in-memory
/// accumulator maps during InsertEntriesV2 to skip expensive SQL queries
/// when ComputeAllAggregates arrives.
fn writer_loop(
    conn: rusqlite::Connection,
    receiver: mpsc::Receiver<WriteMessage>,
    app_handle: Option<AppHandle>,
    expected_total_entries: Arc<AtomicU64>,
) {
    log::debug!("Writer: thread started");
    let mut stats = WriterStats::new();
    let mut accumulator = AccumulatorMaps::new();

    for msg in &receiver {
        if !matches!(msg, WriteMessage::IncrementalVacuum) {
            stats.record(&msg);
        }
        // macOS: drain autoreleased ObjC objects each iteration.
        // app.emit() serializes through WebKit's Cocoa bridge, creating NSData/NSInvocation
        // objects that accumulate without a pool on this std::thread::spawn thread.
        #[cfg(target_os = "macos")]
        let should_exit = objc2::rc::autoreleasepool(|_| {
            process_message(
                &conn,
                msg,
                &stats,
                &mut accumulator,
                &app_handle,
                &expected_total_entries,
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
        );
        if should_exit {
            log::debug!("Writer: shutdown after processing {} messages", stats.current.total);
            return;
        }
        stats.maybe_log_summary();
    }

    log::debug!(
        "Writer: channel closed, thread exiting after processing {} messages",
        stats.current.total,
    );
}

/// Process a single message. Returns `true` if the thread should exit.
fn process_message(
    conn: &rusqlite::Connection,
    msg: WriteMessage,
    stats: &WriterStats,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    expected_total_entries: &AtomicU64,
) -> bool {
    match msg {
        // ── Integer-keyed variants ───────────────────────────────────
        WriteMessage::InsertEntriesV2(entries) => {
            let count = entries.len();
            accumulator.accumulate(&entries);
            let t = Instant::now();
            if let Err(e) = IndexStore::insert_entries_v2_batch(conn, &entries) {
                log::warn!("Index writer: insert_entries_v2_batch failed: {e}");
            }
            let elapsed = t.elapsed().as_millis();
            if elapsed > 100 {
                log::debug!("Writer: insert_entries_v2_batch ({count} entries) took {elapsed}ms");
            }
            WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
            // Emit flushing progress when we know the expected total
            let expected = expected_total_entries.load(Ordering::Relaxed);
            if expected > 0
                && let Some(app) = app_handle
            {
                let _ = app.emit(
                    "index-aggregation-progress",
                    AggregationProgressEvent {
                        phase: phase_to_str(AggregationPhase::SavingEntries),
                        current: accumulator.entries_inserted,
                        total: expected,
                    },
                );
            }
        }
        WriteMessage::UpsertEntryV2 {
            parent_id,
            name,
            is_directory,
            is_symlink,
            size,
            modified_at,
        } => {
            // Check if an entry already exists at (parent_id, name)
            match IndexStore::resolve_component(conn, parent_id, &name) {
                Ok(Some(existing_id)) => {
                    if let Err(e) =
                        IndexStore::update_entry(conn, existing_id, is_directory, is_symlink, size, modified_at)
                    {
                        log::warn!("Index writer: update_entry failed for id={existing_id}: {e}");
                    }
                }
                Ok(None) => {
                    match IndexStore::insert_entry_v2(
                        conn,
                        parent_id,
                        &name,
                        is_directory,
                        is_symlink,
                        size,
                        modified_at,
                    ) {
                        Ok(new_id) => {
                            log::debug!(
                                "Writer: UpsertEntryV2 inserted \"{name}\" (parent_id={parent_id}) → id={new_id}"
                            );
                            // Initialize empty dir_stats for new directories so enrichment
                            // always has a row. Subsequent PropagateDeltaById calls from
                            // child events will update it incrementally.
                            if is_directory
                                && let Err(e) = IndexStore::upsert_dir_stats_by_id(
                                    conn,
                                    &[DirStatsById {
                                        entry_id: new_id,
                                        recursive_size: 0,
                                        recursive_file_count: 0,
                                        recursive_dir_count: 0,
                                    }],
                                )
                            {
                                log::warn!("Writer: init dir_stats for new dir id={new_id} failed: {e}");
                            }
                        }
                        Err(e) => {
                            log::warn!("Index writer: insert_entry_v2 failed for {name}: {e}");
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Index writer: resolve_component failed for {name}: {e}");
                }
            }
            WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        WriteMessage::DeleteEntryById(entry_id) => {
            // Read old entry before deleting to get accurate delta
            let old_entry = IndexStore::get_entry_by_id(conn, entry_id).ok().flatten();
            if let Err(e) = IndexStore::delete_entry_by_id(conn, entry_id) {
                log::warn!("Index writer: delete_entry_by_id failed for id={entry_id}: {e}");
            }
            // Auto-propagate accurate negative delta via parent_id chain
            if let Some(entry) = old_entry {
                let (size_delta, file_delta, dir_delta) = if entry.is_directory {
                    (0i64, 0i32, -1i32)
                } else {
                    (-(entry.size.unwrap_or(0) as i64), -1, 0)
                };
                propagate_delta_by_id(conn, entry.parent_id, size_delta, file_delta, dir_delta);
            }
            WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        WriteMessage::DeleteSubtreeById(root_id) => {
            // Read subtree totals before deleting to get accurate delta
            let totals = IndexStore::get_subtree_totals_by_id(conn, root_id).ok();
            let parent_id = IndexStore::get_parent_id(conn, root_id).ok().flatten();
            if let Err(e) = IndexStore::delete_subtree_by_id(conn, root_id) {
                log::warn!("Index writer: delete_subtree_by_id failed for id={root_id}: {e}");
            }
            // Auto-propagate accurate negative delta via parent_id chain
            if let (Some((total_size, file_count, dir_count)), Some(pid)) = (totals, parent_id) {
                let size_delta = -(total_size as i64);
                let file_delta = -(file_count as i32);
                let dir_delta = -(dir_count as i32);
                propagate_delta_by_id(conn, pid, size_delta, file_delta, dir_delta);
            }
            WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
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
            size_delta,
            file_count_delta,
            dir_count_delta,
        } => {
            propagate_delta_by_id(conn, entry_id, size_delta, file_count_delta, dir_count_delta);
        }
        WriteMessage::TruncateData => {
            accumulator.clear();
            expected_total_entries.store(0, Ordering::Relaxed);
            let t = Instant::now();
            match conn.execute_batch(
                "DELETE FROM dir_stats; DELETE FROM entries; INSERT OR IGNORE INTO entries (id, parent_id, name, is_directory, is_symlink) VALUES (1, 0, '', 1, 0);",
            ) {
                Ok(()) => {
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
            WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        WriteMessage::ComputeAllAggregates => {
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
            // Maps are consumed; clear to free memory.
            // Reset expected_total so subtree-scan inserts don't emit
            // spurious saving_entries progress events after the full scan.
            accumulator.clear();
            expected_total_entries.store(0, Ordering::Relaxed);
            match result {
                Ok(count) => {
                    log::info!(
                        "ComputeAllAggregates: done — {count} directories in {:.1}s",
                        t.elapsed().as_secs_f64(),
                    );
                }
                Err(e) => log::warn!("Index writer: compute_all_aggregates failed: {e}"),
            }
        }
        WriteMessage::ComputeSubtreeAggregates { root } => {
            let t = Instant::now();
            match aggregator::compute_subtree_aggregates(conn, &root) {
                Ok(count) => {
                    log::debug!(
                        "Index writer: computed subtree aggregates for {count} dirs under {root} ({}ms)",
                        t.elapsed().as_millis(),
                    );
                }
                Err(e) => log::warn!("Index writer: compute_subtree_aggregates({root}) failed: {e}"),
            }
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
            log::debug!("Writer: COMMIT transaction ({}ms)", t.elapsed().as_millis());
        }
        WriteMessage::BackfillMissingDirStats => {
            let t = Instant::now();
            match aggregator::backfill_missing_dir_stats(conn) {
                Ok(0) => {
                    log::debug!("BackfillMissingDirStats: no dirs missing stats");
                }
                Ok(count) => {
                    log::info!(
                        "BackfillMissingDirStats: computed stats for {count} dirs in {:.1}s",
                        t.elapsed().as_secs_f64(),
                    );
                }
                Err(e) => log::warn!("BackfillMissingDirStats failed: {e}"),
            }
        }
        WriteMessage::IncrementalVacuum => {
            match conn.pragma_query_value(None, "freelist_count", |row| row.get::<_, i64>(0)) {
                Ok(free) if free > 0 => {
                    if let Err(e) = conn.execute_batch("PRAGMA incremental_vacuum(2000)") {
                        log::warn!("Writer: incremental_vacuum failed: {e}");
                    } else {
                        log::debug!("Writer: incremental_vacuum reclaimed up to 2000 of {free} free pages");
                    }
                }
                Ok(_) => {} // No free pages, nothing to do
                Err(e) => log::warn!("Writer: freelist_count query failed: {e}"),
            }
        }
        WriteMessage::Shutdown => return true,
    }
    false
}

/// Build a progress callback that emits `index-aggregation-progress` events via the AppHandle.
/// If no AppHandle is available, returns a no-op closure.
fn build_progress_callback(app_handle: &Option<AppHandle>) -> impl FnMut(AggregationProgress) + '_ {
    move |progress: AggregationProgress| {
        if let Some(app) = app_handle {
            let _ = app.emit(
                "index-aggregation-progress",
                AggregationProgressEvent {
                    phase: phase_to_str(progress.phase),
                    current: progress.current,
                    total: progress.total,
                },
            );
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
fn propagate_delta_by_id(conn: &rusqlite::Connection, start_id: i64, size_delta: i64, file_delta: i32, dir_delta: i32) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        // Read existing stats
        let existing = IndexStore::get_dir_stats_by_id(conn, current_id).ok().flatten();

        let (new_size, new_files, new_dirs) = match existing {
            Some(s) => (
                (s.recursive_size as i64 + size_delta).max(0) as u64,
                (s.recursive_file_count as i64 + i64::from(file_delta)).max(0) as u64,
                (s.recursive_dir_count as i64 + i64::from(dir_delta)).max(0) as u64,
            ),
            None => (
                size_delta.max(0) as u64,
                i64::from(file_delta).max(0) as u64,
                i64::from(dir_delta).max(0) as u64,
            ),
        };

        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO dir_stats
                 (entry_id, recursive_size, recursive_file_count, recursive_dir_count)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![current_id, new_size, new_files, new_dirs],
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Duration;

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

    // ── Basic lifecycle tests ────────────────────────────────────────

    #[test]
    fn spawn_and_shutdown() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();
        writer.shutdown();
        // Give the thread a moment to process shutdown
        thread::sleep(Duration::from_millis(50));
        // Further sends should fail
        let result = writer.send(WriteMessage::Shutdown);
        // Might succeed or fail depending on timing, but shouldn't panic
        let _ = result;
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
            size: Some(1024),
            modified_at: Some(1700000000),
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "file.txt");
        assert_eq!(children[0].size, Some(1024));
        assert_eq!(children[0].id, 10);
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
                size: Some(256),
                modified_at: Some(1700000000),
            })
            .unwrap();
        thread::sleep(Duration::from_millis(100));

        // Update via UpsertEntryV2 (entry now exists)
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: "new.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(512),
                modified_at: Some(1700000001),
            })
            .unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "new.txt");
        assert_eq!(children[0].size, Some(512), "size should be updated to 512");
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
                size: None,
                modified_at: None,
            })
            .unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        // The new directory should have a zero-valued dir_stats row
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let dir_id = IndexStore::resolve_component(&conn, ROOT_ID, "newdir")
            .unwrap()
            .expect("newdir should exist");

        let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap();
        assert!(stats.is_some(), "new dir should have dir_stats");
        let stats = stats.unwrap();
        assert_eq!(stats.recursive_size, 0);
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_dir_count, 0);
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
            size: Some(100),
            modified_at: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        // Delete by ID
        writer.send(WriteMessage::DeleteEntryById(20)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert!(children.is_empty(), "entry should be deleted");
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
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(50),
                modified_at: None,
            },
            EntryRow {
                id: 12,
                parent_id: 10,
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        // Delete the subtree rooted at id=10
        writer.send(WriteMessage::DeleteSubtreeById(10)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let root_children = store.list_children(ROOT_ID).unwrap();
        assert!(root_children.is_empty(), "dir /a should be deleted");
        let a_children = store.list_children(10).unwrap();
        assert!(a_children.is_empty(), "children of /a should be deleted");
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
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(500),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();

        // Pre-populate dir_stats for the parent
        writer
            .send(WriteMessage::InsertEntriesV2(Vec::new())) // no-op, just to sequence
            .unwrap();
        thread::sleep(Duration::from_millis(100));

        // Manually set dir_stats for parent via direct DB write (using the by-id API)
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_size: 500,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                }],
            )
            .unwrap();
        }

        // Delete the file — writer should auto-propagate (-500, -1, 0) to parent id=10
        writer.send(WriteMessage::DeleteEntryById(11)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_size, 0, "size should be 0 after file deletion");
        assert_eq!(stats.recursive_file_count, 0, "file count should be 0");
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
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "sub".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 12,
                parent_id: 11,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(300),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        // Pre-populate dir_stats for ancestors
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: ROOT_ID,
                        recursive_size: 300,
                        recursive_file_count: 1,
                        recursive_dir_count: 2,
                    },
                    DirStatsById {
                        entry_id: 10,
                        recursive_size: 300,
                        recursive_file_count: 1,
                        recursive_dir_count: 1,
                    },
                ],
            )
            .unwrap();
        }

        // Delete the /root/sub subtree (id=11)
        writer.send(WriteMessage::DeleteSubtreeById(11)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // root_dir(10) should have lost: size=300, files=1, dirs=1
        let root_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(root_stats.recursive_size, 0);
        assert_eq!(root_stats.recursive_file_count, 0);
        assert_eq!(root_stats.recursive_dir_count, 0);

        // ROOT(1) should have lost: size=300, files=1, dirs=1
        let vol_stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(vol_stats.recursive_size, 0);
        assert_eq!(vol_stats.recursive_file_count, 0);
        assert_eq!(vol_stats.recursive_dir_count, 1); // root_dir(10) still exists
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
            size: None,
            modified_at: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        // Pre-populate dir_stats
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_size: 1000,
                    recursive_file_count: 5,
                    recursive_dir_count: 1,
                }],
            )
            .unwrap();
        }

        // Propagate a file addition starting from home's entry_id
        writer
            .send(WriteMessage::PropagateDeltaById {
                entry_id: 10,
                size_delta: 250,
                file_count_delta: 1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(200));

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let result = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(result.recursive_size, 1250);
        assert_eq!(result.recursive_file_count, 6);
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
            size: None,
            modified_at: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                }],
            )
            .unwrap();
        }

        // Delete a non-existent entry — should not propagate any delta
        writer.send(WriteMessage::DeleteEntryById(999)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(stats.recursive_size, 100, "stats should be unchanged");
        assert_eq!(stats.recursive_file_count, 1);
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
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 11,
                parent_id: 10,
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(100),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();

        // Give the writer time to process the insert
        thread::sleep(Duration::from_millis(100));

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
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let status = store.get_index_status().unwrap();
        // test_key is not in IndexStatus struct, read directly via connection
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let val = IndexStore::get_meta(&conn, "test_key").unwrap();
        assert_eq!(val.as_deref(), Some("test_value"));
        drop(store);
        drop(status);
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
            size: Some(512),
            modified_at: Some(1700000000),
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush().await.unwrap();

        // Data should be readable immediately after flush
        let store = open_read(&db_path);
        let children = store.list_children(ROOT_ID).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "test.txt");
        assert_eq!(children[0].size, Some(512));

        writer.shutdown();
    }

    #[test]
    fn update_last_event_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        writer.send(WriteMessage::UpdateLastEventId(12345)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let status = store.get_index_status().unwrap();
        assert_eq!(status.last_event_id.as_deref(), Some("12345"));
    }

    #[test]
    fn db_path_is_available() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();
        assert_eq!(writer.db_path(), db_path);
        writer.shutdown();
    }
}
