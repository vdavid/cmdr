//! `visits-{volume_id}.db`: the navigation-visit signal, a store of its own.
//!
//! The one importance data a scan can't bring back, so it lives in the data dir
//! (backed up), apart from the weights (`importance-{id}.db`, a rebuildable cache
//! kept out of backups). Small: about 100 KB on David's Mac (2026-10-06).
//!
//! Unlike the weights it's NOT a disposable cache: ❌ never delete-and-recreate it
//! on a schema change. A future change to its tables is a migration, and David's
//! call (`../../../CLAUDE.md` § "Rebuild, don't migrate"). The only file it
//! deletes is one SQLite says isn't a sound database, which has nothing left to
//! save.
//!
//! The importance writer ATTACHes it ([`attach`]), so its `visits` table resolves
//! with no schema prefix on the writer's connection; the recompute pass reads it
//! on a connection of its own.

use std::path::Path;

use rusqlite::Connection;

use super::ImportanceStoreError;
use super::connection::{apply_pragmas, register_collation};
use crate::indexing::store::normalize_for_comparison;

/// The visits file's schema, stamped once and never acted on by a recreate. A
/// bump comes with a migration.
const VISITS_SCHEMA_VERSION: &str = "1";

/// The schema name the writer attaches the visits file under. The weights file
/// has no table called `visits`, so the unqualified name resolves here.
const ATTACHED_AS: &str = "visit_log";

// `visits` keys on the same BINARY `path_folded` fold as the weights
// (`DETAILS.md` § The folded-key primary key). `split_from` records each
// older build's combined store whose visits were moved in, by the token
// `legacy_split.rs` stamps into it, and when; `split_rows` is the ledger of how
// much of each row each token contributed, so a re-run, or an older build that
// kept writing into the same file, adds only what's new
// (`legacy_split.rs` § How a row merges).
const CREATE_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS visits (
        path_folded     TEXT    PRIMARY KEY,
        path            TEXT    NOT NULL,
        visit_count     INTEGER NOT NULL DEFAULT 0,
        last_visit_secs INTEGER NOT NULL DEFAULT 0
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS split_from (
        token   TEXT    PRIMARY KEY,
        rows    INTEGER NOT NULL,
        at_secs INTEGER NOT NULL
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS split_rows (
        token       TEXT    NOT NULL,
        path_folded TEXT    NOT NULL,
        visit_count INTEGER NOT NULL,
        PRIMARY KEY (token, path_folded)
    ) WITHOUT ROWID;
";

/// Open (or create) the visits file at `visits_db` for writing: collation, WAL
/// pragmas, tables, and the schema stamp. Creates the parent folder.
///
/// A file SQLite says isn't a sound database is deleted and recreated (there's
/// nothing in it to save); every other failure comes back, so a passing problem
/// (a full disk, a permission) never costs the history.
pub(crate) fn open_write_connection(visits_db: &Path) -> Result<Connection, ImportanceStoreError> {
    match try_open_write_connection(visits_db) {
        Err(e) if e.is_corruption() => {
            log::warn!(
                target: "importance",
                "{} isn't a sound database ({e}); starting its visit history afresh",
                cmdr_fs::log_private::LogPrivate::file_path(visits_db)
            );
            cmdr_fs::sqlite_util::delete_database(visits_db)?;
            try_open_write_connection(visits_db)
        }
        opened => opened,
    }
}

fn try_open_write_connection(visits_db: &Path) -> Result<Connection, ImportanceStoreError> {
    if let Some(parent) = visits_db.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let conn = cmdr_fs::sqlite_util::open(visits_db)?;
    register_collation(&conn)?;
    apply_pragmas(&conn, false)?;
    conn.execute_batch(CREATE_TABLES)?;
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![VISITS_SCHEMA_VERSION],
    )?;
    Ok(conn)
}

/// Attach the visits file at `visits_db` to `conn` (a weights write connection),
/// creating it first if needed, so `visits` statements on `conn` reach it.
pub(crate) fn attach(conn: &Connection, visits_db: &Path) -> Result<(), ImportanceStoreError> {
    // Creates the file and its tables, and sets its (persistent) WAL mode, which
    // an attach alone wouldn't.
    drop(open_write_connection(visits_db)?);
    let path = visits_db.to_str().ok_or_else(|| {
        ImportanceStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the visits file's path isn't valid UTF-8",
        ))
    })?;
    conn.execute(&format!("ATTACH DATABASE ?1 AS {ATTACHED_AS}"), rusqlite::params![path])?;
    conn.execute_batch(&format!("PRAGMA {ATTACHED_AS}.synchronous = NORMAL;"))?;
    Ok(())
}

/// Bump a path's visit count by one and set its last-visit timestamp, on a
/// connection that sees the `visits` table (the writer's, which attached it).
pub(crate) fn bump(conn: &Connection, path: &str, at_secs: u64) -> Result<(), ImportanceStoreError> {
    conn.execute(
        "INSERT INTO visits (path_folded, path, visit_count, last_visit_secs) VALUES (?1, ?2, 1, ?3)
         ON CONFLICT(path_folded) DO UPDATE SET visit_count = visit_count + 1, last_visit_secs = ?3",
        rusqlite::params![normalize_for_comparison(path), path, at_secs as i64],
    )?;
    Ok(())
}

/// One folder's visit record in the visits file at `visits_db`, as
/// `(count, last_visit_secs)`, or `None` if never visited.
#[cfg(test)]
pub(crate) fn visit_for(visits_db: &Path, path: &str) -> Result<Option<(u64, u64)>, ImportanceStoreError> {
    let conn = open_write_connection(visits_db)?;
    super::read_visit(&conn, path)
}

/// Every visit row in the visits file at `visits_db`, as
/// `(path, count, last_visit_secs)` sorted by path.
#[cfg(test)]
pub(crate) fn all_visits(visits_db: &Path) -> Vec<(String, u64, u64)> {
    let conn = open_write_connection(visits_db).expect("open visits");
    let mut stmt = conn
        .prepare("SELECT path, visit_count, last_visit_secs FROM visits ORDER BY path")
        .expect("prepare");
    stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
        ))
    })
    .expect("query")
    .map(|row| row.expect("row"))
    .collect()
}
