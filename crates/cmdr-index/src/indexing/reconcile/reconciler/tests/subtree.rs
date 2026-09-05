//! `reconcile_subtree`: diffing a directory tree against the DB. Adds, removes,
//! updates, nested discovery, type changes, and the coverage epochs it stamps.

use super::*;

// ── MustScanSubDirs uses reconcile, not destructive reinsert ──

/// MustScanSubDirs for a directory that exists in the DB with children and
/// on disk unchanged should preserve all children. reconcile_subtree diffs
/// the filesystem against the DB rather than deleting and reinserting.
/// Regression for 31df59e.
#[test]
fn must_scan_sub_dirs_preserves_existing_children() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Create a directory with children on disk
    let test_dir = non_excluded_tempdir();
    let sub_dir = test_dir.path().join("subdir");
    std::fs::create_dir(&sub_dir).unwrap();
    std::fs::write(sub_dir.join("child1.txt"), "aaa").unwrap();
    std::fs::write(sub_dir.join("child2.txt"), "bbb").unwrap();

    // Populate DB with the directory tree matching disk
    ensure_path_in_db(&db_path, &sub_dir.to_string_lossy(), &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let sub_id = store::resolve_path(&wconn, &sub_dir.to_string_lossy())
            .unwrap()
            .unwrap();

        let meta1 = std::fs::symlink_metadata(sub_dir.join("child1.txt")).unwrap();
        let snap1 = extract_metadata(&meta1, false, false);
        IndexStore::insert_entry_v2(
            &wconn,
            sub_id,
            "child1.txt",
            false,
            false,
            snap1.logical_size,
            snap1.logical_size,
            snap1.modified_at,
            None,
        )
        .unwrap();

        let meta2 = std::fs::symlink_metadata(sub_dir.join("child2.txt")).unwrap();
        let snap2 = extract_metadata(&meta2, false, false);
        IndexStore::insert_entry_v2(
            &wconn,
            sub_id,
            "child2.txt",
            false,
            false,
            snap2.logical_size,
            snap2.logical_size,
            snap2.modified_at,
            None,
        )
        .unwrap();
    }

    // Run reconcile_subtree (what MustScanSubDirs triggers)
    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(&sub_dir, &IndexPathSpace::root(), &conn, &writer, &cancelled, None);
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.added, 0, "no new entries expected");
    assert_eq!(summary.removed, 0, "no entries should be removed");

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify all children are still in the DB
    let store = IndexStore::open(&db_path).unwrap();
    let sub_id = store::resolve_path(store.read_conn(), &sub_dir.to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(sub_id).unwrap();
    assert_eq!(children.len(), 2, "both children should remain after reconcile");
    let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"child1.txt"));
    assert!(names.contains(&"child2.txt"));
}

// ── Subtree reconciliation tests ──────────────────────────────

#[test]
fn reconcile_new_file() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("new_file.txt");
    std::fs::write(&file_path, "hello reconcile").unwrap();

    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    );
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.added, 1);
    assert_eq!(summary.removed, 0);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let parent_str = test_dir.path().to_string_lossy().to_string();
    let parent_id = store::resolve_path(store.read_conn(), &parent_str).unwrap().unwrap();
    let entries = store.list_children(parent_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "new_file.txt");
    assert!(entries[0].logical_size.unwrap_or(0) > 0);
}

#[test]
fn reconcile_deleted_file() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();

    // Insert the test dir and a file entry into the DB, but don't create the file on disk
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_str = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(&wconn, &parent_str).unwrap().unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "ghost.txt",
            false,
            false,
            Some(42),
            Some(42),
            Some(1000),
            None,
        )
        .unwrap();
    }

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    );
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.removed, 1);
    assert_eq!(summary.added, 0);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let parent_str = test_dir.path().to_string_lossy().to_string();
    let parent_id = store::resolve_path(store.read_conn(), &parent_str).unwrap().unwrap();
    let entries = store.list_children(parent_id).unwrap();
    assert!(entries.is_empty(), "ghost entry should be removed from DB");
}

