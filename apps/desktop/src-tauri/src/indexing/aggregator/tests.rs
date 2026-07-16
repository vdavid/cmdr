//! Tests for the dir-stats aggregation algorithm (bottom-up compute, subtree and
//! partial passes, backfill, topological sort). Extracted verbatim from the former
//! `aggregator.rs` `mod tests`; pure code movement.
use super::*;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};

/// Open a write connection to a temp DB with schema initialized.
fn open_temp_conn() -> (Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let store = IndexStore::open(&db_path).expect("failed to open store");
    let conn = IndexStore::open_write_connection(store.db_path()).expect("failed to open write conn");
    // Drop store so the read connection is closed; we only need the write conn for tests
    drop(store);
    (conn, dir)
}

/// Insert a batch of test entries using the v2 integer-keyed API.
fn insert_entries(conn: &Connection, entries: &[EntryRow]) {
    IndexStore::insert_entries_v2_batch(conn, entries).expect("insert failed");
}

fn make_dir(id: i64, parent_id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    }
}

fn make_file(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(size),
        physical_size: Some(size),
        modified_at: None,
        inode: None,
    }
}

fn make_symlink(id: i64, parent_id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: false,
        is_symlink: true,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    }
}

/// Get dir_stats by entry ID.
fn get_stats(conn: &Connection, entry_id: i64) -> Option<DirStatsById> {
    IndexStore::get_dir_stats_by_id(conn, entry_id).unwrap()
}

// ── compute_all_aggregates tests ─────────────────────────────────

#[test]
fn aggregate_simple_tree() {
    let (conn, _dir) = open_temp_conn();

    // Tree structure (root sentinel id=1 already exists):
    //   /root (id=2)
    //   /root/a.txt (id=3, 100 bytes)
    //   /root/b.txt (id=4, 200 bytes)
    //   /root/sub/ (id=5)
    //   /root/sub/c.txt (id=6, 50 bytes)
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "root"),
            make_file(3, 2, "a.txt", 100),
            make_file(4, 2, "b.txt", 200),
            make_dir(5, 2, "sub"),
            make_file(6, 5, "c.txt", 50),
        ],
    );

    let count = compute_all_aggregates(&conn).unwrap();
    assert_eq!(count, 3); // root sentinel + /root + /root/sub

    let sub_stats = get_stats(&conn, 5).unwrap();
    assert_eq!(sub_stats.recursive_logical_size, 50);
    assert_eq!(sub_stats.recursive_file_count, 1);
    assert_eq!(sub_stats.recursive_dir_count, 0);

    let root_dir_stats = get_stats(&conn, 2).unwrap();
    assert_eq!(root_dir_stats.recursive_logical_size, 350); // 100 + 200 + 50
    assert_eq!(root_dir_stats.recursive_file_count, 3);
    assert_eq!(root_dir_stats.recursive_dir_count, 1);

    // Root sentinel (id=1) should have stats summing all top-level entries
    let sentinel_stats = get_stats(&conn, ROOT_ID).unwrap();
    assert_eq!(sentinel_stats.recursive_logical_size, 350);
    assert_eq!(sentinel_stats.recursive_file_count, 3);
    assert_eq!(sentinel_stats.recursive_dir_count, 2); // /root + /root/sub
}

#[test]
fn aggregate_deep_tree() {
    let (conn, _dir) = open_temp_conn();

    // Tree: /a/b/c/d/file.txt (1000 bytes)
    // id=2: /a, id=3: /a/b, id=4: /a/b/c, id=5: /a/b/c/d, id=6: file.txt
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "a"),
            make_dir(3, 2, "b"),
            make_dir(4, 3, "c"),
            make_dir(5, 4, "d"),
            make_file(6, 5, "file.txt", 1000),
        ],
    );

    compute_all_aggregates(&conn).unwrap();

    // Each ancestor should have the file's size propagated up
    for &dir_id in &[5, 4, 3, 2] {
        let stats = get_stats(&conn, dir_id).unwrap();
        assert_eq!(stats.recursive_logical_size, 1000, "wrong size for id={dir_id}");
        assert_eq!(stats.recursive_file_count, 1, "wrong file count for id={dir_id}");
    }

    // Dir counts should increase as we go up
    assert_eq!(get_stats(&conn, 5).unwrap().recursive_dir_count, 0); // /a/b/c/d
    assert_eq!(get_stats(&conn, 4).unwrap().recursive_dir_count, 1); // /a/b/c
    assert_eq!(get_stats(&conn, 3).unwrap().recursive_dir_count, 2); // /a/b
    assert_eq!(get_stats(&conn, 2).unwrap().recursive_dir_count, 3); // /a
}

