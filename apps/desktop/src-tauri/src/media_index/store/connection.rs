//! Connection factories for `media.db`.
//!
//! Mirrors `importance/store/connection.rs`: every connection registers the shared
//! `platform_case` collation (reused from `indexing::store`, not persisted in the
//! file, so it must be re-registered per connection) and applies the same WAL
//! pragmas. Write connections create the tables — including the FTS5 OCR table — if
//! missing; read connections open read-only and assume the tables exist.

use std::path::Path;

use rusqlite::Connection;

use super::{CREATE_TABLES, MediaStoreError};
use crate::indexing::store::register_platform_case_collation;

/// Apply pragmas. Write connections enable WAL + incremental auto-vacuum; both read
/// and write get the busy-timeout and cache tuning. Matches the index and
/// importance stores so all three behave identically under contention.
fn apply_pragmas(conn: &Connection, readonly: bool) -> Result<(), MediaStoreError> {
    if !readonly {
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;",
        )?;
    }
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -16384;",
    )?;
    Ok(())
}

/// Open a write connection: `platform_case` collation, WAL pragmas, tables created
/// if missing (including the FTS5 `media_ocr` virtual table). Used by the writer
/// thread and by `MediaStore::open` (which also owns the schema-version check).
pub(crate) fn open_write_connection(db_path: &Path) -> Result<Connection, MediaStoreError> {
    let conn = Connection::open(db_path)?;
    register_collation(&conn)?;
    apply_pragmas(&conn, false)?;
    // Creating `media_ocr USING fts5` here is also the FTS5 availability guard: a
    // `bundled` SQLite compiled without FTS5 fails on this statement, exactly as
    // `agent/store`'s `fresh_open_builds_current_schema` catches it.
    conn.execute_batch(CREATE_TABLES)?;
    Ok(conn)
}

/// Open a read-only connection with the collation and read pragmas. Never contends
/// with the writer thread's write lock (WAL). The tables are assumed to exist.
pub fn open_read_connection(db_path: &Path) -> Result<Connection, MediaStoreError> {
    let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    register_collation(&conn)?;
    apply_pragmas(&conn, true)?;
    Ok(conn)
}

/// Register the shared `platform_case` collation, mapping the index store's error
/// into ours.
fn register_collation(conn: &Connection) -> Result<(), MediaStoreError> {
    register_platform_case_collation(conn).map_err(|e| match e {
        crate::indexing::store::IndexStoreError::Sqlite(e) => MediaStoreError::Sqlite(e),
        crate::indexing::store::IndexStoreError::Io(e) => MediaStoreError::Io(e),
        other => MediaStoreError::Io(std::io::Error::other(other.to_string())),
    })
}
