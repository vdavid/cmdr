//! `ImportanceWriter`: the single writer thread for one volume's `importance.db`.
//!
//! Mirrors the index's `IndexWriter` discipline: exactly ONE writer thread owns
//! the single write connection per DB (the index's one-writer-per-DB invariant),
//! and all writes cross a bounded channel. The handle is cloneable; every clone
//! shares the one channel and one thread.
//!
//! ## Command surface
//!
//! - [`write_weights`](ImportanceWriter::write_weights): write a recompute pass's
//!   weights, stamping every row with the pass generation and advancing the
//!   stored generation to it. Rows upsert on the folded-path PK (a pass rewrites every
//!   folder).
//! - [`write_weights_incremental`](ImportanceWriter::write_weights_incremental):
//!   reconcile the changed subtrees against a live change's rescored rows at the
//!   CURRENT generation — write only the rows whose SIGNALS moved, delete the ones
//!   the pass no longer scores, leave the rest untouched. Blocks, and reports the
//!   [`WeightDelta`] a weight-map consumer applies instead of reloading.
//! - [`purge_volume`](ImportanceWriter::purge_volume): drop all weights and
//!   visits (a consumer forgot the volume). Schema stays.
//! - [`record_visit`](ImportanceWriter::record_visit): the navigation-visit
//!   signal — bump a path's visit count and last-visit timestamp. Counts and
//!   timestamps only.
//!
//! Writes are applied under a single transaction per message so a crash mid-pass
//! leaves the prior generation intact (crash-safety: recompute is idempotent and
//! re-runs from the bus on the next scan completion).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

use super::store::{ImportanceStoreError, RECOMPUTE_GENERATION_KEY, SCORING_POLICY_KEY, open_write_connection};
use crate::indexing::store::normalize_for_comparison;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::pluralize;

/// Bounded channel capacity. A recompute pass sends one `WriteWeights` message
/// carrying the whole volume, so the queue never holds many messages; a modest
/// bound is plenty and provides backpressure on a pathological visit storm.
const CHANNEL_CAPACITY: usize = 1024;

/// The row-level edits one INCREMENTAL transaction made to the store's NON-ZERO
/// weight rows — what the scheduler turns into a
/// [`WeightsChanged::Delta`](super::read::WeightsChanged::Delta).
///
/// Crate-internal on purpose: the shape a CONSUMER sees is the notice's variant, and
/// this is the writer's intermediate. See [`weight_delta`] for the two normalizations
/// that make it describe the non-zero view rather than the raw table.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct WeightDelta {
    /// `(path, score)` for every folder whose row now scores above zero.
    pub(crate) upserted: Vec<(String, f64)>,
    /// The paths that left the non-zero set.
    pub(crate) removed: Vec<String>,
}