#[test]
fn aggregate_empty_db() {
    let (conn, _dir) = open_temp_conn();
    let count = compute_all_aggregates(&conn).unwrap();
    // Root sentinel exists but has no children, so it may or may not be counted.
    // With the integer-keyed schema, root sentinel is a real directory entry.
    // If no other entries exist, the root sentinel has 0 children -> count is 1 (just root).
    assert!(count <= 1);
}

#[test]
fn aggregate_dir_with_no_files() {
    let (conn, _dir) = open_temp_conn();

    insert_entries(&conn, &[make_dir(2, ROOT_ID, "empty")]);

    compute_all_aggregates(&conn).unwrap();

    let stats = get_stats(&conn, 2).unwrap();
    assert_eq!(stats.recursive_logical_size, 0);
    assert_eq!(stats.recursive_file_count, 0);
    assert_eq!(stats.recursive_dir_count, 0);
}

// ── min_subtree_epoch rollup tests ───────────────────────────────

/// The core honest-coverage rollup: a listed parent with a listed-EMPTY child
/// and an UNlisted child. After aggregation:
/// - the unlisted child rolls to `min_subtree_epoch == 0` (unknown),
/// - the listed-empty child rolls to its own epoch `> 0` with size 0 (genuinely
///   empty, not unknown),
/// - the parent absorbs the unlisted child to `min_subtree_epoch == 0`
///   (incomplete — its subtree has an unknown corner).
#[test]
fn aggregate_min_subtree_epoch_absorbs_unlisted() {
    let (conn, _dir) = open_temp_conn();

    // /parent (id=2): listed at epoch 5
    //   /parent/empty (id=3): listed at epoch 5, no children → genuinely empty
    //   /parent/unlisted (id=4): never listed (listed_epoch stays 0)
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "parent"),
            make_dir(3, 2, "empty"),
            make_dir(4, 2, "unlisted"),
        ],
    );
    // Stamp the parent and the empty child as listed at epoch 5; leave the
    // unlisted child (and root sentinel) at 0.
    IndexStore::mark_dirs_listed(&conn, &[2, 3], 5).unwrap();

    compute_all_aggregates(&conn).unwrap();

    let empty = get_stats(&conn, 3).unwrap();
    assert_eq!(
        empty.min_subtree_epoch, 5,
        "a listed-empty dir keeps its own epoch (>0)"
    );
    assert_eq!(empty.recursive_logical_size, 0, "and reports genuine 0 bytes");

    let unlisted = get_stats(&conn, 4).unwrap();
    assert_eq!(unlisted.min_subtree_epoch, 0, "an unlisted dir is unknown (0)");

    let parent = get_stats(&conn, 2).unwrap();
    assert_eq!(
        parent.min_subtree_epoch, 0,
        "a parent with any unlisted descendant is incomplete (0)"
    );
}

/// A fully-listed subtree (every dir marked at the same epoch) rolls every
/// ancestor's `min_subtree_epoch` up to that epoch (`> 0`, exact).
#[test]
fn aggregate_min_subtree_epoch_all_listed_is_exact() {
    let (conn, _dir) = open_temp_conn();

    // /a/b/c with a file under c; all dirs listed at epoch 3.
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "a"),
            make_dir(3, 2, "b"),
            make_dir(4, 3, "c"),
            make_file(5, 4, "f.txt", 100),
        ],
    );
    IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 2, 3, 4], 3).unwrap();

    compute_all_aggregates(&conn).unwrap();

    for &dir_id in &[ROOT_ID, 2, 3, 4] {
        assert_eq!(
            get_stats(&conn, dir_id).unwrap().min_subtree_epoch,
            3,
            "fully-listed dir id={dir_id} should be exact at epoch 3"
        );
    }
}

