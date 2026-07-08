//! `ImportanceWriter`: the single writer thread for one volume's `importance.db`.
//!
//! Mirrors the index's `IndexWriter` discipline: exactly ONE writer thread owns
//! the single write connection per DB (plan Decision 2 / the index's
//! one-writer-per-DB invariant), and all writes cross a bounded channel. The
//! handle is cloneable; every clone shares the one channel and one thread.
//!
//! ## Command surface (plan M2)
//!
//! - [`write_weights`](ImportanceWriter::write_weights): write a recompute pass's
//!   weights, stamping every row with the pass generation and advancing the
//!   stored generation to it. Rows upsert on the path PK (a pass rewrites every
//!   folder).
//! - [`purge_volume`](ImportanceWriter::purge_volume): drop all weights and
//!   visits (a consumer forgot the volume). Schema stays.
//! - [`record_visit`](ImportanceWriter::record_visit): the navigation-visit
//!   signal — bump a path's visit count and last-visit timestamp (plan Decision
//!   3). Counts and timestamps only.
//!
//! Writes are applied under a single transaction per message so a crash mid-pass
//! leaves the prior generation intact (crash-safety: recompute is idempotent and
//! re-runs from the bus on the next scan completion).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

use rusqlite::Connection;

use super::store::{ImportanceStoreError, RECOMPUTE_GENERATION_KEY, open_write_connection};
use crate::ignore_poison::IgnorePoison;

/// Bounded channel capacity. A recompute pass sends one `WriteWeights` message
/// carrying the whole volume, so the queue never holds many messages; a modest
/// bound is plenty and provides backpressure on a pathological visit storm.
const CHANNEL_CAPACITY: usize = 1024;

/// One folder's weight to persist. The scheduler builds these from the scorer's
/// output; the serialized signal vector rides along (plan Decision 2).
#[derive(Debug, Clone, PartialEq)]
pub struct WeightRow {
    pub path: String,
    pub score: f64,
    /// The serialized [`super::FolderSignals`] JSON.
    pub signals_json: String,
}

/// Messages to the writer thread.
enum WriteMessage {
    /// Write a recompute pass's weights at `generation`, advancing the stored
    /// recompute generation to it. Rows upsert on the path PK.
    WriteWeights { generation: u64, rows: Vec<WeightRow> },
    /// Write an INCREMENTAL rescore's weights at `generation` WITHOUT advancing
    /// the stored generation. Only the touched folders' rows are upserted (stamped
    /// at the current generation, so they stay as-fresh-as the last full pass);
    /// every untouched folder keeps its existing as-of generation. Used by the
    /// changed-subtree recompute (plan Decision 5).
    WriteWeightsIncremental { generation: u64, rows: Vec<WeightRow> },
    /// Drop all weight and visit rows (a consumer forgot the volume).
    PurgeVolume,
    /// Record a navigation visit: bump the path's count and set its last-visit
    /// timestamp to `at_secs` (Unix seconds).
    RecordVisit { path: String, at_secs: u64 },
    /// Read the current recompute generation on the writer's own connection and
    /// reply with `current + 1` — the generation the caller stamps its next pass
    /// at. Reading it here (not on a separate connection) keeps the generation a
    /// single-writer-owned value: no reader races a concurrent write.
    NextGeneration(mpsc::Sender<u64>),
    /// Barrier: signal once all prior messages are committed.
    Flush(mpsc::Sender<()>),
    /// Shut the writer thread down.
    Shutdown,
}

/// A cloneable handle to a volume's importance writer thread.
#[derive(Clone)]
pub struct ImportanceWriter {
    sender: mpsc::SyncSender<WriteMessage>,
    thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    db_path: PathBuf,
}

impl ImportanceWriter {
    /// Spawn the writer thread with its own write connection to `db_path`.
    ///
    /// The DB file and schema must already exist (open the [`ImportanceStore`]
    /// first, or let `open_write_connection` create them — it creates tables but
    /// not the schema-version stamp; `ImportanceStore::open` owns that). In
    /// practice the scheduler opens the store, then spawns the writer.
    ///
    /// [`ImportanceStore`]: super::store::ImportanceStore
    pub fn spawn(db_path: &Path) -> Result<Self, ImportanceStoreError> {
        let conn = open_write_connection(db_path)?;
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(CHANNEL_CAPACITY);

        let handle = thread::Builder::new()
            .name("importance-writer".into())
            .spawn(move || writer_loop(conn, receiver))
            .map_err(ImportanceStoreError::Io)?;

        Ok(Self {
            sender,
            thread_handle: Arc::new(Mutex::new(Some(handle))),
            db_path: db_path.to_path_buf(),
        })
    }

    /// The DB file this writer serves.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Write a recompute pass's weights, stamping them at `generation` and
    /// advancing the stored generation to it. Blocks if the channel is full
    /// (backpressure).
    pub fn write_weights(&self, generation: u64, rows: Vec<WeightRow>) -> Result<(), ImportanceStoreError> {
        self.send(WriteMessage::WriteWeights { generation, rows })
    }

    /// Upsert an INCREMENTAL rescore's rows at `generation` without advancing the
    /// stored generation, so untouched folders keep their as-of marker. The
    /// caller reads the current generation (via [`next_generation`] minus one, or
    /// the read API) and passes it here (plan Decision 5).
    ///
    /// [`next_generation`]: ImportanceWriter::next_generation
    pub fn write_weights_incremental(&self, generation: u64, rows: Vec<WeightRow>) -> Result<(), ImportanceStoreError> {
        self.send(WriteMessage::WriteWeightsIncremental { generation, rows })
    }

