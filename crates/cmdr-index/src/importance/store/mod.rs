//! Per-volume importance stores: the disposable weights and the durable visits.
//!
//! Two DB files per volume, in two folders (`crate::volume_files::StoreDirs`):
//!
//! - **`importance-{volume_id}.db`**: the folder weights, a sibling of the drive
//!   index's `index-{volume_id}.db` in the drive-index dir (out of backups),
//!   carrying the index's disposable-cache discipline verbatim (below).
//! - **`visits-{volume_id}.db`**: the navigation-visit signal, in the data dir
//!   (backed up), because no scan can bring it back. ❌ Never deleted to recover
//!   from a schema change ([`visits`]).
//!
//! An older build kept both in one `importance-{id}.db` in the data dir;
//! `legacy_split.rs` splits that in the background after the build, and whatever
//! opens these files first waits for it ([`wait_for_background_split`]).
//!
//! The weights store's discipline:
//!
//! - **Folded-key rows.** The primary key is a precomputed `path_folded` column
//!   (`normalize_for_comparison(path)` — the same fold the `platform_case`
//!   collation applies) with a plain BINARY collation, so equality and range
//!   lookups are index-served; the verbatim `path` rides along for return values.
//!   The `platform_case` collation is still registered on every connection for
//!   parity with the index store, but no importance query relies on it. Why the
//!   key is precomputed rather than a collated `path`: DETAILS "The folded-key
//!   primary key".
//! - **Delete-and-recreate on a schema mismatch** ([`SCHEMA_VERSION`]); no
//!   migrations, exactly like the index. Weights are regenerable derived data, so
//!   a wipe costs one recompute on the next scan completion.
//! - **One writer thread per DB** ([`ImportanceWriter`](super::writer)); reads go
//!   through short-lived read connections.
//!
//! ## What a weight row holds
//!
//! Keyed by the folded path, and beyond the scalar `score` each row persists the
//! verbatim `path` plus the serialized
//! [`FolderSignals`](crate::importance::FolderSignals) it was computed from, so a future consumer can re-weight the
//! same signals under its own profile without a rescan. Every row carries the
//! **as-of scan generation** it was computed at, so a consumer can tell how stale
//! a weight is (what makes an offline-unmounted read possible). The
//! generation is a per-volume counter in `meta`, bumped once per recompute pass.
//!
//! ## The visit table
//!
//! A compact per-volume `visits` table: `path → (count, last-visit seconds)`, in
//! its own file. Counts and timestamps only — no content, local-only. It feeds the
//! scorer's visit-activity signal on the next recompute. The writer's connection
//! ATTACHes the file, so `visits` resolves there with no schema prefix. Privacy
//! posture is documented in `docs/security.md`.

mod connection;
mod legacy_split;
pub(crate) mod visits;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::indexing::store::normalize_for_comparison;
pub(crate) use connection::open_read_connection;
pub(crate) use connection::open_write_connection;
pub(crate) use legacy_split::{split_in_background, wait_for_background_split};

/// Bump to invalidate on-disk `importance.db` files. A mismatch deletes the DB
/// file and recreates it fresh (the cache is disposable, no migrations).
/// Start at 1; bump on any schema change to the tables below OR a
/// change to what rows/JSON the store persists.
///
/// `2`: storage compaction — floored folders no longer get a row (they're derived
/// on read), and `FolderSignals` serializes only its non-default fields. An older
/// DB (full of floored rows and verbose JSON) recreates fresh on the next scan.
///
/// `3`: folded-key primary key — the row identity is now the precomputed
/// [`normalize_for_comparison`]
/// fold of the path (`path_folded`, a BINARY PK), with the verbatim `path` kept as a
/// plain column for return values. The `platform_case`-collated PK made the
/// incremental subtree-clear DELETE full-scan the table (a custom collation defeats
/// the b-tree range/LIKE optimization); the BINARY folded key restores an
/// index-served range delete. An older DB recreates fresh.
///
/// `4`: weights only. The `visits` table moved to `visits-{id}.db` in the data
/// dir, and this file to the drive-index dir. ⚠️ A schema-3 file is the one
/// older store that must NOT be recreated before the split has moved its visits
/// out (`legacy_split.rs`), which [`ImportanceStore::open`] enforces
/// ([`ImportanceStoreError::HoldsVisits`]).
const SCHEMA_VERSION: &str = "4";