// ── compute_subtree_aggregates tests ─────────────────────────────

#[test]
fn subtree_aggregation() {
    let (conn, _dir) = open_temp_conn();

    // Two separate subtrees under root:
    //   /a (id=2) with /a/f.txt (id=3, 100 bytes)
    //   /b (id=4) with /b/sub (id=5) with /b/sub/g.txt (id=6, 200 bytes)
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "a"),
            make_file(3, 2, "f.txt", 100),
            make_dir(4, ROOT_ID, "b"),
            make_dir(5, 4, "sub"),
            make_file(6, 5, "g.txt", 200),
        ],
    );

    // Only aggregate /b subtree (id 4)
    let count = compute_subtree_aggregates(&conn, 4).unwrap();
    assert_eq!(count, 2); // /b and /b/sub

    // /b/sub should have stats
    let sub_stats = get_stats(&conn, 5).unwrap();
    assert_eq!(sub_stats.recursive_logical_size, 200);

    // /b should have stats
    let b_stats = get_stats(&conn, 4).unwrap();
    assert_eq!(b_stats.recursive_logical_size, 200);
    assert_eq!(b_stats.recursive_file_count, 1);
    assert_eq!(b_stats.recursive_dir_count, 1);

    // /a should NOT have stats (not in subtree)
    assert!(get_stats(&conn, 2).is_none());
}

/// A subtree aggregate sets `min_subtree_epoch` from the scoped `listed_epoch`
/// read (not left at the `0` default): a fully-listed subtree is exact, and an
/// unlisted dir inside it drags its ancestors within the subtree to `0`.
#[test]
fn subtree_aggregation_sets_min_subtree_epoch() {
    let (conn, _dir) = open_temp_conn();

    // /b (id=2): listed at epoch 4
    //   /b/listed (id=3): listed at epoch 4 → exact
    //   /b/unlisted (id=4): never listed → unknown, drags /b to 0
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "b"),
            make_dir(3, 2, "listed"),
            make_dir(4, 2, "unlisted"),
        ],
    );
    IndexStore::mark_dirs_listed(&conn, &[2, 3], 4).unwrap();

    let count = compute_subtree_aggregates(&conn, 2).unwrap();
    assert_eq!(count, 3); // /b, /b/listed, /b/unlisted

    assert_eq!(
        get_stats(&conn, 3).unwrap().min_subtree_epoch,
        4,
        "listed leaf is exact"
    );
    assert_eq!(
        get_stats(&conn, 4).unwrap().min_subtree_epoch,
        0,
        "unlisted leaf is unknown"
    );
    assert_eq!(
        get_stats(&conn, 2).unwrap().min_subtree_epoch,
        0,
        "subtree root absorbs the unlisted child"
    );
}

#[test]
fn subtree_aggregation_nonexistent_root() {
    let (conn, _dir) = open_temp_conn();
    // An id with no directory subtree (never inserted) yields zero rows.
    let count = compute_subtree_aggregates(&conn, 999).unwrap();
    assert_eq!(count, 0);
}

// ── backfill_missing_dir_stats tests ─────────────────────────────

#[test]
fn backfill_fills_missing_stats() {
    let (conn, _dir) = open_temp_conn();

    // Tree: /a (id=2) with /a/f.txt (id=3, 100 bytes), /a/sub (id=4), /a/sub/g.txt (id=5, 200)
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "a"),
            make_file(3, 2, "f.txt", 100),
            make_dir(4, 2, "sub"),
            make_file(5, 4, "g.txt", 200),
        ],
    );

    // Only compute stats for /a/sub (id=4): leave /a (id=2) and root (id=1) missing
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: 4,
            recursive_logical_size: 200,
            recursive_physical_size: 200,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        }],
    )
    .unwrap();

    // Backfill should fill in root sentinel (id=1) and /a (id=2)
    let count = backfill_missing_dir_stats(&conn).unwrap().backfilled;
    assert_eq!(count, 2); // root sentinel + /a

    // /a should now have correct recursive stats
    let a_stats = get_stats(&conn, 2).unwrap();
    assert_eq!(a_stats.recursive_logical_size, 300); // 100 + 200
    assert_eq!(a_stats.recursive_file_count, 2);
    assert_eq!(a_stats.recursive_dir_count, 1);

    // Root sentinel should also be correct
    let root_stats = get_stats(&conn, ROOT_ID).unwrap();
    assert_eq!(root_stats.recursive_logical_size, 300);
}

