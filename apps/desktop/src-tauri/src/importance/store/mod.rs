//! Per-volume `importance.db`: the disposable store for folder-importance weights
//! and the navigation-visit signal.
//!
//! One DB file per volume (`importance-{volume_id}.db`), a sibling of the drive
//! index's `index-{volume_id}.db`, carrying the index's disposable-cache
//! discipline verbatim (plan Decision 2):
//!
//! - **`platform_case` collation on every connection** (reused from
//!   `indexing::store` — it isn't persisted in the file, so every read/write
//!   connection must re-register it before any query touching `path`).
//! - **Delete-and-recreate on a schema mismatch** ([`SCHEMA_VERSION`]); no
//!   migrations, exactly like the index. Weights are regenerable derived data, so
//!   a wipe costs one recompute on the next scan completion.
//! - **One writer thread per DB** ([`ImportanceWriter`](super::writer)); reads go
//!   through short-lived read connections (M2) / a read pool (M3).
//!
//! ## What a weight row holds (plan Decision 2)
//!
//! Path-keyed, and beyond the scalar `score` each row persists the serialized
//! [`FolderSignals`] it was computed from, so a future consumer can re-weight the
//! same signals under its own profile without a rescan. Every row carries the
//! **as-of scan generation** it was computed at, so a consumer can tell how stale
//! a weight is (the offline-unmounted read the plan makes a feature). The
//! generation is a per-volume counter in `meta`, bumped once per recompute pass.
//!
//! ## The visit table (plan Decision 3)
//!
//! A compact per-volume `visits` table: `path → (count, last-visit seconds)`.
//! Counts and timestamps only — no content, local-only. It feeds the scorer's
//! visit-activity signal on the next recompute. Privacy posture is documented in
//! `docs/security.md`.

mod connection;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub use connection::open_read_connection;
pub(crate) use connection::open_write_connection;

/// Bump to invalidate on-disk `importance.db` files. A mismatch deletes the DB
/// file and recreates it fresh (the cache is disposable, no migrations — plan
/// Decision 2). Start at 1; bump on any schema change to the tables below OR a
/// change to what rows/JSON the store persists.
///
/// `2`: storage compaction — floored folders no longer get a row (they're derived
/// on read), and `FolderSignals` serializes only its non-default fields. An older
/// DB (full of floored rows and verbose JSON) recreates fresh on the next scan.
const SCHEMA_VERSION: &str = "2";

/// Meta key for the per-volume recompute generation: a monotonically increasing
/// counter bumped once per full-volume recompute pass. Every weight row is
/// stamped with the value current at the pass that wrote it (its as-of marker).
/// Absent ⇒ generation 0 (no pass has run).
pub const RECOMPUTE_GENERATION_KEY: &str = "recompute_generation";

