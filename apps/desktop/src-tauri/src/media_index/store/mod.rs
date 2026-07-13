//! Per-volume `media.db`: the disposable store for image-enrichment results.
//!
//! Ported from `importance/store/`, carrying the drive index's disposable-cache
//! discipline verbatim (plan Decision 3):
//!
//! - **`platform_case` collation on every connection** (reused from
//!   `indexing::store`; not persisted, so re-registered per connection).
//! - **Delete-and-recreate on a [`SCHEMA_VERSION`] mismatch** — no migrations,
//!   exactly like the index. Enrichment is regenerable derived data.
//! - **Path-keyed rows** (`media_status.path`), because the index has no stable
//!   cross-rebuild entry id (plan Decision 3). A rebuild re-joins by path.
//! - **One writer thread per DB** ([`MediaWriter`](super::writer)); reads go
//!   through short-lived read connections / the [`MediaIndex`](super::read) read API.
//!
//! ## Two deliberate divergences from `importance/store`
//!
//! - **No per-row scan-generation column.** Staleness is `(path, mtime, size)` from
//!   the index row plus the OS/Vision engine stamp (below), which makes a generation
//!   stamp redundant. The lifecycle-bus `generation` is a transient wake counter and
//!   is NEVER persisted (plan Decision 3 — re-read it before adding a column).
//! - **A real GC, not wholesale table replacement.** Media enrichment is expensive
//!   and incremental, so a pass does NOT rewrite the whole table; instead the
//!   scheduler GCs rows for deleted paths on a completed-scan edge (see
//!   [`super::scheduler`]).
//!
//! ## Tables
//!
//! - `media_status` — path identity + `(mtime, size)` staleness + a typed
//!   enrichment state + a lightweight OS/Vision engine-version stamp.
//! - `media_ocr` — a standalone FTS5 table over the recognized text, keyed by path
//!   (see [`CREATE_TABLES`] for why standalone rather than external-content).
//! - `meta` — `schema_version`.

mod connection;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub use connection::open_read_connection;
pub(crate) use connection::open_write_connection;

use super::predicate::MediaKind;

/// Bump to invalidate on-disk `media.db` files. A mismatch deletes the file and
/// recreates it fresh (disposable cache, no migrations). Start at 1; bump on any
/// change to the tables below OR to what rows/text the store persists.
const SCHEMA_VERSION: &str = "1";

/// The FTS5 `media_ocr` table is STANDALONE (its own copy of the text), not
/// external-content. `agent/store`'s `messages_fts` is external-content because it
/// points at `messages.id` (an integer rowid). Our `media_status` is path-keyed and
/// `WITHOUT ROWID`, so there is no integer rowid to hang an external-content index
/// off; a standalone table keyed by an UNINDEXED `path` column keeps enrichment and
/// GC a simple `WHERE path = ?` delete, with no trigger machinery to desync.
const CREATE_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS media_status (
        path            TEXT    PRIMARY KEY COLLATE platform_case,
        mtime           INTEGER,
        size            INTEGER,
        media_kind      TEXT    NOT NULL,
        state           TEXT    NOT NULL,
        engine_version  TEXT    NOT NULL DEFAULT ''
    ) WITHOUT ROWID;

    CREATE VIRTUAL TABLE IF NOT EXISTS media_ocr USING fts5(
        path UNINDEXED,
        text,
        tokenize = 'unicode61 remove_diacritics 2'
    );

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

/// The typed enrichment state of a `media_status` row. Persisted as a stable TEXT
/// token (`sqlite3`-inspectable) and parsed back to this enum — classification is
/// typed, never a substring branch (`no-string-matching`).
///
/// Note: state does NOT drive staleness (that's `(path, mtime, size)` + engine); it
/// records coverage (done vs failed) for the progress surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentState {
    /// OCR ran and produced a result (possibly empty text — an image with no text).
    Done,
    /// OCR was attempted and failed (a broken/undecodable file).
    Failed,
}

impl EnrichmentState {
    /// The stable token persisted in `media_status.state`.
    pub fn as_token(self) -> &'static str {
        match self {
            EnrichmentState::Done => "done",
            EnrichmentState::Failed => "failed",
        }
    }

    /// Parse a persisted token. An unknown token reads as [`EnrichmentState::Done`]
    /// (the cache is disposable; the distinction only feeds the coverage surface).
    pub fn from_token(token: &str) -> EnrichmentState {
        match token {
            "failed" => EnrichmentState::Failed,
            _ => EnrichmentState::Done,
        }
    }
}

/// One `media_status` row: path identity, the `(mtime, size)` staleness key, the
/// typed kind and state, and the OS/Vision engine-version stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaStatusRow {
    pub path: String,
    pub mtime: Option<u64>,
    pub size: Option<u64>,
    pub media_kind: MediaKind,
    pub state: EnrichmentState,
    pub engine_version: String,
}

/// Whether an image at `path` needs (re-)enrichment given its stored status row and
/// its CURRENT `(mtime, size)` and the backend's CURRENT engine version.
///
/// This is the path-keyed staleness predicate (plan Decision 3, an M1 TDD target).
/// Stale when there is no row, or when the `(mtime, size)` identity changed, or when
/// the OS/Vision engine stamp changed (an OS upgrade re-runs OCR even on an
/// unchanged file). The enrichment STATE is deliberately NOT part of the key: a
/// stored row at the same identity+engine is covered whether it succeeded or failed,
/// so a bad file isn't re-hammered every completed scan; a genuine change to the
/// file (mtime/size) re-tries it.
pub fn needs_enrichment(
    stored: Option<&MediaStatusRow>,
    mtime: Option<u64>,
    size: Option<u64>,
    engine_version: &str,
) -> bool {
    match stored {
        None => true,
        Some(row) => row.mtime != mtime || row.size != size || row.engine_version != engine_version,
    }
}

