//! `MediaWriter`: the single writer thread for one volume's `media.db`.
//!
//! Ported from `importance/writer.rs`: exactly ONE writer thread owns the single
//! write connection per DB, and all writes cross a bounded channel. The handle is
//! cloneable; every clone shares the one channel and one thread.
//!
//! This file holds the thread itself: the message enum, the cloneable handle, and the
//! loop that applies each message. The SQL each message runs lives beside it, one file
//! per job: [`upsert`] (record enrichment), [`prune`] (every delete path), and
//! [`maintenance`] (rename, `VACUUM`, purge, WAL checkpoint). [`ann_pending`] holds the
//! ANN ops the loop buffers alongside its CLIP writes.
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
//! - [`gc_paths`](MediaWriter::gc_paths): delete the `media_file` identity row and its
//!   `media_status` + `media_ocr` + `media_tags` + `media_embedding` +
//!   `media_clip_embedding` children for a set of paths whose source files vanished
//!   (deletion-driven GC, run ONLY on a completed-scan edge — see [`super::scheduler`]).
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
//! - [`flush_ann_index`](MediaWriter::flush_ann_index): land the buffered ANN index
//!   ops (plan M6). The writer thread is the ONE producer of incremental ANN
//!   mutations, mirroring each CLIP write/delete it commits; see [`super::ann`].

mod ann_pending;
mod maintenance;
mod prune;
mod upsert;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

use rusqlite::Connection;

use ann_pending::AnnPending;
use maintenance::{apply_purge, apply_rename, apply_vacuum, run_wal_checkpoint};
use prune::{DeletedRow, apply_gc, apply_prune_all_clip, apply_prune_paths, apply_prune_prefix};
pub use upsert::UpsertAnalysis;
use upsert::{ClipWrite, apply_upsert, apply_upsert_clip};

use super::ann;
use super::coverage::accounted;
use super::paths::parent_dir;
use super::store::{EnrichmentState, MediaStatusRow, MediaStoreError, open_write_connection};
use cmdr_fs::ignore_poison::IgnorePoison;

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
    /// Delete every `media_clip_embedding` row and reset every `media_status.clip_stamp`
    /// (the delete-CLIP-model reclaim). Vision columns/tables are untouched. Replies with
    /// the embedding-row count deleted (a barrier). One transaction.
    PruneAllClip { done: mpsc::Sender<usize> },
    /// Move a stored image's enrichment from `old` to `new` by a ONE-ROW
    /// `UPDATE media_file.path` — the whole point of integer-id keying (plan M4): every
    /// child (`media_status`, OCR, tags, embeddings) keys on the unchanged `file_id`, so
    /// they follow for free. Replies whether a row actually moved (a barrier). One
    /// transaction.
    /// ❌ No production sender yet: the rename-following hook this exists for
    /// isn't wired, so a rename still manifests as GC(old) + enrich(new). The
    /// writer handles it and `writer/tests.rs` pins what it does. Keep it until
    /// the capability is deliberately retired, not as a side effect of a
    /// visibility change.
    #[allow(
        dead_code,
        reason = "a supported writer message with no production sender yet; see above"
    )]
    Rename {
        old: String,
        new: String,
        done: mpsc::Sender<bool>,
    },
    /// Reclaim free pages after a prune (`VACUUM`). `media.db` is a disposable cache,
    /// so `VACUUM` is acceptable, and for the privacy retro-delete it's what actually
    /// removes the deleted text from disk. Replies when done (a barrier).
    Vacuum { done: mpsc::Sender<()> },
    /// Drop every status and OCR row for this volume (disable + delete contents).
    /// ❌ No production sender yet: the rename-following hook this exists for
    /// isn't wired, so a rename still manifests as GC(old) + enrich(new). The
    /// writer handles it and `writer/tests.rs` pins what it does. Keep it until
    /// the capability is deliberately retired, not as a side effect of a
    /// visibility change.
    #[allow(
        dead_code,
        reason = "a supported writer message with no production sender yet; see above"
    )]
    PurgeVolume,
    /// Apply the buffered ANN ops to the on-disk index (plan M6) and reply once
    /// saved — a barrier. Called at the same seams that invalidate the resident
    /// vector cache, so the mmap view the query path reloads is current.
    FlushAnn(mpsc::Sender<()>),
    /// Barrier: signal once all prior messages are committed.
    Flush(mpsc::Sender<()>),
    /// TRUNCATE the WAL file at a quiet point (enrichment-pass completion). Replies
    /// once the checkpoint attempt finishes (a barrier). See [`run_wal_checkpoint`].
    Checkpoint(mpsc::Sender<()>),
    /// Shut the writer thread down.
    Shutdown,
}