const CREATE_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS weights (
        path             TEXT    PRIMARY KEY COLLATE platform_case,
        score            REAL    NOT NULL,
        signals          TEXT    NOT NULL,
        as_of_generation INTEGER NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS visits (
        path            TEXT    PRIMARY KEY COLLATE platform_case,
        visit_count     INTEGER NOT NULL DEFAULT 0,
        last_visit_secs INTEGER NOT NULL DEFAULT 0
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

/// Resolve the `importance.db` path for a volume, beside the drive index's DB in
/// the app data dir. Mirrors the index's `index-{volume_id}.db` naming so the two
/// disposable caches live together and relocate together (plan Decision 2:
/// location-independence).
pub fn importance_db_path(data_dir: &Path, volume_id: &str) -> PathBuf {
    data_dir.join(format!("importance-{volume_id}.db"))
}

/// A stored weight for one folder: the scalar, the raw signal vector it was
/// computed from, and the as-of scan generation. The read side (M3's
/// `ImportanceIndex`) hands these back; M2 uses them for round-trip tests and the
/// scheduler's idempotency checks.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredWeight {
    pub path: String,
    pub score: f64,
    /// The serialized [`super::FolderSignals`] JSON (plan Decision 2: a consumer
    /// can re-weight these under its own profile). Kept as the raw string at this
    /// layer; the read API deserializes it.
    pub signals_json: String,
    pub as_of_generation: u64,
}

/// Errors from the importance store. Mirrors `IndexStoreError`'s shape (a schema
/// mismatch is a distinct, non-failure variant that triggers delete-and-recreate).
#[derive(Debug)]
pub enum ImportanceStoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// The on-disk schema version differs from [`SCHEMA_VERSION`]. Not a failure:
    /// `open` deletes and recreates the file fresh.
    SchemaMismatch {
        found: String,
        expected: &'static str,
    },
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

/// A handle to a volume's `importance.db`, owning a read connection.
///
/// `open` applies the full disposable-cache discipline (schema check +
/// delete-and-recreate). The writer thread ([`ImportanceWriter`](super::writer))
/// opens its OWN write connection via [`open_write_connection`]; this handle is
/// the read side and the schema-lifecycle owner.
pub struct ImportanceStore {
    db_path: PathBuf,
    read_conn: Connection,
}

impl ImportanceStore {
    /// Open (or create) the importance DB at `db_path`, applying the disposable
    /// cache discipline: register `platform_case`, run pragmas, create tables,
    /// check the schema version, and on a mismatch or corruption delete and
    /// recreate the file fresh.
    pub fn open(db_path: &Path) -> Result<Self, ImportanceStoreError> {
        match Self::try_open(db_path) {
            Ok(store) => Ok(store),
            Err(ImportanceStoreError::SchemaMismatch { found, expected }) => {
                log::info!(
                    "Importance DB schema version changed (found {found}, expected {expected}), recreating importance DB"
                );
                Self::delete_and_recreate(db_path)
            }
            Err(e) => {
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
        if db_path.exists() {
            std::fs::remove_file(db_path)?;
        }
        for sidecar in [db_path.with_extension("db-wal"), db_path.with_extension("db-shm")] {
            if sidecar.exists() {
                let _ = std::fs::remove_file(&sidecar);
            }
        }
        let conn = open_write_connection(db_path)?;
        stamp_schema_version(&conn)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            read_conn: conn,
        })
    }

    /// The DB file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Borrow the read connection for direct queries (round-trip tests, the
    /// scheduler's idempotency reads). M3's read API adds a proper read pool.
    pub fn read_conn(&self) -> &Connection {
        &self.read_conn
    }

    /// The current recompute generation (the highest as-of marker any pass has
    /// stamped). `0` when no pass has run.
    pub fn recompute_generation(&self) -> Result<u64, ImportanceStoreError> {
        read_generation(&self.read_conn)
    }

    /// Read one folder's stored weight, or `None` if unscored. Path-keyed via the
    /// `platform_case` collation, so a case/normalization variant of a scored path
    /// resolves to the same row.
    pub fn weight_for(&self, path: &str) -> Result<Option<StoredWeight>, ImportanceStoreError> {
        read_weight(&self.read_conn, path)
    }

    /// Read one folder's visit record, or `None` if never visited.
    pub fn visit_for(&self, path: &str) -> Result<Option<(u64, u64)>, ImportanceStoreError> {
        read_visit(&self.read_conn, path)
    }
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

/// Read one weight row.
pub(super) fn read_weight(conn: &Connection, path: &str) -> Result<Option<StoredWeight>, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path, score, signals, as_of_generation FROM weights WHERE path = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![path], |row| {
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

/// Read one visit row as `(count, last_visit_secs)`.
pub(super) fn read_visit(conn: &Connection, path: &str) -> Result<Option<(u64, u64)>, ImportanceStoreError> {
    let mut stmt = conn.prepare_cached("SELECT visit_count, last_visit_secs FROM visits WHERE path = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![path], |row| {
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
