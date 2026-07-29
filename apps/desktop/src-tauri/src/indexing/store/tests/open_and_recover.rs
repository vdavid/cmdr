//! Opening a DB and recovering from a bad one: schema-version rebuilds,
//! corruption vs contention vs read-only, and the pragmas every connection gets.

use super::*;

#[test]
fn schema_creation_and_version() {
    let (store, _dir) = open_temp_store();
    let status = store.get_index_status().unwrap();
    assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
}

/// A schema-version mismatch recreates the DB file; the rebuilt DB still has the
/// new v13 columns (a write/read round-trip through them succeeds).
#[test]
fn schema_bump_rebuild_has_new_columns() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bump.db");

    // Open, then stamp a stale version to force a drop+rebuild on reopen.
    {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        IndexStore::update_meta(&conn, "schema_version", "1").unwrap();
    }

    let store = IndexStore::open(&db_path).unwrap();
    assert_eq!(
        store.get_index_status().unwrap().schema_version.as_deref(),
        Some(SCHEMA_VERSION)
    );

    // The new columns exist and round-trip on the rebuilt schema.
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let a = insert_entry(&conn, ROOT_ID, "a", true, None);
    IndexStore::mark_dirs_listed(&conn, &[a], 5).unwrap();
    assert_eq!(IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(), Some(5));
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: a,
            min_subtree_epoch: 5,
            ..Default::default()
        }],
    )
    .unwrap();
    assert_eq!(
        IndexStore::get_dir_stats_by_id(&conn, a)
            .unwrap()
            .unwrap()
            .min_subtree_epoch,
        5
    );
}

/// `apply_pragmas` must set a non-zero `busy_timeout` on both read and
/// write connections. Without it, concurrent connections fail with
/// `SQLITE_BUSY` on the first lock contention instead of waiting.
#[test]
fn apply_pragmas_sets_busy_timeout_on_both_modes() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let write_timeout: i64 = write_conn
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))
        .unwrap();
    assert!(
        write_timeout > 0,
        "write connection should have busy_timeout set, got {write_timeout}"
    );

    let read_conn = IndexStore::open_read_connection(store.db_path()).unwrap();
    let read_timeout: i64 = read_conn
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))
        .unwrap();
    assert!(
        read_timeout > 0,
        "read connection should have busy_timeout set, got {read_timeout}"
    );
}

/// `open_read_connection` must succeed while another connection holds a
/// write transaction. The live and replay event loops rely on this to
/// open their path-resolution connection without racing the writer
/// thread. Regression: switching this call site to `open_write_connection`
/// (or removing the `busy_timeout` pragma) makes the open fail on every
/// concurrent commit, which silently kills the FSEvents receiver and
/// stops live index updates for the rest of the session.
#[test]
fn open_read_connection_succeeds_under_write_lock() {
    let (store, _dir) = open_temp_store();
    let db_path = store.db_path().to_path_buf();
    let writer = IndexStore::open_write_connection(&db_path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();

    // The read connection should open and be usable while the writer's
    // transaction is still in flight.
    let read = IndexStore::open_read_connection(&db_path).expect("read connection should open under write lock");
    let root = IndexStore::get_entry_by_id(&read, ROOT_ID).unwrap();
    assert!(root.is_some(), "read connection should see committed root sentinel");

    // Release the writer's lock so the tempdir can clean up cleanly.
    writer.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn clear_all_resets_schema() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    insert_entry(&write_conn, ROOT_ID, "x", false, Some(1));

    IndexStore::clear_all(&write_conn).unwrap();

    // Schema version should be re-stamped
    let version = IndexStore::get_meta(&write_conn, "schema_version").unwrap();
    assert_eq!(version.as_deref(), Some(SCHEMA_VERSION));

    // Entries should be gone (except root sentinel)
    let children = store.list_children(ROOT_ID).unwrap();
    assert!(children.is_empty());
}

#[test]
fn schema_mismatch_recreates_to_current_version() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("mismatch.db");

    // Create a store and tamper with the version
    {
        let store = IndexStore::open(&db_path).unwrap();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        IndexStore::update_meta(&write_conn, "schema_version", "0").unwrap();
    }

    // Re-open: should detect the mismatch and recreate the file at the current version
    let store = IndexStore::open(&db_path).unwrap();
    let status = store.get_index_status().unwrap();
    assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
}