/// A cloneable handle to a volume's media writer thread.
#[derive(Clone)]
pub struct MediaWriter {
    sender: mpsc::SyncSender<WriteMessage>,
    thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    /// Read only by the test-gated [`MediaWriter::db_path`].
    #[cfg_attr(not(test), allow(dead_code, reason = "read only by the test-gated accessor"))]
    db_path: PathBuf,
}

impl MediaWriter {
    /// Spawn the writer thread with its own write connection to `db_path`, serving
    /// `volume_id`'s `media.db`. The DB file and schema must already exist (open the
    /// [`MediaStore`] first). The thread carries `volume_id` so it can maintain the
    /// per-volume `accounted` aggregate ([`coverage`](super::coverage)) as rows are inserted/deleted.
    ///
    /// [`MediaStore`]: super::store::MediaStore
    pub fn spawn(db_path: &Path, volume_id: &str) -> Result<Self, MediaStoreError> {
        let conn = open_write_connection(db_path)?;
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(CHANNEL_CAPACITY);
        let volume_id = volume_id.to_string();
        let loop_db_path = db_path.to_path_buf();
        let handle = thread::Builder::new()
            .name("media-writer".into())
            .spawn(move || writer_loop(conn, receiver, volume_id, loop_db_path))
            .map_err(MediaStoreError::Io)?;
        Ok(Self {
            sender,
            thread_handle: Arc::new(Mutex::new(Some(handle))),
            db_path: db_path.to_path_buf(),
        })
    }

    /// The DB file this writer serves.
    #[cfg(test)]
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