/// Meta key for the per-volume recompute generation: a monotonically increasing
/// counter bumped once per full-volume recompute pass. Every weight row is
/// stamped with the value current at the pass that wrote it (its as-of marker).
/// Absent ⇒ generation 0 (no pass has run).
pub(crate) const RECOMPUTE_GENERATION_KEY: &str = "recompute_generation";

/// Meta key for the scoring policy the stored weights were computed under, stamped
/// by each full pass alongside the generation bump
/// (`classify::scoring_policy_fingerprint`).
///
/// A stored value that differs from this build's fingerprint, or an absent one,
/// means every row predates the classification rules this build applies, so the
/// volume needs a full recompute rather than being trusted. Nothing else would
/// revisit them: a full pass runs once and an incremental only touches folders the
/// filesystem changed, so a classification fix stays inert over a store's existing
/// rows forever. (The local `root` volume held 188,760 of them on 2026-09-03.)
///
/// Why a stamp rather than a [`SCHEMA_VERSION`] bump, which would also force a
/// rescore: the stamp re-arms only the volumes whose rows it has to, on the
/// existing file, while a bump recreates every user's store at once.
pub(crate) const SCORING_POLICY_KEY: &str = "scoring_policy";

/// Meta key for the index scan the stored weights were computed against: the drive
/// index's `scan_completed_at` as the last full pass read it before walking, stamped
/// by that pass alongside the generation bump. Absent when the pass walked an index
/// with no completed scan.
///
/// It's what tells "scored" from "scored against the CURRENT scan". A scan's own pass
/// that never landed (the app quit, the memory watchdog stopped it) leaves the store
/// at a generation from an older scan, and a volume that loads `Fresh` at the next
/// launch never fires `ScanCompleted` again, so without this nothing but the hourly
/// tick would redo it.
pub(crate) const SCORED_SCAN_KEY: &str = "scored_scan_completed_at";

// `path_folded` is the BINARY primary key (the precomputed `normalize_for_comparison`
// fold of the path), so equality and range lookups are index-served without the
// custom `platform_case` collation defeating the b-tree optimization. `path` keeps the
// verbatim string for return values (a case/NFD variant folds to the same PK).
const CREATE_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS weights (
        path_folded      TEXT    PRIMARY KEY,
        path             TEXT    NOT NULL,
        score            REAL    NOT NULL,
        signals          TEXT    NOT NULL,
        as_of_generation INTEGER NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

/// Whether a volume's store needs a full recompute before its weights may be
/// trusted. Three independent reasons, any one enough:
///
/// - It carries no [`RECOMPUTE_GENERATION_KEY`] — a fresh install, a
///   schema-recreated store (the prod schema-3 upgrade path), or one maintained
///   only by incremental rescores (which never stamp a generation).
/// - Its [`SCORING_POLICY_KEY`] doesn't match this build's
///   `classify::scoring_policy_fingerprint`, so every row was computed under
///   classification rules we no longer apply.
/// - It trails `index_scan`, the drive index's `scan_completed_at` (see
///   [`ImportanceStore::trails_index_scan`]).
///
/// **The decision binds to the write-path open, deliberately.** The schema
/// delete-and-recreate happens lazily, ONLY inside [`ImportanceStore::open`] on a
/// WRITE-path open (`open_write_connection`); the read path never recreates. So on a
/// schema-upgrade launch a read-side generation probe still reads the OUTGOING
/// schema's stamped generation ("already scored"), skips the full pass, and THEN the
/// recreate fires on the first incremental write — generation gone, trigger already
/// passed, the volume stuck at "never scored" forever. Opening the store here forces
/// the recreate FIRST, so the generation we read reflects the CURRENT schema. This is
/// the exact prod-upgrade ordering; a sweep-time read probe is the trap this
/// avoids.
pub(crate) fn needs_full_pass(
    data_dir: &Path,
    volume_id: &str,
    index_scan: Option<&str>,
) -> Result<bool, ImportanceStoreError> {
    // In a one-folder host the weights path may still be the older combined store.
    wait_for_background_split();
    let store = ImportanceStore::open(&importance_db_path(data_dir, volume_id))?;
    Ok(
        store.recompute_generation()? == 0
            || store.predates_scoring_policy()?
            || store.trails_index_scan(index_scan)?,
    )
}

/// Resolve a volume's weights database for the host whose data dir is
/// `data_dir`: `importance-{volume_id}.db` beside the drive index's
/// `index-{volume_id}.db`, in the folder `StoreDirs` keeps the rebuildable caches
/// in (the data dir itself for a host that keeps everything together).
pub fn importance_db_path(data_dir: &Path, volume_id: &str) -> PathBuf {
    use crate::volume_files::{StoreDirs, VolumeStore};
    StoreDirs::for_data_dir(data_dir).db_path(VolumeStore::Importance, volume_id)
}

/// Resolve a volume's visits database for the host whose data dir is
/// `data_dir`: `visits-{volume_id}.db`, in the data dir.
pub(crate) fn visits_db_path(data_dir: &Path, volume_id: &str) -> PathBuf {
    use crate::volume_files::{StoreDirs, VolumeStore};
    StoreDirs::for_data_dir(data_dir).db_path(VolumeStore::Visits, volume_id)
}

/// A stored weight for one folder: the scalar, the raw signal vector it was
/// computed from, and the as-of scan generation. The read side's
/// `ImportanceIndex` hands these back; the store's own tests use them for
/// round-trip checks and the scheduler's idempotency checks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredWeight {
    pub path: String,
    pub score: f64,
    /// The serialized [`super::FolderSignals`] JSON (a consumer can re-weight
    /// these under its own profile). Kept as the raw string at this layer; the
    /// read API deserializes it.
    pub signals_json: String,
    pub as_of_generation: u64,
}

