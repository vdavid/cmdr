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
//!   stored generation to it. Rows upsert on the folded-path PK (a pass rewrites every
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
use std::time::Duration;

use rusqlite::Connection;

use super::store::{ImportanceStoreError, RECOMPUTE_GENERATION_KEY, open_write_connection};
use crate::ignore_poison::IgnorePoison;
use crate::indexing::store::normalize_for_comparison;
use crate::pluralize::pluralize;

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
    /// recompute generation to it. Rows upsert on the folded-path PK.
    WriteWeights { generation: u64, rows: Vec<WeightRow> },
    /// Write an INCREMENTAL rescore's weights at `generation` WITHOUT advancing the
    /// stored generation, keeping untouched folders' as-of markers. In ONE
    /// transaction it first CLEARS each subtree in `delete_subtrees` (a changed path
    /// and everything under it), then upserts `rows` (the non-floored folders in the
    /// touched set, at the current generation). Clearing the subtree first purges
    /// rows for folders that were renamed away, deleted, or became floored, so only
    /// the currently-scored folders survive — the incremental analog of a full
    /// pass's replace-the-table. Used by the changed-subtree recompute (plan
    /// Decision 5).
    WriteWeightsIncremental {
        generation: u64,
        rows: Vec<WeightRow>,
        delete_subtrees: Vec<String>,
    },
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
    /// TRUNCATE the WAL file at a quiet point (recompute completion). Replies once
    /// the checkpoint attempt finishes, so a caller can sequence "recompute, then
    /// checkpoint" deterministically. See [`run_wal_checkpoint`].
    Checkpoint(mpsc::Sender<()>),
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

    /// Clear each subtree in `delete_subtrees` and upsert `rows` at `generation`
    /// (without advancing the stored generation) in one transaction. Clearing the
    /// changed subtrees first purges rows for folders renamed away, deleted, or now
    /// floored; re-inserting only the non-floored `rows` leaves the store holding
    /// exactly the currently-scored folders. Untouched folders (outside every
    /// cleared subtree) keep their rows and as-of markers. The caller reads the
    /// current generation (via [`next_generation`] minus one, or the read API) and
    /// passes it here (plan Decision 5).
    ///
    /// [`next_generation`]: ImportanceWriter::next_generation
    pub fn write_weights_incremental(
        &self,
        generation: u64,
        rows: Vec<WeightRow>,
        delete_subtrees: Vec<String>,
    ) -> Result<(), ImportanceStoreError> {
        self.send(WriteMessage::WriteWeightsIncremental {
            generation,
            rows,
            delete_subtrees,
        })
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

    /// TRUNCATE the WAL on the writer thread's own connection (the single-writer
    /// invariant) at a quiet point — call it right after a recompute completes.
    /// Blocks until the checkpoint attempt finishes. Best-effort: a reader-blocked
    /// truncate degrades to PASSIVE and logs at debug, never an error (the WAL just
    /// doesn't shrink this time; the next recompute retries). See
    /// [`run_wal_checkpoint`].
    pub fn checkpoint_wal(&self) -> Result<(), ImportanceStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Checkpoint(tx))?;
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
                if let Err(e) = apply_full_pass(&mut conn, generation, &rows) {
                    log::warn!(target: "importance", "write_weights failed (generation {generation}): {e}");
                }
            }
            WriteMessage::WriteWeightsIncremental {
                generation,
                rows,
                delete_subtrees,
            } => {
                if let Err(e) = apply_incremental(&mut conn, generation, &rows, &delete_subtrees) {
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
            WriteMessage::Checkpoint(done) => {
                run_wal_checkpoint(&conn);
                let _ = done.send(());
            }
            WriteMessage::Shutdown => break,
        }
    }
}

