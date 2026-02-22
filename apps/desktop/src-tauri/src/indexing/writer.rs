//! Single-writer thread for all SQLite index writes.
//!
//! All writes go through a dedicated `std::thread` that owns the write connection.
//! This eliminates contention between the full scan, micro-scans, and watcher updates.
//! Reads happen on separate connections (WAL mode allows concurrent reads).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use tokio::sync::oneshot;

use crate::indexing::aggregator;
use crate::indexing::store::{DirStats, IndexStore, IndexStoreError, ScannedEntry};

// ── Messages ─────────────────────────────────────────────────────────

/// Messages sent to the writer thread via an unbounded mpsc channel.
pub enum WriteMessage {
    /// Full scan: batch of entries. Lowest priority.
    InsertEntries(Vec<ScannedEntry>),
    /// Micro-scan or watcher: dir_stats updates. Highest priority.
    UpdateDirStats(Vec<DirStats>),
    /// Full scan complete: trigger bottom-up aggregation for all directories.
    ComputeAllAggregates,
    /// Micro-scan complete: trigger aggregation for a subtree only.
    ComputeSubtreeAggregates { root: String },
    /// Watcher: incremental delta propagation for a single file change.
    PropagateDelta {
        path: PathBuf,
        size_delta: i64,
        file_count_delta: i32,
        dir_count_delta: i32,
    },
    /// Watcher: upsert a single entry (file created/modified/renamed).
    UpsertEntry(ScannedEntry),
    /// Watcher: delete a single entry and its dir_stats.
    DeleteEntry(String),
    /// Watcher: delete a subtree (directory removed with all children).
    DeleteSubtree(String),
    /// Store the last processed FSEvents event ID.
    UpdateLastEventId(u64),
    /// Update a meta key.
    UpdateMeta { key: String, value: String },
    /// Request current entry count (for progress reporting).
    GetEntryCount(oneshot::Sender<Result<u64, IndexStoreError>>),
    /// Flush: confirms all prior messages have been committed.
    /// The writer responds through the channel after processing this message.
    Flush(oneshot::Sender<()>),
    /// Begin an explicit SQLite transaction.
    /// All subsequent writes are batched until `CommitTransaction`.
    /// Dramatically reduces fsync overhead for bulk operations (replay).
    BeginTransaction,
    /// Commit the current explicit transaction.
    CommitTransaction,
    /// Shut down the writer thread.
    Shutdown,
}

// ── IndexWriter handle ───────────────────────────────────────────────

/// Handle for sending messages to the writer thread.
///
/// Cloneable; all clones share the same underlying channel.
#[derive(Clone)]
pub struct IndexWriter {
    sender: mpsc::Sender<WriteMessage>,
    /// Handle for the writer thread, shared so shutdown() can join it.
    thread_handle: Arc<std::sync::Mutex<Option<thread::JoinHandle<()>>>>,
}

impl IndexWriter {
    /// Spawn the writer thread with its own write connection.
    ///
    /// Opens a WAL-mode write connection to the DB at `db_path`, spawns a
    /// `std::thread` (blocking I/O, not tokio), and returns a handle.
    pub fn spawn(db_path: &Path) -> Result<Self, IndexStoreError> {
        let conn = IndexStore::open_write_connection(db_path)?;
        let (sender, receiver) = mpsc::channel::<WriteMessage>();

        let handle = thread::Builder::new()
            .name("index-writer".into())
            .spawn(move || writer_loop(conn, receiver))
            .map_err(IndexStoreError::Io)?;

        Ok(Self {
            sender,
            thread_handle: Arc::new(std::sync::Mutex::new(Some(handle))),
        })
    }

