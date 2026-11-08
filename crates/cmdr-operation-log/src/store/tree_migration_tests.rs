//! The v6 step (`migrate_v6_tree_summary`): an older journal keeps every row, its ops
//! read unbuilt, and the folder listings the item tree reads are index-served.

use super::migrations::read_schema_version;
use super::*;

/// A v5 file with one op of two rows, written with raw SQL (the v5 shape, not today's
/// writer), at `path`.
fn seed_v5(path: &Path) {
    let conn = cmdr_fs::sqlite_util::open(path).expect("open");
    run_migrations(&conn, &MIGRATIONS[..5]).expect("migrate to v5");
    conn.execute_batch(
        "INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (1, 'v', NULL, '', '');
         INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (2, 'v', 1, 'A', 'a');
         INSERT INTO operations (op_id, kind, initiator, execution_status, rollback_state, started_at, ended_at)
             VALUES ('old', 'trash', 'user', 'done', 'rollbackable', 1, 2);
         INSERT INTO operation_items
             (op_id, seq, entry_type, row_role, source_dir_id, source_name, source_name_folded, outcome)
             VALUES ('old', 0, 'dir', 'rollback_unit', 1, 'A', 'a', 'done'),
                    ('old', 1, 'file', 'search_only', 2, 'x.txt', 'x.txt', 'done');",
    )
    .expect("seed v5 rows");
}

#[test]
fn a_v5_journal_migrates_keeping_every_row_and_reads_unbuilt() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    seed_v5(&path);

    let store = OperationLogStore::open(&path, &|_| None).expect("migrating open");
    let conn = store.conn();
    assert_eq!(read_schema_version(conn).expect("version"), 6);

    let items = read_operation_items(conn, "old", 10).expect("items");
    assert_eq!(items.len(), 2, "every row survives the step");
    assert!(
        items.iter().all(|item| item.self_dir_id.is_none()),
        "an older DIR row has no own folder until a backfill interns it"
    );
    let (summary, generation): (Option<String>, i64) = conn
        .query_row(
            "SELECT tree_summary, outcome_generation FROM operations WHERE op_id = 'old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("op row");
    assert_eq!(summary, None, "an op journaled before the summary reads unbuilt");
    assert_eq!(generation, 0);
    let summary_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM operation_tree_dirs", [], |row| row.get(0))
        .expect("count");
    assert_eq!(summary_rows, 0, "the step builds no summary: that's on demand, per op");
    assert!(
        !tree_item_indexes_exist(conn).expect("lookup"),
        "the ladder runs on the app's main thread, so it never builds the journal-sized indexes"
    );
}

/// `OperationLog::open` queues the index build on the writer thread, after the ladder
/// and the reconcile, so the open itself pays nothing that scales with the journal.
#[test]
fn opening_the_journal_builds_the_folder_indexes_on_the_writer() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    seed_v5(&path);

    let log = crate::OperationLog::detached();
    log.open(&path, &|_| None).expect("open");
    log.writer().expect("writer").flush_blocking().expect("flush");
    let conn = open_read_connection(&path).expect("read connection");
    assert!(tree_item_indexes_exist(&conn).expect("lookup"));
}

/// A tree waiting on the indexes hears once when the writer has built them, and a
/// later ask hears at once, so the host can turn it into an event instead of a poll.
#[test]
fn the_registry_says_once_when_the_folder_indexes_exist() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    seed_v5(&path);
    let log = crate::OperationLog::detached();
    let heard = Arc::new(AtomicUsize::new(0));
    let count = |heard: &Arc<AtomicUsize>| {
        let heard = Arc::clone(heard);
        Box::new(move || {
            heard.fetch_add(1, Ordering::SeqCst);
        })
    };

    log.tree_backfills().when_indexes_ready(count(&heard));
    assert_eq!(
        heard.load(Ordering::SeqCst),
        0,
        "nothing is built before the journal opens"
    );

    log.open(&path, &|_| None).expect("open");
    log.writer().expect("writer").flush_blocking().expect("flush");
    assert_eq!(heard.load(Ordering::SeqCst), 1, "the build tells whoever waited, once");
    let conn = open_read_connection(&path).expect("read connection");
    assert!(
        tree_item_indexes_exist(&conn).expect("lookup"),
        "it says so only once they exist"
    );

    log.tree_backfills().when_indexes_ready(count(&heard));
    assert_eq!(heard.load(Ordering::SeqCst), 2, "an ask after the build hears at once");
}

fn plan(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).expect("explain");
    stmt.query_map(rusqlite::params!["op", 7, 100], |row| row.get::<_, String>(3))
        .expect("plan")
        .collect::<Result<_, _>>()
        .expect("rows")
}

/// "This folder's files of this op, in name order, from position k", on either side,
/// and "this folder's child folders in name order", each walk an index in order: no
/// table scan, and no temp b-tree sorting a folder per window.
#[test]
fn a_folders_listing_is_index_served_in_name_order() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = OperationLogStore::open(&operation_log_db_path(dir.path()), &|_| None).expect("open");
    build_tree_item_indexes(store.conn()).expect("indexes");
    let cases = [
        (
            "SELECT seq FROM operation_items WHERE op_id = ?1 AND source_dir_id = ?2 AND entry_type = 'file' \
             ORDER BY source_name_folded LIMIT 50 OFFSET ?3",
            "operation_items_source_dir",
        ),
        (
            "SELECT seq FROM operation_items WHERE op_id = ?1 AND dest_dir_id = ?2 AND entry_type = 'file' \
             ORDER BY dest_name_folded DESC LIMIT 50 OFFSET ?3",
            "operation_items_dest_dir",
        ),
        (
            "SELECT dir_id FROM operation_tree_dirs WHERE op_id = ?1 AND parent_dir_id = ?2 \
             ORDER BY name_folded LIMIT 50 OFFSET ?3",
            "operation_tree_dirs_children",
        ),
    ];
    for (sql, index) in cases {
        let steps = plan(store.conn(), sql);
        assert!(
            steps.iter().any(|s| s.contains(index)),
            "expected the index {index} in {sql}: {steps:?}"
        );
        for step in &steps {
            assert!(!step.starts_with("SCAN"), "a scan in {sql}: {steps:?}");
            assert!(!step.contains("TEMP B-TREE"), "a per-window sort in {sql}: {steps:?}");
        }
    }
}
