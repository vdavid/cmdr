//! `MediaWriter`: the single writer thread for one volume's `media.db`.
//!
//! Ported from `importance/writer.rs`: exactly ONE writer thread owns the single
//! write connection per DB, and all writes cross a bounded channel. The handle is
//! cloneable; every clone shares the one channel and one thread.
//!
//! ## Command surface (M1)
//!
//! - [`upsert`](MediaWriter::upsert): record one image's enrichment — upsert its
//!   `media_status` row and replace its `media_ocr` text (or clear the text on a
//!   failure) in ONE transaction.
//! - [`gc_paths`](MediaWriter::gc_paths): delete the `media_status` + `media_ocr`
//!   rows for a set of paths whose source files vanished (deletion-driven GC, run
//!   ONLY on a completed-scan edge — see [`super::scheduler`]).
//! - [`purge_volume`](MediaWriter::purge_volume): drop all rows (the feature was
//!   disabled and the user chose to delete `media.db`'s contents).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

use rusqlite::Connection;

use super::store::{MediaStatusRow, MediaStoreError, open_write_connection};
use crate::ignore_poison::IgnorePoison;

/// Bounded channel capacity. Enrichment sends one `Upsert` per image; a modest
/// bound gives backpressure without holding many messages.
const CHANNEL_CAPACITY: usize = 1024;

/// Messages to the writer thread.
enum WriteMessage {
    /// Upsert one image's status row and replace its OCR text. `ocr_text` is
    /// `Some` on success (replaces the FTS rows for this path) and `None` on a
    /// failure/skip (clears any prior FTS rows). One transaction.
    Upsert {
        row: MediaStatusRow,
        ocr_text: Option<String>,
    },
    /// Delete the status + OCR rows for each path (deletion-driven GC). One
    /// transaction over the whole batch.
    GcPaths { paths: Vec<String> },
    /// Drop every status and OCR row for this volume (disable + delete contents).
    PurgeVolume,
    /// Barrier: signal once all prior messages are committed.
    Flush(mpsc::Sender<()>),
    /// Shut the writer thread down.
    Shutdown,
}

/// A cloneable handle to a volume's media writer thread.
#[derive(Clone)]
pub struct MediaWriter {
    sender: mpsc::SyncSender<WriteMessage>,
    thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    db_path: PathBuf,
}

impl MediaWriter {
    /// Spawn the writer thread with its own write connection to `db_path`. The DB
    /// file and schema must already exist (open the [`MediaStore`] first).
    ///
    /// [`MediaStore`]: super::store::MediaStore
    pub fn spawn(db_path: &Path) -> Result<Self, MediaStoreError> {
        let conn = open_write_connection(db_path)?;
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(CHANNEL_CAPACITY);
        let handle = thread::Builder::new()
            .name("media-writer".into())
            .spawn(move || writer_loop(conn, receiver))
            .map_err(MediaStoreError::Io)?;
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

    /// Upsert one image's enrichment. On success pass `Some(text)`; on a failure
    /// pass `None` (the status row records the failure; any prior OCR text is
    /// cleared).
    pub fn upsert(&self, row: MediaStatusRow, ocr_text: Option<String>) -> Result<(), MediaStoreError> {
        self.send(WriteMessage::Upsert { row, ocr_text })
    }

    /// GC the status + OCR rows for `paths` (their source files vanished). A no-op
    /// on an empty batch.
    pub fn gc_paths(&self, paths: Vec<String>) -> Result<(), MediaStoreError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.send(WriteMessage::GcPaths { paths })
    }

    /// Drop every status and OCR row for this volume. Schema stays.
    pub fn purge_volume(&self) -> Result<(), MediaStoreError> {
        self.send(WriteMessage::PurgeVolume)
    }

    /// Block until all prior messages are committed.
    pub fn flush_blocking(&self) -> Result<(), MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Flush(tx))?;
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

    fn send(&self, msg: WriteMessage) -> Result<(), MediaStoreError> {
        self.sender.send(msg).map_err(|_| {
            MediaStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "media writer thread is gone",
            ))
        })
    }
}

/// The writer thread's main loop: own the write connection, apply each message
/// under a transaction, exit on `Shutdown` or when the channel closes.
fn writer_loop(mut conn: Connection, receiver: mpsc::Receiver<WriteMessage>) {
    while let Ok(msg) = receiver.recv() {
        match msg {
            WriteMessage::Upsert { row, ocr_text } => {
                if let Err(e) = apply_upsert(&mut conn, &row, ocr_text.as_deref()) {
                    log::warn!(target: "media_index", "upsert failed for '{}': {e}", row.path);
                }
            }
            WriteMessage::GcPaths { paths } => {
                if let Err(e) = apply_gc(&mut conn, &paths) {
                    log::warn!(target: "media_index", "gc failed ({} paths): {e}", paths.len());
                }
            }
            WriteMessage::PurgeVolume => {
                if let Err(e) = apply_purge(&conn) {
                    log::warn!(target: "media_index", "purge_volume failed: {e}");
                }
            }
            WriteMessage::Flush(done) => {
                let _ = done.send(());
            }
            WriteMessage::Shutdown => break,
        }
    }
}

/// Upsert one status row and replace its OCR text in one transaction. The OCR text
/// is always cleared first (a re-enrichment must not leave stale FTS rows), then
/// re-inserted only when `ocr_text` is `Some`.
fn apply_upsert(conn: &mut Connection, row: &MediaStatusRow, ocr_text: Option<&str>) -> Result<(), MediaStoreError> {
    let tx = conn.transaction()?;
    {
        tx.execute(
            "INSERT INTO media_status (path, mtime, size, media_kind, state, engine_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                mtime = ?2, size = ?3, media_kind = ?4, state = ?5, engine_version = ?6",
            rusqlite::params![
                row.path,
                row.mtime.map(|v| v as i64),
                row.size.map(|v| v as i64),
                row.media_kind.as_token(),
                row.state.as_token(),
                row.engine_version,
            ],
        )?;
        tx.execute("DELETE FROM media_ocr WHERE path = ?1", rusqlite::params![row.path])?;
        if let Some(text) = ocr_text {
            tx.execute(
                "INSERT INTO media_ocr (path, text) VALUES (?1, ?2)",
                rusqlite::params![row.path, text],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Delete the status + OCR rows for each path in one transaction.
fn apply_gc(conn: &mut Connection, paths: &[String]) -> Result<(), MediaStoreError> {
    let tx = conn.transaction()?;
    {
        let mut del_status = tx.prepare_cached("DELETE FROM media_status WHERE path = ?1")?;
        let mut del_ocr = tx.prepare_cached("DELETE FROM media_ocr WHERE path = ?1")?;
        for path in paths {
            del_status.execute(rusqlite::params![path])?;
            del_ocr.execute(rusqlite::params![path])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Drop every status and OCR row. Schema stays.
fn apply_purge(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute_batch("DELETE FROM media_status; DELETE FROM media_ocr;")?;
    Ok(())
}
