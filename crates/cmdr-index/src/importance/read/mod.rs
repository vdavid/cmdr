//! `ImportanceIndex`: the consumable read API over a volume's `importance.db`.
//!
//! The canonical way a consumer (the in-app agent, media-ML enrichment, future
//! cleanup/prefetch) reaches folder importance — mirroring how `search/` reaches
//! the drive index through `ReadPool`/`IndexStore`. No consumer
//! takes a raw `rusqlite` dep on `importance.db`; they call this.
//!
//! ## What it owns
//!
//! - A per-volume read pool over `importance.db` (a thread-local read connection,
//!   mirroring the index's `ReadPool`). Lookups bind the folded path
//!   (`normalize_for_comparison`) against the `path_folded` key, so a
//!   case/normalization variant of a path resolves to the same row, and reads never
//!   contend with the single writer thread (WAL).
//! - The read calls: [`ImportanceIndex::weight_for`], [`ImportanceIndex::top_n`],
//!   [`ImportanceIndex::above_threshold`], [`ImportanceIndex::explain`] — each result
//!   carrying the **as-of recompute generation** it was computed at, so a consumer
//!   can caveat staleness (what makes an offline-unmounted read possible).
//! - A **recompute subscription** ([`subscribe`]): a `broadcast` receiver that
//!   fires when a volume's weights finish a recompute, carrying WHAT changed
//!   ([`WeightsChanged`]), so a consumer reacts instead of polling (the
//!   subscribe-don't-poll house rule).
//!
//! ## Staleness
//!
//! `weight_for` returns a weight even when it's from an older pass than the
//! store's current generation; the caller compares [`ScoredWeight::as_of_generation`]
//! to [`ImportanceIndex::recompute_generation`] to decide whether to caveat. The
//! read API never hides a stale weight — staleness is first-class, never an error.
//!
//! ## `explain` recomputes, never re-derives
//!
//! [`explain`] reads the STORED [`FolderSignals`] and runs the pure scorer's
//! [`explain`] over them — the SAME formula the score
//! was written from. There is no second scoring path; a consumer's breakdown and
//! the stored scalar can't drift.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;

use tokio::sync::broadcast;

use super::scorer::{Explanation, FolderSignals, Score, SignalSet, Weights, explain};
use super::store::{ImportanceStoreError, importance_db_path, open_read_connection};
use crate::indexing::store::normalize_for_comparison;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::sqlite_util::{THREAD_CONN_SLOTS, ThreadConnCache};

/// A stored weight for one folder, as the read API hands it back: the scalar, the
/// deserialized raw signal vector it was computed from (a consumer applying its own
/// weighting profile re-scores these instead of the scalar), and the as-of recompute
/// generation.
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

/// A typed classification of what the store knows about a path (the documented
/// lookup surface, distinct from the bare-scalar [`ImportanceIndex::weight_for`]).
///
/// The store holds a row ONLY for scored (non-floored) folders. A path with no row
/// is one of two very different things, and this enum keeps them apart:
///
/// - [`Scored`](WeightLookup::Scored): the folder has a stored weight.
/// - [`Floored`](WeightLookup::Floored): the folder has no row *because it floors*
///   (denylisted / hidden / under a floored ancestor), derived live from the path
///   via the shared classifiers — a `node_modules`, a `.git` subtree, a cache. Its
///   effective weight is `0.0`, but it's floored-by-design, not simply unseen.
/// - [`Unscored`](WeightLookup::Unscored): the folder has no row and doesn't floor
///   — it's genuinely not in the store (never scored, an unrelated path, a purged
///   cache). Also effectively `0.0` to a scalar consumer.
///
/// A scalar consumer that only wants the number can use [`WeightLookup::score`] (or
/// [`ImportanceIndex::weight_for`], which flattens floored ⇒ absent ⇒ `0.0`); a
/// consumer that cares WHY a folder scores nothing reads the variant (and, for a
/// floored folder, the [`FloorReason`]).
#[derive(Debug, Clone, PartialEq)]
pub enum WeightLookup {
    /// The folder has a stored weight.
    Scored(ScoredWeight),
    /// The folder floors by its path (no row stored; derived live), carrying WHY
    /// it floors. Effective weight `0.0`.
    Floored(FloorReason),
    /// The folder isn't in the store and doesn't floor. Effective weight `0.0`.
    Unscored,
}

