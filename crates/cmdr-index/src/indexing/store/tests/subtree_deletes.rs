//! Deleting a directory and its subtree, including the post-order crash-safety
//! guard: interrupting a delete anywhere must never strand a row.

use super::*;

#[test]
fn delete_entry_and_subtree() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // Build tree: /a, /a/b.txt, /a/c, /a/c/d.txt
    let a_id = insert_entry(&write_conn, ROOT_ID, "a", true, None);
    let b_id = insert_entry(&write_conn, a_id, "b.txt", false, Some(10));
    let c_id = insert_entry(&write_conn, a_id, "c", true, None);
    insert_entry(&write_conn, c_id, "d.txt", false, Some(20));

    // Delete single entry
    IndexStore::delete_entry_by_id(&write_conn, b_id).unwrap();
    let children = store.list_children(a_id).unwrap();
    assert_eq!(children.len(), 1); // only c remains

    // Delete subtree
    IndexStore::delete_subtree_by_id(&write_conn, a_id).unwrap();
    let children = store.list_children(a_id).unwrap();
    assert!(children.is_empty());
    let root_children = store.list_children(ROOT_ID).unwrap();
    assert!(root_children.is_empty()); // /a itself is also gone
}

#[test]
fn delete_subtree_by_id_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Build tree: /a/b/c.txt
    let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None, None, None).unwrap();
    let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None, None, None).unwrap();
    let c = IndexStore::insert_entry_v2(&conn, b, "c.txt", false, false, Some(42), Some(42), None, None).unwrap();

    // Add dir_stats for a and b
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: a,
                recursive_logical_size: 42,
                recursive_physical_size: 42,
                recursive_file_count: 1,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
            DirStatsById {
                entry_id: b,
                recursive_logical_size: 42,
                recursive_physical_size: 42,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
        ],
    )
    .unwrap();

    // Delete subtree rooted at /a
    IndexStore::delete_subtree_by_id(&conn, a).unwrap();

    assert!(IndexStore::get_entry_by_id(&conn, a).unwrap().is_none());
    assert!(IndexStore::get_entry_by_id(&conn, b).unwrap().is_none());
    assert!(IndexStore::get_entry_by_id(&conn, c).unwrap().is_none());
    assert!(IndexStore::get_dir_stats_by_id(&conn, a).unwrap().is_none());
    assert!(IndexStore::get_dir_stats_by_id(&conn, b).unwrap().is_none());
}

/// Insert one dir plus one file per level, `breadth` wide and `depth` deep, under
/// `parent`. Returns how many rows landed.
fn seed_subtree(conn: &Connection, parent: i64, depth: u32, breadth: usize) -> u64 {
    if depth == 0 {
        return 0;
    }
    let mut rows = 0;
    for i in 0..breadth {
        let d = insert_entry(conn, parent, &format!("d{depth}-{i}"), true, None);
        insert_entry(conn, d, &format!("f{depth}-{i}.bin"), false, Some(7));
        rows += 2 + seed_subtree(conn, d, depth - 1, breadth);
    }
    rows
}

fn entry_count(conn: &Connection) -> u64 {
    conn.query_row("SELECT count(*) FROM entries", [], |row| row.get::<_, u64>(0))
        .expect("count entries")
}

fn orphan_count(conn: &Connection) -> usize {
    IndexStore::find_orphan_entries(conn).expect("find orphans").0.len()
}

/// **The crash-safety guard for every subtree delete.** A top-down delete severs
/// the tree at whatever point the process dies, so every row below the cut loses
/// its path to the root and NO later descent can ever reach it again — one
/// interrupted bulk delete on the author's QNAP left 9 793 362 rows that no later
/// pass could see or repair.
///
/// So the delete is post-order: a directory row goes only once its whole subtree
/// is gone. Checked over EVERY prefix of the deletion order, and each prefix must
/// still be completable by a plain re-run.
#[test]
fn interrupting_a_subtree_delete_never_strands_a_row() {
    let seed = |conn: &Connection| {
        let root = insert_entry(conn, ROOT_ID, "doomed", true, None);
        let under = seed_subtree(conn, root, 3, 2);
        let mine = insert_entry(conn, ROOT_ID, "photos", true, None);
        let user_rows = seed_subtree(conn, mine, 2, 2);
        assert_eq!(
            (under, user_rows),
            (28, 12),
            "the fixture's shape is load-bearing below"
        );
        (root, under)
    };

    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let total = {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        seed(&conn).1
    };

    for stop_after in 1..=total {
        let (_store, dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();
        let (root, under) = seed(&conn);
        let before = entry_count(&conn);

        let cut =
            IndexStore::delete_descendants_by_id_stopping_after(&conn, root, stop_after).expect("interrupted delete");
        assert_eq!(cut, stop_after, "the simulated interruption must stop where asked");
        assert_eq!(
            orphan_count(&conn),
            0,
            "interrupting at {stop_after}/{under} left rows unreachable from the root"
        );

        let rest = IndexStore::delete_descendants_by_id(&conn, root).expect("resume");
        assert_eq!(cut + rest, under, "a re-run must finish exactly the rows left over");
        assert_eq!(
            entry_count(&conn),
            before - under,
            "the resumed run must land on the same index an uninterrupted one would"
        );
    }
}
