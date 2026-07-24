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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use rusqlite::Connection;

use super::ann;
use super::backend::Tag;
use super::coverage;
use super::scheduler::enrich::parent_dir;
use super::store::{EnrichmentState, MediaStatusRow, MediaStoreError, encode_embedding, open_write_connection};
use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;

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

/// The writer thread's buffered ANN mutations (plan M6): ops accumulate beside the
/// DB writes they mirror and land on the `.usearch` file at flush seams. The dirty
/// marker goes on disk BEFORE the first tracked commit, so a crash with unflushed
/// ops is detectable next session (`ann::wipe_if_crashed`).
struct AnnPending {
    db_path: PathBuf,
    ops: Vec<ann::AnnOp>,
    dirty_marked: bool,
}

impl AnnPending {
    fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            ops: Vec::new(),
            dirty_marked: false,
        }
    }

    /// Put the dirty marker on disk (once per batch). MUST run before the DB write
    /// it tracks commits — that ordering is what makes a crash between the commit
    /// and the flush detectable rather than a silently-lagging index.
    fn mark_dirty(&mut self) {
        if !self.dirty_marked {
            ann::mark_dirty(&self.db_path, ann::AnnSpace::Clip);
            self.dirty_marked = true;
        }
    }

    /// Buffer one op; auto-flush past the bound so a long pass can't hold an
    /// unbounded vector buffer.
    fn push(&mut self, op: ann::AnnOp) {
        self.ops.push(op);
        if self.ops.len() >= ann::ANN_PENDING_FLUSH_LIMIT {
            self.flush();
        }
    }

    /// Apply the buffered ops to the on-disk index (best-effort; an unusable index
    /// is wiped for rebuild). Clears the dirty marker via `ann::flush_ops`.
    ///
    /// While a rebuild is IN FLIGHT the buffer is RETAINED instead (ops kept, dirty
    /// marker kept): a flush landing mid-rebuild would lose the ops — applied to a
    /// file the install is about to overwrite, or dropped against a missing/stale
    /// file whose replacement was snapshotted BEFORE these rows committed. The next
    /// seam flush replays the retained batch idempotently on top of the installed
    /// index. The `is_in_flight` → `kick` race is benign in the other direction
    /// too: if a rebuild starts right after this check returns false, its snapshot
    /// includes the rows this flush just applied (their DB writes committed before
    /// the rebuild opens its read connection). The buffer may exceed
    /// [`ann::ANN_PENDING_FLUSH_LIMIT`] during the window — accepted, bounded by
    /// the rebuild's duration (minutes at worst).
    fn flush(&mut self) {
        let space = ann::AnnSpace::Clip;
        if ann::rebuild::is_in_flight(&self.db_path, space) {
            log::debug!(
                target: "media_index",
                "ann flush deferred for {} (rebuild in flight; {} ops retained)",
                self.db_path.display(),
                self.ops.len()
            );
            return;
        }
        let ops = std::mem::take(&mut self.ops);
        let outcome = ann::flush_ops(&self.db_path, space, space.current_model_id(), ops);
        log::debug!(target: "media_index", "ann flush for {}: {outcome:?}", self.db_path.display());
        self.dirty_marked = false;
    }

    /// The volume's ANN index files are being deleted wholesale (purge /
    /// delete-CLIP-model): drop the buffered ops with them.
    fn clear_after_delete(&mut self) {
        self.ops.clear();
        self.dirty_marked = false;
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
    coverage::seed_accounted_from_conn(&volume_id, &conn);
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
                        coverage::accounted_dec(&volume_id, old_dir);
                        coverage::accounted_inc(&volume_id, new_dir);
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
                        coverage::accounted_reset(&volume_id);
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
        coverage::accounted_dec(volume_id, parent_dir(&row.path));
        ann_pending.push(ann::AnnOp::Remove {
            key: row.file_id as u64,
        });
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
    // Resolve the path to its `media_file` id, creating the identity row if it's new. A
    // brand-new `media_file` row means a genuinely-new image (media_file ⇔ media_status
    // 1:1: they're written together and deleted together), which the caller uses to bump
    // the accounted aggregate only on a first completion.
    let (file_id, inserted) = resolve_or_create_file_id(&tx, &row.path)?;
    {
        tx.execute(
            "INSERT INTO media_status (file_id, mtime, size, media_kind, state, engine_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(file_id) DO UPDATE SET
                mtime = ?2, size = ?3, media_kind = ?4, state = ?5, engine_version = ?6",
            rusqlite::params![
                file_id,
                row.mtime.map(|v| v as i64),
                row.size.map(|v| v as i64),
                row.media_kind.as_token(),
                row.state.as_token(),
                row.engine_version,
            ],
        )?;
        // Clear every prior derived row for this file (one `WHERE file_id = ?` each).
        tx.execute("DELETE FROM media_ocr WHERE file_id = ?1", rusqlite::params![file_id])?;
        tx.execute("DELETE FROM media_tags WHERE file_id = ?1", rusqlite::params![file_id])?;
        tx.execute(
            "DELETE FROM media_embedding WHERE file_id = ?1",
            rusqlite::params![file_id],
        )?;

        if let Some(analysis) = analysis {
            if !analysis.ocr_text.is_empty() {
                tx.execute(
                    "INSERT INTO media_ocr (file_id, source, text) VALUES (?1, 'ocr', ?2)",
                    rusqlite::params![file_id, analysis.ocr_text],
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
                    "INSERT INTO media_ocr (file_id, source, text) VALUES (?1, 'tag', ?2)",
                    rusqlite::params![file_id, labels],
                )?;
                let mut ins_tag =
                    tx.prepare_cached("INSERT INTO media_tags (file_id, label, score) VALUES (?1, ?2, ?3)")?;
                for tag in &analysis.tags {
                    ins_tag.execute(rusqlite::params![file_id, tag.label, tag.score as f64])?;
                }
            }
            if let Some(vector) = &analysis.embedding {
                tx.execute(
                    "INSERT INTO media_embedding (file_id, dims, vector) VALUES (?1, ?2, ?3)",
                    rusqlite::params![file_id, vector.len() as i64, encode_embedding(vector)],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(inserted)
}

/// Resolve `path` to its `media_file` id, inserting a new identity row when the path is not
/// yet known. Returns `(file_id, inserted)` where `inserted` is `true` only when a fresh row
/// was created — the "genuinely-new image" signal the accounted aggregate rides on.
fn resolve_or_create_file_id(tx: &rusqlite::Transaction<'_>, path: &str) -> Result<(i64, bool), MediaStoreError> {
    if let Some(id) = lookup_file_id(tx, path)? {
        return Ok((id, false));
    }
    tx.execute("INSERT INTO media_file (path) VALUES (?1)", rusqlite::params![path])?;
    Ok((tx.last_insert_rowid(), true))
}

/// Look up an existing `media_file` id for `path`, or `None` if the path is unknown.
fn lookup_file_id(conn: &Connection, path: &str) -> Result<Option<i64>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT id FROM media_file WHERE path = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![path], |r| r.get::<_, i64>(0))?;
    match rows.next() {
        Some(Ok(id)) => Ok(Some(id)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// What a CLIP upsert did — the writer buffers the matching ANN op off this (plan M6).
enum ClipWrite {
    /// The embedding row was replaced with a fresh vector (ANN: upsert the key).
    Stored { file_id: i64 },
    /// The row was stamped but the embedding cleared — a CLIP failure/skip (ANN:
    /// remove the key so a ghost vector can't linger).
    Cleared { file_id: i64 },
    /// No `media_status` row for the path; nothing was written.
    NoRow,
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
) -> Result<ClipWrite, MediaStoreError> {
    let tx = conn.transaction()?;
    let mut write = ClipWrite::NoRow;
    {
        // CLIP is eligible only for a path Vision already covered, so its `media_file` +
        // `media_status` rows exist. A missing row (shouldn't happen) skips the write
        // rather than orphaning an embedding.
        if let Some(file_id) = lookup_file_id(&tx, path)? {
            let updated = tx.execute(
                "UPDATE media_status SET clip_stamp = ?2 WHERE file_id = ?1",
                rusqlite::params![file_id, clip_stamp],
            )?;
            tx.execute(
                "DELETE FROM media_clip_embedding WHERE file_id = ?1",
                rusqlite::params![file_id],
            )?;
            write = ClipWrite::Cleared { file_id };
            if updated > 0
                && let Some(vector) = embedding
            {
                tx.execute(
                    "INSERT INTO media_clip_embedding (file_id, dims, vector) VALUES (?1, ?2, ?3)",
                    rusqlite::params![file_id, vector.len() as i64, encode_embedding(vector)],
                )?;
                write = ClipWrite::Stored { file_id };
            }
        }
    }
    tx.commit()?;
    Ok(write)
}

/// One row a delete actually removed: its path (the accounted decrement) and its
/// `media_file` id (the ANN index key to remove — plan M6).
struct DeletedRow {
    path: String,
    file_id: i64,
}

/// Delete the status + text + tag + embedding + clip-embedding rows for each path in one
/// transaction. Returns the rows whose `media_status` row actually existed and was
/// deleted, so the caller decrements the accounted aggregate once per genuinely-removed
/// row (a GC of a path with no row moves nothing) and removes the matching ANN keys.
fn apply_gc(conn: &mut Connection, paths: &[String]) -> Result<Vec<DeletedRow>, MediaStoreError> {
    delete_rows_for_paths(conn, paths)
}

/// Prune the four tables for an explicit path list in one transaction (the same delete
/// primitive GC uses, reused for the user-explicit prune). Returns the rows actually
/// removed, so the count matches the images the user removed and the caller can
/// decrement the accounted aggregate (and the ANN keys) per removed row.
fn apply_prune_paths(conn: &mut Connection, paths: &[String]) -> Result<Vec<DeletedRow>, MediaStoreError> {
    delete_rows_for_paths(conn, paths)
}

/// Delete every table's rows for each path in one transaction, returning the rows whose
/// `media_status` row existed (so `delete_status.execute` reported a removal). Shared by
/// GC and the explicit prune so both report the SAME "rows that actually left" set.
fn delete_rows_for_paths(conn: &mut Connection, paths: &[String]) -> Result<Vec<DeletedRow>, MediaStoreError> {
    let tx = conn.transaction()?;
    let mut deleted = Vec::new();
    {
        let mut find = tx.prepare_cached("SELECT id FROM media_file WHERE path = ?1")?;
        let mut del_status = tx.prepare_cached("DELETE FROM media_status WHERE file_id = ?1")?;
        let mut del_ocr = tx.prepare_cached("DELETE FROM media_ocr WHERE file_id = ?1")?;
        let mut del_tags = tx.prepare_cached("DELETE FROM media_tags WHERE file_id = ?1")?;
        let mut del_emb = tx.prepare_cached("DELETE FROM media_embedding WHERE file_id = ?1")?;
        let mut del_clip = tx.prepare_cached("DELETE FROM media_clip_embedding WHERE file_id = ?1")?;
        let mut del_file = tx.prepare_cached("DELETE FROM media_file WHERE id = ?1")?;
        for path in paths {
            // A path with no `media_file` row was never enriched: nothing to remove, and
            // it must NOT count toward the accounted decrement.
            let Some(file_id) = find
                .query_map(rusqlite::params![path], |r| r.get::<_, i64>(0))?
                .next()
                .transpose()?
            else {
                continue;
            };
            del_status.execute(rusqlite::params![file_id])?;
            del_ocr.execute(rusqlite::params![file_id])?;
            del_tags.execute(rusqlite::params![file_id])?;
            del_emb.execute(rusqlite::params![file_id])?;
            del_clip.execute(rusqlite::params![file_id])?;
            del_file.execute(rusqlite::params![file_id])?;
            deleted.push(DeletedRow {
                path: path.clone(),
                file_id,
            });
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
fn apply_prune_prefix(conn: &mut Connection, prefix: &str) -> Result<Vec<DeletedRow>, MediaStoreError> {
    let doomed: Vec<String> = {
        let mut stmt = conn.prepare_cached("SELECT path FROM media_file")?;
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

/// Delete every `media_clip_embedding` row and reset every `media_status.clip_stamp` to
/// empty (no model), in one transaction. Returns the embedding rows removed. Resetting the
/// stamp is what makes a later re-install re-embed (`needs_clip` sees `'' != model_stamp`).
/// Touches NO Vision column or table — deleting the CLIP model must not re-run OCR/tags.
fn apply_prune_all_clip(conn: &mut Connection) -> Result<usize, MediaStoreError> {
    let tx = conn.transaction()?;
    let removed = tx.execute("DELETE FROM media_clip_embedding", [])?;
    tx.execute("UPDATE media_status SET clip_stamp = '' WHERE clip_stamp != ''", [])?;
    tx.commit()?;
    Ok(removed)
}

/// Move a stored image's identity row from `old` to `new` in one transaction — the O(1)
/// rename the integer-id keying buys (plan M4): a single `UPDATE media_file.path`, and every
/// `file_id`-keyed child (status, OCR, tags, embeddings) follows untouched. Returns whether a
/// row moved: `false` when `old` has no row, or `new` is already a distinct enriched path
/// (the `UNIQUE(path)` constraint would reject the update, so it's a no-op, not a crash).
fn apply_rename(conn: &mut Connection, old: &str, new: &str) -> Result<bool, MediaStoreError> {
    let tx = conn.transaction()?;
    // Rename only when `old` has a row AND `new` is free (a taken `new` would violate the
    // `UNIQUE(path)` constraint, so skip it rather than error).
    let moved = if let Some(old_id) = lookup_file_id(&tx, old)?
        && lookup_file_id(&tx, new)?.is_none()
    {
        tx.execute(
            "UPDATE media_file SET path = ?2 WHERE id = ?1",
            rusqlite::params![old_id, new],
        )?;
        true
    } else {
        false
    };
    tx.commit()?;
    Ok(moved)
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
        "DELETE FROM media_status; DELETE FROM media_ocr; DELETE FROM media_tags; DELETE FROM media_embedding; DELETE FROM media_clip_embedding; DELETE FROM media_file;",
    )?;
    Ok(())
}

/// TRUNCATE the WAL file so its high-water mark doesn't sit on disk. Mirrors
/// `importance/writer.rs::run_wal_checkpoint` (this whole module is a port of
/// `importance/`): SQLite's default PASSIVE `wal_autocheckpoint` copies frames back
/// into the main DB but reuses the WAL file in place and never shrinks it; only an
/// explicit TRUNCATE reclaims the space. An enrichment pass upserts a row per image,
/// so without this the WAL creeps up in place (plan M9).
///
/// Runs on the writer thread's own connection in autocommit: every message commits its
/// transaction before the loop reads the next, so `wal_checkpoint(TRUNCATE)` (which
/// SQLite refuses inside a transaction) is always safe here.
///
/// A long-lived reader snapshot can block the truncate. We give readers a short, bounded
/// grace (mirroring the index writer's ~250 ms cap in `indexing/writer/maintenance.rs`)
/// then degrade to PASSIVE (`busy = 1`): the frames still checkpoint into the main DB,
/// the file just doesn't shrink this time, and the next pass retries. No retry loop.
fn run_wal_checkpoint(conn: &Connection) {
    // A short busy timeout around the truncate: without it the connection's default 5 s
    // timeout (set in `store/connection.rs`) would stall the writer thread (and every
    // write queued behind it) waiting a reader out. Restored right after.
    let _ = conn.busy_timeout(Duration::from_millis(250));
    let result: rusqlite::Result<(i64, i64, i64)> = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    });
    let _ = conn.busy_timeout(Duration::from_millis(5000));
    match result {
        Ok((0, log_size, checkpointed)) => {
            log::debug!(target: "media_index", "wal_checkpoint TRUNCATE done ({checkpointed} of {})", pluralize(log_size as u64, "frame"));
        }
        Ok((_, log_size, checkpointed)) => {
            log::debug!(target: "media_index", "wal_checkpoint partial ({checkpointed} of {}, blocked by readers)", pluralize(log_size as u64, "frame"));
        }
        Err(e) => {
            log::warn!(target: "media_index", "wal_checkpoint failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests;