/// A schema-version mismatch recreates the DB as a fresh, zero-freelist FILE
/// (delete + recreate), rather than DROP-ing tables on the live file (which
/// leaves the freed pages stranded on the freelist). The reclaim is the whole
/// point, so this asserts `freelist_count == 0` after reopen.
#[test]
fn schema_mismatch_recreates_file_reclaiming_freelist() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bloat.db");

    {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        // Bloat the file so DROP-ing the tables strands many pages on the
        // freelist (auto_vacuum = INCREMENTAL never returns them on its own).
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..5000 {
            insert_entry(&conn, ROOT_ID, &format!("entry-{i}"), false, Some(i));
        }
        conn.execute_batch("COMMIT").unwrap();

        // Stamp an OLD schema version, then DROP entries + dir_stats but KEEP
        // `meta` intact. If we dropped `meta`, the reopen would read version
        // `None`, treat the DB as fresh, and never recreate -> false pass.
        IndexStore::update_meta(&conn, "schema_version", "1").unwrap();
        conn.execute_batch("DROP TABLE entries; DROP TABLE dir_stats;").unwrap();

        let (_pages, freelist) = IndexStore::db_page_stats(&conn).unwrap();
        assert!(freelist > 0, "expected a non-zero freelist after DROP, got {freelist}");
    }

    // Reopen: the schema mismatch must recreate the file fresh, not DROP on it.
    let store = IndexStore::open(&db_path).unwrap();

    assert_eq!(
        store.get_index_status().unwrap().schema_version.as_deref(),
        Some(SCHEMA_VERSION),
        "schema version should be re-stamped to current"
    );
    assert!(
        store.list_children(ROOT_ID).unwrap().is_empty(),
        "recreated DB should hold only the ROOT sentinel"
    );
    let (_pages, freelist) = IndexStore::db_page_stats(store.read_conn()).unwrap();
    assert_eq!(freelist, 0, "recreated file must have zero freelist (disk reclaimed)");
}

#[test]
fn corruption_recovery_deletes_and_recreates() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("corrupt.db");

    // Write garbage to simulate corruption
    std::fs::write(&db_path, b"this is not a sqlite database").unwrap();

    // open() should recover by deleting and recreating
    let store = IndexStore::open(&db_path).unwrap();
    let status = store.get_index_status().unwrap();
    assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
}

/// A DB that's momentarily locked by another connection must NEVER be deleted:
/// `open` retries and recovers the existing index. A real 6.9M-entry index costs
/// tens of minutes to rebuild, so losing one to a checkpoint-length write lock is
/// the failure mode this guards.
///
/// The induction is honest: a second connection holds `BEGIN EXCLUSIVE` for longer
/// than the 5 s `busy_timeout`, so the first `try_open` really does come back
/// `SQLITE_BUSY`. That's why this test takes ~6 s.
#[test]
fn busy_db_is_retried_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("busy.db");

    // Build an index worth keeping, and checkpoint it into the main DB file.
    {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        insert_entry(&conn, ROOT_ID, "precious.txt", false, Some(42));
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    }

    // Hold the write lock past `busy_timeout`, then release it.
    let holder_path = db_path.clone();
    let holder = std::thread::spawn(move || {
        let conn = IndexStore::open_write_connection(&holder_path).expect("holder connection");
        conn.execute_batch("BEGIN EXCLUSIVE").expect("hold the write lock");
        // allowed-test-sleep: holding the lock PAST the 5 s `busy_timeout` is the subject; the open
        // below must hit a real `SQLITE_BUSY` and recover rather than delete the index
        std::thread::sleep(std::time::Duration::from_millis(5_500));
        conn.execute_batch("COMMIT").expect("release the write lock");
    });

    let store = IndexStore::open(&db_path).expect("a contended open must recover, not fail");
    let children = store.list_children(ROOT_ID).unwrap();
    assert_eq!(
        children.len(),
        1,
        "the existing index must survive transient contention, got {children:?}"
    );

    holder.join().expect("holder thread");
}

