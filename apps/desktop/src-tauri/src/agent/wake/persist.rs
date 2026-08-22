//! The inbox's edge: mapping the pure rows onto `agent_inbox` and back.
//!
//! Everything else under `wake/` is values in and values out. This file is the one place that
//! takes a `Connection`, and it holds no policy of its own: it maps, it reads, it writes. The
//! decisions (when a row is due, what a restart drops) stay in `inbox.rs` where they can be
//! tested without a database.
//!
//! The store keeps its own flat row type and this maps onto it, rather than the store
//! importing the wake vocabulary — the same direction `proposals/` takes with `NewGroup`.

use rusqlite::Connection;

use super::{ChangeCounters, EventBundle, Inbox, InboxRow, Interest};
use crate::agent::store::{AgentStoreError, StoredInboxRow, clear_inbox, load_inbox, replace_inbox, upsert_inbox_row};

/// Read the whole inbox back, for a launch to reconcile.
pub fn load(conn: &Connection) -> Result<Inbox, AgentStoreError> {
    Ok(Inbox::from_rows(load_inbox(conn)?.iter().map(to_row).collect()))
}

/// Write one row, replacing whatever was waiting for that folder-window.
pub fn save_row(conn: &Connection, row: &InboxRow) -> Result<(), AgentStoreError> {
    upsert_inbox_row(conn, &to_stored(row))
}

/// Write the whole inbox, replacing what was there. What a restart writes back once it has
/// dropped the stale rows and deferred the overdue ones.
pub fn save_all(conn: &Connection, inbox: &Inbox) -> Result<(), AgentStoreError> {
    let rows: Vec<StoredInboxRow> = inbox.rows().iter().map(to_stored).collect();
    replace_inbox(conn, &rows)
}

/// Empty the table, which is what a wake does once it has drained the rows.
pub fn clear(conn: &Connection) -> Result<(), AgentStoreError> {
    clear_inbox(conn)
}

/// Times are unsigned seconds here and signed in SQLite, so each crossing saturates rather
/// than wrapping: a clock that produced something absurd must not turn a waiting row into one
/// that is overdue by an epoch.
fn to_stored(row: &InboxRow) -> StoredInboxRow {
    StoredInboxRow {
        folder: row.bundle.folder.clone(),
        window_start: clamp_to_i64(row.bundle.window_start),
        created: row.bundle.counters.created,
        modified: row.bundle.counters.modified,
        removed: row.bundle.counters.removed,
        renamed: row.bundle.counters.renamed,
        last_event_at: clamp_to_i64(row.bundle.last_event_at),
        interest: row.interest.value(),
        deliver_by: row.deliver_by.map(clamp_to_i64),
    }
}

fn to_row(stored: &StoredInboxRow) -> InboxRow {
    InboxRow {
        bundle: EventBundle {
            folder: stored.folder.clone(),
            counters: ChangeCounters {
                created: stored.created,
                modified: stored.modified,
                removed: stored.removed,
                renamed: stored.renamed,
            },
            window_start: clamp_to_u64(stored.window_start),
            last_event_at: clamp_to_u64(stored.last_event_at),
        },
        interest: Interest::of(stored.interest),
        deliver_by: stored.deliver_by.map(clamp_to_u64),
    }
}

fn clamp_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn clamp_to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::store::{MIGRATIONS, run_migrations};
    use crate::agent::wake::{ChangeCounters, EventBundle, FolderImportance};

    fn migrated_conn() -> Connection {
        let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
        run_migrations(&conn, MIGRATIONS).expect("migrate");
        conn
    }

    fn bundle(folder: &str, created: u32, window_start: u64) -> EventBundle {
        EventBundle {
            folder: folder.to_string(),
            counters: ChangeCounters {
                created,
                ..ChangeCounters::default()
            },
            window_start,
            last_event_at: window_start + 5,
        }
    }

    /// An inbox that goes through the table comes back the same inbox: same rows, same
    /// counters, same deadlines. If any of that drifted, a restart would change what the agent
    /// is waiting for, silently.
    #[test]
    fn an_inbox_survives_a_round_trip_through_the_table() {
        let conn = migrated_conn();
        let mut inbox = Inbox::default();
        inbox.admit(
            bundle("/Users/someone/Downloads", 4, 100),
            FolderImportance::Scored(0.9),
            1_000,
        );
        inbox.admit(bundle("/tmp/log", 2, 100), FolderImportance::Unknown, 1_000);

        save_all(&conn, &inbox).expect("write");
        let loaded = load(&conn).expect("read");

        assert_eq!(loaded, inbox);
    }

    /// A cold row waits with NO deadline, and it has to come back that way. Reloaded with one, it
    /// would come due on its own after the next launch, which is exactly what the null prevents.
    #[test]
    fn a_cold_row_reloads_without_a_deadline() {
        let conn = migrated_conn();
        let mut inbox = Inbox::default();
        inbox.admit(bundle("/tmp/junk", 2, 100), FolderImportance::Floored, 1_000);
        save_all(&conn, &inbox).expect("write");

        let loaded = load(&conn).expect("read");

        assert_eq!(loaded, inbox);
        assert_eq!(loaded.next_deadline(), None, "and it still causes no wake of its own");
    }

    /// Saving one row at a time is what admit does on the live path, and it has to land the
    /// same way a whole-inbox write would.
    #[test]
    fn saving_one_row_at_a_time_builds_the_same_inbox() {
        let conn = migrated_conn();
        let mut inbox = Inbox::default();
        inbox.admit(
            bundle("/Users/someone/Downloads", 4, 100),
            FolderImportance::Scored(0.9),
            1_000,
        );
        for row in inbox.rows() {
            save_row(&conn, row).expect("write");
        }

        assert_eq!(load(&conn).expect("read"), inbox);
    }

    /// The merge key survives the round trip: two writes for one folder-window are one row,
    /// the same answer the in-memory inbox gives.
    #[test]
    fn two_writes_for_one_folder_window_load_as_one_row() {
        let conn = migrated_conn();
        let mut inbox = Inbox::default();
        inbox.admit(
            bundle("/Users/someone/Downloads", 4, 100),
            FolderImportance::Scored(0.9),
            1_000,
        );
        save_all(&conn, &inbox).expect("write");
        inbox.admit(
            bundle("/Users/someone/Downloads", 3, 100),
            FolderImportance::Scored(0.9),
            1_100,
        );
        save_all(&conn, &inbox).expect("write again");

        let loaded = load(&conn).expect("read");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.rows()[0].bundle.counters.created, 7);
    }

    #[test]
    fn clearing_leaves_nothing_to_load() {
        let conn = migrated_conn();
        let mut inbox = Inbox::default();
        inbox.admit(bundle("/x", 1, 100), FolderImportance::Unknown, 1_000);
        save_all(&conn, &inbox).expect("write");

        clear(&conn).expect("clear");

        assert!(load(&conn).expect("read").is_empty());
    }
}