/// Resolve the `media.db` path for a volume, beside the index's `index-{id}.db` and
/// `importance-{id}.db` in the app data dir.
pub fn media_db_path(data_dir: &Path, volume_id: &str) -> PathBuf {
    data_dir.join(format!("media-{volume_id}.db"))
}

/// Errors from the media store. Mirrors the index/importance store shape (a schema
/// mismatch is a distinct, non-failure variant that triggers delete-and-recreate).
#[derive(Debug)]
pub enum MediaStoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    SchemaMismatch { found: String, expected: &'static str },
}

impl std::fmt::Display for MediaStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaStoreError::Sqlite(e) => write!(f, "media store sqlite error: {e}"),
            MediaStoreError::Io(e) => write!(f, "media store io error: {e}"),
            MediaStoreError::SchemaMismatch { found, expected } => {
                write!(f, "media store schema mismatch (found {found}, expected {expected})")
            }
        }
    }
}

impl std::error::Error for MediaStoreError {}

impl From<rusqlite::Error> for MediaStoreError {
    fn from(e: rusqlite::Error) -> Self {
        MediaStoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for MediaStoreError {
    fn from(e: std::io::Error) -> Self {
        MediaStoreError::Io(e)
    }
}

/// A handle to a volume's `media.db`, owning a read connection and the
/// schema-lifecycle. The writer thread opens its OWN write connection.
pub struct MediaStore {
    db_path: PathBuf,
    read_conn: Connection,
}

impl MediaStore {
    /// Open (or create) the media DB, applying the disposable-cache discipline:
    /// register `platform_case`, run pragmas, create tables (incl. FTS5), check the
    /// schema version, and on a mismatch or corruption delete and recreate fresh.
    pub fn open(db_path: &Path) -> Result<Self, MediaStoreError> {
        match Self::try_open(db_path) {
            Ok(store) => Ok(store),
            Err(MediaStoreError::SchemaMismatch { found, expected }) => {
                log::info!(
                    target: "media_index",
                    "Media DB schema changed (found {found}, expected {expected}), recreating media DB"
                );
                Self::delete_and_recreate(db_path)
            }
            Err(e) => {
                log::warn!(target: "media_index", "Media DB open failed ({e}), deleting and recreating");
                Self::delete_and_recreate(db_path)
            }
        }
    }

    fn try_open(db_path: &Path) -> Result<Self, MediaStoreError> {
        let conn = open_write_connection(db_path)?;
        let version = read_meta_value(&conn, "schema_version")?;
        match version {
            Some(v) if v == SCHEMA_VERSION => {}
            Some(v) => {
                return Err(MediaStoreError::SchemaMismatch {
                    found: v,
                    expected: SCHEMA_VERSION,
                });
            }
            None => stamp_schema_version(&conn)?,
        }
        Ok(Self {
            db_path: db_path.to_path_buf(),
            read_conn: conn,
        })
    }

    fn delete_and_recreate(db_path: &Path) -> Result<Self, MediaStoreError> {
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

    /// Borrow the read connection for direct queries (round-trip tests).
    pub fn read_conn(&self) -> &Connection {
        &self.read_conn
    }

    /// Read one folder's status row, or `None` if the image was never enriched.
    pub fn status_for(&self, path: &str) -> Result<Option<MediaStatusRow>, MediaStoreError> {
        read_status(&self.read_conn, path)
    }
}

// ── Shared query helpers ──────────────────────────────────────────────────

fn stamp_schema_version(conn: &Connection) -> Result<(), MediaStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![SCHEMA_VERSION],
    )?;
    Ok(())
}

fn read_meta_value(conn: &Connection, key: &str) -> Result<Option<String>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Read one `media_status` row.
pub(super) fn read_status(conn: &Connection, path: &str) -> Result<Option<MediaStatusRow>, MediaStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT path, mtime, size, media_kind, state, engine_version FROM media_status WHERE path = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![path], row_to_status)?;
    match rows.next() {
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Load every `media_status` row into a `path → row` map. The scheduler loads one
/// snapshot per pass to decide staleness and to derive the stored-path set for GC.
pub(crate) fn read_all_status(conn: &Connection) -> Result<Vec<MediaStatusRow>, MediaStoreError> {
    let mut stmt =
        conn.prepare_cached("SELECT path, mtime, size, media_kind, state, engine_version FROM media_status")?;
    let rows = stmt.query_map([], row_to_status)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_to_status(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaStatusRow> {
    Ok(MediaStatusRow {
        path: row.get(0)?,
        mtime: row.get::<_, Option<i64>>(1)?.map(|v| v as u64),
        size: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
        media_kind: MediaKind::from_token(&row.get::<_, String>(3)?),
        state: EnrichmentState::from_token(&row.get::<_, String>(4)?),
        engine_version: row.get(5)?,
    })
}

pub(super) const CREATE_TABLES: &str = CREATE_TABLES_SQL;