#[test]
fn reconcile_unchanged() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("stable.txt");
    std::fs::write(&file_path, "no changes").unwrap();

    // Insert the directory into the DB
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    // Get the file's actual metadata and insert a matching DB entry
    let meta = std::fs::symlink_metadata(&file_path).unwrap();
    let snap = extract_metadata(&meta, false, false);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_str = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(&wconn, &parent_str).unwrap().unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "stable.txt",
            false,
            false,
            snap.logical_size,
            snap.logical_size,
            snap.modified_at,
            None,
        )
        .unwrap();
    }

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    );
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.updated, 0);

    writer.shutdown();
}

#[test]
fn reconcile_modified_file() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("changed.txt");
    std::fs::write(&file_path, "original content").unwrap();

    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    // Insert DB entry with stale metadata (different size)
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_str = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(&wconn, &parent_str).unwrap().unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "changed.txt",
            false,
            false,
            Some(999),
            Some(999),
            Some(0),
            None,
        )
        .unwrap();
    }

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    );
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.updated, 1);
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify the DB entry was updated with real metadata
    let store = IndexStore::open(&db_path).unwrap();
    let parent_str = test_dir.path().to_string_lossy().to_string();
    let parent_id = store::resolve_path(store.read_conn(), &parent_str).unwrap().unwrap();
    let entries = store.list_children(parent_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_ne!(entries[0].logical_size, Some(999), "size should have been updated");
    assert_ne!(entries[0].modified_at, Some(0), "mtime should have been updated");
}

// ── Nested directory reconciliation tests ──────────────────────

/// reconcile_subtree with one new nested dir + child tests the flush+re-resolve
/// cycle: the reconciler must flush the new directory to the writer, then
/// re-resolve its ID before inserting the child.
#[test]
fn reconcile_subtree_new_nested_dir_with_child() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let new_dir = parent.join("new_dir");
    std::fs::create_dir(&new_dir).unwrap();
    std::fs::write(new_dir.join("child.txt"), "nested child").unwrap();

    // DB only knows about /parent/; new_dir and child.txt are unknown
    ensure_path_in_db(&db_path, &parent.to_string_lossy(), &writer);

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(&parent, &IndexPathSpace::root(), &conn, &writer, &cancelled, None);
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.added, 2, "new_dir and child.txt should both be added");
    assert_eq!(summary.removed, 0);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify both entries exist with correct parent relationships
    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &parent.to_string_lossy())
        .unwrap()
        .unwrap();
    let parent_children = store.list_children(parent_id).unwrap();
    assert_eq!(parent_children.len(), 1);
    assert_eq!(parent_children[0].name, "new_dir");
    assert!(parent_children[0].is_directory);

    let new_dir_id = store::resolve_path(store.read_conn(), &new_dir.to_string_lossy())
        .unwrap()
        .unwrap();
    let new_dir_children = store.list_children(new_dir_id).unwrap();
    assert_eq!(new_dir_children.len(), 1);
    assert_eq!(new_dir_children[0].name, "child.txt");
    assert!(!new_dir_children[0].is_directory);
}