impl WeightLookup {
    /// The effective scalar score for this lookup: the stored score when scored,
    /// `0.0` for floored or unscored (both contribute nothing to ranking).
    pub fn score(&self) -> f64 {
        match self {
            WeightLookup::Scored(w) => w.score.value(),
            WeightLookup::Floored(_) | WeightLookup::Unscored => 0.0,
        }
    }
}

/// Why a rowless folder floors, derived live from its path (the store keeps no row
/// for a floored folder). The three FLOOR overrides, in the precedence the read
/// side reports: a folder that both denylists and hides reports the denylist. A
/// consumer explaining "why does this score nothing" reads this instead of
/// re-deriving from `classify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorReason {
    /// The folder's own name is denylisted (`node_modules`, `.git`, a cache dir).
    NameDenylisted,
    /// The folder is hidden (dot-prefixed) or system-owned.
    HiddenOrSystem,
    /// A denylisted / hidden / system ANCESTOR floors this whole subtree.
    UnderFlooredAncestor,
}

/// Derive why a path floors, or `None` when it doesn't. The single derivation the
/// read side uses for both [`ImportanceIndex::lookup`]'s [`WeightLookup::Floored`]
/// reason and the floored-signals reconstruction, so the two agree by construction.
fn floor_reason_for(path: &str, home: &str) -> Option<FloorReason> {
    use crate::importance::classify::{is_denylisted, is_hidden_or_system, leaf_name};
    if !crate::importance::classify::floors_by_path(path, home) {
        return None;
    }
    let name = leaf_name(path);
    if is_denylisted(&name) {
        Some(FloorReason::NameDenylisted)
    } else if is_hidden_or_system(path, &name, home) {
        Some(FloorReason::HiddenOrSystem)
    } else {
        Some(FloorReason::UnderFlooredAncestor)
    }
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
    /// The user's home dir, for deriving a floored-vs-unscored classification of a
    /// path with no stored row (the compaction's derive-on-read). Path-class and
    /// hidden/system priors are home-relative, so the derivation needs it. Defaults
    /// to `$HOME`; a caller measuring an arbitrary volume can override it.
    home: String,
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
            home: std::env::var("HOME").unwrap_or_default(),
        }
    }

    /// Override the home dir used to derive a floored-vs-unscored classification
    /// (see [`lookup`](ImportanceIndex::lookup)). Defaults to `$HOME`.
    pub fn with_home(mut self, home: impl Into<String>) -> Self {
        self.home = home.into();
        self
    }

    /// Override the weights used by [`explain`]. The dev tuning surface sets a
    /// candidate `Weights` to re-score the stored signals and eyeball the ranking.
    /// Reads are unaffected — only `explain` re-scores.
    pub fn with_weights(mut self, weights: Weights) -> Self {
        self.weights = weights;
        self
    }

    /// The current recompute generation for this volume (`0` if never scored). A
    /// consumer compares a weight's `as_of_generation` to this to gauge staleness.
    pub fn recompute_generation(&self) -> Result<u64, ImportanceStoreError> {
        self.with_conn(super::store::read_generation)
    }

    /// The weight for one folder, or `None` if it has no stored row. A floored
    /// folder has no row, so it reads `None` here too (its effective weight is
    /// `0.0`); a consumer that needs to tell *floored* from *unscored* uses
    /// [`lookup`](ImportanceIndex::lookup) instead. Keyed by the folded path, so a
    /// case/normalization variant resolves to the same row.
    pub fn weight_for(&self, path: &str) -> Result<Option<ScoredWeight>, ImportanceStoreError> {
        // A never-scored volume has no `importance.db` at all (offline/unmounted,
        // fresh install, purged cache). A read-only open of a missing file fails
        // `CannotOpen`, so short-circuit to "no row": an offline lookup then derives
        // floored-vs-unscored from the path instead of erroring (the offline read
        // the plan makes a feature). Mirrors the guard in `all_nonzero_weights`.
        if !self.db_path.exists() {
            return Ok(None);
        }
        self.with_conn(|conn| read_scored_weight(conn, path))
    }

    /// The number of scored (non-floored) folders stored for this volume — the
    /// `weights` table row count. `0` for a never-scored / missing DB. Cheap (a
    /// `COUNT(*)`, no per-row deserialization); the `cmdr://importance` overview.
    pub fn scored_folder_count(&self) -> Result<u64, ImportanceStoreError> {
        if !self.db_path.exists() {
            return Ok(0);
        }
        self.with_conn(read_folder_count)
    }

    /// Whether importance genuinely has data for this volume — the "has it scored?"
    /// check every consumer gates on before treating a missing weight as meaningful.
    ///
    /// Keys on live weight rows, NOT solely the `recompute_generation` stamp: a store
    /// maintained only by INCREMENTAL rescores carries hundreds of thousands of weight
    /// rows but no generation (the incremental path deliberately never bumps it), and a
    /// schema-recreated store starts at generation 0 until its first FULL pass stamps
    /// one. Gating on the generation alone reads such a volume as "never scored" forever
    /// and reports "0 covered" at every threshold, even though the weights are perfectly
    /// usable (`DETAILS.md` § Generation-stamp semantics). So: scored when a full pass
    /// stamped a generation OR any weight row exists. Reuses the cheap
    /// [`scored_folder_count`](ImportanceIndex::scored_folder_count) probe (a
    /// `COUNT(*)`, short-circuits to 0 for a missing DB) — don't add a second probe.
    pub fn is_scored(&self) -> bool {
        self.recompute_generation().unwrap_or(0) > 0 || self.scored_folder_count().unwrap_or(0) > 0
    }

    /// The typed [`WeightLookup`] for one folder — the documented lookup surface.
    ///
    /// Resolves a stored row to [`WeightLookup::Scored`]; for a path with NO row, it
    /// derives whether the folder floors (from the path, via the shared classifiers)
    /// and returns [`WeightLookup::Floored`] or [`WeightLookup::Unscored`]
    /// accordingly. This is how a consumer distinguishes "this is machine output we
    /// deliberately floor" from "we simply haven't scored this" — the store no longer
    /// persists a `0.0` row for the floored case (storage compaction), so the read
    /// side reconstructs it.
    pub fn lookup(&self, path: &str) -> Result<WeightLookup, ImportanceStoreError> {
        match self.weight_for(path)? {
            Some(w) => Ok(WeightLookup::Scored(w)),
            None => Ok(match floor_reason_for(path, &self.home) {
                Some(reason) => WeightLookup::Floored(reason),
                None => WeightLookup::Unscored,
            }),
        }
    }

    /// The `n` most important folders on the volume, highest score first (ties
    /// broken by path for determinism). Media-ML's "enrich important first". A
    /// missing DB (offline/never-scored volume) reads empty, not an error.
    pub fn top_n(&self, n: usize) -> Result<Vec<ScoredWeight>, ImportanceStoreError> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        self.with_conn(|conn| read_ordered(conn, Some(n), None))
    }

    /// Every folder scoring at or above `threshold`, highest first. The agent's
    /// summary gate. An inclusive bound: a folder exactly at `threshold` is in. A
    /// missing DB reads empty, not an error.
    pub fn above_threshold(&self, threshold: f64) -> Result<Vec<ScoredWeight>, ImportanceStoreError> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        self.with_conn(|conn| read_ordered(conn, None, Some(threshold)))
    }

    /// The top `n` folders scoring at or above `threshold`, highest first — the
    /// `top_n` cap and the `above_threshold` filter in one bounded query. The
    /// `cmdr://importance?threshold=` read fetches `cap + 1` this way to detect
    /// truncation without loading the whole tail (a low threshold can match every
    /// scored folder). A missing DB reads empty, not an error.
    pub fn top_above_threshold(&self, n: usize, threshold: f64) -> Result<Vec<ScoredWeight>, ImportanceStoreError> {
        if !self.db_path.exists() {
            return Ok(Vec::new());
        }
        self.with_conn(|conn| read_ordered(conn, Some(n), Some(threshold)))
    }

    /// Stream every scored folder's `(path, score)` with a NON-ZERO score to `visit`.
    /// The search ranker's entry point: it folds one snapshot per volume into its own
    /// compact representation and blends the weights into result ordering.
    ///
    /// Streams rather than returning a map because the caller's representation is far
    /// smaller than a `path → score` map would be (a measured 368,043 scored folders
    /// cost 58 MB as one), and materializing the wide form first would make the load
    /// spike to many times the resident cost. Each `path` borrows SQLite's row buffer,
    /// so a row costs no allocation at all.
    ///
    /// Zero-scored folders (floored: `node_modules`, caches, hidden/system, and their
    /// subtrees) are OMITTED — a `0.0` weight is the neutral default a consumer's
    /// lookup already returns, so visiting those rows would only add cost (on a
    /// 646k-folder home, the ~312k folders under `node_modules` alone all floor to
    /// `0.0`). This keeps the stream to the folders that carry a ranking signal.
    pub fn for_each_nonzero_weight(&self, visit: impl FnMut(&str, f64)) -> Result<(), ImportanceStoreError> {
        // A never-scored volume has no `importance.db` at all (fresh install,
        // offline volume, purged cache). That's the neutral "no weights" state, not
        // an error the ranker must decode — a read-only open of a missing file would
        // fail `CannotOpen`, so short-circuit to visiting nothing. A present-but-empty
        // DB still opens and streams zero rows through the normal path.
        if !self.db_path.exists() {
            return Ok(());
        }
        self.with_conn(|conn| stream_nonzero_weights(conn, visit))
    }

    /// The per-signal contribution breakdown for one folder, or `None` if the
    /// folder is genuinely unscored (no row and doesn't floor).
    ///
    /// For a SCORED folder, recomputes the breakdown from the STORED signals via the
    /// pure scorer — the SAME formula the score was written from, so the breakdown
    /// and the stored scalar can't drift. For a FLOORED folder
    /// (no row, floors by path), reports a floored `Explanation` (score `0.0`,
    /// `floored == true`) whose flag reflects WHY it floors, derived live from the
    /// path. The floored breakdown loses the stored "would-have-contributed" additive
    /// terms — the store no longer keeps them for floored folders — which is
    /// acceptable: tuning cares about the non-floored ranking, and the floored-with-
    /// reason answer is what a consumer needs.
    pub fn explain(&self, path: &str, now_secs: u64) -> Result<Option<Explanation>, ImportanceStoreError> {
        if let Some(w) = self.weight_for(path)? {
            return Ok(Some(explain(&w.signals, &self.available, &self.weights, now_secs)));
        }
        // No row: floored folders carry no stored signals, so derive a floored
        // explanation from the path (which flag fired), rather than reporting the
        // stored breakdown it no longer has.
        Ok(self
            .derived_floored_signals(path)
            .map(|signals| explain(&signals, &self.available, &self.weights, now_secs)))
    }

    /// The floored-flag `FolderSignals` for a path that floors purely by its path,
    /// or `None` when the path doesn't floor. Only the FLOOR flags are set (the
    /// listing-derived signals aren't stored for a floored folder, so they can't be
    /// reconstructed); that's enough for `explain` to report `floored == true` with
    /// the reason, which is all a floored folder's breakdown carries.
    fn derived_floored_signals(&self, path: &str) -> Option<FolderSignals> {
        let mut signals = FolderSignals::neutral();
        // One flag, the precedence winner — `explain`'s `floored` is the OR of the
        // three, so setting only the reported reason is enough to floor the score,
        // and it keeps this in lockstep with `lookup`'s reported reason.
        match floor_reason_for(path, &self.home)? {
            FloorReason::NameDenylisted => signals.name_denylisted = true,
            FloorReason::HiddenOrSystem => signals.hidden_or_system = true,
            FloorReason::UnderFlooredAncestor => signals.under_floored_ancestor = true,
        }
        Some(signals)
    }

    /// Run `f` with a thread-local read connection to this volume's
    /// `importance.db`, opening (and caching) it on first use. The connection is
    /// read-only, so it never contends with the writer thread (WAL); path lookups
    /// resolve through the folded `path_folded` key the store wrote.
    fn with_conn<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<T, ImportanceStoreError>,
    ) -> Result<T, ImportanceStoreError> {
        // Generation `0`: importance reads have no invalidation generation (a
        // recompute rewrites rows in place, it never swaps the DB file), so the
        // cache keys on the path alone.
        READ_CONNS.with(|cell| cell.borrow_mut().with(&self.db_path, 0, open_read_connection, f))?
    }
}