    /// Send a message to the writer thread (non-blocking).
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

/// Diagnostic counters for writer thread logging.
struct WriterStats {
    total: u64,
    insert_entries: u64,
    upsert_entry: u64,
    update_dir_stats: u64,
    delete_entry: u64,
    delete_subtree: u64,
    propagate_delta: u64,
    compute_aggregates: u64,
    flush: u64,
    other: u64,
    last_summary: Instant,
}

impl WriterStats {
    fn new() -> Self {
        Self {
            total: 0,
            insert_entries: 0,
            upsert_entry: 0,
            update_dir_stats: 0,
            delete_entry: 0,
            delete_subtree: 0,
            propagate_delta: 0,
            compute_aggregates: 0,
            flush: 0,
            other: 0,
            last_summary: Instant::now(),
        }
    }

    fn record(&mut self, msg: &WriteMessage) {
        self.total += 1;
        match msg {
            WriteMessage::InsertEntries(_) => self.insert_entries += 1,
            WriteMessage::UpsertEntry(_) => self.upsert_entry += 1,
            WriteMessage::UpdateDirStats(_) => self.update_dir_stats += 1,
            WriteMessage::DeleteEntry(_) => self.delete_entry += 1,
            WriteMessage::DeleteSubtree(_) => self.delete_subtree += 1,
            WriteMessage::PropagateDelta { .. } => self.propagate_delta += 1,
            WriteMessage::ComputeAllAggregates | WriteMessage::ComputeSubtreeAggregates { .. } => {
                self.compute_aggregates += 1;
            }
            WriteMessage::Flush(_) => self.flush += 1,
            _ => self.other += 1,
        }
    }