/// An index DB we can't write to (a read-only volume, a permissions mishap) must
/// be left alone. Only corruption justifies throwing an index away; everything
/// else fails loudly so the caller can report it and the data stays recoverable.
#[cfg(unix)]
#[test]
fn unwritable_db_is_not_deleted_on_open_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("readonly.db");

    {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        insert_entry(&conn, ROOT_ID, "precious.txt", false, Some(42));
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    }

    std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o444)).unwrap();
    // The open may still succeed when the test runs as root (mode bits don't
    // apply); either way the entries must be there afterwards.
    let _ = IndexStore::open(&db_path);

    // SQLite mirrors the main file's mode onto any `-wal` / `-shm` it recreates,
    // so restore all three before reopening.
    for path in [
        db_path.clone(),
        db_path.with_extension("db-wal"),
        db_path.with_extension("db-shm"),
    ] {
        if path.exists() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
    }

    let store = IndexStore::open(&db_path).expect("reopen after restoring permissions");
    assert_eq!(
        store.list_children(ROOT_ID).unwrap().len(),
        1,
        "a non-corruption open failure must not delete the index"
    );
}

#[test]
fn db_file_size_returns_nonzero() {
    let (store, _dir) = open_temp_store();
    let size = store.db_file_size().unwrap();
    assert!(size > 0, "DB file should have nonzero size after creation");
}

/// The WAL/checkpoint cadence pragmas must actually be in effect on a fresh write
/// connection. These tame the fsync/checkpoint storm during the big aggregate
/// finalize (the most likely trigger of the mid-scan `SQLITE_IOERR`): a bounded
/// `wal_autocheckpoint` cuts implicit-checkpoint frequency ~4x vs the 1 000-page
/// default, and `journal_size_limit` caps the resting `-wal` file. Read-only
/// connections never commit or checkpoint, so the pragmas are gated to writers;
/// this asserts the writer path applies both.
#[test]
fn write_connection_applies_wal_cadence_pragmas() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    let autocheckpoint: i64 = conn
        .pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))
        .unwrap();
    assert_eq!(
        autocheckpoint, 4000,
        "wal_autocheckpoint must be the bounded 4 000-page threshold, not SQLite's 1 000-page default"
    );

    let journal_size_limit: i64 = conn
        .pragma_query_value(None, "journal_size_limit", |row| row.get(0))
        .unwrap();
    assert_eq!(
        journal_size_limit, 67_108_864,
        "journal_size_limit must cap the -wal file at 64 MiB, not the -1 (unlimited) default"
    );
}

/// A savepoint-wrapped store call that FAILS must leave the connection in
/// autocommit. `ROLLBACK TO <name>` undoes the work but leaves the savepoint —
/// and the implicit transaction it opened — in place, so without a matching
/// `RELEASE` one failed write parks the writer's connection in an open
/// transaction holding the write lock: every other connection then sees
/// `database is locked` forever, and the writer's own later writes never commit.
#[test]
fn a_failed_savepoint_call_leaves_the_connection_in_autocommit() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    // Reject one specific `dir_stats` write with a real SQLite failure.
    conn.execute_batch(
        "CREATE TRIGGER reject_ds BEFORE INSERT ON dir_stats WHEN NEW.entry_id = 42
         BEGIN SELECT RAISE(ABORT, 'nope'); END;",
    )
    .unwrap();

    let stats = [DirStatsById {
        entry_id: 42,
        recursive_logical_size: 1,
        recursive_physical_size: 1,
        recursive_file_count: 1,
        recursive_dir_count: 0,
        recursive_has_symlinks: false,
        min_subtree_epoch: 0,
    }];
    assert!(IndexStore::upsert_dir_stats_by_id(&conn, &stats).is_err());
    assert!(
        conn.is_autocommit(),
        "a failed savepoint must not park the connection in an open transaction"
    );
}

// ── Page-cache budget ────────────────────────────────────────────────

/// A read-only connection must open with the SMALLER page cache and the write
/// connection with the bigger one. Both are upper bounds drawn from the
/// process-wide slab (`crate::sqlite_util`), not reservations; the writer's is
/// larger because it holds a whole `wal_autocheckpoint` window of dirty pages.
/// `open` itself is a write path, so its `read_conn` field is NOT the
/// small-cache one.
#[test]
fn read_connections_get_a_smaller_page_cache_than_write_connections() {
    use crate::sqlite_util::{READ_PAGE_CACHE_KIB, WRITE_PAGE_CACHE_KIB, page_cache_kib};

    let (store, _dir) = open_temp_store();
    let write = IndexStore::open_write_connection(store.db_path()).expect("write conn");
    let read = IndexStore::open_read_connection(store.db_path()).expect("read conn");

    assert_eq!(page_cache_kib(&write), WRITE_PAGE_CACHE_KIB);
    assert_eq!(page_cache_kib(&read), READ_PAGE_CACHE_KIB);
}