/// Directory replaced by a file on disk: the old directory entry should become
/// a file entry and the old directory's children should be cleaned up.
///
/// This may reveal a latent bug: `reconcile_subtree` compares by normalized
/// name and detects that `is_directory` changed. When a dir becomes a file,
/// the reconciler deletes the old subtree before upserting the replacement,
/// preventing orphaned children.
#[test]
fn reconcile_subtree_dir_replaced_by_file() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().join("parent");
    std::fs::create_dir(&parent).unwrap();

    // On disk: /parent/item is now a regular file
    std::fs::write(parent.join("item"), "I am a file now").unwrap();

    // DB: /parent/item/ is a directory with a child
    ensure_path_in_db(&db_path, &parent.to_string_lossy(), &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_id = store::resolve_path(&wconn, &parent.to_string_lossy()).unwrap().unwrap();
        let item_id =
            IndexStore::insert_entry_v2(&wconn, parent_id, "item", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            item_id,
            "child.txt",
            false,
            false,
            Some(50),
            Some(50),
            None,
            None,
        )
        .unwrap();
    }

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(&parent, &IndexPathSpace::root(), &conn, &writer, &cancelled, None);
    assert!(result.is_ok());
    let summary = result.unwrap();

    // The reconciler should see "item" as matching by name, but changed.
    // It sends an UpsertEntryV2 with is_directory=false. That's 1 update.
    // The old child.txt is never visited because a file has no children to recurse into.
    assert_eq!(summary.updated, 1, "item should be updated (dir -> file)");

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &parent.to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert_eq!(children.len(), 1, "parent should have exactly one child (item)");
    assert_eq!(children[0].name, "item");

    let item_id = children[0].id;
    let item_children = store.list_children(item_id).unwrap();

    assert!(!children[0].is_directory, "item should now be a file, not a directory");
    assert!(
        item_children.is_empty(),
        "file entry should have no children (old directory's child.txt should be cleaned up)"
    );
}

/// reconcile_subtree with 3+ levels of new nested directories tests the
/// multi-level flush cycle: each BFS level must be flushed and re-resolved
/// before the next level's parents can be resolved.
#[test]
fn reconcile_subtree_deep_nested_dirs() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let root_dir = test_dir.path().join("root_dir");
    std::fs::create_dir(&root_dir).unwrap();

    // Create 3 levels of new dirs + a file: root_dir/a/b/c/file.txt
    let dir_a = root_dir.join("a");
    let dir_b = dir_a.join("b");
    let dir_c = dir_b.join("c");
    std::fs::create_dir_all(&dir_c).unwrap();
    std::fs::write(dir_c.join("file.txt"), "deep content").unwrap();

    // DB only knows about /root_dir/; everything inside is new
    ensure_path_in_db(&db_path, &root_dir.to_string_lossy(), &writer);

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(&root_dir, &IndexPathSpace::root(), &conn, &writer, &cancelled, None);
    assert!(result.is_ok());
    let summary = result.unwrap();
    assert_eq!(summary.added, 4, "dirs a, b, c and file.txt should all be added");
    assert_eq!(summary.removed, 0);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify the full path chain exists with correct parent->child relationships
    let store = IndexStore::open(&db_path).unwrap();

    let root_id = store::resolve_path(store.read_conn(), &root_dir.to_string_lossy())
        .unwrap()
        .unwrap();
    let root_children = store.list_children(root_id).unwrap();
    assert_eq!(root_children.len(), 1);
    assert_eq!(root_children[0].name, "a");
    assert!(root_children[0].is_directory);

    let a_id = store::resolve_path(store.read_conn(), &dir_a.to_string_lossy())
        .unwrap()
        .unwrap();
    let a_children = store.list_children(a_id).unwrap();
    assert_eq!(a_children.len(), 1);
    assert_eq!(a_children[0].name, "b");
    assert!(a_children[0].is_directory);

    let b_id = store::resolve_path(store.read_conn(), &dir_b.to_string_lossy())
        .unwrap()
        .unwrap();
    let b_children = store.list_children(b_id).unwrap();
    assert_eq!(b_children.len(), 1);
    assert_eq!(b_children[0].name, "c");
    assert!(b_children[0].is_directory);

    let c_id = store::resolve_path(store.read_conn(), &dir_c.to_string_lossy())
        .unwrap()
        .unwrap();
    let c_children = store.list_children(c_id).unwrap();
    assert_eq!(c_children.len(), 1);
    assert_eq!(c_children[0].name, "file.txt");
    assert!(!c_children[0].is_directory);
}

// ── Bug regression tests ────────────────────────────────────────

