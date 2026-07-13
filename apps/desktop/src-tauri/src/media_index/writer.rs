//! `MediaWriter`: the single writer thread for one volume's `media.db`.
//!
//! Ported from `importance/writer.rs`: exactly ONE writer thread owns the single
//! write connection per DB, and all writes cross a bounded channel. The handle is
//! cloneable; every clone shares the one channel and one thread.
//!
//! ## Command surface
//!
//! - [`upsert`](MediaWriter::upsert): record one image's enrichment — upsert its
//!   `media_status` row and replace its searchable text (OCR + folded tag labels in
//!   `media_ocr`), its structured `media_tags`, and its `media_embedding` in ONE
//!   transaction. On a failure the text/tags/embedding are cleared (only the status
//!   row records the failure).
//! - [`gc_paths`](MediaWriter::gc_paths): delete the `media_status` + `media_ocr` +
//!   `media_tags` + `media_embedding` rows for a set of paths whose source files
//!   vanished (deletion-driven GC, run ONLY on a completed-scan edge — see
//!   [`super::scheduler`]).
//! - [`purge_volume`](MediaWriter::purge_volume): drop all rows (the feature was
//!   disabled and the user chose to delete `media.db`'s contents).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

use rusqlite::Connection;

use super::backend::Tag;
use super::store::{MediaStatusRow, MediaStoreError, encode_embedding, open_write_connection};
use crate::ignore_poison::IgnorePoison;

/// Bounded channel capacity. Enrichment sends one `Upsert` per image; a modest
/// bound gives backpressure without holding many messages.
const CHANNEL_CAPACITY: usize = 1024;

/// Messages to the writer thread.
enum WriteMessage {
    /// Upsert one image's status row and replace its searchable text, tags, and
    /// embedding. On success `analysis` is `Some` (replaces the FTS + tag + embedding
    /// rows for this path); on a failure/skip it's `None` (clears any prior rows). One
    /// transaction.
    Upsert {
        row: MediaStatusRow,
        analysis: Option<UpsertAnalysis>,
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

/// The enrichment outputs one successful `upsert` persists for an image: the
/// searchable OCR text, the scene/object tags, and the feature-print embedding.
/// Assembled by the enrich core from a backend [`Analysis`](super::backend::Analysis).
#[derive(Debug, Clone, Default)]
pub struct UpsertAnalysis {
    /// The recognized OCR text (empty for an image with no text). Stored as the
    /// `source = 'ocr'` FTS row when non-empty.
    pub ocr_text: String,
    /// The scene/object tags (label + score). Stored structurally in `media_tags`
    /// and their labels folded into the FTS as the `source = 'tag'` row.
    pub tags: Vec<Tag>,
    /// The image feature-print embedding, or `None` if the backend produced none.
    pub embedding: Option<Vec<f32>>,
}

impl UpsertAnalysis {
    /// An analysis carrying only OCR text (no tags, no embedding) — the shape the
    /// store/writer round-trip tests use to assert the OCR path in isolation.
    pub fn ocr_only(text: impl Into<String>) -> Self {
        Self {
            ocr_text: text.into(),
            ..Default::default()
        }
    }
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

    /// Upsert one image's enrichment. On success pass `Some(analysis)`; on a failure
    /// pass `None` (the status row records the failure; any prior text/tags/embedding
    /// are cleared).
    pub fn upsert(&self, row: MediaStatusRow, analysis: Option<UpsertAnalysis>) -> Result<(), MediaStoreError> {
        self.send(WriteMessage::Upsert { row, analysis })
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
            WriteMessage::Upsert { row, analysis } => {
                if let Err(e) = apply_upsert(&mut conn, &row, analysis.as_ref()) {
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

/// Upsert one status row and replace its searchable text, tags, and embedding in one
/// transaction. The prior text/tags/embedding rows are always cleared first (a
/// re-enrichment must not leave stale rows), then re-inserted only when `analysis` is
/// `Some` (a success). The OCR FTS row is written only for non-empty text; the folded
/// tag FTS row + structured `media_tags` only for non-empty tags; the embedding only
/// when present.
fn apply_upsert(
    conn: &mut Connection,
    row: &MediaStatusRow,
    analysis: Option<&UpsertAnalysis>,
) -> Result<(), MediaStoreError> {
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
        // Clear every prior derived row for this path (one `WHERE path = ?` each).
        tx.execute("DELETE FROM media_ocr WHERE path = ?1", rusqlite::params![row.path])?;
        tx.execute("DELETE FROM media_tags WHERE path = ?1", rusqlite::params![row.path])?;
        tx.execute(
            "DELETE FROM media_embedding WHERE path = ?1",
            rusqlite::params![row.path],
        )?;

        if let Some(analysis) = analysis {
            if !analysis.ocr_text.is_empty() {
                tx.execute(
                    "INSERT INTO media_ocr (path, source, text) VALUES (?1, 'ocr', ?2)",
                    rusqlite::params![row.path, analysis.ocr_text],
                )?;
            }
            if !analysis.tags.is_empty() {
                // Fold the tag labels into the FTS as one searchable row, and store
                // the structured (label, score) rows for tag-score filtering.
                let labels = analysis
                    .tags
                    .iter()
                    .map(|t| t.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                tx.execute(
                    "INSERT INTO media_ocr (path, source, text) VALUES (?1, 'tag', ?2)",
                    rusqlite::params![row.path, labels],
                )?;
                let mut ins_tag =
                    tx.prepare_cached("INSERT INTO media_tags (path, label, score) VALUES (?1, ?2, ?3)")?;
                for tag in &analysis.tags {
                    ins_tag.execute(rusqlite::params![row.path, tag.label, tag.score as f64])?;
                }
            }
            if let Some(vector) = &analysis.embedding {
                tx.execute(
                    "INSERT INTO media_embedding (path, dims, vector) VALUES (?1, ?2, ?3)",
                    rusqlite::params![row.path, vector.len() as i64, encode_embedding(vector)],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

/// Delete the status + text + tag + embedding rows for each path in one transaction.
fn apply_gc(conn: &mut Connection, paths: &[String]) -> Result<(), MediaStoreError> {
    let tx = conn.transaction()?;
    {
        let mut del_status = tx.prepare_cached("DELETE FROM media_status WHERE path = ?1")?;
        let mut del_ocr = tx.prepare_cached("DELETE FROM media_ocr WHERE path = ?1")?;
        let mut del_tags = tx.prepare_cached("DELETE FROM media_tags WHERE path = ?1")?;
        let mut del_emb = tx.prepare_cached("DELETE FROM media_embedding WHERE path = ?1")?;
        for path in paths {
            del_status.execute(rusqlite::params![path])?;
            del_ocr.execute(rusqlite::params![path])?;
            del_tags.execute(rusqlite::params![path])?;
            del_emb.execute(rusqlite::params![path])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Drop every derived row. Schema stays.
fn apply_purge(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute_batch(
        "DELETE FROM media_status; DELETE FROM media_ocr; DELETE FROM media_tags; DELETE FROM media_embedding;",
    )?;
    Ok(())
}
