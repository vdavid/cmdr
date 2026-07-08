//! `ImportanceIndex`: the consumable read API over a volume's `importance.db`.
//!
//! The canonical way a consumer (the in-app agent, media-ML enrichment, future
//! cleanup/prefetch) reaches folder importance — mirroring how `search/` reaches
//! the drive index through `ReadPool`/`IndexStore` (plan Decision 6). No consumer
//! takes a raw `rusqlite` dep on `importance.db`; they call this.
//!
//! ## What it owns
//!
//! - A per-volume read pool over `importance.db` (a thread-local read connection
//!   with the `platform_case` collation registered, mirroring the index's
//!   `ReadPool`), so a case/normalization variant of a path resolves to the same
//!   row and reads never contend with the single writer thread (WAL).
//! - The read calls: [`weight_for`], [`top_n`], [`above_threshold`], [`explain`],
//!   [`signals_for`] — each result carrying the **as-of recompute generation** it
//!   was computed at, so a consumer can caveat staleness (the offline-unmounted
//!   read M4 makes a feature).
//! - A **recompute subscription** ([`subscribe`]): a `watch` receiver that fires
//!   when a volume's weights finish a recompute, so a consumer reacts instead of
//!   polling (the subscribe-don't-poll house rule).
//!
//! ## Staleness
//!
//! `weight_for` returns a weight even when it's from an older pass than the
//! store's current generation; the caller compares [`ScoredWeight::as_of_generation`]
//! to [`ImportanceIndex::recompute_generation`] to decide whether to caveat. The
//! read API never hides a stale weight — staleness is first-class, never an error
//! (plan cross-cutting, agent-spec D7).
//!
//! ## `explain` recomputes, never re-derives
//!
//! [`explain`] reads the STORED [`FolderSignals`] and runs the pure scorer's
//! [`explain`](crate::importance::explain) over them — the SAME formula the score
//! was written from. There is no second scoring path; a consumer's breakdown and
//! the stored scalar can't drift.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;

use tokio::sync::watch;

use super::scorer::{Explanation, FolderSignals, Score, SignalSet, Weights, explain};
use super::store::{ImportanceStoreError, importance_db_path, open_read_connection};
use crate::ignore_poison::IgnorePoison;

/// A stored weight for one folder, as the read API hands it back: the scalar, the
/// deserialized raw signal vector it was computed from (plan Decision 2: a
/// consumer can re-weight these under its own profile via [`signals_for`]), and
/// the as-of recompute generation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScoredWeight {
    /// The folder's absolute path (the index's real identity).
    pub path: String,
    /// The importance scalar, `0.0..=1.0`.
    pub score: Score,
    /// The raw signal vector the score was computed from.
    pub signals: FolderSignals,
    /// The recompute generation this weight was stamped at. Compare to the
    /// store's current generation for staleness.
    pub as_of_generation: u64,
}

/// A read handle over a volume's `importance.db`.
///
/// Constructed per volume from the app data dir. Cheap to hold; the actual read
/// connection lives in a thread-local, reopened lazily. A consumer keeps one per
/// volume it cares about (or resolves them from the data dir on demand).
pub struct ImportanceIndex {
    db_path: PathBuf,
    /// The availability mask for the volume kind, used by `explain` so the
    /// redistributed weights match what the recompute wrote. Local is
    /// `SignalSet::all()`; a network volume degrades (M4).
    available: SignalSet,
    weights: Weights,
}

impl ImportanceIndex {
    /// Open the read API for `volume_id` under `data_dir`. Does not touch the DB
    /// until the first read (the connection is lazy), so this is cheap and never
    /// fails on a missing file — a `weight_for` on an unscored volume returns
    /// `None`.
    pub fn open(data_dir: &std::path::Path, volume_id: &str, available: SignalSet) -> Self {
        Self::open_at(importance_db_path(data_dir, volume_id), available)
    }

    /// Open the read API directly at an `importance.db` path, for a caller that
    /// already has the path (the dev tuning surface points at an arbitrary DB).
    pub fn open_at(db_path: PathBuf, available: SignalSet) -> Self {
        Self {
            db_path,
            available,
            weights: Weights::default(),
        }
    }

    /// Override the weights used by [`explain`]. The dev tuning surface sets a
    /// candidate `Weights` to re-score the stored signals and eyeball the ranking
    /// (plan Decision 6, §18.3). Reads are unaffected — only `explain` re-scores.
    pub fn with_weights(mut self, weights: Weights) -> Self {
        self.weights = weights;
        self
    }