/// Errors from the importance store. Mirrors `IndexStoreError`'s shape (a schema
/// mismatch is a distinct, non-failure variant that triggers delete-and-recreate).
#[derive(Debug)]
pub enum ImportanceStoreError {
    /// SQLite refused the statement or the file.
    Sqlite(rusqlite::Error),
    /// The database file itself couldn't be read, written, or removed.
    Io(std::io::Error),
    /// The on-disk schema version differs from [`SCHEMA_VERSION`]. Not a failure:
    /// `open` deletes and recreates the file fresh.
    SchemaMismatch {
        /// The `schema_version` the file on disk carries.
        found: String,
        /// The one this build writes.
        expected: &'static str,
    },
    /// The file at the weights path still holds navigation visits nothing has
    /// moved out yet (an older build's combined store, before its split), so it's
    /// left alone rather than recreated. The next build-time split moves them.
    HoldsVisits,
}

impl std::fmt::Display for ImportanceStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportanceStoreError::Sqlite(e) => write!(f, "importance store sqlite error: {e}"),
            ImportanceStoreError::Io(e) => write!(f, "importance store io error: {e}"),
            ImportanceStoreError::SchemaMismatch { found, expected } => {
                write!(
                    f,
                    "importance store schema mismatch (found {found}, expected {expected})"
                )
            }
            ImportanceStoreError::HoldsVisits => {
                write!(f, "importance store still holds visits that haven't been split out")
            }
        }
    }
}

impl std::error::Error for ImportanceStoreError {}

impl From<rusqlite::Error> for ImportanceStoreError {
    fn from(e: rusqlite::Error) -> Self {
        ImportanceStoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for ImportanceStoreError {
    fn from(e: std::io::Error) -> Self {
        ImportanceStoreError::Io(e)
    }
}

impl ImportanceStoreError {
    /// Whether SQLite says the file isn't a sound database (not one at all, or a
    /// corrupt one): the only case where deleting a file can't lose more than is
    /// already lost. Typed on the result code, never the message.
    pub(crate) fn is_corruption(&self) -> bool {
        matches!(
            self,
            ImportanceStoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _))
                if matches!(e.code, rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt)
        )
    }
}

