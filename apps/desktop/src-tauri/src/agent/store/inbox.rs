//! Persisting the wake inbox (`agent_inbox`, migration v6).
//!
//! The in-memory inbox in `agent/wake/` is the working copy and the only thing that decides
//! anything; this module just makes it survive a restart. It keeps its own flat row type
//! rather than importing the wake types, the same direction `proposals/` takes with
//! `NewGroup`: persistence depends on `rusqlite` and the vocabulary below it, never on the
//! service layer above.

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use super::AgentStoreError;

/// One waiting folder-window, as stored. Times are unix seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredInboxRow {
    pub folder: String,
    pub window_start: i64,
    pub created: u32,
    pub modified: u32,
    pub removed: u32,
    pub renamed: u32,
    pub last_event_at: i64,
    pub interest: f64,
    pub deliver_by: i64,
}

const COLUMNS: &str = "folder, window_start, created, modified, removed, renamed, last_event_at, interest, deliver_by";

/// Every row waiting, oldest window first.
pub fn load_inbox(conn: &Connection) -> Result<Vec<StoredInboxRow>, AgentStoreError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM agent_inbox ORDER BY window_start, folder"
    ))?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_row(row)?);
    }
    Ok(out)
}

/// Write one row, replacing whatever was waiting for that folder-window.
///
/// The conflict target is the primary key, which IS the merge key the inbox merges on, so a
/// second write for the same folder-window can only ever update the first.
pub fn upsert_inbox_row(conn: &Connection, row: &StoredInboxRow) -> Result<(), AgentStoreError> {
    conn.prepare_cached(
        "INSERT INTO agent_inbox (folder, window_start, created, modified, removed, renamed, last_event_at, interest, deliver_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT (folder, window_start) DO UPDATE SET
            created = ?3, modified = ?4, removed = ?5, renamed = ?6,
            last_event_at = ?7, interest = ?8, deliver_by = ?9",
    )?
    .execute(params![
        row.folder,
        row.window_start,
        row.created,
        row.modified,
        row.removed,
        row.renamed,
        row.last_event_at,
        row.interest,
        row.deliver_by,
    ])?;
    Ok(())
}

/// Swap the whole set in one transaction: what restart reconciliation writes back after it has
/// dropped the stale rows and deferred the overdue ones.
pub fn replace_inbox(conn: &Connection, rows: &[StoredInboxRow]) -> Result<(), AgentStoreError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute("DELETE FROM agent_inbox", [])?;
    for row in rows {
        upsert_inbox_row(&tx, row)?;
    }
    tx.commit()?;
    Ok(())
}

/// Empty the inbox, which is what a wake does once it has drained the rows.
pub fn clear_inbox(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute("DELETE FROM agent_inbox", [])?;
    Ok(())
}

fn map_row(row: &rusqlite::Row<'_>) -> Result<StoredInboxRow, AgentStoreError> {
    Ok(StoredInboxRow {
        folder: row.get(0)?,
        window_start: row.get(1)?,
        created: row.get(2)?,
        modified: row.get(3)?,
        removed: row.get(4)?,
        renamed: row.get(5)?,
        last_event_at: row.get(6)?,
        interest: row.get(7)?,
        deliver_by: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::store::{MIGRATIONS, run_migrations};

    fn migrated_conn() -> Connection {
        let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
        run_migrations(&conn, MIGRATIONS).expect("migrate");
        conn
    }

    fn row(folder: &str, window_start: i64, created: u32) -> StoredInboxRow {
        StoredInboxRow {
            folder: folder.to_string(),
            window_start,
            created,
            modified: 0,
            removed: 0,
            renamed: 0,
            last_event_at: window_start + 5,
            interest: 0.75,
            deliver_by: window_start + 60,
        }
    }

    #[test]
    fn a_row_survives_a_round_trip() {
        let conn = migrated_conn();
        let written = row("/Users/someone/Downloads", 100, 4);
        upsert_inbox_row(&conn, &written).expect("write");

        assert_eq!(load_inbox(&conn).expect("read"), vec![written]);
    }

    /// The primary key IS the merge key, so a second write for the same folder-window updates
    /// rather than duplicating. Without that the table could hold two rows the in-memory inbox
    /// would have merged, and a restart would resurrect a split the pipeline had healed.
    #[test]
    fn writing_the_same_folder_and_window_twice_updates_one_row() {
        let conn = migrated_conn();
        upsert_inbox_row(&conn, &row("/Users/someone/Downloads", 100, 4)).expect("write");
        upsert_inbox_row(&conn, &row("/Users/someone/Downloads", 100, 11)).expect("write again");

        let loaded = load_inbox(&conn).expect("read");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].created, 11, "the newer counts win");
    }

    /// The same folder in a different window is a different row, matching how the coalescer
    /// and the inbox keep two bursts apart.
    #[test]
    fn two_windows_of_one_folder_are_two_rows() {
        let conn = migrated_conn();
        upsert_inbox_row(&conn, &row("/Users/someone/Downloads", 100, 4)).expect("write");
        upsert_inbox_row(&conn, &row("/Users/someone/Downloads", 40_000, 4)).expect("write");

        assert_eq!(load_inbox(&conn).expect("read").len(), 2);
    }

    #[test]
    fn replacing_swaps_the_whole_set() {
        let conn = migrated_conn();
        upsert_inbox_row(&conn, &row("/gone", 100, 4)).expect("write");

        replace_inbox(&conn, &[row("/kept", 200, 9)]).expect("replace");

        let loaded = load_inbox(&conn).expect("read");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].folder, "/kept");
    }

    #[test]
    fn clearing_empties_the_inbox() {
        let conn = migrated_conn();
        upsert_inbox_row(&conn, &row("/Users/someone/Downloads", 100, 4)).expect("write");

        clear_inbox(&conn).expect("clear");

        assert!(load_inbox(&conn).expect("read").is_empty());
    }
}