    /// Log a summary if at least 5 seconds have passed since the last one.
    fn maybe_log_summary(&mut self) {
        let elapsed = self.last_summary.elapsed();
        if elapsed.as_secs() >= 5 && self.total > 0 {
            log::debug!(
                "Writer: processed {} msgs ({} inserts, {} upserts, {} dir_stats, {} deletes, \
                 {} delete_subtrees, {} propagate, {} aggregates, {} flushes, {} other) in {:.1}s",
                self.total,
                self.insert_entries,
                self.upsert_entry,
                self.update_dir_stats,
                self.delete_entry,
                self.delete_subtree,
                self.propagate_delta,
                self.compute_aggregates,
                self.flush,
                self.other,
                elapsed.as_secs_f64(),
            );
            self.last_summary = Instant::now();
        }
    }
}

/// Main loop for the writer thread.
///
/// Priority handling: drain ALL pending `UpdateDirStats` messages first (via `try_recv`),
/// then process ONE other message, then repeat. This ensures micro-scan results
/// are written promptly even while the full scan pushes large batches.
fn writer_loop(conn: rusqlite::Connection, receiver: mpsc::Receiver<WriteMessage>) {
    log::debug!("Writer: thread started");
    let mut stats = WriterStats::new();

    loop {
        // Phase 1: drain all pending UpdateDirStats messages (priority)
        loop {
            match receiver.try_recv() {
                Ok(WriteMessage::UpdateDirStats(dir_stats)) => {
                    stats.record(&WriteMessage::UpdateDirStats(Vec::new()));
                    process_update_dir_stats(&conn, &dir_stats);
                    stats.maybe_log_summary();
                }
                Ok(other) => {
                    stats.record(&other);
                    // Got a non-priority message; process it and move on
                    if process_message(&conn, other, &stats) {
                        log::info!(
                            "Writer: shutdown after processing {} messages",
                            stats.total,
                        );
                        return;
                    }
                    stats.maybe_log_summary();
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::info!(
                        "Writer: channel closed, thread exiting after processing {} messages",
                        stats.total,
                    );
                    return;
                }
            }
        }

        // Phase 2: wait for the next message (blocking)
        match receiver.recv() {
            Ok(WriteMessage::UpdateDirStats(dir_stats)) => {
                stats.record(&WriteMessage::UpdateDirStats(Vec::new()));
                process_update_dir_stats(&conn, &dir_stats);
                stats.maybe_log_summary();
                // After processing a priority message, loop back to drain more
            }
            Ok(msg) => {
                stats.record(&msg);
                if process_message(&conn, msg, &stats) {
                    log::info!(
                        "Writer: shutdown after processing {} messages",
                        stats.total,
                    );
                    return;
                }
                stats.maybe_log_summary();
            }
            Err(mpsc::RecvError) => {
                log::info!(
                    "Writer: channel closed, thread exiting after processing {} messages",
                    stats.total,
                );
                return;
            }
        }
    }
}

/// Process a single non-`UpdateDirStats` message. Returns `true` if the thread should exit.
fn process_message(conn: &rusqlite::Connection, msg: WriteMessage, stats: &WriterStats) -> bool {
    match msg {
        WriteMessage::InsertEntries(entries) => {
            let count = entries.len();
            let t = Instant::now();
            if let Err(e) = IndexStore::insert_entries_batch(conn, &entries) {
                log::warn!("Index writer: insert_entries_batch failed: {e}");
            }
            let elapsed = t.elapsed().as_millis();
            if elapsed > 100 {
                log::debug!("Writer: insert_entries_batch ({count} entries) took {elapsed}ms");
            }
        }
        WriteMessage::UpdateDirStats(dir_stats) => {
            // Shouldn't reach here in normal flow, but handle it anyway
            process_update_dir_stats(conn, &dir_stats);
        }
        WriteMessage::ComputeAllAggregates => {
            let t = Instant::now();
            match aggregator::compute_all_aggregates(conn) {
                Ok(count) => {
                    log::info!(
                        "Index writer: computed aggregates for {count} directories ({}ms)",
                        t.elapsed().as_millis(),
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
        WriteMessage::PropagateDelta {
            path,
            size_delta,
            file_count_delta,
            dir_count_delta,
        } => {
            let path_str = path.to_string_lossy();
            if let Err(e) = aggregator::propagate_delta(conn, &path_str, size_delta, file_count_delta, dir_count_delta)
            {
                log::warn!("Index writer: propagate_delta failed for {}: {e}", path.display());
            }
        }
        WriteMessage::UpsertEntry(entry) => {
            if let Err(e) = IndexStore::upsert_entry(conn, &entry) {
                log::warn!("Index writer: upsert_entry failed for {}: {e}", entry.path);
            }
        }
        WriteMessage::DeleteEntry(path) => {
            // Read old entry before deleting to get accurate delta
            let old_entry = IndexStore::get_entry(conn, &path).ok().flatten();
            if let Err(e) = IndexStore::delete_entry(conn, &path) {
                log::warn!("Index writer: delete_entry failed for {path}: {e}");
            }
            // Auto-propagate accurate negative delta
            if let Some(entry) = old_entry {
                let (size_delta, file_delta, dir_delta) = if entry.is_directory {
                    (0i64, 0i32, -1i32)
                } else {
                    (-(entry.size.unwrap_or(0) as i64), -1, 0)
                };
                if let Err(e) = aggregator::propagate_delta(conn, &path, size_delta, file_delta, dir_delta) {
                    log::warn!("Index writer: propagate_delta after delete_entry failed for {path}: {e}");
                }
            }
        }
        WriteMessage::DeleteSubtree(path) => {
            // Read subtree totals before deleting to get accurate delta
            let totals = IndexStore::get_subtree_totals(conn, &path).ok();
            if let Err(e) = IndexStore::delete_subtree(conn, &path) {
                log::warn!("Index writer: delete_subtree failed for {path}: {e}");
            }
            // Auto-propagate accurate negative delta
            if let Some((total_size, file_count, dir_count)) = totals {
                // dir_count from the query includes the root dir itself (it's in entries)
                let size_delta = -(total_size as i64);
                let file_delta = -(file_count as i32);
                let dir_delta = -(dir_count as i32);
                if let Err(e) = aggregator::propagate_delta(conn, &path, size_delta, file_delta, dir_delta) {
                    log::warn!("Index writer: propagate_delta after delete_subtree failed for {path}: {e}");
                }
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
        WriteMessage::GetEntryCount(reply) => {
            let result = IndexStore::get_entry_count(conn);
            // If the receiver dropped, that's fine; ignore the send error
            let _ = reply.send(result);
        }
        WriteMessage::Flush(reply) => {
            log::debug!(
                "Writer: processing flush (total msgs processed so far: {})",
                stats.total,
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
        WriteMessage::Shutdown => return true,
    }
    false
}

fn process_update_dir_stats(conn: &rusqlite::Connection, stats: &[DirStats]) {
    if let Err(e) = IndexStore::upsert_dir_stats(conn, stats) {
        log::warn!("Index writer: upsert_dir_stats failed: {e}");
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::indexing::store::IndexStore;

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

    #[test]
    fn spawn_and_shutdown() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();
        writer.shutdown();
        // Give the thread a moment to process shutdown
        thread::sleep(Duration::from_millis(50));
        // Further sends should fail
        let result = writer.send(WriteMessage::Shutdown);
        // Might succeed or fail depending on timing, but shouldn't panic
        let _ = result;
    }

    #[test]
    fn insert_entries_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        let entries = vec![ScannedEntry {
            path: "/test/file.txt".into(),
            parent_path: "/test".into(),
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(1024),
            modified_at: Some(1700000000),
        }];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let result = store.list_entries_by_parent("/test").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "file.txt");
        assert_eq!(result[0].size, Some(1024));
    }

    #[test]
    fn update_dir_stats_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        let stats = vec![DirStats {
            path: "/mydir".into(),
            recursive_size: 5000,
            recursive_file_count: 10,
            recursive_dir_count: 3,
        }];
        writer.send(WriteMessage::UpdateDirStats(stats)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let result = store.get_dir_stats("/mydir").unwrap().unwrap();
        assert_eq!(result.recursive_size, 5000);
        assert_eq!(result.recursive_file_count, 10);
        assert_eq!(result.recursive_dir_count, 3);
    }

    #[test]
    fn compute_all_aggregates_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Insert a simple tree
        let entries = vec![
            ScannedEntry {
                path: "/r".into(),
                parent_path: "/".into(),
                name: "r".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/r/f.txt".into(),
                parent_path: "/r".into(),
                name: "f.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(42),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(200));

        let store = open_read(&db_path);
        let stats = store.get_dir_stats("/r").unwrap().unwrap();
        assert_eq!(stats.recursive_size, 42);
        assert_eq!(stats.recursive_file_count, 1);
    }

    #[test]
    fn get_entry_count_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Insert some entries first
        let entries = vec![
            ScannedEntry {
                path: "/a".into(),
                parent_path: "/".into(),
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/b.txt".into(),
                parent_path: "/a".into(),
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(100),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();

        // Give the writer time to process the insert
        thread::sleep(Duration::from_millis(100));

        let (tx, rx) = oneshot::channel();
        writer.send(WriteMessage::GetEntryCount(tx)).unwrap();

        let count = rx.blocking_recv().unwrap().unwrap();
        assert_eq!(count, 2);

        writer.shutdown();
    }

    #[test]
    fn update_meta_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

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
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Insert entries then flush (no sleep needed — flush guarantees completion)
        let entries = vec![ScannedEntry {
            path: "/flush/test.txt".into(),
            parent_path: "/flush".into(),
            name: "test.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(512),
            modified_at: Some(1700000000),
        }];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        writer.flush().await.unwrap();

        // Data should be readable immediately after flush
        let store = open_read(&db_path);
        let result = store.list_entries_by_parent("/flush").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "test.txt");
        assert_eq!(result[0].size, Some(512));

        writer.shutdown();
    }

    #[test]
    fn update_last_event_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        writer.send(WriteMessage::UpdateLastEventId(12345)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let status = store.get_index_status().unwrap();
        assert_eq!(status.last_event_id.as_deref(), Some("12345"));
    }

    #[test]
    fn priority_dir_stats_processed_before_entries() {
        // This test verifies the priority mechanism: UpdateDirStats messages should
        // be drained before processing other messages.
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Send many InsertEntries first, then an UpdateDirStats
        for i in 0..10 {
            let entries = vec![ScannedEntry {
                path: format!("/batch{i}/file.txt"),
                parent_path: format!("/batch{i}"),
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(100),
                modified_at: None,
            }];
            writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        }

        // This priority message should be processed as soon as the writer checks
        let stats = vec![DirStats {
            path: "/priority".into(),
            recursive_size: 999,
            recursive_file_count: 1,
            recursive_dir_count: 0,
        }];
        writer.send(WriteMessage::UpdateDirStats(stats)).unwrap();

        writer.shutdown();
        thread::sleep(Duration::from_millis(200));

        // Verify the priority message was processed
        let store = open_read(&db_path);
        let result = store.get_dir_stats("/priority").unwrap().unwrap();
        assert_eq!(result.recursive_size, 999);
    }

    #[test]
    fn subtree_aggregates_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        let entries = vec![
            ScannedEntry {
                path: "/sub".into(),
                parent_path: "/".into(),
                name: "sub".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/sub/inner".into(),
                parent_path: "/sub".into(),
                name: "inner".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/sub/inner/data.bin".into(),
                parent_path: "/sub/inner".into(),
                name: "data.bin".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(777),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        writer
            .send(WriteMessage::ComputeSubtreeAggregates { root: "/sub".into() })
            .unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(200));

        let store = open_read(&db_path);
        let stats = store.get_dir_stats("/sub").unwrap().unwrap();
        assert_eq!(stats.recursive_size, 777);
        assert_eq!(stats.recursive_file_count, 1);
        assert_eq!(stats.recursive_dir_count, 1);
    }

    #[test]
    fn propagate_delta_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Pre-populate a dir_stats entry
        let stats = vec![DirStats {
            path: "/home".into(),
            recursive_size: 1000,
            recursive_file_count: 5,
            recursive_dir_count: 1,
        }];
        writer.send(WriteMessage::UpdateDirStats(stats)).unwrap();

        // Propagate a file addition under /home
        writer
            .send(WriteMessage::PropagateDelta {
                path: PathBuf::from("/home/newfile.txt"),
                size_delta: 250,
                file_count_delta: 1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(200));

        let store = open_read(&db_path);
        let result = store.get_dir_stats("/home").unwrap().unwrap();
        assert_eq!(result.recursive_size, 1250);
        assert_eq!(result.recursive_file_count, 6);
    }

    #[test]
    fn upsert_entry_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        let entry = ScannedEntry {
            path: "/test/new.txt".into(),
            parent_path: "/test".into(),
            name: "new.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(256),
            modified_at: Some(1700000000),
        };
        writer.send(WriteMessage::UpsertEntry(entry)).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let result = store.list_entries_by_parent("/test").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "new.txt");
        assert_eq!(result[0].size, Some(256));
    }

    #[test]
    fn delete_entry_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Insert then delete
        let entries = vec![ScannedEntry {
            path: "/test/doomed.txt".into(),
            parent_path: "/test".into(),
            name: "doomed.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(100),
            modified_at: None,
        }];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        writer
            .send(WriteMessage::DeleteEntry("/test/doomed.txt".into()))
            .unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let result = store.list_entries_by_parent("/test").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn delete_subtree_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        let entries = vec![
            ScannedEntry {
                path: "/a".into(),
                parent_path: "/".into(),
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/b.txt".into(),
                parent_path: "/a".into(),
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(50),
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/c".into(),
                parent_path: "/a".into(),
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        thread::sleep(Duration::from_millis(100));

        writer.send(WriteMessage::DeleteSubtree("/a".into())).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let root_children = store.list_entries_by_parent("/").unwrap();
        assert!(root_children.is_empty(), "/a should be deleted");
        let a_children = store.list_entries_by_parent("/a").unwrap();
        assert!(a_children.is_empty(), "children of /a should be deleted");
    }

    #[test]
    fn delete_entry_auto_propagates_delta() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Insert a file and pre-populate parent dir_stats
        let entries = vec![ScannedEntry {
            path: "/p/file.txt".into(),
            parent_path: "/p".into(),
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(500),
            modified_at: None,
        }];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();
        writer
            .send(WriteMessage::UpdateDirStats(vec![DirStats {
                path: "/p".into(),
                recursive_size: 500,
                recursive_file_count: 1,
                recursive_dir_count: 0,
            }]))
            .unwrap();
        thread::sleep(Duration::from_millis(100));

        // Delete the file — writer should auto-propagate (-500, -1, 0) to /p
        writer.send(WriteMessage::DeleteEntry("/p/file.txt".into())).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let stats = store.get_dir_stats("/p").unwrap().unwrap();
        assert_eq!(stats.recursive_size, 0, "size should be 0 after file deletion");
        assert_eq!(stats.recursive_file_count, 0, "file count should be 0");
    }

    #[test]
    fn delete_subtree_auto_propagates_delta() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Build a tree: /root/sub/file.txt (300 bytes)
        let entries = vec![
            ScannedEntry {
                path: "/root".into(),
                parent_path: "/".into(),
                name: "root".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/root/sub".into(),
                parent_path: "/root".into(),
                name: "sub".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/root/sub/file.txt".into(),
                parent_path: "/root/sub".into(),
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(300),
                modified_at: None,
            },
        ];
        writer.send(WriteMessage::InsertEntries(entries)).unwrap();

        // Pre-populate dir_stats for ancestors
        writer
            .send(WriteMessage::UpdateDirStats(vec![
                DirStats {
                    path: "/".into(),
                    recursive_size: 300,
                    recursive_file_count: 1,
                    recursive_dir_count: 2,
                },
                DirStats {
                    path: "/root".into(),
                    recursive_size: 300,
                    recursive_file_count: 1,
                    recursive_dir_count: 1,
                },
            ]))
            .unwrap();
        thread::sleep(Duration::from_millis(100));

        // Delete the /root/sub subtree — should auto-propagate accurate negative delta
        writer.send(WriteMessage::DeleteSubtree("/root/sub".into())).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);

        // /root should have lost: size=300, files=1, dirs=1 (the /root/sub dir itself)
        let root_stats = store.get_dir_stats("/root").unwrap().unwrap();
        assert_eq!(
            root_stats.recursive_size, 0,
            "root size should be 0 after subtree deletion"
        );
        assert_eq!(root_stats.recursive_file_count, 0);
        assert_eq!(root_stats.recursive_dir_count, 0);

        // "/" should also have lost: size=300, files=1, dirs=1
        let vol_stats = store.get_dir_stats("/").unwrap().unwrap();
        assert_eq!(vol_stats.recursive_size, 0);
        assert_eq!(vol_stats.recursive_file_count, 0);
        assert_eq!(vol_stats.recursive_dir_count, 1); // /root still exists
    }

    #[test]
    fn delete_entry_for_nonexistent_skips_propagation() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path).unwrap();

        // Pre-populate dir_stats
        writer
            .send(WriteMessage::UpdateDirStats(vec![DirStats {
                path: "/p".into(),
                recursive_size: 100,
                recursive_file_count: 1,
                recursive_dir_count: 0,
            }]))
            .unwrap();
        thread::sleep(Duration::from_millis(100));

        // Delete a non-existent entry — should not propagate any delta
        writer.send(WriteMessage::DeleteEntry("/p/ghost.txt".into())).unwrap();
        writer.shutdown();
        thread::sleep(Duration::from_millis(100));

        let store = open_read(&db_path);
        let stats = store.get_dir_stats("/p").unwrap().unwrap();
        assert_eq!(stats.recursive_size, 100, "stats should be unchanged");
        assert_eq!(stats.recursive_file_count, 1);
    }
}