/// Backfill sets `min_subtree_epoch` on the dirs it fills (not left at the `0`
/// default): a fully-listed subtree backfills to its epoch, exact.
#[test]
fn backfill_sets_min_subtree_epoch() {
    let (conn, _dir) = open_temp_conn();

    // /a (id=2) with /a/f.txt (id=3) and /a/sub (id=4) with /a/sub/g.txt (id=5).
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "a"),
            make_file(3, 2, "f.txt", 100),
            make_dir(4, 2, "sub"),
            make_file(5, 4, "g.txt", 200),
        ],
    );
    IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 2, 4], 6).unwrap();

    // Seed only /a/sub's stats (with its honest epoch); leave root + /a missing.
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: 4,
            recursive_logical_size: 200,
            recursive_physical_size: 200,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 6,
        }],
    )
    .unwrap();

    let count = backfill_missing_dir_stats(&conn).unwrap().backfilled;
    assert_eq!(count, 2); // root sentinel + /a

    assert_eq!(
        get_stats(&conn, 2).unwrap().min_subtree_epoch,
        6,
        "/a backfills to its fully-listed epoch (exact)"
    );
    assert_eq!(get_stats(&conn, ROOT_ID).unwrap().min_subtree_epoch, 6);
}

#[test]
fn backfill_noop_when_all_stats_present() {
    let (conn, _dir) = open_temp_conn();

    insert_entries(&conn, &[make_dir(2, ROOT_ID, "a"), make_file(3, 2, "f.txt", 100)]);

    // Compute all stats first
    compute_all_aggregates(&conn).unwrap();

    // Backfill should find nothing to do
    let count = backfill_missing_dir_stats(&conn).unwrap().backfilled;
    assert_eq!(count, 0);
}

// ── topological sort test ────────────────────────────────────────

// ── recursive_has_symlinks tests ─────────────────────────────────

#[test]
fn aggregate_propagates_recursive_has_symlinks() {
    let (conn, _dir) = open_temp_conn();

    // Tree:
    //   /grand (id=2)
    //   /grand/parent (id=3)
    //   /grand/parent/leaf (id=4)
    //   /grand/parent/leaf/link (id=5, symlink)
    //   /grand/sibling (id=6), no symlinks
    //   /grand/sibling/file.txt (id=7, 100 bytes)
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "grand"),
            make_dir(3, 2, "parent"),
            make_dir(4, 3, "leaf"),
            make_symlink(5, 4, "link"),
            make_dir(6, 2, "sibling"),
            make_file(7, 6, "file.txt", 100),
        ],
    );

    compute_all_aggregates(&conn).unwrap();

    // The symlink leaf has the flag (direct child symlink)
    assert!(
        get_stats(&conn, 4).unwrap().recursive_has_symlinks,
        "leaf has direct symlink"
    );
    // Parent gets it via subdir aggregation
    assert!(
        get_stats(&conn, 3).unwrap().recursive_has_symlinks,
        "parent should propagate up"
    );
    // Grand gets it from /grand/parent
    assert!(
        get_stats(&conn, 2).unwrap().recursive_has_symlinks,
        "grand should propagate up"
    );
    // Sibling has no symlinks anywhere in its subtree
    assert!(
        !get_stats(&conn, 6).unwrap().recursive_has_symlinks,
        "sibling without symlinks should be false"
    );
    // Root sentinel inherits from /grand
    assert!(get_stats(&conn, ROOT_ID).unwrap().recursive_has_symlinks);
}

#[test]
fn aggregate_no_symlinks_anywhere() {
    let (conn, _dir) = open_temp_conn();
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "a"),
            make_file(3, 2, "f.txt", 100),
            make_dir(4, 2, "b"),
            make_file(5, 4, "g.txt", 200),
        ],
    );
    compute_all_aggregates(&conn).unwrap();
    assert!(!get_stats(&conn, 2).unwrap().recursive_has_symlinks);
    assert!(!get_stats(&conn, 4).unwrap().recursive_has_symlinks);
    assert!(!get_stats(&conn, ROOT_ID).unwrap().recursive_has_symlinks);
}

