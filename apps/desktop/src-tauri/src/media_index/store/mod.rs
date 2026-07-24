//! Per-volume `media.db`: the disposable store for image-enrichment results.
//!
//! Ported from `importance/store/`, carrying the drive index's disposable-cache
//! discipline verbatim (plan Decision 3):
//!
//! - **`platform_case` collation on every connection** (reused from
//!   `indexing::store`; not persisted, so re-registered per connection).
//! - **Delete-and-recreate on a [`SCHEMA_VERSION`] mismatch** — no migrations,
//!   exactly like the index. Enrichment is regenerable derived data.
//! - **Integer-id-keyed rows** (plan M4): one [`media_file`](CREATE_TABLES) `(id, path)`
//!   identity table holds each path ONCE; every other table keys on the integer `file_id`.
//!   A path averaging ~80 B is stored once, not once per table, and a rename is a one-row
//!   `UPDATE media_file.path` instead of a rewrite across five tables. The Rust layer stays
//!   path-addressed: the reads join `media_file` back to a `path`, so callers never see an
//!   id (the index still has no stable cross-rebuild entry id — the `media_file` id is
//!   internal to `media.db` and dies with a rebuild).
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
//! - `media_file` — the identity table: `(id INTEGER PRIMARY KEY, path TEXT UNIQUE)`. Each
//!   path is stored ONCE here; every other table references the integer `file_id` (plan
//!   M4). A rename is a one-row `UPDATE media_file.path`.
//! - `media_status` — `(mtime, size)` staleness + a typed enrichment state + the combined
//!   analyze provenance stamp (`engine_version` column: OCR engine + tag taxonomy +
//!   feature-print revision; see
//!   [`VisionBackend::analysis_stamp`](super::backend::VisionBackend::analysis_stamp)),
//!   keyed by `file_id`.
//! - `media_ocr` — a standalone FTS5 table over searchable image text, keyed by `file_id`
//!   (see [`CREATE_TABLES`] for why standalone rather than external-content). Holds
//!   BOTH the recognized OCR text AND the scene/object tag labels (one folded row
//!   per source), so a keyword search matches tags alongside OCR.
//! - `media_tags` — the structured tags (`file_id, label, score`) for tag-score
//!   filtering; the folded FTS rows above are its keyword-search index.
//! - `media_embedding` — the image feature-print embedding (`file_id, dims, vector` `f16`
//!   BLOB) for image↔image similarity + dedup (plan Decision 2).
//! - `media_clip_embedding` — the CLIP image embedding (`file_id, dims, vector` `f16` BLOB)
//!   for natural-language text→image search. A SEPARATE table from
//!   `media_embedding`: CLIP and the Vision feature print are DIFFERENT vector spaces,
//!   so mixing them would let a similarity query silently compare across spaces. Its
//!   staleness is the independent `clip_stamp` on `media_status` (below), NOT the
//!   Vision `engine_version` — installing/upgrading the CLIP model re-embeds CLIP
//!   without re-running OCR/tags for everyone, and vice versa.
//! - `meta` — `schema_version`.

mod connection;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use half::f16;
use rusqlite::Connection;

pub use connection::open_read_connection;
pub(crate) use connection::open_write_connection;

use super::predicate::MediaKind;

/// Bump to invalidate on-disk `media.db` files. A mismatch deletes the file and
/// recreates it fresh (disposable cache, no migrations). Start at 1; bump on any
/// change to the tables below OR to what rows/text the store persists.
///
/// `3` added the `media_clip_embedding` table + the `media_status.clip_stamp` column
/// (CLIP semantic search).
///
/// `4` is the "make it small at NAS scale" bump (plan M3 + M4), landed as ONE bump so a
/// user's corpus re-enriches exactly once: (a) embeddings are stored as `f16` blobs, not
/// `f32` (half the vector bytes on disk and in the resident cache, precision loss far below
/// ranking noise), and (b) the path moved into one `media_file(id, path)` identity table and
/// every other table keys on the integer `file_id` (paths averaging ~80 B stored once, not
/// once per table). Because it's a disposable cache with no migrations, the bump
/// delete-and-recreates every `media.db` on first launch after the upgrade, so beta users
/// re-enrich from scratch (Vision recompute only, no re-download) — an accepted cost of the
/// disposable-cache design.
const SCHEMA_VERSION: &str = "4";

