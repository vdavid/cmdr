//! `idx_child_dirs`, the partial index over directory rows: every "child dirs of
//! X" query reads it instead of every child of X, and an index DB built before it
//! existed gains it on its next open.

use super::*;
use crate::indexing::read::coverage::CHILD_DIR_COVERAGE_SQL;
use crate::indexing::store::dir_stats::CHILD_DIRS_MIN_SUBTREE_EPOCH_SQL;
use crate::indexing::store::entries::{CHILD_DIR_IDS_AND_NAMES_SQL, child_directories_of_sql};

fn has_index(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

/// Each query here once read a folder's every child through `idx_parent_name_folded`
/// to keep its dirs: 60 ms for a 92,219-entry folder on a real index, against
/// ~10 µs off the partial index. A plan that stops naming it is that regression.
#[test]
fn child_dir_queries_are_served_by_the_partial_index() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let parent = insert_entry(&conn, ROOT_ID, "parent", true, None);
    for i in 0..200 {
        insert_entry(&conn, parent, &format!("file{i}"), false, Some(1));
    }
    for i in 0..5 {
        insert_entry(&conn, parent, &format!("dir{i}"), true, None);
    }

    for (name, sql) in [
        (
            "recompute_min_subtree_epoch",
            CHILD_DIRS_MIN_SUBTREE_EPOCH_SQL.to_string(),
        ),
        ("read_child_dir_coverage", CHILD_DIR_COVERAGE_SQL.to_string()),
        ("list_child_dir_ids_and_names", CHILD_DIR_IDS_AND_NAMES_SQL.to_string()),
        ("for_each_child_directory_of", child_directories_of_sql(3)),
    ] {
        let plan = explain_query_plan(&conn, &sql);
        assert!(
            plan.contains("idx_child_dirs"),
            "{name} isn't served by idx_child_dirs:\n{plan}"
        );
    }
}

/// The rollout path: an index DB from a build without the partial indexes
/// (`idx_child_dirs`, `idx_child_symlinks`) keeps its rows (no `SCHEMA_VERSION`
/// bump, so no rescan) and gains them on open.
#[test]
fn an_index_built_without_the_partial_indexes_gains_them_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("older.db");
    let kept = {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        let kept = insert_entry(&conn, ROOT_ID, "kept", true, None);
        conn.execute_batch("DROP INDEX IF EXISTS idx_child_dirs; DROP INDEX IF EXISTS idx_child_symlinks")
            .unwrap();
        assert!(!has_index(&conn, "idx_child_dirs"));
        assert!(!has_index(&conn, "idx_child_symlinks"));
        kept
    };

    let store = IndexStore::open(&db_path).unwrap();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    assert!(has_index(&conn, "idx_child_dirs"));
    assert!(has_index(&conn, "idx_child_symlinks"));
    assert_eq!(
        IndexStore::list_child_dir_ids_and_names(&conn, ROOT_ID).unwrap(),
        vec![(kept, "kept".to_string())]
    );
}
