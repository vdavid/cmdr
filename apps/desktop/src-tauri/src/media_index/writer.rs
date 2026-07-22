//! `MediaWriter`: the single writer thread for one volume's `media.db`.
//!
//! Ported from `importance/writer.rs`: exactly ONE writer thread owns the single
//! write connection per DB, and all writes cross a bounded channel. The handle is
//! cloneable; every clone shares the one channel and one thread.
//!
//! ## Command surface
//!
//! - [`upsert`](MediaWriter::upsert): record one image's VISION enrichment — upsert its
//!   `media_status` row (identity + `engine_version`, NOT `clip_stamp`) and replace its
//!   searchable text (OCR + folded tag labels in `media_ocr`), its structured
//!   `media_tags`, and its `media_embedding` in ONE transaction. On a failure the
//!   text/tags/embedding are cleared (only the status row records the failure).
//! - [`upsert_clip`](MediaWriter::upsert_clip): record one image's CLIP embedding —
//!   stamp `media_status.clip_stamp` and replace `media_clip_embedding`, WITHOUT touching
//!   the Vision columns or tables. The two provenance stamps have two independent owners
//!   (plan M3 two-part staleness): installing/upgrading the CLIP model re-embeds CLIP
//!   without re-running OCR/tags, and a Vision engine bump re-runs OCR/tags without
//!   re-embedding CLIP.
//! - [`gc_paths`](MediaWriter::gc_paths): delete the `media_status` + `media_ocr` +
//!   `media_tags` + `media_embedding` rows for a set of paths whose source files
//!   vanished (deletion-driven GC, run ONLY on a completed-scan edge — see
//!   [`super::scheduler`]).
//! - [`prune_paths`](MediaWriter::prune_paths) /
//!   [`prune_under_folder`](MediaWriter::prune_under_folder): USER-EXPLICIT deletion
//!   (the privacy retro-delete + the reclaim prune), by an explicit path list or by a
//!   folder prefix. Distinct from GC (which derives from scan state): these delete
//!   because the user asked, so they need no completed-scan edge. Both return the row
//!   count deleted (blocking, so they double as a flush barrier).
//! - [`vacuum`](MediaWriter::vacuum): reclaim the free pages a prune leaves behind
//!   (privacy: the deleted OCR text is gone from disk, not just logically). Blocking.
//! - [`purge_volume`](MediaWriter::purge_volume): drop all rows (the feature was
//!   disabled and the user chose to delete `media.db`'s contents).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

use rusqlite::Connection;