/// The FTS5 `media_ocr` table is STANDALONE (its own copy of the text), not
/// external-content. External-content would point the FTS index at another table's integer
/// rowid and sync via triggers; a standalone table keyed by an UNINDEXED `file_id` column
/// keeps enrichment and GC a simple `WHERE file_id = ?` delete, with no trigger machinery to
/// desync. (`media_file` carries the integer key now — plan M4 — but the standalone shape
/// stays the simpler one here.)
///
/// `media_ocr` folds tags in by carrying a `source` (`'ocr'` / `'tag'`) column and up
/// to two rows per file: the recognized OCR text, and the space-joined tag labels. A
/// keyword search matches either, so tags are searchable alongside OCR; the
/// `WHERE file_id = ?` delete still clears every row for a file in one statement.
const CREATE_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS media_file (
        id    INTEGER PRIMARY KEY,
        path  TEXT NOT NULL UNIQUE COLLATE platform_case
    );

    CREATE TABLE IF NOT EXISTS media_status (
        file_id         INTEGER PRIMARY KEY,
        mtime           INTEGER,
        size            INTEGER,
        media_kind      TEXT    NOT NULL,
        state           TEXT    NOT NULL,
        engine_version  TEXT    NOT NULL DEFAULT '',
        clip_stamp      TEXT    NOT NULL DEFAULT ''
    ) WITHOUT ROWID;

    CREATE VIRTUAL TABLE IF NOT EXISTS media_ocr USING fts5(
        file_id UNINDEXED,
        source UNINDEXED,
        text,
        tokenize = 'unicode61 remove_diacritics 2'
    );

    CREATE TABLE IF NOT EXISTS media_tags (
        file_id  INTEGER NOT NULL,
        label    TEXT NOT NULL,
        score    REAL NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_media_tags_file  ON media_tags(file_id);
    CREATE INDEX IF NOT EXISTS idx_media_tags_label ON media_tags(label);

    CREATE TABLE IF NOT EXISTS media_embedding (
        file_id  INTEGER PRIMARY KEY,
        dims     INTEGER NOT NULL,
        vector   BLOB    NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS media_clip_embedding (
        file_id  INTEGER PRIMARY KEY,
        dims     INTEGER NOT NULL,
        vector   BLOB    NOT NULL
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
/// typed kind and state, the combined analyze provenance stamp (`engine_version`: OCR
/// engine + tag taxonomy + feature-print revision), and the INDEPENDENT CLIP provenance
/// stamp (`clip_stamp`: the installed CLIP model id + OS version, empty when no model
/// has embedded this row). The two stamps drive two-part staleness ([`needs_enrichment`]
/// for Vision, [`needs_clip`] for CLIP) so each side re-runs on its own model change
/// without disturbing the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaStatusRow {
    pub path: String,
    pub mtime: Option<u64>,
    pub size: Option<u64>,
    pub media_kind: MediaKind,
    pub state: EnrichmentState,
    pub engine_version: String,
    pub clip_stamp: String,
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

/// Whether an image at `path` needs (re-)CLIP-embedding — the INDEPENDENT CLIP half of
/// two-part staleness (plan M3). `clip_stamp` is the currently-installed CLIP model's
/// provenance stamp, or `None` when NO CLIP model is installed.
///
/// - `None` ⇒ never stale: with no model there's nothing to embed, so an un-installed
///   CLIP never forces a pass and a stored row's empty `clip_stamp` just stays empty.
/// - `Some(current)` ⇒ stale when there's no row yet, or the row's stored `clip_stamp`
///   differs from `current` (a first install stamps `""` → the model stamp; a model or
///   OS change re-embeds). This is deliberately decoupled from the Vision
///   `engine_version`: installing/upgrading CLIP must NOT re-run OCR/tags for everyone,
///   and a Vision engine bump must NOT re-embed CLIP.
pub fn needs_clip(stored: Option<&MediaStatusRow>, clip_stamp: Option<&str>) -> bool {
    let Some(current) = clip_stamp else {
        return false;
    };
    match stored {
        None => true,
        Some(row) => row.clip_stamp != current,
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
        // The ANN index is a derivative of this DB's rows, so a schema wipe takes it
        // (and its sidecars) along — a fresh DB must never be searched through an
        // index built from the old rows (plan M6).
        super::ann::delete_index_files(db_path, super::ann::AnnSpace::Clip);
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

/// Read one `media_status` row, joined to its path in `media_file`.
///
/// `pub(crate)` so test probes can poll it over [`open_read_connection`] instead of
/// re-opening a full `MediaStore` (a WRITE connection) per poll — see
/// `scheduler/kick_tests.rs::has_enriched_row` for why that contention matters.
pub(crate) fn read_status(conn: &Connection, path: &str) -> Result<Option<MediaStatusRow>, MediaStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT f.path, s.mtime, s.size, s.media_kind, s.state, s.engine_version, s.clip_stamp
         FROM media_status s JOIN media_file f ON f.id = s.file_id
         WHERE f.path = ?1",
    )?;
    let mut rows = stmt.query_map(rusqlite::params![path], row_to_status)?;
    match rows.next() {
        Some(Ok(r)) => Ok(Some(r)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Load every `media_status` row into a `path → row` map (joining `media_file` for the
/// path). The scheduler loads one snapshot per pass to decide staleness and to derive the
/// stored-path set for GC.
pub(crate) fn read_all_status(conn: &Connection) -> Result<Vec<MediaStatusRow>, MediaStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT f.path, s.mtime, s.size, s.media_kind, s.state, s.engine_version, s.clip_stamp
         FROM media_status s JOIN media_file f ON f.id = s.file_id",
    )?;
    let rows = stmt.query_map([], row_to_status)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Every stored path (the reclaim partition's stored-row set). A cheap scan of
/// `media_file`; the partition then classifies each in Rust.
pub(crate) fn read_status_paths(conn: &Connection) -> Result<Vec<String>, MediaStoreError> {
    let mut stmt = conn.prepare_cached("SELECT path FROM media_file")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Sum the on-disk content bytes of the rows for `paths` across `media_ocr` (the OCR +
/// folded-tag FTS text), `media_tags` (the structured tag labels), and `media_embedding`
/// (the feature-print BLOBs) — the honest "about" byte estimate a reclaim prune would
/// free. Streams each table once and sums only the paths in the set, so it
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
        "SELECT f.path, length(o.text) FROM media_ocr o JOIN media_file f ON f.id = o.file_id",
        "SELECT f.path, length(t.label) FROM media_tags t JOIN media_file f ON f.id = t.file_id",
        "SELECT f.path, length(e.vector) FROM media_embedding e JOIN media_file f ON f.id = e.file_id",
        "SELECT f.path, length(c.vector) FROM media_clip_embedding c JOIN media_file f ON f.id = c.file_id",
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
        clip_stamp: row.get(6)?,
    })
}

// ── Embedding codec (feature-print BLOBs, f16) ─────────────────────────────

/// Serialize an embedding to a little-endian `f16` BLOB (2 bytes/element, half the `f32`
/// footprint — plan M3). The `dims` column stores the element count so a decode can
/// validate the byte length. The precision loss is far below ranking noise (cosine delta
/// ~1e-3), and the resident vector cache scores against the stored `f16` directly, so the
/// halving lands on disk AND in RAM.
pub(crate) fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 2);
    for f in vector {
        bytes.extend_from_slice(&f16::from_f32(*f).to_le_bytes());
    }
    bytes
}

/// Decode a little-endian `f16` BLOB, WIDENING each element to `f32` — the query direction
/// (a find-similar source vector, a round-trip check). Returns `None` when the byte length
/// isn't a whole number of `f16`s (a corrupt row degrades to "no embedding" rather than
/// failing the whole read — the cache just skips it).
pub(crate) fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    Some(decode_embedding_f16(bytes)?.into_iter().map(f16::to_f32).collect())
}

/// Decode a little-endian `f16` BLOB to `f16` elements (NO widening) — the resident-cache
/// load direction, where keeping `f16` is what halves the cache's RAM. Returns `None` on a
/// non-`f16`-multiple length (a corrupt row degrades to "no embedding").
pub(crate) fn decode_embedding_f16(bytes: &[u8]) -> Option<Vec<f16>> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    Some(
        bytes
            .chunks_exact(2)
            .map(|c| f16::from_le_bytes([c[0], c[1]]))
            .collect(),
    )
}

/// One stored embedding: the image path and its feature-print vector, kept as `f16` (the
/// on-disk representation — plan M3). The vector store loads a `Vec` of these per volume
/// into its resident cache and scores against the `f16` directly, so the cache stays half
/// the size of an `f32` one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmbeddingRow {
    pub(crate) path: String,
    pub(crate) vector: Vec<f16>,
}

/// Load every stored embedding for a volume as `f16` (the resident vector cache's load-once
/// source; kept `f16` to halve the cache's RAM). A row whose BLOB can't be decoded is
/// skipped, not fatal. `table` is the embedding table (`media_embedding` for the Vision
/// feature print, or `media_clip_embedding` for CLIP) — the two live in DIFFERENT vector
/// spaces and each backs its own resident cache, so the caller names which one it wants.
pub(crate) fn read_all_embeddings_from(
    conn: &Connection,
    table: EmbeddingTable,
) -> Result<Vec<EmbeddingRow>, MediaStoreError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT f.path, e.vector FROM {} e JOIN media_file f ON f.id = e.file_id",
        table.name()
    ))?;
    let rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        Ok((path, blob))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (path, blob) = row?;
        if let Some(vector) = decode_embedding_f16(&blob) {
            out.push(EmbeddingRow { path, vector });
        }
    }
    Ok(out)
}