/// Apply a FULL recompute pass under one transaction: REPLACE the whole weights
/// table with `rows` (stamped at `generation`) and bump the stored generation.
///
/// A full pass rewrites every folder, so it clears the table first — otherwise a
/// folder that was scored last pass but now floors (or vanished from the index)
/// would leave a stale row behind, and the compacted store must never carry a row
/// for a floored folder. Clearing + inserting + bumping in ONE transaction keeps
/// the generation and the rows consistent — a reader never sees a bumped generation
/// with un-written (or stale) rows.
fn apply_full_pass(conn: &mut Connection, generation: u64, rows: &[WeightRow]) -> Result<(), ImportanceStoreError> {
    let tx = conn.transaction()?;
    {
        tx.execute("DELETE FROM weights", [])?;
        insert_rows(&tx, generation, rows)?;
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![RECOMPUTE_GENERATION_KEY, generation.to_string()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Apply an INCREMENTAL rescore under one transaction: CLEAR each subtree in
/// `delete_subtrees` (a changed path and every descendant), then upsert the touched
/// folders' `rows` at the CURRENT `generation` (no bump, so untouched folders keep
/// their as-of marker). Clearing before inserting purges rows for folders renamed
/// away, deleted, or now floored, so only the currently-scored folders survive.
/// Both in one transaction so a reader never sees a half-applied transition.
///
/// The subtree clear is an index-served BINARY range over the folded PK
/// (`path_folded`): an equality on the changed folder's own folded key, plus the
/// half-open range `[folded(prefix) + "/", folded(prefix) + "0")` covering exactly
/// its descendants. Every descendant's folded key starts with `folded(prefix) + "/"`
/// (folding is byte-for-byte stable across the `/` boundary — `/` is ASCII, so NFD
/// and case-folding never cross it), and `"0"` (0x30) is one past `"/"` (0x2f), so
/// the range holds all descendants and nothing else. The `/` boundary means clearing
/// `/a` never touches a sibling like `/ab`. See [`SUBTREE_CLEAR_SQL`].
fn apply_incremental(
    conn: &mut Connection,
    generation: u64,
    rows: &[WeightRow],
    delete_subtrees: &[String],
) -> Result<(), ImportanceStoreError> {
    let tx = conn.transaction()?;
    {
        if !delete_subtrees.is_empty() {
            let mut del = tx.prepare_cached(SUBTREE_CLEAR_SQL)?;
            for prefix in delete_subtrees {
                let f = normalize_for_comparison(prefix);
                // `?1` matches the changed folder itself (its folded PK); `?2`..`?3`
                // is the half-open BINARY range covering exactly its descendants.
                del.execute(rusqlite::params![f, format!("{f}/"), format!("{f}0")])?;
            }
        }
        insert_rows(&tx, generation, rows)?;
    }
    tx.commit()?;
    Ok(())
}

/// The subtree-clear DELETE an incremental rescore runs per changed prefix.
///
/// Served by the BINARY `path_folded` primary key: an equality on the prefix's own
/// folded key plus a half-open range over its descendants (`folded(prefix) + "/"` up
/// to, but not including, `folded(prefix) + "0"`). Because the PK is BINARY (no custom
/// collation), SQLite serves both with index SEARCHes instead of full-scanning every
/// row and re-running the NFD-folding `platform_case` comparison on each — the fix
/// that stops the incremental from pegging a CPU core. Kept as a `const` so the
/// `subtree_clear_delete_is_index_served` test EXPLAINs the exact production SQL.
pub(crate) const SUBTREE_CLEAR_SQL: &str =
    "DELETE FROM weights WHERE path_folded = ?1 OR (path_folded >= ?2 AND path_folded < ?3)";

/// Upsert `rows` on the folded-path PK, stamping each at `generation`. Shared by the full
/// pass and the incremental rescore.
fn insert_rows(
    tx: &rusqlite::Transaction<'_>,
    generation: u64,
    rows: &[WeightRow],
) -> Result<(), ImportanceStoreError> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO weights (path_folded, path, score, signals, as_of_generation) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(path_folded) DO UPDATE SET path = ?2, score = ?3, signals = ?4, as_of_generation = ?5",
    )?;
    for row in rows {
        // `path_folded` is the fold of the verbatim path — the same rule the read
        // side binds and the subtree-clear range compares against, so the three
        // agree by going through one `normalize_for_comparison`.
        stmt.execute(rusqlite::params![
            normalize_for_comparison(&row.path),
            row.path,
            row.score,
            row.signals_json,
            generation as i64
        ])?;
    }
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
        "INSERT INTO visits (path_folded, path, visit_count, last_visit_secs) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(path_folded) DO UPDATE SET visit_count = visit_count + 1, last_visit_secs = ?3",
        rusqlite::params![normalize_for_comparison(path), path, at_secs as i64],
    )?;
    Ok(())
}