/// Bug 1: reconcile_subtree on a NEW directory (exists on disk, parent in
/// DB, but the directory itself NOT in DB) should create the directory entry
/// and index its children. Previously it returned early with added=0 because
/// resolve_path for the root returned None.
#[test]
fn reconcile_subtree_indexes_new_directory_not_in_db() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Create a directory with children on disk
    let test_dir = non_excluded_tempdir();
    let new_dir = test_dir.path().join("brand_new");
    std::fs::create_dir(&new_dir).unwrap();
    std::fs::write(new_dir.join("file1.txt"), "aaa").unwrap();
    std::fs::write(new_dir.join("file2.txt"), "bbb").unwrap();

    // Only the PARENT is in the DB; the new directory itself is NOT.
    // This simulates what happens when FSEvents fires must_scan_sub_dirs
    // for a newly copied/created directory.
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let cancelled = CancellationToken::new();
    let result = reconcile_subtree(&new_dir, &IndexPathSpace::root(), &conn, &writer, &cancelled, None);
    assert!(result.is_ok());
    let summary = result.unwrap();

    // The directory's children should be indexed
    assert!(
        summary.added >= 2,
        "expected at least 2 entries added, got {}",
        summary.added
    );

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify the children are in the DB
    let store = IndexStore::open(&db_path).unwrap();
    let new_dir_id = store::resolve_path(store.read_conn(), &new_dir.to_string_lossy())
        .unwrap()
        .expect("new directory should be in the DB after reconcile");
    let children = store.list_children(new_dir_id).unwrap();
    assert_eq!(children.len(), 2, "both child files should be indexed");
}

/// A reconcile-discovered subtree must be stamped `listed_epoch = current`
/// for every dir it lists (including empty ones), and ancestor coverage must
/// lift. Without the mark, the subtree stays `listed_epoch = 0` forever and
/// drags ancestors to incomplete — the exact local-live-path regression the coverage model
/// guards against.
#[test]
fn reconcile_subtree_marks_listed_dirs_at_current_epoch() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Stamp the volume's current epoch at 5.
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::update_meta(&wconn, "current_epoch", "5").unwrap();
    }

    // On disk: a new tree with a child dir (non-empty) and an empty dir.
    let test_dir = non_excluded_tempdir();
    let new_dir = test_dir.path().join("tree");
    std::fs::create_dir(&new_dir).unwrap();
    std::fs::create_dir(new_dir.join("sub")).unwrap();
    std::fs::write(new_dir.join("sub").join("f.txt"), "x").unwrap();
    std::fs::create_dir(new_dir.join("empty")).unwrap();

    // Only the parent of `tree` is in the DB (mimics must_scan_sub_dirs).
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let cancelled = CancellationToken::new();
    reconcile_subtree(&new_dir, &IndexPathSpace::root(), &conn, &writer, &cancelled, None).unwrap();
    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let rconn = store.read_conn();
    let resolve = |p: &Path| {
        store::resolve_path(rconn, &p.to_string_lossy())
            .unwrap()
            .unwrap_or_else(|| panic!("{} should be in DB", p.display()))
    };
    let tree_id = resolve(&new_dir);
    let sub_id = resolve(&new_dir.join("sub"));
    let empty_id = resolve(&new_dir.join("empty"));

    // Every listed dir (including the empty one) is stamped at epoch 5.
    for (label, id) in [("tree", tree_id), ("sub", sub_id), ("empty", empty_id)] {
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(rconn, id).unwrap(),
            Some(5),
            "{label} must be listed at the current epoch"
        );
    }

    // Coverage lifted: the whole reconciled subtree is complete at epoch 5.
    assert_eq!(
        IndexStore::get_dir_stats_by_id(rconn, tree_id)
            .unwrap()
            .unwrap()
            .min_subtree_epoch,
        5,
        "tree's min_subtree_epoch lifts to 5 (fully listed)"
    );
}