    /// Drop every weight and visit row for this volume (forget). Schema stays.
    pub fn purge_volume(&self) -> Result<(), ImportanceStoreError> {
        self.send(WriteMessage::PurgeVolume)
    }

    /// Record a navigation visit to `path` at `at_secs` (Unix seconds).
    pub fn record_visit(&self, path: &str, at_secs: u64) -> Result<(), ImportanceStoreError> {
        self.send(WriteMessage::RecordVisit {
            path: path.to_string(),
            at_secs,
        })
    }

    /// The generation the next recompute pass should stamp: the current stored
    /// generation plus one, read on the writer thread's own connection. Blocks
    /// until the writer replies, so it also acts as a barrier for prior messages.
    pub fn next_generation(&self) -> Result<u64, ImportanceStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::NextGeneration(tx))?;
        rx.recv().map_err(|_| {
            ImportanceStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "importance writer thread is gone",
            ))
        })
    }

    /// Block until all prior messages are committed. Returns once the writer
    /// thread has drained the queue up to this barrier.
    pub fn flush_blocking(&self) -> Result<(), ImportanceStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Flush(tx))?;
        // The writer thread signals after committing everything before the
        // barrier. A recv error means the thread is gone; treat as flushed.
        let _ = rx.recv();
        Ok(())
    }

    /// Shut the writer down and join its thread. Idempotent.
    pub fn shutdown(&self) {
        let _ = self.sender.send(WriteMessage::Shutdown);
        if let Some(handle) = self.thread_handle.lock_ignore_poison().take() {
            let _ = handle.join();
        }
    }

    fn send(&self, msg: WriteMessage) -> Result<(), ImportanceStoreError> {
        self.sender.send(msg).map_err(|_| {
            ImportanceStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "importance writer thread is gone",
            ))
        })
    }
}

/// The writer thread's main loop: own the write connection, apply each message
/// under a transaction, exit on `Shutdown` or when the channel closes.
fn writer_loop(mut conn: Connection, receiver: mpsc::Receiver<WriteMessage>) {
    while let Ok(msg) = receiver.recv() {
        match msg {
            WriteMessage::WriteWeights { generation, rows } => {
                if let Err(e) = apply_weights(&mut conn, generation, &rows, true) {
                    log::warn!(target: "importance", "write_weights failed (generation {generation}): {e}");
                }
            }
            WriteMessage::WriteWeightsIncremental { generation, rows } => {
                if let Err(e) = apply_weights(&mut conn, generation, &rows, false) {
                    log::warn!(target: "importance", "write_weights_incremental failed (generation {generation}): {e}");
                }
            }
            WriteMessage::PurgeVolume => {
                if let Err(e) = apply_purge(&conn) {
                    log::warn!(target: "importance", "purge_volume failed: {e}");
                }
            }
            WriteMessage::RecordVisit { path, at_secs } => {
                if let Err(e) = apply_visit(&conn, &path, at_secs) {
                    log::warn!(target: "importance", "record_visit failed: {e}");
                }
            }
            WriteMessage::NextGeneration(reply) => {
                let next = super::store::read_generation(&conn).map(|g| g + 1).unwrap_or_else(|e| {
                    log::warn!(target: "importance", "next_generation read failed: {e}");
                    1
                });
                let _ = reply.send(next);
            }
            WriteMessage::Flush(done) => {
                let _ = done.send(());
            }
            WriteMessage::Shutdown => break,
        }
    }
}

/// Apply a recompute's weights under one transaction: upsert every row at
/// `generation`, and (for a full pass, `advance_generation`) bump the stored
/// generation. Doing both in one transaction keeps the generation and the rows
/// consistent — a reader never sees a bumped generation with un-written rows.
///
/// An INCREMENTAL pass (`advance_generation == false`) upserts only the touched
/// folders' rows stamped at the CURRENT generation and does NOT bump it, so every
/// untouched folder keeps its as-of marker and doesn't turn stale (plan Decision
/// 5).
fn apply_weights(
    conn: &mut Connection,
    generation: u64,
    rows: &[WeightRow],
    advance_generation: bool,
) -> Result<(), ImportanceStoreError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO weights (path, score, signals, as_of_generation) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET score = ?2, signals = ?3, as_of_generation = ?4",
        )?;
        for row in rows {
            stmt.execute(rusqlite::params![
                row.path,
                row.score,
                row.signals_json,
                generation as i64
            ])?;
        }
        if advance_generation {
            tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                rusqlite::params![RECOMPUTE_GENERATION_KEY, generation.to_string()],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Drop every weight and visit row. Schema stays.
fn apply_purge(conn: &Connection) -> Result<(), ImportanceStoreError> {
    conn.execute_batch("DELETE FROM weights; DELETE FROM visits;")?;
    Ok(())
}

/// Bump a path's visit count by one and set its last-visit timestamp.
fn apply_visit(conn: &Connection, path: &str, at_secs: u64) -> Result<(), ImportanceStoreError> {
    conn.execute(
        "INSERT INTO visits (path, visit_count, last_visit_secs) VALUES (?1, 1, ?2)
         ON CONFLICT(path) DO UPDATE SET visit_count = visit_count + 1, last_visit_secs = ?2",
        rusqlite::params![path, at_secs as i64],
    )?;
    Ok(())
}