/// One folder's weight to persist. The scheduler builds these from the scorer's
/// output; the serialized signal vector rides along.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightRow {
    /// The folder this weight is for, absolute.
    pub path: String,
    /// Its importance score.
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
    /// transaction it READS each subtree in `rescored_subtrees` (a changed path and
    /// everything under it) and writes back only what moved: `rows` whose signals
    /// differ from what's stored (or that aren't stored yet), and a delete for every
    /// stored row `rows` no longer covers. That purges folders renamed away, deleted,
    /// or newly floored — the incremental analog of a full pass's replace-the-table —
    /// while a folder nothing touched costs no write at all. Used by the
    /// changed-subtree recompute.
    ///
    /// Replies with the [`IncrementalWrite`] the transaction produced, so the
    /// scheduler can log what really changed and tell a weight-map consumer exactly
    /// what moved instead of making it re-read the table.
    WriteWeightsIncremental {
        generation: u64,
        rows: Vec<WeightRow>,
        rescored_subtrees: Vec<String>,
        reply: mpsc::Sender<IncrementalWrite>,
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

    /// Reconcile each subtree in `rescored_subtrees` against `rows` at `generation`
    /// (without advancing the stored generation) in one transaction: write the rows
    /// whose signals moved, delete the stored rows `rows` no longer covers (folders
    /// renamed away, deleted, or now floored), and leave every row whose signals are
    /// unchanged completely alone. The store ends up holding exactly the
    /// currently-scored folders either way. Untouched folders (outside every rescored
    /// subtree) keep their rows and as-of markers. The caller reads the current
    /// generation (via [`next_generation`] minus one, or the read API) and passes it
    /// here.
    ///
    /// **Blocks until the transaction commits** and hands back what it changed: the
    /// rows actually written plus the [`WeightDelta`] a weight-map consumer applies
    /// (`None` past [`MAX_DELTA_ROWS`], where the consumer is better off reloading).
    /// Because it waits for the reply, it doubles as the barrier a caller would
    /// otherwise get from [`flush_blocking`](ImportanceWriter::flush_blocking).
    ///
    /// Crate-internal because it hands back a crate-internal [`IncrementalWrite`]:
    /// what leaves the subsystem is the notice variant the scheduler builds from it,
    /// not this. An app-side test stages weights through
    /// [`write_weights`](ImportanceWriter::write_weights).
    ///
    /// [`next_generation`]: ImportanceWriter::next_generation
    pub(crate) fn write_weights_incremental(
        &self,
        generation: u64,
        rows: Vec<WeightRow>,
        rescored_subtrees: Vec<String>,
    ) -> Result<IncrementalWrite, ImportanceStoreError> {
        let (tx, rx) = mpsc::channel();
        self.send(WriteMessage::WriteWeightsIncremental {
            generation,
            rows,
            rescored_subtrees,
            reply: tx,
        })?;
        rx.recv().map_err(|_| {
            ImportanceStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "importance writer thread is gone",
            ))
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
                rescored_subtrees,
                reply,
            } => {
                // A failed transaction changed nothing we can describe, so the
                // consumer is told to reload rather than handed a partial delta.
                let write = apply_incremental(&mut conn, generation, &rows, &rescored_subtrees).unwrap_or_else(|e| {
                    log::warn!(target: "importance", "write_weights_incremental failed (generation {generation}): {e}");
                    IncrementalWrite::default()
                });
                let _ = reply.send(write);
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
/// table with `rows` (stamped at `generation`), bump the stored generation, and
/// stamp the scoring policy the rows were computed under.
///
/// A full pass rewrites every folder, so it clears the table first — otherwise a
/// folder that was scored last pass but now floors (or vanished from the index)
/// would leave a stale row behind, and the compacted store must never carry a row
/// for a floored folder. Clearing + inserting + bumping in ONE transaction keeps
/// the generation and the rows consistent — a reader never sees a bumped generation
/// with un-written (or stale) rows.
///
/// ❌ Stamp [`SCORING_POLICY_KEY`] HERE and nowhere else. A full pass is the one
/// moment the table provably holds nothing but rows this build's classifiers
/// produced; an incremental only touches the folders the filesystem changed, so it
/// can't vouch for the rest and stamping there would strand every untouched row
/// under a policy it was never scored by.
fn apply_full_pass(conn: &mut Connection, generation: u64, rows: &[WeightRow]) -> Result<(), ImportanceStoreError> {
    let tx = conn.transaction()?;
    {
        tx.execute("DELETE FROM weights", [])?;
        insert_rows(&tx, generation, rows)?;
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![RECOMPUTE_GENERATION_KEY, generation.to_string()],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            rusqlite::params![SCORING_POLICY_KEY, super::classify::scoring_policy_fingerprint()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Apply an INCREMENTAL rescore under one transaction: READ each subtree in
/// `rescored_subtrees` (a changed path and every descendant), then write back only
/// what actually moved — the rows whose signals changed or that the store doesn't
/// hold yet, plus a DELETE for every stored row the pass no longer scores. Rows go in
/// at the CURRENT `generation` (no bump, so untouched folders keep their as-of
/// marker), all in one transaction so a reader never sees a half-applied transition.
///
/// **The read is the same index-served BINARY range the write side keys on**
/// (`path_folded`): an equality on the changed folder's own folded key, plus the
/// half-open range `[folded(prefix) + "/", folded(prefix) + "0")` covering exactly
/// its descendants. Every descendant's folded key starts with `folded(prefix) + "/"`
/// (folding is byte-for-byte stable across the `/` boundary — `/` is ASCII, so NFD
/// and case-folding never cross it), and `"0"` (0x30) is one past `"/"` (0x2f), so
/// the range holds all descendants and nothing else. The `/` boundary means reading
/// `/a` never reaches a sibling like `/ab`. See [`SUBTREE_READ_SQL`].
///
/// **Clear and insert still agree, because one pass over that range decides both.**
/// Every stored row in a rescored subtree gets exactly one [`StoredRowFate`], so a
/// row can't be deleted by the clear and then skipped by the insert — the case that
/// would silently lose weights until the next full pass. ❌ Don't split this into an
/// independent "what to delete" and "what to write" query.
///
/// Returns the rows written and the [`WeightDelta`] describing what moved, or `None`
/// for the delta when the pass grew past [`MAX_DELTA_ROWS`] and a consumer should
/// reload instead. The subtree READ is the ONLY place the removed rows are knowable:
/// a rescored SUBTREE ROOT can't be expanded into keys consumer-side, because the
/// search ranker's map is keyed by a path HASH and hashes carry no prefix structure.
fn apply_incremental(
    conn: &mut Connection,
    generation: u64,
    rows: &[WeightRow],
    rescored_subtrees: &[String],
) -> Result<IncrementalWrite, ImportanceStoreError> {
    // The fresh rows keyed the way the store keys them, so each stored row can be
    // matched against the row that would replace it. `usize` rather than a reference
    // so the `unchanged` marks below stay index-addressable.
    let mut fresh: HashMap<String, usize> = HashMap::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        fresh.insert(normalize_for_comparison(&row.path), i);
    }
    // Which fresh rows the store already holds verbatim, so the insert can skip them.
    let mut unchanged = vec![false; rows.len()];
    // Which fresh rows the subtree read already reached, so the ancestor probe below
    // doesn't ask the store a second time about a row it just decided.
    let mut covered = vec![false; rows.len()];
    // `(folded key, verbatim path)` per row to drop: the key deletes it, the path
    // describes it to a weight-map consumer.
    let mut removed: Vec<(String, String)> = Vec::new();

    let tx = conn.transaction()?;
    let written: Vec<&WeightRow> = {
        if !rescored_subtrees.is_empty() {
            let mut read = tx.prepare_cached(SUBTREE_READ_SQL)?;
            for prefix in rescored_subtrees {
                let f = normalize_for_comparison(prefix);
                // `?1` matches the changed folder itself (its folded PK); `?2`..`?3`
                // is the half-open BINARY range covering exactly its descendants.
                let mut stored = read.query(rusqlite::params![f, format!("{f}/"), format!("{f}0")])?;
                while let Some(row) = stored.next()? {
                    // Borrowed, not `get::<String>`: the common row is unchanged and
                    // must cost no allocation at all. Only a Remove owns its strings.
                    let folded = row.get_ref(0)?.as_str().map_err(rusqlite::Error::from)?;
                    let index = fresh.get(folded).copied();
                    let stored_signals = row.get_ref(2)?.as_str().map_err(rusqlite::Error::from)?;
                    let fate = fate_of_stored_row(index.map(|i| rows[i].signals_json.as_str()), stored_signals);
                    if let Some(i) = index {
                        covered[i] = true;
                    }
                    match fate {
                        StoredRowFate::Keep => {
                            unchanged[index.expect("Keep is only reached with a fresh row")] = true;
                        }
                        StoredRowFate::Rewrite => {}
                        StoredRowFate::Remove => removed.push((folded.to_string(), row.get(1)?)),
                    }
                }
            }
        }

        // The rows OUTSIDE every rescored subtree: on the full-walk path each origin's
        // capped ancestor chain is rescored too, and the range read above can't reach
        // it. Bounded by `ANCESTOR_WALK_CAP` × the origin count, so a handful of PK
        // probes — and without them an idle `$HOME`-origin pass still rewrote `/Users`
        // and `/` every 60 s. ❌ A stored row here is never REMOVED: only a rescored
        // subtree gets cleared, which is what keeps clear and insert on one slice.
        {
            let mut probe = tx.prepare_cached(ROW_SIGNALS_SQL)?;
            for (folded, &i) in &fresh {
                if covered[i] {
                    continue;
                }
                let stored: Option<String> = probe
                    .query_row(rusqlite::params![folded], |row| row.get(0))
                    .optional()?;
                // No stored row means the folder is new here, so it has to be written.
                unchanged[i] =
                    stored.is_some_and(|s| fate_of_stored_row(Some(&rows[i].signals_json), &s) == StoredRowFate::Keep);
            }
        }
        // Collected before either write, so the delete and the insert are two halves
        // of ONE decision over the subtree rather than two independent passes.
        let written: Vec<&WeightRow> = rows
            .iter()
            .zip(&unchanged)
            .filter(|(_, skip)| !**skip)
            .map(|(row, _)| row)
            .collect();
        // After the read cursors close: SQLite gives no ordering guarantee for a
        // table modified while a SELECT over it is still stepping.
        if !removed.is_empty() {
            let mut del = tx.prepare_cached(ROW_DELETE_SQL)?;
            for (folded, _) in &removed {
                del.execute(rusqlite::params![folded])?;
            }
        }
        insert_rows(&tx, generation, written.iter().copied())?;
        written
    };
    let count = written.len();
    // Past the cap, stop describing and let the consumer re-read: the paths would
    // cost more to ship than the table costs to stream. Both sides are known before
    // the notice is built, so the decision is one comparison rather than a running
    // count.
    let describing = count <= MAX_DELTA_ROWS && removed.len() <= MAX_DELTA_ROWS;
    let delta = describing.then(|| weight_delta(&written, removed.into_iter().map(|(_, path)| path).collect()));
    tx.commit()?;
    Ok(IncrementalWrite { count, delta })
}

/// What one incremental transaction did: how many rows it wrote, and the
/// [`WeightDelta`] a cached-weight consumer applies (`None` past [`MAX_DELTA_ROWS`],
/// where reloading beats patching).
#[derive(Debug, Default)]
pub(crate) struct IncrementalWrite {
    /// Rows actually written. A pass over a subtree nothing touched writes zero.
    pub(crate) count: usize,
    /// The row-level edits, or `None` when the consumer should reload instead.
    pub(crate) delta: Option<WeightDelta>,
}

/// What becomes of one row the store already holds inside a rescored subtree.
///
/// Typed rather than a pair of bools so the three cases are exhaustive at the match
/// site: every stored row takes exactly one of them, which is what keeps the delete
/// and the insert from disagreeing about the subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredRowFate {
    /// The pass rescored this folder to exactly the signals already stored: leave the
    /// row completely alone.
    Keep,
    /// The pass rescored it differently: the insert overwrites it.
    Rewrite,
    /// The pass no longer scores this folder at all — deleted, renamed away, or newly
    /// floored — so the row has to go.
    Remove,
}

/// Decide a stored row's fate from the signals the pass just computed for it
/// (`None` when the pass produced no row for that path at all).
///
/// **The equality key is the SIGNALS blob, never the score.** A score is a function
/// of the signals AND `now_secs`, and `scorer::recency` decays continuously, so every
/// score moves a little every pass even when nothing about the folder changed:
/// measured on the real 160,719-row root store, a pass 60 s later left **99.88% of
/// rows with a byte-identical signals blob but only 0.03% with an identical score**
/// (`docs/notes/importance-treadmill-2026-08-04.md`). ❌ Don't "simplify" this to a
/// score comparison — it would skip 17 rows in 51,081 and the treadmill comes back.
/// [`FolderSignals`](super::FolderSignals) carries no clock (raw `mtime_secs`,
/// counts, and flags), which is exactly what makes it a sound identity here.
///
/// The cost of keeping a row is that its score stays at the `now_secs` it was last
/// written at. That's the same bounded staleness `RescoreScope::ChangedSubtreesOnly`
/// already accepts for an origin's ancestors, and it makes the store MORE uniform:
/// every folder now ages between full passes instead of only the churny ones being
/// re-decayed. See `../scheduler/DETAILS.md` § "Only what moved is written".
fn fate_of_stored_row(fresh_signals: Option<&str>, stored_signals: &str) -> StoredRowFate {
    match fresh_signals {
        Some(fresh) if fresh == stored_signals => StoredRowFate::Keep,
        Some(_) => StoredRowFate::Rewrite,
        None => StoredRowFate::Remove,
    }
}

/// Fold a committed incremental's inserted `rows` and CLEARED paths into the delta a
/// weight-map consumer applies.
///
/// Two normalizations make the result a description of the NON-ZERO weight set (what
/// `ImportanceIndex::for_each_nonzero_weight` streams) rather than of the raw table:
///
/// - A row scoring `0.0` is a REMOVAL. The store keeps such a row (it isn't floored),
///   but the stream skips it and an absent key already reads `0.0`, so treating it as
///   a removal is what keeps a patched map identical to a rebuilt one.
/// - A path that was cleared and then re-inserted nets down to its upsert. The common
///   incremental clears a subtree and rewrites the same folders, so without this the
///   delta would carry nearly every touched folder twice.
fn weight_delta(rows: &[&WeightRow], cleared: Vec<String>) -> WeightDelta {
    let mut upserted: Vec<(String, f64)> = Vec::new();
    let mut zeroed: Vec<String> = Vec::new();
    for row in rows {
        if row.score > 0.0 {
            upserted.push((row.path.clone(), row.score));
        } else {
            zeroed.push(row.path.clone());
        }
    }

    let rewritten: std::collections::HashSet<&str> = upserted.iter().map(|(path, _)| path.as_str()).collect();
    let rezeroed: std::collections::HashSet<&str> = zeroed.iter().map(String::as_str).collect();
    let mut removed: Vec<String> = cleared
        .into_iter()
        .filter(|path| !rewritten.contains(path.as_str()) && !rezeroed.contains(path.as_str()))
        .collect();
    removed.extend(zeroed);

    WeightDelta { upserted, removed }
}

/// The most rows an incremental will describe before telling the consumer to reload
/// instead. Past it, cloning and shipping every path approaches the cost of streaming
/// the table back (the thing the delta exists to avoid), and the notice channel would
/// hold that much per buffered pass. A typical incremental is a handful of rows; only
/// the full-walk fallback over a batch covering most of a volume gets near this.
const MAX_DELTA_ROWS: usize = 10_000;

/// The subtree READ an incremental rescore runs per rescored prefix, to decide each
/// stored row's [`StoredRowFate`].
///
/// Served by the BINARY `path_folded` primary key: an equality on the prefix's own
/// folded key plus a half-open range over its descendants (`folded(prefix) + "/"` up
/// to, but not including, `folded(prefix) + "0"`). Because the PK is BINARY (no custom
/// collation), SQLite serves both with index SEARCHes instead of full-scanning every
/// row and re-running the NFD-folding `platform_case` comparison on each — the fix
/// that stops the incremental from pegging a CPU core. Kept as a `const` so the
/// `subtree_read_is_index_served` test EXPLAINs the exact production SQL.
///
/// It selects `path` alongside the key because the verbatim path of a removed row is
/// what a weight-map consumer needs: a rescored SUBTREE ROOT can't be expanded into
/// keys consumer-side (the search ranker's map is keyed by a path HASH, and hashes
/// carry no prefix structure).
pub(crate) const SUBTREE_READ_SQL: &str =
    "SELECT path_folded, path, signals FROM weights WHERE path_folded = ?1 OR (path_folded >= ?2 AND path_folded < ?3)";

/// Read one row's stored signals by its folded PK, for a rescored folder that sits
/// OUTSIDE every rescored subtree (a full-walk pass's touched ancestors).
pub(crate) const ROW_SIGNALS_SQL: &str = "SELECT signals FROM weights WHERE path_folded = ?1";

/// Drop one row by its folded PK — a folder the pass no longer scores.
///
/// Point deletes rather than one ranged DELETE per prefix, because the pass now knows
/// exactly which rows have to go: on an idle volume that is zero statements instead of
/// a subtree-wide rewrite. A whole subtree genuinely disappearing costs one PK delete
/// per row, which is the rare case.
pub(crate) const ROW_DELETE_SQL: &str = "DELETE FROM weights WHERE path_folded = ?1";

/// Upsert `rows` on the folded-path PK, stamping each at `generation`. Shared by the full
/// pass and the incremental rescore.
///
/// Takes an ITERATOR so the incremental can hand it a filtered borrow of its row set
/// (only what actually moved) without copying the rows it decided to skip.
fn insert_rows<'a>(
    tx: &rusqlite::Transaction<'_>,
    generation: u64,
    rows: impl IntoIterator<Item = &'a WeightRow>,
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
/// DB and stays there.
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
        // An empty WAL with nothing copied: the checkpoint had no work, so it says
        // nothing (same rule as `indexing/writer/maintenance.rs`).
        Ok((0, 0, 0)) => {}
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
mod tests;