#[test]
fn aggregate_dir_with_only_symlinks_has_zero_size() {
    let (conn, _dir) = open_temp_conn();
    // /links contains only two symlinks: total size 0, but flag is true
    insert_entries(
        &conn,
        &[
            make_dir(2, ROOT_ID, "links"),
            make_symlink(3, 2, "a"),
            make_symlink(4, 2, "b"),
        ],
    );
    compute_all_aggregates(&conn).unwrap();
    let stats = get_stats(&conn, 2).unwrap();
    assert_eq!(stats.recursive_logical_size, 0, "symlink-only folder reports 0 bytes");
    assert_eq!(stats.recursive_file_count, 2, "symlinks count as files");
    assert!(stats.recursive_has_symlinks, "flag must be set");
}

#[test]
fn topological_sort_produces_bottom_up_order() {
    // Tree: 1 -> 2 -> 3 -> 4 (root -> a -> b -> c)
    let entries = vec![(1, 0), (2, 1), (3, 2), (4, 3)];
    let sorted = topological_sort_bottom_up(&entries);
    // Leaf (4) should come before its ancestors
    let pos_4 = sorted.iter().position(|&id| id == 4).unwrap();
    let pos_3 = sorted.iter().position(|&id| id == 3).unwrap();
    let pos_2 = sorted.iter().position(|&id| id == 2).unwrap();
    let pos_1 = sorted.iter().position(|&id| id == 1).unwrap();
    assert!(pos_4 < pos_3);
    assert!(pos_3 < pos_2);
    assert!(pos_2 < pos_1);
}

// ── compute_partial_aggregates_sql tests ─────────────────────────
//
// The SQL-sourced partial path reads committed `entries` / `dir_stats` rows
// (no accumulator maps), so these build the tree with a direct
// `insert_entries_v2_batch` — which leaves `dir_stats` empty exactly like a
// reconcile before the final aggregate.

/// A hot path writes ONLY the hot dir + its DIRECT CHILDREN, with correct
/// partial recursive sizes; deeper / unrelated / ancestor dirs get no rows; and a
/// subsequent full aggregate fills everything to byte-exact totals.
#[test]
fn sql_partial_writes_hot_dir_and_direct_children_only() {
    let (conn, _dir) = open_temp_conn();

    // /a/b/{c -> f1(100), deep -> f2(200)}, plus an unrelated /x/y -> f3(500).
    insert_entries(
        &conn,
        &[
            make_dir(10, ROOT_ID, "a"),
            make_dir(11, 10, "b"),
            make_dir(12, 11, "c"),
            make_file(13, 12, "f1.dat", 100),
            make_dir(14, 11, "deep"),
            make_file(15, 14, "f2.dat", 200),
            make_dir(16, ROOT_ID, "x"),
            make_dir(17, 16, "y"),
            make_file(18, 17, "f3.dat", 500),
        ],
    );

    let stats = compute_partial_aggregates_sql(&conn, &["/a/b".to_string()], 100_000).unwrap();
    assert_eq!(stats.hot_paths_resolved, 1);
    assert_eq!(stats.rows_written, 3, "hot dir /a/b plus its two direct child dirs");

    // Hot dir /a/b: recursive over its whole subtree.
    let b = get_stats(&conn, 11).expect("hot dir gets a row");
    assert_eq!(b.recursive_logical_size, 300);
    assert_eq!(b.recursive_file_count, 2);
    assert_eq!(b.recursive_dir_count, 2, "/a/b has /c and /deep beneath it");
    // Direct children get rows with their own subtree totals.
    assert_eq!(
        get_stats(&conn, 12).expect("direct child /c").recursive_logical_size,
        100
    );
    assert_eq!(
        get_stats(&conn, 14).expect("direct child /deep").recursive_logical_size,
        200
    );

    // The hot dir's ANCESTOR /a is not a direct child, so it gets no row.
    assert!(get_stats(&conn, 10).is_none(), "ancestor /a must not be written");
    // Unrelated dirs (siblings and deeper) get no rows.
    assert!(get_stats(&conn, 16).is_none(), "unrelated /x must not be written");
    assert!(get_stats(&conn, 17).is_none(), "unrelated /x/y must not be written");

    // The final full aggregate fills every dir with byte-exact totals.
    compute_all_aggregates(&conn).unwrap();
    let a = get_stats(&conn, 10).expect("final pass writes /a");
    assert_eq!(a.recursive_logical_size, 300);
    assert_eq!(a.recursive_file_count, 2);
    assert_eq!(a.recursive_dir_count, 3, "/a has b, c, deep beneath it");
    let x = get_stats(&conn, 16).expect("final pass writes /x");
    assert_eq!(x.recursive_logical_size, 500);
    // The hot dir's partial total already matched the final total (idempotent).
    assert_eq!(get_stats(&conn, 11).unwrap().recursive_logical_size, 300);
}