/// A handle to a volume's `importance.db`, owning a read connection.
///
/// `open` applies the full disposable-cache discipline (schema check +
/// delete-and-recreate). The writer thread ([`ImportanceWriter`](super::writer))
/// opens its OWN write connection via [`open_write_connection`]; this handle is
/// the read side and the schema-lifecycle owner.
pub(crate) struct ImportanceStore {
    /// Read only by the test-gated [`ImportanceStore::db_path`].
    #[cfg_attr(not(test), allow(dead_code, reason = "read only by the test-gated accessor"))]
    db_path: PathBuf,
    read_conn: Connection,
}

impl ImportanceStore {
    /// Open (or create) the importance DB at `db_path`, applying the disposable
    /// cache discipline: register `platform_case`, run pragmas, create tables,
    /// check the schema version, and on a mismatch or corruption delete and
    /// recreate the file fresh.
    ///
    /// ❗ Never deletes a file that still holds visits ([`holds_visits`]): that's
    /// an older build's combined store whose split hasn't run, and recreating it
    /// would throw away the user's navigation history.
    pub fn open(db_path: &Path) -> Result<Self, ImportanceStoreError> {
        match Self::try_open(db_path) {
            Ok(store) => Ok(store),
            Err(ImportanceStoreError::SchemaMismatch { found, expected }) => {
                refuse_if_holds_visits(db_path)?;
                log::info!(
                    "Importance DB schema version changed (found {found}, expected {expected}), recreating importance DB"
                );
                Self::delete_and_recreate(db_path)
            }
            Err(e) => {
                refuse_if_holds_visits(db_path)?;
                log::warn!("Importance DB open failed ({e}), deleting and recreating");
                Self::delete_and_recreate(db_path)
            }
        }
    }

    fn try_open(db_path: &Path) -> Result<Self, ImportanceStoreError> {
        let conn = open_write_connection(db_path)?;
        let version = read_meta_value(&conn, "schema_version")?;
        match version {
            Some(v) if v == SCHEMA_VERSION => {}
            Some(v) => {
                return Err(ImportanceStoreError::SchemaMismatch {
                    found: v,
                    expected: SCHEMA_VERSION,
                });
            }
            None => {
                stamp_schema_version(&conn)?;
            }
        }
        Ok(Self {
            db_path: db_path.to_path_buf(),
            read_conn: conn,
        })
    }

    fn delete_and_recreate(db_path: &Path) -> Result<Self, ImportanceStoreError> {
        // Through the one delete, so the read connections threads have cached to
        // the outgoing file are retired: they'd otherwise keep answering from the
        // unlinked inode, since importance reads carry no generation of their own.
        cmdr_fs::sqlite_util::delete_database(db_path)?;
        let conn = open_write_connection(db_path)?;
        stamp_schema_version(&conn)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            read_conn: conn,
        })
    }

    /// The DB file path.
    #[cfg(test)]
    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Borrow the read connection for direct queries (round-trip tests, the
    /// scheduler's idempotency reads). M3's read API adds a proper read pool.
    #[cfg(test)]
    pub(crate) fn read_conn(&self) -> &Connection {
        &self.read_conn
    }

    /// The current recompute generation (the highest as-of marker any pass has
    /// stamped). `0` when no pass has run.
    pub fn recompute_generation(&self) -> Result<u64, ImportanceStoreError> {
        read_generation(&self.read_conn)
    }

    /// Whether the stored weights were computed under a scoring policy this build
    /// no longer applies (see [`SCORING_POLICY_KEY`]). An absent stamp counts as
    /// stale: every store written before the stamp existed holds rows from an
    /// older policy, and a redundant recompute costs one pass while a skipped one
    /// leaves wrong scores in place indefinitely.
    pub fn predates_scoring_policy(&self) -> Result<bool, ImportanceStoreError> {
        let stored = read_meta_value(&self.read_conn, SCORING_POLICY_KEY)?;
        Ok(stored.as_deref() != Some(&super::classify::scoring_policy_fingerprint()))
    }

    /// Whether the stored weights were computed against an older scan than the one
    /// the drive index holds (`index_scan`, its `scan_completed_at`; see
    /// [`SCORED_SCAN_KEY`]).
    ///
    /// An index with no completed scan owes nothing here: it's mid-scan or was never
    /// scanned, and the scan's own `ScanCompleted` drives the pass, so answering yes
    /// would stack a pass on a cold-start scan. An absent stamp beside a completed
    /// scan counts as trailing, the way an absent policy stamp does: a store written
    /// before the stamp existed can't say which scan it saw, and a redundant pass costs
    /// one pass while a skipped one leaves stale scores until the hourly tick.
    pub fn trails_index_scan(&self, index_scan: Option<&str>) -> Result<bool, ImportanceStoreError> {
        let Some(index_scan) = index_scan else {
            return Ok(false);
        };
        Ok(read_meta_value(&self.read_conn, SCORED_SCAN_KEY)?.as_deref() != Some(index_scan))
    }

    /// Read one folder's stored weight, or `None` if unscored. Keyed by the folded
    /// path (`normalize_for_comparison`), so a case/normalization variant of a scored
    /// path resolves to the same row.
    #[cfg(test)]
    pub(crate) fn weight_for(&self, path: &str) -> Result<Option<StoredWeight>, ImportanceStoreError> {
        read_weight(&self.read_conn, path)
    }
}