thread_local! {
    /// This thread's open read connections, keyed by db path. A bounded LRU
    /// rather than one slot: a thread that reads two volumes' weights (the
    /// ranker folding a snapshot per volume, an agent walking both panes) would
    /// otherwise reopen on every alternation and lose the connection's
    /// `prepare_cached` statements. See `cmdr_fs::sqlite_util::ThreadConnCache`.
    static READ_CONNS: std::cell::RefCell<ThreadConnCache> =
        const { std::cell::RefCell::new(ThreadConnCache::new(THREAD_CONN_SLOTS)) };
}

// ── Read queries ──────────────────────────────────────────────────────────

/// Read one folder's scored weight, deserializing its stored signal vector. Keyed by
/// the folded path, so a case/NFD variant resolves to the same row; the verbatim
/// `path` column is returned.
fn read_scored_weight(conn: &rusqlite::Connection, path: &str) -> Result<Option<ScoredWeight>, ImportanceStoreError> {
    let mut stmt =
        conn.prepare_cached("SELECT path, score, signals, as_of_generation FROM weights WHERE path_folded = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![normalize_for_comparison(path)], row_to_scored_weight)?;
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

/// Count the stored (non-floored) weight rows. One aggregate query, no per-row
/// deserialization — the overview surface's "how many folders scored" answer.
fn read_folder_count(conn: &rusqlite::Connection) -> Result<u64, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT COUNT(*) FROM weights")?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count as u64)
}

/// Read every non-zero-scored folder into a `path → score` map. One statement, no
/// per-row deserialization (the search ranker needs only the scalar, not the
/// signal vector), and the `score > 0.0` filter drops the floored folders so the
/// map holds only folders that carry a ranking signal.
fn stream_nonzero_weights(
    conn: &rusqlite::Connection,
    mut visit: impl FnMut(&str, f64),
) -> Result<(), ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path, score FROM weights WHERE score > 0.0")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        // `get_ref` borrows the row buffer, so a path costs no allocation here — the
        // consumer decides whether it needs an owned copy.
        let path = row.get_ref(0)?.as_str().map_err(rusqlite::Error::from)?;
        visit(path, row.get(1)?);
    }
    Ok(())
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

