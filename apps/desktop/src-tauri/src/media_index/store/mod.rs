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
//!   enrichment state + the combined analyze provenance stamp (`engine_version`
//!   column: OCR engine + tag taxonomy + feature-print revision; see
//!   [`VisionBackend::analysis_stamp`](super::backend::VisionBackend::analysis_stamp)).
//! - `media_ocr` — a standalone FTS5 table over searchable image text, keyed by path
//!   (see [`CREATE_TABLES`] for why standalone rather than external-content). Holds
//!   BOTH the recognized OCR text AND the scene/object tag labels (one folded row
//!   per source), so a keyword search matches tags alongside OCR.
//! - `media_tags` — the structured tags (`path, label, score`) for tag-score
//!   filtering; the folded FTS rows above are its keyword-search index.
//! - `media_embedding` — the image feature-print embedding (`path, dims, vector`
//!   BLOB) for image↔image similarity + dedup (plan Decision 2).
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
const SCHEMA_VERSION: &str = "2";

/// The FTS5 `media_ocr` table is STANDALONE (its own copy of the text), not
/// external-content. `agent/store`'s `messages_fts` is external-content because it
/// points at `messages.id` (an integer rowid). Our `media_status` is path-keyed and
/// `WITHOUT ROWID`, so there is no integer rowid to hang an external-content index
/// off; a standalone table keyed by an UNINDEXED `path` column keeps enrichment and
/// GC a simple `WHERE path = ?` delete, with no trigger machinery to desync.
///
/// `media_ocr` folds tags in by carrying a `source` (`'ocr'` / `'tag'`) column and up
/// to two rows per path: the recognized OCR text, and the space-joined tag labels. A
/// keyword search matches either, so tags are searchable alongside OCR; the
/// `WHERE path = ?` delete still clears every row for a path in one statement.
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
        source UNINDEXED,
        text,
        tokenize = 'unicode61 remove_diacritics 2'
    );

    CREATE TABLE IF NOT EXISTS media_tags (
        path   TEXT NOT NULL COLLATE platform_case,
        label  TEXT NOT NULL,
        score  REAL NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_media_tags_path  ON media_tags(path);
    CREATE INDEX IF NOT EXISTS idx_media_tags_label ON media_tags(label);

    CREATE TABLE IF NOT EXISTS media_embedding (
        path    TEXT PRIMARY KEY COLLATE platform_case,
        dims    INTEGER NOT NULL,
        vector  BLOB    NOT NULL
    ) WITHOUT ROWID;

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
/// typed kind and state, and the combined analyze provenance stamp (stored in the
/// `engine_version` column: OCR engine + tag taxonomy + feature-print revision).
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
/// This is the path-keyed staleness predicate (plan Decision 3, a TDD target).
/// Stale when there is no row, or when the `(mtime, size)` identity changed, or when
/// the analyze provenance stamp changed (an OS upgrade to the OCR engine, tag
/// taxonomy, or feature-print model re-runs analysis even on an unchanged file — one
/// decode produces all three, so re-running all of it is free). The enrichment STATE
/// is deliberately NOT part of the key: a
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

/// Every stored `media_status` path (the reclaim partition's stored-row set — plan M4).
/// A cheap `path`-only scan; the partition then classifies each in Rust.
pub(crate) fn read_status_paths(conn: &Connection) -> Result<Vec<String>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path FROM media_status")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Sum the on-disk content bytes of the rows for `paths` across `media_ocr` (the OCR +
/// folded-tag FTS text), `media_tags` (the structured tag labels), and `media_embedding`
/// (the feature-print BLOBs) — the honest "about" byte estimate a reclaim prune would
/// free (plan M4). Streams each table once and sums only the paths in the set, so it
/// needs no giant `IN (…)` for a doomed set of hundreds of thousands and no temp table
/// on the read connection. It's a content estimate (excludes FTS index + page overhead),
/// so a `VACUUM` reclaims at least this much on disk.
pub(crate) fn sum_bytes_for_paths(
    conn: &Connection,
    paths: &std::collections::HashSet<String>,
) -> Result<u64, MediaStoreError> {
    if paths.is_empty() {
        return Ok(0);
    }
    let mut total: u64 = 0;
    for sql in [
        "SELECT path, length(text) FROM media_ocr",
        "SELECT path, length(label) FROM media_tags",
        "SELECT path, length(vector) FROM media_embedding",
    ] {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (path, len) = row?;
            if paths.contains(&path) {
                total += len.max(0) as u64;
            }
        }
    }
    Ok(total)
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

// ── Embedding codec (feature-print BLOBs) ──────────────────────────────────

/// Serialize a feature-print embedding to a little-endian `f32` BLOB. The `dims`
/// column stores the element count so a decode can validate the byte length.
pub(crate) fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for f in vector {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

/// Decode a little-endian `f32` BLOB back to a vector. Returns `None` when the byte
/// length isn't a whole number of `f32`s (a corrupt row degrades to "no embedding"
/// rather than failing the whole read — the cache just skips it).
pub(crate) fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// One stored embedding: the image path and its feature-print vector. The vector
/// store loads a `Vec` of these per volume into its resident cache.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmbeddingRow {
    pub(crate) path: String,
    pub(crate) vector: Vec<f32>,
}

/// Load every stored embedding for a volume (the resident vector cache's load-once
/// source). A row whose BLOB can't be decoded is skipped, not fatal.
pub(crate) fn read_all_embeddings(conn: &Connection) -> Result<Vec<EmbeddingRow>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path, vector FROM media_embedding")?;
    let rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        Ok((path, blob))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (path, blob) = row?;
        if let Some(vector) = decode_embedding(&blob) {
            out.push(EmbeddingRow { path, vector });
        }
    }
    Ok(out)
}

/// Read one image's stored embedding, or `None` if it has none (the source vector for
/// a "find similar images" query).
pub(crate) fn read_embedding_for(conn: &Connection, path: &str) -> Result<Option<Vec<f32>>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT vector FROM media_embedding WHERE path = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![path], |row| row.get::<_, Vec<u8>>(0))?;
    match rows.next() {
        Some(Ok(blob)) => Ok(decode_embedding(&blob)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// The paths whose stored tags include `label` at or above `min_score`, each with the
/// matching tag's score (the tag-score filter). Ordered by score descending.
pub(crate) fn read_tag_matches(
    conn: &Connection,
    label: &str,
    min_score: f32,
) -> Result<Vec<(String, f32)>, MediaStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT path, score FROM media_tags WHERE label = ?1 AND score >= ?2 ORDER BY score DESC, path ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![label, min_score], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)? as f32))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) const CREATE_TABLES: &str = CREATE_TABLES_SQL;