/// Refuse to delete `db_path` while it still holds visits: see
/// [`ImportanceStore::open`].
fn refuse_if_holds_visits(db_path: &Path) -> Result<(), ImportanceStoreError> {
    if holds_visits(db_path) {
        log::error!(
            target: "importance",
            "{} still holds visits that haven't been split out; leaving it for the next launch's split",
            cmdr_fs::log_private::LogPrivate::file_path(db_path)
        );
        return Err(ImportanceStoreError::HoldsVisits);
    }
    Ok(())
}

/// Whether the file at `db_path` has a `visits` table with at least one row. A
/// file that can't be read answers no: nothing in it can be saved either.
pub(crate) fn holds_visits(db_path: &Path) -> bool {
    crate::volume_files::holds_visit_rows(db_path)
}

// ── Shared query helpers (used by the store handle and the writer thread) ─────

/// Stamp the schema version and ensure the tables exist. Called on a fresh DB.
fn stamp_schema_version(conn: &Connection) -> Result<(), ImportanceStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![SCHEMA_VERSION],
    )?;
    Ok(())
}

/// Read a single meta value.
pub(super) fn read_meta_value(conn: &Connection, key: &str) -> Result<Option<String>, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Read the recompute generation counter (absent ⇒ 0).
pub(super) fn read_generation(conn: &Connection) -> Result<u64, ImportanceStoreError> {
    Ok(read_meta_value(conn, RECOMPUTE_GENERATION_KEY)?
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0))
}

/// Read one weight row. Keyed by the folded path (`normalize_for_comparison`), so a
/// case/NFD variant of a stored path resolves to the same row; the verbatim `path`
/// column is returned.
pub(super) fn read_weight(conn: &Connection, path: &str) -> Result<Option<StoredWeight>, ImportanceStoreError> {
    let mut stmt =
        conn.prepare_cached("SELECT path, score, signals, as_of_generation FROM weights WHERE path_folded = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![normalize_for_comparison(path)], |row| {
        Ok(StoredWeight {
            path: row.get(0)?,
            score: row.get(1)?,
            signals_json: row.get(2)?,
            as_of_generation: row.get::<_, i64>(3)? as u64,
        })
    })?;
    match rows.next() {
        Some(Ok(w)) => Ok(Some(w)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Read one visit row as `(count, last_visit_secs)` from a connection that sees
/// the `visits` table (the visits file's own, or one that attached it).
#[cfg(test)]
pub(crate) fn read_visit(conn: &Connection, path: &str) -> Result<Option<(u64, u64)>, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT visit_count, last_visit_secs FROM visits WHERE path_folded = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![normalize_for_comparison(path)], |row| {
        Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
    })?;
    match rows.next() {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

pub(super) const CREATE_TABLES: &str = CREATE_TABLES_SQL;

#[cfg(test)]
mod tests;
