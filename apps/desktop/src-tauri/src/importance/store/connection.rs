//! Connection factories for `importance.db`.
//!
//! Every connection registers the shared `platform_case` collation (reused from
//! `indexing::store` — it's the SAME filesystem case/normalization rule, and it
//! isn't persisted in the file, so it must be re-registered per connection) and
//! creates the tables if missing. Write connections get WAL pragmas; read
//! connections open read-only.

use std::path::Path;

use rusqlite::Connection;

use super::{CREATE_TABLES, ImportanceStoreError};
use crate::indexing::store::register_platform_case_collation;

/// Apply pragmas. Write connections enable WAL + incremental auto-vacuum; both
/// read and write get the busy-timeout and cache tuning. Mirrors the index store's
/// `apply_pragmas` so the two DBs behave identically under contention.
fn apply_pragmas(conn: &Connection, readonly: bool) -> Result<(), ImportanceStoreError> {
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

/// Open a write connection: `platform_case` collation, WAL pragmas, tables
/// created if missing. Used by the writer thread and by `ImportanceStore::open`
/// (which also owns the schema-version check). Callers own the returned
/// connection.
pub(crate) fn open_write_connection(db_path: &Path) -> Result<Connection, ImportanceStoreError> {
    let conn = Connection::open(db_path)?;
    register_collation(&conn)?;
    apply_pragmas(&conn, false)?;
    conn.execute_batch(CREATE_TABLES)?;
    Ok(conn)
}

/// Open a read-only connection with the collation and read pragmas. Never
/// contends with the writer thread's write lock (WAL). The tables are assumed to
/// exist (the writer/`open` path created them); a read-only connection can't
/// create them.
pub fn open_read_connection(db_path: &Path) -> Result<Connection, ImportanceStoreError> {
    let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    register_collation(&conn)?;
    apply_pragmas(&conn, true)?;
    Ok(conn)
}

/// Register the shared `platform_case` collation, mapping the index store's error
/// into ours. The registrar only fails with an underlying sqlite error (a
/// collation-registration failure), which our `Sqlite` variant carries directly.
fn register_collation(conn: &Connection) -> Result<(), ImportanceStoreError> {
    register_platform_case_collation(conn).map_err(|e| match e {
        crate::indexing::store::IndexStoreError::Sqlite(e) => ImportanceStoreError::Sqlite(e),
        crate::indexing::store::IndexStoreError::Io(e) => ImportanceStoreError::Io(e),
        other => ImportanceStoreError::Io(std::io::Error::other(other.to_string())),
    })
}