    /// The current recompute generation for this volume (`0` if never scored). A
    /// consumer compares a weight's `as_of_generation` to this to gauge staleness.
    pub fn recompute_generation(&self) -> Result<u64, ImportanceStoreError> {
        self.with_conn(super::store::read_generation)
    }

    /// The weight for one folder, or `None` if unscored. Path-keyed via
    /// `platform_case`, so a case/normalization variant resolves to the same row.
    pub fn weight_for(&self, path: &str) -> Result<Option<ScoredWeight>, ImportanceStoreError> {
        self.with_conn(|conn| read_scored_weight(conn, path))
    }

    /// The `n` most important folders on the volume, highest score first (ties
    /// broken by path for determinism). Media-ML's "enrich important first".
    pub fn top_n(&self, n: usize) -> Result<Vec<ScoredWeight>, ImportanceStoreError> {
        self.with_conn(|conn| read_ordered(conn, Some(n), None))
    }

    /// Every folder scoring at or above `threshold`, highest first. The agent's
    /// summary gate. An inclusive bound: a folder exactly at `threshold` is in.
    pub fn above_threshold(&self, threshold: f64) -> Result<Vec<ScoredWeight>, ImportanceStoreError> {
        self.with_conn(|conn| read_ordered(conn, None, Some(threshold)))
    }

    /// Every scored folder's `(path, score)` with a NON-ZERO score, as a bulk
    /// path→weight map. The search ranker's entry point: it loads one snapshot per
    /// volume and blends the weights into result ordering. Zero-scored folders
    /// (floored: `node_modules`, caches, hidden/system, and their subtrees) are
    /// OMITTED — a `0.0` weight is the neutral default a consumer's lookup already
    /// returns, so storing those rows would only bloat the map (on a 646k-folder
    /// home, the ~312k folders under `node_modules` alone all floor to `0.0`).
    /// This keeps the map to the folders that actually carry a ranking signal.
    pub fn all_nonzero_weights(&self) -> Result<HashMap<String, f64>, ImportanceStoreError> {
        // A never-scored volume has no `importance.db` at all (fresh install,
        // offline volume, purged cache). That's the neutral "no weights" state, not
        // an error the ranker must decode — a read-only open of a missing file would
        // fail `CannotOpen`, so short-circuit to an empty map. A present-but-empty DB
        // still opens and returns an empty map through the normal path.
        if !self.db_path.exists() {
            return Ok(HashMap::new());
        }
        self.with_conn(read_nonzero_weight_map)
    }

    /// The stored raw signal vector for one folder, or `None` if unscored. For a
    /// consumer applying its own weighting profile instead of the default scalar
    /// (plan Decision 2). The scalar stays the common currency.
    pub fn signals_for(&self, path: &str) -> Result<Option<FolderSignals>, ImportanceStoreError> {
        Ok(self.weight_for(path)?.map(|w| w.signals))
    }

    /// The per-signal contribution breakdown for one folder, or `None` if
    /// unscored. Recomputes the breakdown from the STORED signals via the pure
    /// scorer — the SAME formula the score was written from, so the breakdown and
    /// the stored scalar can't drift (plan Decision 6).
    pub fn explain(&self, path: &str, now_secs: u64) -> Result<Option<Explanation>, ImportanceStoreError> {
        Ok(self
            .weight_for(path)?
            .map(|w| explain(&w.signals, &self.available, &self.weights, now_secs)))
    }

    /// Run `f` with a thread-local read connection to this volume's
    /// `importance.db`, opening (and caching) it on first use. The connection is
    /// read-only with the `platform_case` collation registered, so it never
    /// contends with the writer thread (WAL) and resolves paths the way the store
    /// wrote them.
    fn with_conn<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<T, ImportanceStoreError>,
    ) -> Result<T, ImportanceStoreError> {
        READ_CONN.with(|cell| {
            let mut slot = cell.borrow_mut();
            let reuse = matches!(&*slot, Some((path, _)) if path == &self.db_path);
            if !reuse {
                let conn = open_read_connection(&self.db_path)?;
                *slot = Some((self.db_path.clone(), conn));
            }
            let (_, conn) = slot.as_ref().expect("just populated");
            f(conn)
        })
    }
}

thread_local! {
    /// A thread-local read connection, keyed by db path so a thread that reads
    /// several volumes reopens on a path change (one live read conn per thread).
    static READ_CONN: std::cell::RefCell<Option<(PathBuf, rusqlite::Connection)>> =
        const { std::cell::RefCell::new(None) };
}

// ── Read queries ──────────────────────────────────────────────────────────

