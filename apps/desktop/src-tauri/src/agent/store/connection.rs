//! Connection factories for `main.db`.
//!
//! Mirrors the operation log's pragmas (WAL, incremental auto-vacuum, 5 s busy timeout,
//! `synchronous = NORMAL`, foreign keys on) so the durable agent store behaves
//! identically under contention, and — like the operation log — registers NO custom
//! collation: the store precomputes any comparison key in Rust and queries plain b-tree
//! equality, so the file stays openable in any stock `sqlite3` browser.
//!
//! A write connection runs the migration ladder (`super::migrations`) on open, so the
//! schema is current before any query. A read connection opens read-only and assumes the
//! schema is already migrated (a read-only connection can't migrate).

use std::path::Path;

use rusqlite::Connection;

use super::AgentStoreError;
use super::migrations::{MIGRATIONS, run_migrations};

/// Apply pragmas. Write connections enable WAL + incremental auto-vacuum (the auto-vacuum
/// mode must be set before the first table is created, so it runs before the migration
/// ladder). Both read and write get the busy-timeout, `synchronous = NORMAL`, foreign
/// keys, and cache tuning.
fn apply_pragmas(conn: &Connection, readonly: bool) -> Result<(), AgentStoreError> {
    if !readonly {
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;",
        )?;
    }
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA cache_size = -16384;",
    )?;
    Ok(())
}

/// Open a write connection: WAL pragmas, then the migration ladder brings the schema to
/// the current version (creating it on a fresh file). Callers own the returned
/// connection.
pub(crate) fn open_write_connection(db_path: &Path) -> Result<Connection, AgentStoreError> {
    let conn = Connection::open(db_path)?;
    apply_pragmas(&conn, false)?;
    run_migrations(&conn, MIGRATIONS)?;
    Ok(conn)
}

/// Open a read-only connection with the read pragmas. Never contends with a write lock
/// (WAL). The schema is assumed current (the write path migrated it); a read-only
/// connection can neither create nor migrate tables.
pub fn open_read_connection(db_path: &Path) -> Result<Connection, AgentStoreError> {
    let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    apply_pragmas(&conn, true)?;
    Ok(conn)
}