    /// Move a stored image's enrichment from `old` to `new` with a one-row
    /// `UPDATE media_file.path`; the `file_id`-keyed children (status, OCR, tags,
    /// embeddings) follow untouched (plan M4). Blocks until committed and returns whether a
    /// row actually moved (`false` when `old` had no row, or `new` was already taken). This
    /// is the seam a rename-following hook calls; until one is wired, a rename still
    /// manifests as GC(old) + enrich(new), which this replaces with an O(1) update.
    /// ❌ No production caller yet: the rename-following hook this exists for
    /// isn't wired, so a rename still manifests as GC(old) + enrich(new).
    #[allow(
        dead_code,
        reason = "a supported writer call with no production caller yet; see above"
    )]
    pub fn rename_path(&self, old: &str, new: &str) -> Result<bool, MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Rename {
            old: old.to_string(),
            new: new.to_string(),
            done: tx,
        })?;
        Ok(rx.recv().unwrap_or(false))
    }

    /// Delete every CLIP embedding and reset every row's `clip_stamp` (the delete-model
    /// reclaim). Blocks until committed and returns the embedding-row count removed.
    /// Resetting each stamp to empty ("no model") means a later re-install re-embeds
    /// (the row goes CLIP-stale again). Vision data (OCR/tags/feature print) is kept.
    pub fn prune_all_clip(&self) -> Result<usize, MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::PruneAllClip { done: tx })?;
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
    /// ❌ No production caller yet: the rename-following hook this exists for
    /// isn't wired, so a rename still manifests as GC(old) + enrich(new).
    #[allow(
        dead_code,
        reason = "a supported writer call with no production caller yet; see above"
    )]
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

    /// TRUNCATE the WAL on the writer thread's own connection (the single-writer
    /// invariant) at a quiet point — call it once an enrichment pass completes. Blocks
    /// until the checkpoint attempt finishes. Best-effort: a reader-blocked truncate
    /// degrades to PASSIVE and logs at debug, never an error. See [`run_wal_checkpoint`].
    pub fn checkpoint_wal(&self) -> Result<(), MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::Checkpoint(tx))?;
        let _ = rx.recv();
        Ok(())
    }

    /// Apply the buffered ANN index ops (CLIP upserts/removes since the last flush)
    /// to the on-disk `.usearch` file and block until saved (plan M6). Call at the
    /// same quiet points that invalidate the resident vector cache, BEFORE the
    /// invalidation, so the reloaded mmap view sees the pass's writes. Best-effort:
    /// an unusable index is wiped for rebuild, never an error to the pass.
    pub fn flush_ann_index(&self) -> Result<(), MediaStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::FlushAnn(tx))?;
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
/// The loop is the ONE mutator of `media.db`, this volume's `accounted` aggregate,
/// AND the volume's ANN index deltas (plan M6): it SEEDS the aggregate from the
/// existing rows before processing any write (so every delta composes onto a correct
/// baseline), increments on a genuinely-new `done`/`failed` insert and decrements on
/// each deleted row, and buffers an ANN op per CLIP write/delete
/// ([`AnnPending`]).
fn writer_loop(mut conn: Connection, receiver: mpsc::Receiver<WriteMessage>, volume_id: String, db_path: PathBuf) {
    // A dirty marker from a previous session means that session crashed with
    // unflushed ANN ops, so the on-disk index silently lags the DB: wipe it (the
    // next query rebuilds from the DB, the truth). Before any write.
    ann::wipe_if_crashed(&db_path, ann::AnnSpace::Clip);
    // Seed BEFORE the first write (§ accounted): if a row is ever committed, the seed
    // already ran, so a concurrent command-side seed can never race a delta.
    accounted::seed_from_conn(&volume_id, &conn);
    let mut ann_pending = AnnPending::new(db_path);
    while let Ok(msg) = receiver.recv() {
        match msg {
            WriteMessage::Upsert { row, analysis } => {
                match apply_upsert(&mut conn, &row, analysis.as_ref()) {
                    // A genuinely-new `done`/`failed` row (no prior row for this path)
                    // adds one to its dir's accounted count. A re-enrich or a
                    // `done`↔`failed` transition on an existing path does NOT (the path
                    // was already counted).
                    Ok(true) if matches!(row.state, EnrichmentState::Done | EnrichmentState::Failed) => {
                        accounted::inc(&volume_id, parent_dir(&row.path));
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
                // Dirty BEFORE the commit (see `AnnPending::mark_dirty`).
                ann_pending.mark_dirty();
                match apply_upsert_clip(&mut conn, &path, &clip_stamp, embedding.as_deref()) {
                    Ok(ClipWrite::Stored { file_id }) => {
                        if let Some(vector) = embedding {
                            ann_pending.push(ann::AnnOp::Upsert {
                                key: file_id as u64,
                                vector,
                            });
                        }
                    }
                    Ok(ClipWrite::Cleared { file_id }) => {
                        ann_pending.push(ann::AnnOp::Remove { key: file_id as u64 });
                    }
                    Ok(ClipWrite::NoRow) => {}
                    Err(e) => log::warn!(target: "media_index", "clip upsert failed for '{path}': {e}"),
                }
            }
            WriteMessage::GcPaths { paths } => {
                ann_pending.mark_dirty();
                match apply_gc(&mut conn, &paths) {
                    Ok(deleted) => note_deleted(&volume_id, &mut ann_pending, &deleted),
                    Err(e) => log::warn!(target: "media_index", "gc failed ({} paths): {e}", paths.len()),
                }
            }
            // ❌ Decrement BEFORE signalling `done`, here and in `PrunePrefix`. These are
            // the BLOCKING prunes, and a caller that blocked on a delete reads the
            // aggregate next (reclaim, the coverage badges). Sending first races that
            // read — a race macOS usually wins and Linux usually loses, so it surfaces as
            // a flaky test rather than the stale folder count it is.
            WriteMessage::PrunePaths { paths, done } => {
                ann_pending.mark_dirty();
                let deleted = apply_prune_paths(&mut conn, &paths).unwrap_or_else(|e| {
                    log::warn!(target: "media_index", "prune ({} paths) failed: {e}", paths.len());
                    Vec::new()
                });
                note_deleted(&volume_id, &mut ann_pending, &deleted);
                let _ = done.send(deleted.len());
            }
            WriteMessage::PrunePrefix { prefix, done } => {
                ann_pending.mark_dirty();
                let deleted = apply_prune_prefix(&mut conn, &prefix).unwrap_or_else(|e| {
                    log::warn!(target: "media_index", "prune under '{prefix}' failed: {e}");
                    Vec::new()
                });
                note_deleted(&volume_id, &mut ann_pending, &deleted);
                let _ = done.send(deleted.len());
            }
            WriteMessage::Rename { old, new, done } => {
                // Deliberately NO ANN op: the index keys on the `media_file` id, which a
                // rename leaves unchanged (plan M4/M6) — hits resolve ids back to the
                // CURRENT path at query time.
                let moved = apply_rename(&mut conn, &old, &new).unwrap_or_else(|e| {
                    log::warn!(target: "media_index", "rename '{old}' -> '{new}' failed: {e}");
                    false
                });
                // A rename that crosses parent dirs moves one accounted unit between them.
                if moved {
                    let (old_dir, new_dir) = (parent_dir(&old), parent_dir(&new));
                    if old_dir != new_dir {
                        accounted::dec(&volume_id, old_dir);
                        accounted::inc(&volume_id, new_dir);
                    }
                }
                let _ = done.send(moved);
            }
            WriteMessage::PruneAllClip { done } => {
                // CLIP embeddings aren't part of the accounted aggregate (that counts
                // `media_status` rows, which this leaves intact), so no delta here.
                ann_pending.mark_dirty();
                let removed = apply_prune_all_clip(&mut conn).unwrap_or_else(|e| {
                    log::warn!(target: "media_index", "prune-all-clip failed: {e}");
                    0
                });
                // Every CLIP vector is gone, so the whole CLIP index goes with the rows
                // (incl. the dirty marker); pending clip ops are moot.
                ann::delete_index_files(&ann_pending.db_path, ann::AnnSpace::Clip);
                ann_pending.clear_after_delete();
                let _ = done.send(removed);
            }
            WriteMessage::Vacuum { done } => {
                if let Err(e) = apply_vacuum(&conn) {
                    log::warn!(target: "media_index", "vacuum failed: {e}");
                }
                let _ = done.send(());
            }
            WriteMessage::PurgeVolume => {
                ann_pending.mark_dirty();
                match apply_purge(&conn) {
                    Ok(()) => {
                        accounted::reset(&volume_id);
                        // All rows are gone; the derivative index goes with them.
                        ann::delete_index_files(&ann_pending.db_path, ann::AnnSpace::Clip);
                        ann_pending.clear_after_delete();
                    }
                    Err(e) => log::warn!(target: "media_index", "purge_volume failed: {e}"),
                }
            }
            WriteMessage::FlushAnn(done) => {
                ann_pending.flush();
                let _ = done.send(());
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
    // Land any straggler ANN ops before the thread dies (a clean shutdown must not
    // look like a crash to the next session's dirty-marker check). If a rebuild is
    // in flight this RETAINS instead — deliberately: nobody is left to replay the
    // buffer, so the dirty marker stays on disk and the next session's spawn wipes
    // the possibly-lagging index for a fresh rebuild (conservative, never silent
    // loss).
    ann_pending.flush();
}

/// The deletion bookkeeping GC and both prunes share: decrement the accounted
/// aggregate per removed row, and buffer an ANN remove per id (an absent key is a
/// no-op at flush, so rows without a CLIP vector cost nothing).
fn note_deleted(volume_id: &str, ann_pending: &mut AnnPending, deleted: &[DeletedRow]) {
    for row in deleted {
        accounted::dec(volume_id, parent_dir(&row.path));
        ann_pending.push(ann::AnnOp::Remove {
            key: row.file_id as u64,
        });
    }
}

#[cfg(test)]
mod tests;