/// TRUNCATE the WAL file so its high-water mark doesn't sit on disk. SQLite's
/// default PASSIVE `wal_autocheckpoint` copies frames back into the main DB but
/// reuses the WAL file in place and never shrinks it; only an explicit TRUNCATE
/// reclaims the space. A full recompute REPLACES the whole `weights` table and the
/// every-60s incremental churns pages, so without this the WAL grows to ~100% of the
/// DB and stays there (plan M9).
///
/// Runs on the writer thread's own connection in autocommit: every message commits
/// its transaction before the loop reads the next, so `wal_checkpoint(TRUNCATE)`
/// (which SQLite refuses inside a transaction) is always safe here.
///
/// A long-lived reader snapshot can block the truncate. We give readers a short,
/// bounded grace (mirroring the index writer's ~250 ms cap in
/// `indexing/writer/maintenance.rs`) then degrade to PASSIVE (`busy = 1`): the frames
/// still checkpoint into the main DB, the file just doesn't shrink this time, and the
/// next recompute retries. No retry loop: a persistent reader is working-as-designed.
fn run_wal_checkpoint(conn: &Connection) {
    // A short busy timeout around the truncate: without it the connection's default
    // 5 s timeout (set in `store/connection.rs`) would stall the writer thread (and
    // every write queued behind it) waiting a reader out. Restored right after.
    let _ = conn.busy_timeout(Duration::from_millis(250));
    let result: rusqlite::Result<(i64, i64, i64)> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    });
    let _ = conn.busy_timeout(Duration::from_millis(5000));
    match result {
        Ok((0, log_size, checkpointed)) => {
            log::debug!(target: "importance", "wal_checkpoint TRUNCATE done ({checkpointed} of {})", pluralize(log_size as u64, "frame"));
        }
        Ok((_, log_size, checkpointed)) => {
            log::debug!(target: "importance", "wal_checkpoint partial ({checkpointed} of {}, blocked by readers)", pluralize(log_size as u64, "frame"));
        }
        Err(e) => {
            log::warn!(target: "importance", "wal_checkpoint failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importance::store::{ImportanceStore, importance_db_path, open_read_connection};

    /// The on-disk size of the DB's `-wal` sidecar, or 0 if it's absent.
    fn wal_len(db_path: &Path) -> u64 {
        std::fs::metadata(db_path.with_extension("db-wal"))
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// A fresh importance store + writer over a scratch volume.
    fn writer(dir: &Path) -> (ImportanceWriter, PathBuf) {
        let db_path = importance_db_path(dir, "root");
        ImportanceStore::open(&db_path).expect("open store");
        let w = ImportanceWriter::spawn(&db_path).expect("spawn writer");
        (w, db_path)
    }

    /// Write a full pass of `n` rows through `w` and block until it commits.
    fn write_pass(w: &ImportanceWriter, n: usize) {
        let rows: Vec<WeightRow> = (0..n)
            .map(|i| WeightRow {
                path: format!("/folder/{i}"),
                score: 0.5,
                signals_json: "{}".to_string(),
            })
            .collect();
        let generation = w.next_generation().expect("generation");
        w.write_weights(generation, rows).expect("write weights");
        w.flush_blocking().expect("flush");
    }

    #[test]
    fn checkpoint_truncates_the_wal_at_rest() {
        let dir = tempfile::tempdir().expect("temp");
        let (w, db_path) = writer(dir.path());

        // A committed full pass leaves frames in the WAL; passive autocheckpoint never
        // truncates the file, so it sits non-empty on disk.
        write_pass(&w, 500);
        assert!(wal_len(&db_path) > 0, "the WAL holds frames before the checkpoint");

        // The checkpoint hook truncates it to zero (no reader is blocking).
        w.checkpoint_wal().expect("checkpoint");
        assert_eq!(wal_len(&db_path), 0, "the checkpoint truncated the WAL to zero at rest");

        w.shutdown();
    }

    #[test]
    fn checkpoint_tolerates_a_blocking_reader_without_erroring() {
        let dir = tempfile::tempdir().expect("temp");
        let (w, db_path) = writer(dir.path());
        write_pass(&w, 50);

        // Pin an old read snapshot: an open read transaction holds a WAL read mark, so a
        // later TRUNCATE can't reclaim the frames past it.
        let reader = open_read_connection(&db_path).expect("reader");
        reader.execute_batch("BEGIN").expect("begin read txn");
        let _pinned: i64 = reader
            .query_row("SELECT COUNT(*) FROM weights", [], |r| r.get(0))
            .expect("pin snapshot");

        // Advance the WAL past the reader's snapshot, then checkpoint. The truncate is
        // blocked, but the hook must NOT surface an error (it degrades to PASSIVE).
        write_pass(&w, 50);
        w.checkpoint_wal()
            .expect("checkpoint tolerates the reader without erroring");

        reader.execute_batch("END").ok();

        // The writer keeps working after a blocked checkpoint (the recompute path is intact).
        write_pass(&w, 10);
        w.shutdown();
    }
}