use super::backend::Tag;
use super::coverage;
use super::scheduler::enrich::parent_dir;
use super::store::{EnrichmentState, MediaStatusRow, MediaStoreError, encode_embedding, open_write_connection};
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
    /// Stamp one path's `media_status.clip_stamp` and replace its `media_clip_embedding`
    /// (CLIP two-part staleness). `embedding` is `None` on a CLIP failure/skip (stamps
    /// the row so it isn't retried, but stores no vector). Only ever runs for a path that
    /// already has a `media_status` row (CLIP is eligible only when Vision is current), so
    /// a missing row skips the embedding write rather than orphaning it. One transaction.
    UpsertClip {
        path: String,
        clip_stamp: String,
        embedding: Option<Vec<f32>>,
    },
    /// Delete the status + OCR rows for each path (deletion-driven GC). One
    /// transaction over the whole batch.
    GcPaths { paths: Vec<String> },
    /// USER-EXPLICIT prune of an explicit path list (the reclaim prune passes its
    /// Rust-selected doomed set here). Replies with the row count deleted, so the
    /// caller both learns the count and gets a flush barrier. One transaction.
    PrunePaths {
        paths: Vec<String>,
        done: mpsc::Sender<usize>,
    },
    /// USER-EXPLICIT prune of every row at or under a folder `prefix` (the privacy
    /// retro-delete). The doomed set is derived on the writer thread from the CURRENT
    /// committed rows (trailing-slash-safe `path_is_within`), so it can't miss a row a
    /// concurrent upsert just committed. Replies with the row count deleted. One
    /// transaction.
    PrunePrefix { prefix: String, done: mpsc::Sender<usize> },
    /// Reclaim free pages after a prune (`VACUUM`). `media.db` is a disposable cache,
    /// so `VACUUM` is acceptable, and for the privacy retro-delete it's what actually
    /// removes the deleted text from disk. Replies when done (a barrier).
    Vacuum { done: mpsc::Sender<()> },
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
    /// Spawn the writer thread with its own write connection to `db_path`, serving
    /// `volume_id`'s `media.db`. The DB file and schema must already exist (open the
    /// [`MediaStore`] first). The thread carries `volume_id` so it can maintain the
    /// per-volume `accounted` aggregate ([`coverage`]) as rows are inserted/deleted.
    ///
    /// [`MediaStore`]: super::store::MediaStore
    pub fn spawn(db_path: &Path, volume_id: &str) -> Result<Self, MediaStoreError> {
        let conn = open_write_connection(db_path)?;
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(CHANNEL_CAPACITY);
        let volume_id = volume_id.to_string();
        let handle = thread::Builder::new()
            .name("media-writer".into())
            .spawn(move || writer_loop(conn, receiver, volume_id))
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

    /// Stamp `path`'s CLIP provenance and replace its `media_clip_embedding`. `embedding`
    /// is `Some` on success and `None` on a CLIP failure/skip (stamps so it isn't retried,
    /// stores no vector). Independent of [`upsert`](MediaWriter::upsert) — it touches only
    /// `media_status.clip_stamp` and `media_clip_embedding`, never the Vision columns/tables.
    pub fn upsert_clip(
        &self,
        path: String,
        clip_stamp: String,
        embedding: Option<Vec<f32>>,
    ) -> Result<(), MediaStoreError> {
        self.send(WriteMessage::UpsertClip {
            path,
            clip_stamp,
            embedding,
        })
    }

    /// GC the status + OCR rows for `paths` (their source files vanished). A no-op
    /// on an empty batch.
    pub fn gc_paths(&self, paths: Vec<String>) -> Result<(), MediaStoreError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.send(WriteMessage::GcPaths { paths })
    }

    /// Prune an explicit path list (the reclaim prune's Rust-selected doomed set).
    /// Blocks until the delete commits and returns the row count removed. A no-op on an
    /// empty batch.
    pub fn prune_paths(&self, paths: Vec<String>) -> Result<usize, MediaStoreError> {
        if paths.is_empty() {
            return Ok(0);
        }
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::PrunePaths { paths, done: tx })?;
        Ok(rx.recv().unwrap_or(0))
    }

    /// Prune every row at or under a folder `prefix` (the privacy retro-delete). Blocks
    /// until the delete commits and returns the row count removed. Because it blocks
    /// until committed, calling it twice in a row is a "delete → barrier → delete"
    /// double-tap: the second call sweeps any straggler an in-flight upsert re-added
    /// between the first delete and its barrier.
    pub fn prune_under_folder(&self, prefix: &str) -> Result<usize, MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::PrunePrefix {
            prefix: prefix.to_string(),
            done: tx,
        })?;
        Ok(rx.recv().unwrap_or(0))
    }

    /// `VACUUM` the DB to reclaim the free pages a prune left (and, for the privacy
    /// retro-delete, actually remove the deleted text from disk). Blocks until done.
    pub fn vacuum(&self) -> Result<(), MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Vacuum { done: tx })?;
        let _ = rx.recv();
        Ok(())
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
///
/// The loop is the ONE mutator of both `media.db` and this volume's `accounted`
/// aggregate: it SEEDS the aggregate from the existing rows before processing any write
/// (so every delta composes onto a correct baseline), then increments on a genuinely-new
/// `done`/`failed` insert and decrements on each deleted row.
fn writer_loop(mut conn: Connection, receiver: mpsc::Receiver<WriteMessage>, volume_id: String) {
    // Seed BEFORE the first write (§ accounted): if a row is ever committed, the seed
    // already ran, so a concurrent command-side seed can never race a delta.
    coverage::seed_accounted_from_conn(&volume_id, &conn);
    while let Ok(msg) = receiver.recv() {
        match msg {
            WriteMessage::Upsert { row, analysis } => {
                match apply_upsert(&mut conn, &row, analysis.as_ref()) {
                    // A genuinely-new `done`/`failed` row (no prior row for this path)
                    // adds one to its dir's accounted count. A re-enrich or a
                    // `done`↔`failed` transition on an existing path does NOT (the path
                    // was already counted).
                    Ok(true) if matches!(row.state, EnrichmentState::Done | EnrichmentState::Failed) => {
                        coverage::accounted_inc(&volume_id, parent_dir(&row.path));
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!(target: "media_index", "upsert failed for '{}': {e}", row.path),
                }
            }
            WriteMessage::UpsertClip {
                path,
                clip_stamp,
                embedding,
            } => {
                if let Err(e) = apply_upsert_clip(&mut conn, &path, &clip_stamp, embedding.as_deref()) {
                    log::warn!(target: "media_index", "clip upsert failed for '{path}': {e}");
                }
            }
            WriteMessage::GcPaths { paths } => match apply_gc(&mut conn, &paths) {
                Ok(deleted) => decrement_accounted(&volume_id, &deleted),
                Err(e) => log::warn!(target: "media_index", "gc failed ({} paths): {e}", paths.len()),
            },
            WriteMessage::PrunePaths { paths, done } => {
                let deleted = apply_prune_paths(&mut conn, &paths).unwrap_or_else(|e| {
                    log::warn!(target: "media_index", "prune ({} paths) failed: {e}", paths.len());
                    Vec::new()
                });
                let _ = done.send(deleted.len());
                decrement_accounted(&volume_id, &deleted);
            }
            WriteMessage::PrunePrefix { prefix, done } => {
                let deleted = apply_prune_prefix(&mut conn, &prefix).unwrap_or_else(|e| {
                    log::warn!(target: "media_index", "prune under '{prefix}' failed: {e}");
                    Vec::new()
                });
                let _ = done.send(deleted.len());
                decrement_accounted(&volume_id, &deleted);
            }
            WriteMessage::Vacuum { done } => {
                if let Err(e) = apply_vacuum(&conn) {
                    log::warn!(target: "media_index", "vacuum failed: {e}");
                }
                let _ = done.send(());
            }
            WriteMessage::PurgeVolume => match apply_purge(&conn) {
                Ok(()) => coverage::accounted_reset(&volume_id),
                Err(e) => log::warn!(target: "media_index", "purge_volume failed: {e}"),
            },
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
///
/// Returns whether this upsert INSERTED a new `media_status` row (no prior row for the
/// path) vs updated an existing one — a cheap PK existence check inside the same
/// transaction, so the caller can bump the accounted aggregate only on a genuinely-new
/// completion (a re-enrich or `done`↔`failed` transition leaves the count unchanged).
fn apply_upsert(
    conn: &mut Connection,
    row: &MediaStatusRow,
    analysis: Option<&UpsertAnalysis>,
) -> Result<bool, MediaStoreError> {
    let tx = conn.transaction()?;
    // Distinguish insert from update BEFORE the upsert (a cheap point lookup on the PK).
    let exists: i64 = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM media_status WHERE path = ?1)",
        rusqlite::params![row.path],
        |r| r.get(0),
    )?;
    let inserted = exists == 0;
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
    Ok(inserted)
}

/// Stamp `path`'s `media_status.clip_stamp` and replace its `media_clip_embedding` in one
/// transaction, touching NO Vision column or table. If no `media_status` row exists (CLIP
/// only runs when Vision is current, so this shouldn't happen) the embedding write is
/// skipped rather than orphaned.
fn apply_upsert_clip(
    conn: &mut Connection,
    path: &str,
    clip_stamp: &str,
    embedding: Option<&[f32]>,
) -> Result<(), MediaStoreError> {
    let tx = conn.transaction()?;
    {
        let updated = tx.execute(
            "UPDATE media_status SET clip_stamp = ?2 WHERE path = ?1",
            rusqlite::params![path, clip_stamp],
        )?;
        tx.execute(
            "DELETE FROM media_clip_embedding WHERE path = ?1",
            rusqlite::params![path],
        )?;
        if updated > 0
            && let Some(vector) = embedding
        {
            tx.execute(
                "INSERT INTO media_clip_embedding (path, dims, vector) VALUES (?1, ?2, ?3)",
                rusqlite::params![path, vector.len() as i64, encode_embedding(vector)],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Delete the status + text + tag + embedding + clip-embedding rows for each path in one
/// transaction. Returns the paths whose `media_status` row actually existed and was
/// deleted, so the caller decrements the accounted aggregate once per genuinely-removed
/// row (a GC of a path with no row moves nothing).
fn apply_gc(conn: &mut Connection, paths: &[String]) -> Result<Vec<String>, MediaStoreError> {
    delete_rows_for_paths(conn, paths)
}

/// Prune the four tables for an explicit path list in one transaction (the same delete
/// primitive GC uses, reused for the user-explicit prune). Returns the paths whose
/// `media_status` row was actually removed, so the count matches the images the user
/// removed and the caller can decrement the accounted aggregate per removed row.
fn apply_prune_paths(conn: &mut Connection, paths: &[String]) -> Result<Vec<String>, MediaStoreError> {
    delete_rows_for_paths(conn, paths)
}

/// Delete every table's rows for each path in one transaction, returning the paths whose
/// `media_status` row existed (so `delete_status.execute` reported a removal). Shared by
/// GC and the explicit prune so both report the SAME "rows that actually left" set.
fn delete_rows_for_paths(conn: &mut Connection, paths: &[String]) -> Result<Vec<String>, MediaStoreError> {
    let tx = conn.transaction()?;
    let mut deleted = Vec::new();
    {
        let mut del_status = tx.prepare_cached("DELETE FROM media_status WHERE path = ?1")?;
        let mut del_ocr = tx.prepare_cached("DELETE FROM media_ocr WHERE path = ?1")?;
        let mut del_tags = tx.prepare_cached("DELETE FROM media_tags WHERE path = ?1")?;
        let mut del_emb = tx.prepare_cached("DELETE FROM media_embedding WHERE path = ?1")?;
        let mut del_clip = tx.prepare_cached("DELETE FROM media_clip_embedding WHERE path = ?1")?;
        for path in paths {
            let removed = del_status.execute(rusqlite::params![path])?;
            del_ocr.execute(rusqlite::params![path])?;
            del_tags.execute(rusqlite::params![path])?;
            del_emb.execute(rusqlite::params![path])?;
            del_clip.execute(rusqlite::params![path])?;
            if removed > 0 {
                deleted.push(path.clone());
            }
        }
    }
    tx.commit()?;
    Ok(deleted)
}

/// Prune every row at or under a folder `prefix`. The doomed set is derived on the
/// writer thread from the CURRENT committed `media_status` paths, filtered by the SAME
/// trailing-slash-safe [`path_is_within`](super::network::config::path_is_within) the
/// exclusion veto uses (so the delete set can't drift from what the veto forbids), then
/// deleted via [`apply_prune_paths`]. An empty `prefix` matches every path (the whole
/// volume — the user excluded the mount root). Returns the paths actually removed (for
/// the accounted decrement + the delete count).
fn apply_prune_prefix(conn: &mut Connection, prefix: &str) -> Result<Vec<String>, MediaStoreError> {
    let doomed: Vec<String> = {
        let mut stmt = conn.prepare_cached("SELECT path FROM media_status")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for path in rows {
            let path = path?;
            if super::network::config::path_is_within(&path, prefix) {
                out.push(path);
            }
        }
        out
    };
    apply_prune_paths(conn, &doomed)
}

/// `VACUUM` the DB (reclaim free pages; can't run inside a transaction, so it's its own
/// statement).
fn apply_vacuum(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute_batch("VACUUM")?;
    Ok(())
}

/// Drop every derived row. Schema stays.
fn apply_purge(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute_batch(
        "DELETE FROM media_status; DELETE FROM media_ocr; DELETE FROM media_tags; DELETE FROM media_embedding; DELETE FROM media_clip_embedding;",
    )?;
    Ok(())
}

/// Decrement the accounted aggregate once per deleted path, bucketed by parent dir — the
/// shared bookkeeping the GC and both prune paths run after committing their deletes.
fn decrement_accounted(volume_id: &str, deleted: &[String]) {
    for path in deleted {
        coverage::accounted_dec(volume_id, parent_dir(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_index::backend::Tag;
    use crate::media_index::predicate::MediaKind;
    use crate::media_index::store::{MediaStore, media_db_path, open_read_connection};

    /// A fresh media store + writer over a scratch volume.
    fn writer(dir: &Path, volume_id: &str) -> MediaWriter {
        let db_path = media_db_path(dir, volume_id);
        MediaStore::open(&db_path).expect("open media store");
        MediaWriter::spawn(&db_path, volume_id).expect("media writer")
    }

    /// Seed one fully-enriched image (a row in ALL FOUR tables), so a prune test can
    /// assert every kind of row goes.
    fn seed(writer: &MediaWriter, path: &str) {
        writer
            .upsert(
                MediaStatusRow {
                    path: path.to_string(),
                    mtime: Some(1),
                    size: Some(2),
                    media_kind: MediaKind::Image,
                    state: EnrichmentState::Done,
                    engine_version: "e1".to_string(),
                    clip_stamp: String::new(),
                },
                Some(UpsertAnalysis {
                    ocr_text: "some text".to_string(),
                    tags: vec![Tag {
                        label: "beach".to_string(),
                        score: 0.5,
                    }],
                    embedding: Some(vec![1.0, 0.0, 0.0]),
                }),
            )
            .expect("seed");
    }

    /// Count the rows for `path` across all four tables (status, ocr, tags, embedding),
    /// so a deletion assertion covers every table, not just `media_status`. A fully
    /// `seed`ed image has TWO `media_ocr` rows (the OCR text + the folded tag labels),
    /// so a kept image reads `(1, 2, 1, 1)`.
    fn row_counts(db_path: &Path, path: &str) -> (i64, i64, i64, i64) {
        let conn = open_read_connection(db_path).expect("open read");
        let count = |sql: &str| -> i64 {
            conn.query_row(sql, rusqlite::params![path], |r| r.get(0))
                .expect("count")
        };
        (
            count("SELECT COUNT(*) FROM media_status WHERE path = ?1"),
            count("SELECT COUNT(*) FROM media_ocr WHERE path = ?1"),
            count("SELECT COUNT(*) FROM media_tags WHERE path = ?1"),
            count("SELECT COUNT(*) FROM media_embedding WHERE path = ?1"),
        )
    }

    #[test]
    fn prune_under_folder_deletes_rows_at_or_under_and_only_those() {
        let dir = tempfile::tempdir().expect("temp");
        let w = writer(dir.path(), "root");
        let db_path = media_db_path(dir.path(), "root");
        seed(&w, "/a/x.jpg");
        seed(&w, "/a/sub/y.jpg");
        seed(&w, "/b/z.jpg");
        w.flush_blocking().expect("flush");

        // Pruning /a removes both rows under it, across all four tables, and only those.
        let deleted = w.prune_under_folder("/a").expect("prune");
        assert_eq!(deleted, 2, "both rows under /a are counted");

        assert_eq!(row_counts(&db_path, "/a/x.jpg"), (0, 0, 0, 0), "/a/x.jpg fully gone");
        assert_eq!(
            row_counts(&db_path, "/a/sub/y.jpg"),
            (0, 0, 0, 0),
            "/a/sub/y.jpg fully gone (nested under /a)"
        );
        assert_eq!(row_counts(&db_path, "/b/z.jpg"), (1, 2, 1, 1), "/b/z.jpg untouched");
        w.shutdown();
    }

    #[test]
    fn prune_under_folder_is_trailing_slash_safe() {
        let dir = tempfile::tempdir().expect("temp");
        let w = writer(dir.path(), "root");
        let db_path = media_db_path(dir.path(), "root");
        seed(&w, "/Photos/a.jpg");
        seed(&w, "/Photos2/b.jpg");
        w.flush_blocking().expect("flush");

        // A sibling that shares a name prefix is NOT within (/Photos2 ≠ under /Photos).
        let deleted = w.prune_under_folder("/Photos").expect("prune");
        assert_eq!(deleted, 1, "only the real child of /Photos goes");
        assert_eq!(row_counts(&db_path, "/Photos/a.jpg").0, 0, "/Photos/a.jpg gone");
        assert_eq!(row_counts(&db_path, "/Photos2/b.jpg").0, 1, "/Photos2 kept");
        w.shutdown();
    }

    #[test]
    fn prune_paths_deletes_only_the_explicit_set() {
        let dir = tempfile::tempdir().expect("temp");
        let w = writer(dir.path(), "root");
        let db_path = media_db_path(dir.path(), "root");
        seed(&w, "/p1.jpg");
        seed(&w, "/p2.jpg");
        seed(&w, "/p3.jpg");
        w.flush_blocking().expect("flush");

        let deleted = w
            .prune_paths(vec!["/p1.jpg".to_string(), "/p3.jpg".to_string()])
            .expect("prune");
        assert_eq!(deleted, 2);
        assert_eq!(row_counts(&db_path, "/p1.jpg"), (0, 0, 0, 0), "/p1 gone");
        assert_eq!(row_counts(&db_path, "/p2.jpg"), (1, 2, 1, 1), "/p2 kept");
        assert_eq!(row_counts(&db_path, "/p3.jpg"), (0, 0, 0, 0), "/p3 gone");
        w.shutdown();
    }

    #[test]
    fn prune_then_vacuum_round_trips() {
        // A smoke that VACUUM after a prune commits cleanly and the rows stay gone
        // (VACUUM can't run inside a transaction, so this guards the message ordering).
        let dir = tempfile::tempdir().expect("temp");
        let w = writer(dir.path(), "root");
        let db_path = media_db_path(dir.path(), "root");
        seed(&w, "/a/x.jpg");
        w.flush_blocking().expect("flush");
        assert_eq!(w.prune_under_folder("/a").expect("prune"), 1);
        w.vacuum().expect("vacuum");
        assert_eq!(row_counts(&db_path, "/a/x.jpg"), (0, 0, 0, 0), "row gone after vacuum");
        w.shutdown();
    }

    // ── The accounted aggregate maintained through the writer path ───────────────
    // These use a UNIQUE volume id per test: the accounted cache is process-global and
    // keyed by volume id alone, so reusing one id would cross-contaminate.

    /// Upsert one image with a given state through `w` and block until it lands.
    fn upsert_state(w: &MediaWriter, path: &str, mtime: u64, state: EnrichmentState) {
        w.upsert(
            MediaStatusRow {
                path: path.to_string(),
                mtime: Some(mtime),
                size: Some(2),
                media_kind: MediaKind::Image,
                state,
                engine_version: "e1".to_string(),
                clip_stamp: String::new(),
            },
            (state == EnrichmentState::Done).then(|| UpsertAnalysis::ocr_only("t")),
        )
        .expect("upsert");
        w.flush_blocking().expect("flush");
    }

    /// The accounted subtree total for `dir` on `volume_id`.
    fn accounted(volume_id: &str, dir: &str) -> u64 {
        coverage::accounted_subtrees(volume_id, &[dir.to_string()])[0]
    }

    #[test]
    fn accounted_increments_only_on_a_genuinely_new_done_or_failed_row() {
        let dir = tempfile::tempdir().expect("temp");
        let vid = "writer-test-accounted-increment";
        coverage::invalidate_accounted(vid);
        let w = writer(dir.path(), vid);

        // A brand-new done row bumps its dir's accounted count.
        upsert_state(&w, "/photos/a.jpg", 1, EnrichmentState::Done);
        assert_eq!(accounted(vid, "/photos"), 1, "a genuinely-new done row counts");

        // Re-enriching the SAME path (a later mtime, still done) must NOT double-count.
        upsert_state(&w, "/photos/a.jpg", 2, EnrichmentState::Done);
        assert_eq!(
            accounted(vid, "/photos"),
            1,
            "a re-enrich of an existing path adds nothing"
        );

        // A done → failed transition on the existing path keeps accounted stable.
        upsert_state(&w, "/photos/a.jpg", 2, EnrichmentState::Failed);
        assert_eq!(
            accounted(vid, "/photos"),
            1,
            "done↔failed on an existing path is stable"
        );

        // A brand-new FAILED row DOES count (a corrupt file is accounted, not pending).
        upsert_state(&w, "/photos/b.jpg", 1, EnrichmentState::Failed);
        assert_eq!(accounted(vid, "/photos"), 2, "a new failed row counts toward accounted");

        w.shutdown();
        coverage::invalidate_accounted(vid);
    }

    #[test]
    fn accounted_decrements_on_delete_and_never_goes_negative() {
        let dir = tempfile::tempdir().expect("temp");
        let vid = "writer-test-accounted-decrement";
        coverage::invalidate_accounted(vid);
        let w = writer(dir.path(), vid);

        upsert_state(&w, "/p/a.jpg", 1, EnrichmentState::Done);
        upsert_state(&w, "/p/b.jpg", 1, EnrichmentState::Done);
        assert_eq!(accounted(vid, "/p"), 2);

        // GC of one path drops the count by one.
        w.gc_paths(vec!["/p/a.jpg".to_string()]).expect("gc");
        w.flush_blocking().expect("flush");
        assert_eq!(accounted(vid, "/p"), 1, "a GC'd row leaves the accounted count");

        // GC of a path with NO row moves nothing (no phantom decrement).
        w.gc_paths(vec!["/p/never.jpg".to_string()]).expect("gc");
        w.flush_blocking().expect("flush");
        assert_eq!(accounted(vid, "/p"), 1, "GC of a non-existent row is a no-op");

        // An explicit prune of the last row drains it to zero.
        assert_eq!(w.prune_paths(vec!["/p/b.jpg".to_string()]).expect("prune"), 1);
        assert_eq!(accounted(vid, "/p"), 0, "the folder drains to zero, never negative");

        w.shutdown();
        coverage::invalidate_accounted(vid);
    }

    #[test]
    fn accounted_is_seeded_from_existing_rows_when_a_writer_spawns() {
        // A row written this session, then the writer torn down and the cache dropped,
        // simulates a fresh launch over a populated `media.db`: the NEW writer's spawn
        // must seed accounted from the on-disk rows, not start at zero.
        let dir = tempfile::tempdir().expect("temp");
        let vid = "writer-test-accounted-seed-on-spawn";
        coverage::invalidate_accounted(vid);
        let w1 = writer(dir.path(), vid);
        upsert_state(&w1, "/seed/a.jpg", 1, EnrichmentState::Done);
        upsert_state(&w1, "/seed/b.jpg", 1, EnrichmentState::Failed);
        w1.shutdown();

        // Drop the in-memory aggregate, then spawn a fresh writer over the same DB.
        coverage::invalidate_accounted(vid);
        let w2 = writer(dir.path(), vid);
        // A flush barrier guarantees the writer thread ran its seed (its first action).
        w2.flush_blocking().expect("flush");
        assert_eq!(accounted(vid, "/seed"), 2, "the spawn seeded both stored rows");

        w2.shutdown();
        coverage::invalidate_accounted(vid);
    }

    #[test]
    fn purge_resets_the_accounted_aggregate() {
        let dir = tempfile::tempdir().expect("temp");
        let vid = "writer-test-accounted-purge";
        coverage::invalidate_accounted(vid);
        let w = writer(dir.path(), vid);
        upsert_state(&w, "/x/a.jpg", 1, EnrichmentState::Done);
        upsert_state(&w, "/y/b.jpg", 1, EnrichmentState::Done);
        assert_eq!(accounted(vid, "/"), 2, "root rolls up both dirs");

        w.purge_volume().expect("purge");
        w.flush_blocking().expect("flush");
        assert_eq!(accounted(vid, "/"), 0, "purge zeroes the whole aggregate");

        w.shutdown();
        coverage::invalidate_accounted(vid);
    }
}