/// The conservative cap skips a hot dir whose CURRENT `dir_stats` subtree counts
/// exceed `cap` (the final aggregate fills it later); a small subtree is scoped
/// normally. Pins the writer-thread stability guard.
#[test]
fn sql_partial_cap_skips_oversized_subtree() {
    let (conn, _dir) = open_temp_conn();

    // /a/b with one 50-byte file. /a/b is the hot dir.
    insert_entries(
        &conn,
        &[
            make_dir(10, ROOT_ID, "a"),
            make_dir(11, 10, "b"),
            make_file(12, 11, "f.dat", 50),
        ],
    );

    // Stand in for a prior scan's stats on /a/b: a large recursive_file_count, so
    // the cheap cap check sees an "oversized" subtree.
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_file_count: 50,
            recursive_dir_count: 0,
            ..Default::default()
        }],
    )
    .unwrap();

    // cap = 10 < 50: the hot dir is skipped, its stale row untouched.
    let skipped = compute_partial_aggregates_sql(&conn, &["/a/b".to_string()], 10).unwrap();
    assert_eq!(skipped.hot_paths_resolved, 1, "the path still resolved");
    assert_eq!(skipped.rows_written, 0, "an oversized hot dir writes nothing");
    let stale = get_stats(&conn, 11).unwrap();
    assert_eq!(stale.recursive_file_count, 50, "the stale row is left as-is");
    assert_eq!(stale.recursive_logical_size, 0, "not recomputed");

    // cap = 1000 > 50: the small subtree is scoped and the row recomputed.
    let scoped = compute_partial_aggregates_sql(&conn, &["/a/b".to_string()], 1_000).unwrap();
    assert!(scoped.rows_written >= 1, "a within-cap hot dir is written");
    let fresh = get_stats(&conn, 11).unwrap();
    assert_eq!(fresh.recursive_file_count, 1, "recomputed from committed entries");
    assert_eq!(fresh.recursive_logical_size, 50);
}

/// When a pane's parent AND child are both hot, only the DEEPEST is scoped: the
/// ancestor is dropped so its (potentially whole-tree) subtree CTE never runs.
#[test]
fn sql_partial_collapses_parent_and_child_to_deepest() {
    let (conn, _dir) = open_temp_conn();

    // /a/b/c with a file under c. Both /a and /a/b/c are "visible".
    insert_entries(
        &conn,
        &[
            make_dir(10, ROOT_ID, "a"),
            make_dir(11, 10, "b"),
            make_dir(12, 11, "c"),
            make_file(13, 12, "f.dat", 70),
        ],
    );

    let stats = compute_partial_aggregates_sql(&conn, &["/a".to_string(), "/a/b/c".to_string()], 100_000).unwrap();
    // Only the deepest path (`/a/b/c`) is scoped; `/a` is dropped as an ancestor.
    assert_eq!(stats.hot_paths_resolved, 1, "only the deepest hot path resolves");
    let c = get_stats(&conn, 12).expect("the deepest hot dir is written");
    assert_eq!(c.recursive_logical_size, 70);
    // The dropped ancestor /a is NOT written (its big subtree CTE never ran).
    assert!(
        get_stats(&conn, 10).is_none(),
        "the ancestor hot path is collapsed away"
    );
}