/// Read one folder's scored weight, deserializing its stored signal vector.
fn read_scored_weight(conn: &rusqlite::Connection, path: &str) -> Result<Option<ScoredWeight>, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path, score, signals, as_of_generation FROM weights WHERE path = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![path], row_to_scored_weight)?;
    match rows.next() {
        Some(Ok(w)) => Ok(Some(w)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Read weights ordered by score descending (ties by path), optionally limited to
/// the top `n` and/or filtered to `>= threshold`. One query serves both `top_n`
/// (limit) and `above_threshold` (filter); the ORDER BY is stable so a threshold
/// query and a top-n query agree on ranking.
fn read_ordered(
    conn: &rusqlite::Connection,
    limit: Option<usize>,
    threshold: Option<f64>,
) -> Result<Vec<ScoredWeight>, ImportanceStoreError> {
    let mut sql = String::from("SELECT path, score, signals, as_of_generation FROM weights");
    if threshold.is_some() {
        sql.push_str(" WHERE score >= ?1");
    }
    sql.push_str(" ORDER BY score DESC, path ASC");
    if let Some(n) = limit {
        sql.push_str(&format!(" LIMIT {n}"));
    }

    let mut stmt = conn.prepare_cached(&sql)?;
    let out = match threshold {
        Some(t) => stmt
            .query_map(rusqlite::params![t], row_to_scored_weight)?
            .collect::<Result<Vec<_>, _>>()?,
        None => stmt
            .query_map([], row_to_scored_weight)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(out)
}

/// Read every non-zero-scored folder into a `path → score` map. One statement, no
/// per-row deserialization (the search ranker needs only the scalar, not the
/// signal vector), and the `score > 0.0` filter drops the floored folders so the
/// map holds only folders that carry a ranking signal.
fn read_nonzero_weight_map(conn: &rusqlite::Connection) -> Result<HashMap<String, f64>, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path, score FROM weights WHERE score > 0.0")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, score) = row?;
        map.insert(path, score);
    }
    Ok(map)
}

/// Map a `(path, score, signals, as_of_generation)` row to a [`ScoredWeight`],
/// deserializing the stored signal JSON. A malformed signal vector degrades to
/// `FolderSignals::neutral()` rather than failing the read (the scalar is still
/// good; a re-weight consumer just loses the raw vector for that one row).
fn row_to_scored_weight(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScoredWeight> {
    let path: String = row.get(0)?;
    let score: f64 = row.get(1)?;
    let signals_json: String = row.get(2)?;
    let as_of_generation = row.get::<_, i64>(3)? as u64;
    let signals = serde_json::from_str(&signals_json).unwrap_or_else(|_| FolderSignals::neutral());
    Ok(ScoredWeight {
        path,
        score: Score(score),
        signals,
        as_of_generation,
    })
}

// ── Recompute subscription ──────────────────────────────────────────────────

/// The per-volume recompute-completed `watch` senders, keyed by volume id and
/// living for the process (so a subscription survives an unmount, like the
/// indexing lifecycle bus). Retains the last generation completed.
static RECOMPUTE_BUS: LazyLock<Mutex<HashMap<String, watch::Sender<u64>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_recompute_sender<T>(volume_id: &str, f: impl FnOnce(&watch::Sender<u64>) -> T) -> T {
    let mut bus = RECOMPUTE_BUS.lock_ignore_poison();
    let sender = bus.entry(volume_id.to_string()).or_insert_with(|| watch::channel(0).0);
    f(sender)
}

/// Announce that a volume finished a recompute at `generation`. Called by the
/// scheduler after a full or incremental pass commits. Retains the value for a
/// late subscriber (`send_replace`), so a consumer that subscribes after a pass
/// still sees the current generation.
pub(super) fn notify_recompute_completed(volume_id: &str, generation: u64) {
    with_recompute_sender(volume_id, |sender| {
        sender.send_replace(generation);
    });
}

/// Test-only crate-visible shim for [`notify_recompute_completed`], so a consumer's
/// subscribe→reload wiring (the search importance weight subscriber) can be tested
/// without widening the production notifier past the scheduler.
#[cfg(test)]
pub(crate) fn notify_recompute_completed_for_test(volume_id: &str, generation: u64) {
    notify_recompute_completed(volume_id, generation);
}

/// Subscribe to a volume's recompute-completed notifications. The receiver
/// carries the last generation that finished (or `0` if none yet); each recompute
/// bumps it. A consumer awaits `changed()` instead of polling (plan Decision 6,
/// subscribe-don't-poll).
pub fn subscribe(volume_id: &str) -> watch::Receiver<u64> {
    with_recompute_sender(volume_id, |sender| sender.subscribe())
}

#[cfg(test)]
mod tests;