/// Stream every stored embedding of one space as `(file_id, f16 vector)` — the ANN
/// rebuild's source (plan M6: the index keys on the `media_file` integer ids). Streams
/// row by row (no whole-corpus `Vec`); a row whose BLOB can't be decoded is skipped, not
/// fatal. The callback's error type is generic so the ANN layer can thread its own
/// error through.
pub(crate) fn for_each_embedding_with_id<E: From<MediaStoreError>>(
    conn: &Connection,
    table: EmbeddingTable,
    mut f: impl FnMut(i64, &[f16]) -> Result<(), E>,
) -> Result<(), E> {
    let mut stmt = conn
        .prepare_cached(&format!("SELECT file_id, vector FROM {}", table.name()))
        .map_err(MediaStoreError::from)?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)))
        .map_err(MediaStoreError::from)?;
    for row in rows {
        let (file_id, blob) = row.map_err(MediaStoreError::from)?;
        if let Some(vector) = decode_embedding_f16(&blob) {
            f(file_id, &vector)?;
        }
    }
    Ok(())
}

/// The number of stored embeddings in one space, read on an existing connection
/// (the ANN rebuild's `reserve` size).
pub(crate) fn embedding_count_on(conn: &Connection, table: EmbeddingTable) -> Result<u64, MediaStoreError> {
    let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {}", table.name()), [], |row| row.get(0))?;
    Ok(count.max(0) as u64)
}