// ── Property-based tests ─────────────────────────────────────────
//
// The function takes a slice of `(id, parent_id)` pairs and returns a
// bottom-up ordering. The properties we pin here are the ones the callers
// (`compute_all_aggregates`, the incremental aggregator paths) rely on:
// each id appears at most once, descendants come before ancestors, and
// pathological inputs (cycles, duplicates, large random forests) don't
// panic or hang.

mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    /// Generate an acyclic forest of `n` nodes where every node's parent
    /// is either `0` (forest root, treated as "out of set") or one of
    /// the already-emitted nodes. Returns `Vec<(id, parent_id)>` with
    /// ids in `1..=n`.
    fn forest_strategy(max_nodes: usize) -> impl Strategy<Value = Vec<(i64, i64)>> {
        (1usize..=max_nodes).prop_flat_map(|n| {
            // For each node i (1-indexed), pick a parent index in 0..i.
            // Index 0 maps to parent_id 0 (sentinel for "no parent in set").
            let parent_picks: Vec<_> = (0..n).map(|i| 0usize..=i).collect();
            parent_picks.prop_map(move |picks| {
                picks
                    .into_iter()
                    .enumerate()
                    .map(|(i, pick)| {
                        let id = (i as i64) + 1;
                        let parent_id = pick as i64; // 0 means "no parent in set"
                        (id, parent_id)
                    })
                    .collect::<Vec<_>>()
            })
        })
    }

    proptest! {
        /// For any acyclic forest, the sort emits each node exactly once
        /// and places every descendant before its ancestor.
        #[test]
        fn forest_descendant_before_ancestor(entries in forest_strategy(40)) {
            let sorted = topological_sort_bottom_up(&entries);

            // Every id appears exactly once.
            let unique_ids: HashSet<i64> = entries.iter().map(|&(id, _)| id).collect();
            prop_assert_eq!(sorted.len(), unique_ids.len(), "output length must match unique input ids");
            let sorted_set: HashSet<i64> = sorted.iter().copied().collect();
            prop_assert_eq!(&sorted_set, &unique_ids, "output must be a permutation of the input ids");

            // Build position map and parent map.
            let pos: HashMap<i64, usize> =
                sorted.iter().enumerate().map(|(i, &id)| (id, i)).collect();
            let parent_of: HashMap<i64, i64> =
                entries.iter().copied().collect();

            // For every (child, parent_in_set) pair, child must come first.
            for &(id, pid) in &entries {
                if pid != 0 && unique_ids.contains(&pid) {
                    let cp = pos[&id];
                    let pp = pos[&pid];
                    prop_assert!(
                        cp < pp,
                        "descendant {} at pos {} must come before ancestor {} at pos {}",
                        id, cp, pid, pp
                    );
                }
                // Transitively the same must hold for any ancestor,
                // chain through `parent_of` to be sure.
                let mut cursor = pid;
                let mut hops = 0;
                while cursor != 0 && unique_ids.contains(&cursor) && hops < entries.len() + 1 {
                    prop_assert!(
                        pos[&id] < pos[&cursor],
                        "descendant {} must come before transitive ancestor {}",
                        id, cursor
                    );
                    cursor = *parent_of.get(&cursor).unwrap_or(&0);
                    hops += 1;
                }
            }
        }

        /// Robustness: the function must not panic and must produce a
        /// subset of unique input ids, even on arbitrary (possibly
        /// cyclic, duplicate, or detached) (id, parent_id) lists.
        #[test]
        fn arbitrary_input_is_panic_free_and_subset(
            entries in proptest::collection::vec((-50i64..50i64, -50i64..50i64), 0..30)
        ) {
            let sorted = topological_sort_bottom_up(&entries);
            let unique_ids: HashSet<i64> = entries.iter().map(|&(id, _)| id).collect();

            // No duplicates in output.
            let sorted_set: HashSet<i64> = sorted.iter().copied().collect();
            prop_assert_eq!(sorted.len(), sorted_set.len(), "output must have no duplicate ids");

            // Output is a subset of unique input ids.
            for id in &sorted_set {
                prop_assert!(unique_ids.contains(id), "output id {} must come from input", id);
            }
        }
    }
}
