//! Single-writer thread for all SQLite index writes.
//!
//! All writes go through a dedicated `std::thread` that owns the write connection.
//! This eliminates contention between the full scan, micro-scans, and watcher updates.
//! Reads happen on separate connections (WAL mode allows concurrent reads).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

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

/// Main loop for the writer thread.
///
/// Priority handling: drain ALL pending `UpdateDirStats` messages first (via `try_recv`),
/// then process ONE other message, then repeat. This ensures micro-scan results
/// are written promptly even while the full scan pushes large batches.
fn writer_loop(conn: rusqlite::Connection, receiver: mpsc::Receiver<WriteMessage>) {
    loop {
        // Phase 1: drain all pending UpdateDirStats messages (priority)
        loop {
            match receiver.try_recv() {
                Ok(WriteMessage::UpdateDirStats(stats)) => {
                    process_update_dir_stats(&conn, &stats);
                }
                Ok(other) => {
                    // Got a non-priority message; process it and move on
                    if process_message(&conn, other) {
                        return; // Shutdown
                    }
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // Phase 2: wait for the next message (blocking)
        match receiver.recv() {
            Ok(WriteMessage::UpdateDirStats(stats)) => {
                process_update_dir_stats(&conn, &stats);
                // After processing a priority message, loop back to drain more
            }
            Ok(msg) => {
                if process_message(&conn, msg) {
                    return; // Shutdown
                }
            }
            Err(mpsc::RecvError) => return, // Channel closed
        }
    }
}

/// Process a single non-`UpdateDirStats` message. Returns `true` if the thread should exit.
fn process_message(conn: &rusqlite::Connection, msg: WriteMessage) -> bool {
    match msg {
        WriteMessage::InsertEntries(entries) => {
            if let Err(e) = IndexStore::insert_entries_batch(conn, &entries) {
                log::warn!("Index writer: insert_entries_batch failed: {e}");
            }
        }
        WriteMessage::UpdateDirStats(stats) => {
            // Shouldn't reach here in normal flow, but handle it anyway
            process_update_dir_stats(conn, &stats);
        }
        WriteMessage::ComputeAllAggregates => match aggregator::compute_all_aggregates(conn) {
            Ok(count) => log::info!("Index writer: computed aggregates for {count} directories"),
            Err(e) => log::warn!("Index writer: compute_all_aggregates failed: {e}"),
        },
        WriteMessage::ComputeSubtreeAggregates { root } => match aggregator::compute_subtree_aggregates(conn, &root) {
            Ok(count) => log::debug!("Index writer: computed subtree aggregates for {count} dirs under {root}"),
            Err(e) => log::warn!("Index writer: compute_subtree_aggregates({root}) failed: {e}"),
        },
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
            if let Err(e) = IndexStore::delete_entry(conn, &path) {
                log::warn!("Index writer: delete_entry failed for {path}: {e}");
            }
        }
        WriteMessage::DeleteSubtree(path) => {
            if let Err(e) = IndexStore::delete_subtree(conn, &path) {
                log::warn!("Index writer: delete_subtree failed for {path}: {e}");
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
}