/// The number of stored embeddings in one space, by DB path — the ANN route's
/// corpus-size input. `0` for a missing/unreadable DB (the offline / never-enriched
/// case routes to brute force, which answers empty).
pub(crate) fn embedding_count(db_path: &Path, table: EmbeddingTable) -> u64 {
    if !db_path.exists() {
        return 0;
    }
    open_read_connection(db_path)
        .and_then(|conn| embedding_count_on(&conn, table))
        .unwrap_or_else(|e| {
            log::warn!(target: "media_index", "embedding count failed for {}: {e}", db_path.display());
            0
        })
}

/// The current `(path, f16 vector)` for each requested `media_file` id in one space —
/// the ANN re-rank's exact-score source. Chunked at [`u64`] ids ≤ 900 per `IN (…)`
/// (SQLite's host-parameter ceiling). An id with no row (a ghost key whose file was
/// GC'd after the index last saved) simply yields nothing, which is what silently
/// drops ghosts from ANN results; the path is the CURRENT `media_file.path`, so hits
/// follow renames.
pub(crate) fn read_embeddings_for_ids(
    conn: &Connection,
    table: EmbeddingTable,
    ids: &[u64],
) -> Result<Vec<(String, Vec<f16>)>, MediaStoreError> {
    let mut out = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(900) {
        let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT f.path, e.vector FROM {} e JOIN media_file f ON f.id = e.file_id
             WHERE e.file_id IN ({placeholders})",
            table.name()
        ))?;
        let params = rusqlite::params_from_iter(chunk.iter().map(|id| *id as i64));
        let rows = stmt.query_map(params, |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)))?;
        for row in rows {
            let (path, blob) = row?;
            if let Some(vector) = decode_embedding_f16(&blob) {
                out.push((path, vector));
            }
        }
    }
    Ok(out)
}

/// Which embedding table a load/read targets. The two are separate vector spaces (plan
/// M3): `FeaturePrint` = Vision `media_embedding` (image↔image similarity, dedup),
/// `Clip` = `media_clip_embedding` (natural-language text→image). Never mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EmbeddingTable {
    FeaturePrint,
    Clip,
}

impl EmbeddingTable {
    fn name(self) -> &'static str {
        match self {
            EmbeddingTable::FeaturePrint => "media_embedding",
            EmbeddingTable::Clip => "media_clip_embedding",
        }
    }
}

/// Read one image's stored embedding, or `None` if it has none (the source vector for
/// a "find similar images" query).
pub(crate) fn read_embedding_for(conn: &Connection, path: &str) -> Result<Option<Vec<f32>>, MediaStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.vector FROM media_embedding e JOIN media_file f ON f.id = e.file_id WHERE f.path = ?1",
    )?;
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
        "SELECT f.path, t.score FROM media_tags t JOIN media_file f ON f.id = t.file_id
         WHERE t.label = ?1 AND t.score >= ?2 ORDER BY t.score DESC, f.path ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![label, min_score], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)? as f32))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(super) const CREATE_TABLES: &str = CREATE_TABLES_SQL;