/// Enumerate the volume ids that have an `importance.db` on disk under `data_dir`,
/// root first then the rest sorted (a stable roster). The importance stores outlive
/// their volume's mount by design, so this is the offline-capable answer to "which
/// volumes have importance data" without a live scheduler, index registry, or mount
/// — the roster the `cmdr://importance` resource iterates. MTP is never
/// background-scored, so no `importance-mtp-*.db` exists to list.
pub fn scored_volume_ids(data_dir: &std::path::Path) -> Vec<String> {
    let mut ids: Vec<String> = match std::fs::read_dir(data_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|name| {
                // `importance-{volume_id}.db`. The `-wal` / `-shm` sidecars end
                // `.db-wal` / `.db-shm`, so the `.db` suffix check drops them.
                name.strip_prefix("importance-")
                    .and_then(|rest| rest.strip_suffix(".db"))
                    .map(str::to_string)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    ids.sort();
    ids.sort_by_key(|id| id != crate::ROOT_VOLUME_ID);
    ids
}

// ── Recompute subscription ──────────────────────────────────────────────────

/// What a completed importance pass did to a volume's weights — the recompute
/// subscription's payload.
///
/// **The reload contract:** a consumer caching weights must end up with the same map
/// a fresh [`ImportanceIndex::for_each_nonzero_weight`] would build. Two of the three
/// ways that happens say "rebuild"; only [`Delta`](WeightsChanged::Delta) is a patch.
/// The third is a LAGGED receiver: the channel is a `broadcast`, so a consumer that
/// falls behind is TOLD (`RecvError::Lagged`) instead of silently missing a delta, and
/// must rebuild. ❌ Never treat a lag as "nothing happened" — a missed delta drifts
/// the map from the store with nothing to detect it until the next full pass.
///
/// **A delta describes the same view [`ImportanceIndex::for_each_nonzero_weight`]
/// streams**, not the raw table: `upserted` carries only rows that now score ABOVE
/// zero, and a folder whose row was deleted OR rescored to `0.0` lands in `removed`.
/// A zero weight is the neutral default every consumer's lookup already returns, so
/// the two shapes are interchangeable — and keeping only one of them is what makes a
/// patched map comparable to a rebuilt one.
#[derive(Debug, Clone)]
pub enum WeightsChanged {
    /// Rebuild from scratch. Sent by a FULL pass (which replaces the whole table),
    /// and by an incremental whose delta grew past the point where shipping it beats
    /// re-reading the table.
    ReloadAll {
        /// The generation the pass stamped.
        generation: u64,
    },
    /// Patch the cached map with these edits. Sent by an incremental pass, which
    /// writes at the CURRENT generation without bumping it.
    ///
    /// The two lists are **disjoint by path**: an incremental clears each changed
    /// subtree and immediately rewrites most of it, and the pass nets that down to
    /// the upsert. A consumer still applies `removed` FIRST, so a path-hash collision
    /// (a consumer keying on a hash rather than the path) resolves in favor of the
    /// fresher fact. Both ride an `Arc`, so a second subscriber costs a refcount
    /// rather than a copy of every path.
    Delta {
        /// The generation the written rows carry.
        generation: u64,
        /// `(path, score)` for every folder whose row now scores above zero.
        upserted: std::sync::Arc<[(String, f64)]>,
        /// The paths that left the non-zero set: their row was deleted (the folder
        /// was renamed away, deleted, or became floored) or rescored to `0.0`.
        removed: std::sync::Arc<[String]>,
    },
}

/// How many notices a volume's channel buffers before a slow receiver starts
/// lagging. A lagged receiver recovers with a full reload, so this only has to
/// absorb a consumer that's briefly busy; it doesn't have to guarantee delivery.
const NOTICE_BUFFER: usize = 16;

/// The per-volume recompute-completed senders, keyed by volume id and living for
/// the process (so a subscription survives an unmount, like the indexing lifecycle
/// bus).
///
/// A `broadcast`, deliberately, NOT a `watch`: a `watch` is last-value-wins, which is
/// fine for an idempotent generation counter but silently drops a delta, and a dropped
/// delta leaves a consumer's map wrong with nothing to notice it. `broadcast` buffers
/// and reports the overflow.
static RECOMPUTE_BUS: LazyLock<Mutex<HashMap<String, broadcast::Sender<WeightsChanged>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_recompute_sender<T>(volume_id: &str, f: impl FnOnce(&broadcast::Sender<WeightsChanged>) -> T) -> T {
    let mut bus = RECOMPUTE_BUS.lock_ignore_poison();
    let sender = bus
        .entry(volume_id.to_string())
        .or_insert_with(|| broadcast::channel(NOTICE_BUFFER).0);
    f(sender)
}

/// Announce what a volume's finished recompute changed. Called by the scheduler
/// after a full or incremental pass commits. A send with no subscribers is a no-op,
/// not an error — nothing caches weights for that volume yet.
pub(super) fn notify_recompute_completed(volume_id: &str, change: WeightsChanged) {
    with_recompute_sender(volume_id, |sender| {
        let _ = sender.send(change);
    });
}

/// Test-only crate-visible shim for [`notify_recompute_completed`], so a consumer's
/// subscribe→apply wiring (the search importance weight subscriber) can be tested
/// without widening the production notifier past the scheduler.
#[cfg(any(test, feature = "testing"))]
pub fn notify_recompute_completed_for_test(volume_id: &str, change: WeightsChanged) {
    notify_recompute_completed(volume_id, change);
}

/// Subscribe to a volume's recompute-completed notifications. The receiver sees
/// every pass that completes AFTER it subscribes (edge-triggered — there's no
/// retained value to catch up on), and a consumer that falls behind gets
/// `RecvError::Lagged` rather than a hole. A consumer awaits `recv()` instead of
/// polling (subscribe-don't-poll).
pub fn subscribe(volume_id: &str) -> broadcast::Receiver<WeightsChanged> {
    with_recompute_sender(volume_id, |sender| sender.subscribe())
}

#[cfg(test)]
mod tests;
