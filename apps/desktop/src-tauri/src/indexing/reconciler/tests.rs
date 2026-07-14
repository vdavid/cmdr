use super::*;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::watcher::FsEventFlags;
use std::time::Duration;

fn make_event(path: &str, event_id: u64, flags: FsEventFlags) -> FsChangeEvent {
    FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags,
    }
}

fn created_file_flags() -> FsEventFlags {
    FsEventFlags {
        item_created: true,
        item_is_file: true,
        ..Default::default()
    }
}

fn removed_file_flags() -> FsEventFlags {
    FsEventFlags {
        item_removed: true,
        item_is_file: true,
        ..Default::default()
    }
}

fn modified_file_flags() -> FsEventFlags {
    FsEventFlags {
        item_modified: true,
        item_is_file: true,
        ..Default::default()
    }
}

fn created_dir_flags() -> FsEventFlags {
    FsEventFlags {
        item_created: true,
        item_is_dir: true,
        ..Default::default()
    }
}

fn removed_dir_flags() -> FsEventFlags {
    FsEventFlags {
        item_removed: true,
        item_is_dir: true,
        ..Default::default()
    }
}

fn history_done_flags() -> FsEventFlags {
    FsEventFlags {
        history_done: true,
        ..Default::default()
    }
}

// ── Reconciler buffer/replay tests ───────────────────────────────

#[test]
fn reconciler_starts_in_buffering_mode() {
    let reconciler = EventReconciler::new();
    assert!(reconciler.is_buffering());
    assert_eq!(reconciler.buffer_len(), 0);
}

#[test]
fn buffer_events_during_scan() {
    let mut reconciler = EventReconciler::new();

    reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
    reconciler.buffer_event(make_event("/test/b.txt", 20, modified_file_flags()));
    reconciler.buffer_event(make_event("/test/c.txt", 30, removed_file_flags()));

    assert_eq!(reconciler.buffer_len(), 3);
}

#[test]
fn switch_to_live_clears_buffer() {
    let mut reconciler = EventReconciler::new();

    reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
    reconciler.buffer_event(make_event("/test/b.txt", 20, created_file_flags()));

    reconciler.switch_to_live();

    assert!(!reconciler.is_buffering());
    assert_eq!(reconciler.buffer_len(), 0);
}

#[test]
fn events_not_buffered_in_live_mode() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    // In live mode, buffer_event is a no-op
    reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
    assert_eq!(reconciler.buffer_len(), 0);
}

// ── Event processing tests ───────────────────────────────────────

#[test]
fn excluded_paths_are_skipped() {
    // Use a platform-appropriate excluded path
    #[cfg(target_os = "macos")]
    let excluded_path = "/System/Volumes/VM/swapfile0";
    #[cfg(target_os = "linux")]
    let excluded_path = "/proc/1/status";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let excluded_path = "/dev/null";

    let event = make_event(excluded_path, 1, created_file_flags());
    let (writer, _dir, conn) = setup_test_writer();
    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_none());
    writer.shutdown();
}

#[test]
#[cfg(target_os = "macos")]
fn system_paths_without_firmlink_are_skipped() {
    // /System/foo paths that aren't firmlinked should be excluded
    let event = make_event("/System/Library/Frameworks/foo", 1, created_file_flags());
    let (writer, _dir, conn) = setup_test_writer();
    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_none());
    writer.shutdown();
}

#[test]
fn history_done_events_are_skipped() {
    let event = make_event("/test/file.txt", 1, history_done_flags());
    let (writer, _dir, conn) = setup_test_writer();
    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_none());
    writer.shutdown();
}

#[test]
fn compute_parent_path_cases() {
    assert_eq!(compute_parent_path("/Users/foo/bar.txt"), "/Users/foo");
    assert_eq!(compute_parent_path("/Users"), "/");
    assert_eq!(compute_parent_path("/"), "/");
}

#[tokio::test]
async fn must_scan_sub_dirs_queued() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (writer, _dir, _conn) = setup_test_writer();
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);

    // Should not have any pending rescans after starting one
    // (it was popped from the set and started)
    assert!(reconciler.pending_rescans.lock().unwrap().is_empty());
    assert!(reconciler.rescan_active.load(Ordering::Relaxed));

    writer.shutdown();
}

#[tokio::test]
async fn must_scan_sub_dirs_deduplication() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    // Mark rescan as active so new ones get queued
    reconciler.rescan_active.store(true, Ordering::Relaxed);

    let (writer, _dir, _conn) = setup_test_writer();
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/other"), &writer);

    // Deduplication: only 2 unique paths should be queued
    assert_eq!(reconciler.pending_rescans.lock().unwrap().len(), 2);

    writer.shutdown();
}

// ── Event processing with real files ────────────────────────────

#[test]
fn process_file_creation_writes_entry() {
    let (writer, dir, conn) = setup_test_writer();

    // Create a real file so stat() works (must be outside excluded paths)
    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("created.txt");
    std::fs::write(&file_path, "hello world").unwrap();

    // Pre-populate DB with the parent directory chain so resolve_path works.
    // In production, the full scan populates all directories before live events.
    let db_path = dir.path().join("test-reconciler.db");
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let event = make_event(&file_path.to_string_lossy(), 50, created_file_flags());

    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_some());

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify the entry was written to DB
    let store = IndexStore::open(&db_path).unwrap();
    let parent = test_dir.path().to_string_lossy().to_string();
    let parent_id = store::resolve_path(store.read_conn(), &parent).unwrap().unwrap();
    let entries = store.list_children(parent_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "created.txt");
    assert!(entries[0].logical_size.unwrap_or(0) > 0);
}

#[test]
fn process_file_removal_deletes_entry() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Pre-populate the parent dir and entry using integer-keyed inserts
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let gone_id =
            IndexStore::insert_entry_v2(&wconn, ROOT_ID, "gone", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            gone_id,
            "deleted.txt",
            false,
            false,
            Some(100),
            Some(100),
            None,
            None,
        )
        .unwrap();
    }

    let event = make_event("/gone/deleted.txt", 60, removed_file_flags());
    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_some());

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let gone_id = store::resolve_path(store.read_conn(), "/gone").unwrap().unwrap();
    let entries = store.list_children(gone_id).unwrap();
    assert!(entries.is_empty(), "deleted entry should be removed from DB");
}

#[test]
fn process_dir_creation_writes_entry_and_propagates() {
    let (writer, dir, conn) = setup_test_writer();

    // Create a real directory (must be outside excluded paths)
    let test_dir = non_excluded_tempdir();
    let new_dir = test_dir.path().join("newdir");
    std::fs::create_dir(&new_dir).unwrap();

    // Pre-populate DB with the parent directory chain
    let db_path = dir.path().join("test-reconciler.db");
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let event = make_event(&new_dir.to_string_lossy(), 70, created_dir_flags());

    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_some());

    // The affected paths should include both the parent and the new dir itself
    let paths = result.unwrap();
    assert!(!paths.is_empty());

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let parent = test_dir.path().to_string_lossy().to_string();
    let parent_id = store::resolve_path(store.read_conn(), &parent).unwrap().unwrap();
    let entries = store.list_children(parent_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].is_directory);
    assert_eq!(entries[0].name, "newdir");
}

#[test]
fn process_dir_removal_deletes_subtree() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Pre-populate with a directory subtree using integer-keyed inserts
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_id =
            IndexStore::insert_entry_v2(&wconn, ROOT_ID, "parent", true, false, None, None, None, None).unwrap();
        let removed_dir_id =
            IndexStore::insert_entry_v2(&wconn, parent_id, "removed_dir", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            removed_dir_id,
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

    let event = make_event("/parent/removed_dir", 80, removed_dir_flags());
    process_fs_event(&event, &conn, &writer, None);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), "/parent").unwrap().unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert!(children.is_empty(), "directory and its children should be deleted");
}

#[test]
fn process_nonexistent_file_treated_as_removal() {
    let (writer, _dir, conn) = setup_test_writer();

    // Event for a file that was created and immediately deleted
    // Use a path not under any excluded prefix (for example, /tmp/ is excluded on Linux)
    let event = make_event("/nonexistent_cmdr_test_dir/ghost_file.txt", 90, created_file_flags());
    let result = process_fs_event(&event, &conn, &writer, None);
    // Should still return Some (stat fails, treated as removal)
    assert!(result.is_some());

    writer.shutdown();
}

/// Removal event for a path that STILL EXISTS on disk should upsert, not delete.
/// This is the key regression test for the false-removal bug: FSEvents can deliver
/// item_removed for paths that were atomically swapped or had coalesced flags.
#[test]
fn removal_event_for_existing_path_upserts_instead_of_deleting() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Create a real file on disk (must be outside excluded paths)
    let test_dir = non_excluded_tempdir();
    let real_file = test_dir.path().join("still_here.txt");
    std::fs::write(&real_file, "I exist!").unwrap();

    // Pre-populate DB with the parent directory chain + the file
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_id = store::resolve_path(&wconn, &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "still_here.txt",
            false,
            false,
            Some(100),
            Some(100),
            None,
            None,
        )
        .unwrap();
    }

    // Send a removal event even though the file exists on disk
    let event = make_event(&real_file.to_string_lossy(), 99, removed_file_flags());
    process_fs_event(&event, &conn, &writer, None);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // The file should still be in the DB (upserted, not deleted)
    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert_eq!(
        children.len(),
        1,
        "file should still be in DB (removal was a false alarm)"
    );
    assert_eq!(children[0].name, "still_here.txt");
}

// ── Atomic swap: event with both item_removed AND item_created ──

/// When FSEvents delivers a single event with both item_removed=true and
/// item_created=true (atomic file swap), the file should be upserted, not
/// deleted. process_fs_event checks item_removed first, but handle_removal
/// stats the path: if the file exists on disk, it delegates to upsert.
#[test]
fn atomic_swap_event_upserts_existing_file() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("swapped.txt");
    std::fs::write(&file_path, "new content after swap").unwrap();

    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_id = store::resolve_path(&wconn, &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "swapped.txt",
            false,
            false,
            Some(50),
            Some(50),
            Some(1000),
            None,
        )
        .unwrap();
    }

    // Both item_removed and item_created set (atomic swap scenario)
    let flags = FsEventFlags {
        item_removed: true,
        item_created: true,
        item_is_file: true,
        ..Default::default()
    };
    let event = make_event(&file_path.to_string_lossy(), 120, flags);
    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_some());

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // The file should still be in the DB (upserted, not deleted)
    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert_eq!(children.len(), 1, "file should be upserted, not deleted (atomic swap)");
    assert_eq!(children[0].name, "swapped.txt");
}

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
    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(&sub_dir, &conn, &writer, &cancelled);
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

// ── False removal of a directory ──────────────────────────────

/// item_removed for a DIRECTORY that still exists on disk should upsert,
/// not delete. This is more damaging than the file case because
/// DeleteSubtreeById wipes the entire subtree. Regression for f0c225f.
#[test]
fn removal_event_for_existing_directory_upserts_not_deletes() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // Create a directory with a child on disk
    let test_dir = non_excluded_tempdir();
    let target_dir = test_dir.path().join("still_here");
    std::fs::create_dir(&target_dir).unwrap();
    std::fs::write(target_dir.join("precious.txt"), "don't delete me").unwrap();

    // Populate DB with the directory tree
    ensure_path_in_db(&db_path, &target_dir.to_string_lossy(), &writer);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let dir_id = store::resolve_path(&wconn, &target_dir.to_string_lossy())
            .unwrap()
            .unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            dir_id,
            "precious.txt",
            false,
            false,
            Some(100),
            Some(100),
            Some(1000),
            None,
        )
        .unwrap();
    }

    // Send a false removal event for the directory (item_is_dir)
    let flags = FsEventFlags {
        item_removed: true,
        item_is_dir: true,
        ..Default::default()
    };
    let event = make_event(&target_dir.to_string_lossy(), 150, flags);
    let result = process_fs_event(&event, &conn, &writer, None);
    assert!(result.is_some());

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // The directory should still be in the DB
    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let parent_children = store.list_children(parent_id).unwrap();
    assert_eq!(
        parent_children.len(),
        1,
        "directory should still exist in DB (false removal, stat-before-delete)"
    );
    assert_eq!(parent_children[0].name, "still_here");
    assert!(parent_children[0].is_directory);

    // The child should also still be in the DB (no subtree wipe)
    let dir_id = store::resolve_path(store.read_conn(), &target_dir.to_string_lossy())
        .unwrap()
        .unwrap();
    let dir_children = store.list_children(dir_id).unwrap();
    assert_eq!(
        dir_children.len(),
        1,
        "child file should survive (DeleteSubtreeById must not have been sent)"
    );
    assert_eq!(dir_children[0].name, "precious.txt");
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(&parent, &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(&parent, &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(&root_dir, &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    let result = reconcile_subtree(&new_dir, &conn, &writer, &cancelled);
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

    let cancelled = AtomicBool::new(false);
    reconcile_subtree(&new_dir, &conn, &writer, &cancelled).unwrap();
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

/// Bug 2: after a MustScanSubDirs rescan completes, pending queued rescans
/// should be started automatically. Previously the spawned task set
/// rescan_active=false but never called start_next_rescan, so queued paths
/// were abandoned unless a new must_scan_sub_dirs event happened to arrive.
#[tokio::test]
async fn queued_rescans_start_after_active_completes() {
    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (writer, _dir, _conn) = setup_test_writer();

    // Start a rescan for a nonexistent path (completes almost immediately
    // because reconcile_subtree returns early when root isn't in DB).
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/nonexistent_cmdr_test/first"), &writer);
    assert!(reconciler.rescan_active.load(Ordering::Relaxed));

    // Queue a second path while the first is active
    reconciler.queue_must_scan_sub_dirs(PathBuf::from("/nonexistent_cmdr_test/second"), &writer);
    assert_eq!(
        reconciler.pending_rescans.lock().unwrap().len(),
        1,
        "second path should be queued"
    );

    // Wait for the first rescan to complete (it should be near-instant since
    // the path doesn't exist in the DB).
    for _ in 0..100 {
        if !reconciler.rescan_active.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !reconciler.rescan_active.load(Ordering::Relaxed),
        "first rescan should have completed"
    );

    // Give the system a moment for the completion handler to start the next rescan
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The second queued rescan should have started automatically after
    // the first completed. Without the fix, it stays in pending_rescans
    // forever.
    let remaining = reconciler.pending_rescans.lock().unwrap().len();
    assert!(
        remaining == 0,
        "pending rescans should be drained after active rescan completes, \
             but {} remain",
        pluralize(remaining as u64, "path")
    );

    writer.shutdown();
}

// ── Replay tests ─────────────────────────────────────────────────

#[test]
fn replay_skips_events_at_or_before_scan_start() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("old.txt");
    std::fs::write(&file_path, "old").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 5, created_file_flags()));
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 10, created_file_flags()));

    let mut callback_called = false;
    let result = reconciler
        .replay(10, &conn, &writer, &mut |_| callback_called = true)
        .unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // All events at or before scan_start_event_id=10 are skipped
    assert_eq!(result, 10);
    assert!(!callback_called);

    // Nothing written to DB
    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert!(children.is_empty());
}

#[test]
fn replay_processes_events_after_scan_start() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("new.txt");
    std::fs::write(&file_path, "new content").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 20, created_file_flags()));

    let result = reconciler.replay(10, &conn, &writer, &mut |_| {}).unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    assert_eq!(result, 20);

    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
        .unwrap()
        .unwrap();
    let children = store.list_children(parent_id).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "new.txt");
}

#[test]
fn replay_sends_update_last_event_id() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_a = test_dir.path().join("a.txt");
    let file_b = test_dir.path().join("b.txt");
    std::fs::write(&file_a, "a").unwrap();
    std::fs::write(&file_b, "b").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_a.to_string_lossy(), 15, created_file_flags()));
    reconciler.buffer_event(make_event(&file_b.to_string_lossy(), 25, created_file_flags()));

    let result = reconciler.replay(10, &conn, &writer, &mut |_| {}).unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Returns the highest event_id
    assert_eq!(result, 25);

    // Verify last_event_id was persisted to the DB
    let store = IndexStore::open(&db_path).unwrap();
    let stored_id = IndexStore::get_meta(store.read_conn(), "last_event_id").unwrap();
    assert_eq!(stored_id, Some("25".to_string()));
}

#[test]
fn replay_calls_callback_with_affected_paths() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("notify.txt");
    std::fs::write(&file_path, "hi").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 20, created_file_flags()));

    let mut notified_paths: Vec<String> = Vec::new();
    reconciler
        .replay(10, &conn, &writer, &mut |paths| {
            notified_paths = paths;
        })
        .unwrap();

    writer.shutdown();

    assert!(!notified_paths.is_empty());
    // The parent directory should appear in affected paths
    let parent = test_dir.path().to_string_lossy().to_string();
    assert!(
        notified_paths.iter().any(|p| p == &parent),
        "expected parent dir in affected paths, got: {notified_paths:?}"
    );
}

#[test]
fn replay_empty_buffer_returns_scan_start_unchanged() {
    let (writer, _dir, conn) = setup_test_writer();

    let mut reconciler = EventReconciler::new();
    // No events buffered

    let mut callback_called = false;
    let result = reconciler
        .replay(42, &conn, &writer, &mut |_| callback_called = true)
        .unwrap();

    writer.shutdown();

    assert_eq!(result, 42);
    assert!(!callback_called);
}

#[test]
fn replay_all_events_before_scan_start_returns_unchanged() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let file_path = test_dir.path().join("stale.txt");
    std::fs::write(&file_path, "stale").unwrap();
    ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy(), &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 3, created_file_flags()));
    reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 7, modified_file_flags()));

    let mut callback_called = false;
    let result = reconciler
        .replay(100, &conn, &writer, &mut |_| callback_called = true)
        .unwrap();

    writer.shutdown();

    assert_eq!(result, 100);
    assert!(!callback_called);
}

// ── Test helpers ─────────────────────────────────────────────────

/// Set up a writer and a read connection for tests.
fn setup_test_writer() -> (IndexWriter, tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test-reconciler.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    let conn = IndexStore::open_write_connection(&db_path).expect("open WAL conn for reads");
    (writer, dir, conn)
}

/// Ensure all components of an absolute path exist in the DB as directory entries.
///
/// Walks from root downward, inserting each missing component. This simulates
/// what the full scan does in production: all directories are indexed before
/// live events arrive. Also syncs the writer's shared `next_id` counter.
fn ensure_path_in_db(db_path: &Path, abs_path: &str, writer: &IndexWriter) {
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    let components: Vec<&str> = abs_path
        .strip_prefix('/')
        .unwrap_or(abs_path)
        .split('/')
        .filter(|c| !c.is_empty())
        .collect();

    let mut current_id = ROOT_ID;
    for component in components {
        match IndexStore::resolve_component(&conn, current_id, component).unwrap() {
            Some(id) => current_id = id,
            None => {
                current_id =
                    IndexStore::insert_entry_v2(&conn, current_id, component, true, false, None, None, None, None)
                        .unwrap();
            }
        }
    }
    // Sync the writer's next_id counter with what we just inserted
    let db_next_id = IndexStore::get_next_id(&conn).unwrap();
    writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
}

/// Create a temp directory outside indexing-excluded paths.
/// On Linux, `/tmp/` is excluded from indexing; use the current directory instead.
fn non_excluded_tempdir() -> tempfile::TempDir {
    // Create in CWD instead of /tmp/ to avoid:
    // - Linux: /tmp/ is in EXCLUDED_PREFIXES
    // - macOS: /tmp is a symlink to /private/tmp, causing path mismatches with normalize_path() which
    //   resolves /tmp → /private/tmp
    tempfile::Builder::new()
        .prefix("cmdr_test_")
        .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .expect("tempdir in cwd")
}

// ── Live throttle integration (through the real reconciler + temp index) ──

/// Read one child file's logical size from the DB by name, `None` if absent.
fn db_child_size(db_path: &Path, parent: &str, name: &str) -> Option<u64> {
    let store = IndexStore::open(db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), parent).unwrap().unwrap();
    store
        .list_children(parent_id)
        .unwrap()
        .into_iter()
        .find(|e| e.name == name)
        .and_then(|e| e.logical_size)
}

/// Rapid sub-floor rewrites of ONE file collapse to a single index write within
/// the window (leading edge), and the trailing sweep applies the LAST-seen size.
/// This exercises the real live path: `process_live_event` → the throttle, then
/// `sweep_throttle`. Uses a short window so no real 60 s sleep is needed.
#[test]
fn live_throttle_collapses_rapid_rewrites_and_trailing_flushes_last_size() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    let file = test_dir.path().join("hot.log");
    ensure_path_in_db(&db_path, &parent, &writer);

    let window = Duration::from_millis(150);
    let mut reconciler = EventReconciler::new_with_throttle_window(window);
    reconciler.switch_to_live();
    let mut pending = HashSet::<String>::new();
    let path_str = file.to_string_lossy().to_string();

    // Leading edge: first change applies immediately (a normal one-off edit).
    std::fs::write(&file, vec![b'x'; 1_000]).unwrap();
    reconciler.process_live_event(
        &make_event(&path_str, 1, modified_file_flags()),
        &conn,
        &writer,
        &mut pending,
    );
    writer.flush_blocking().unwrap();
    assert_eq!(
        db_child_size(&db_path, &parent, "hot.log"),
        Some(1_000),
        "leading edge applied immediately"
    );

    // N rapid sub-floor rewrites within the window: all suppressed. The DB keeps
    // the leading size, proving N events collapse to the single leading write.
    let n = 50u64;
    for i in 0..n {
        let size = 1_000 + (i + 1) * 1_000; // grows by 1 KB each: always sub-floor
        std::fs::write(&file, vec![b'x'; size as usize]).unwrap();
        reconciler.process_live_event(
            &make_event(&path_str, 100 + i, modified_file_flags()),
            &conn,
            &writer,
            &mut pending,
        );
    }
    writer.flush_blocking().unwrap();
    let last_size = 1_000 + n * 1_000;
    assert_eq!(
        db_child_size(&db_path, &parent, "hot.log"),
        Some(1_000),
        "all {n} in-window rewrites suppressed; DB still shows the leading size (1 write, not {})",
        n + 1
    );

    // After the window, the trailing sweep applies the LAST-seen size (no re-stat).
    std::thread::sleep(window + Duration::from_millis(50));
    let affected = reconciler.sweep_throttle(&writer, Instant::now());
    assert!(
        !affected.is_empty(),
        "trailing flush surfaces ancestor paths for the UI"
    );
    writer.flush_blocking().unwrap();
    assert_eq!(
        db_child_size(&db_path, &parent, "hot.log"),
        Some(last_size),
        "trailing flush wrote the last-seen size"
    );

    writer.shutdown();
}

/// A significant jump (over the 2% + 512 KiB floor) applies immediately even
/// mid-window, through the real reconciler.
#[test]
fn live_throttle_significant_jump_applies_immediately() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    let file = test_dir.path().join("grow.bin");
    ensure_path_in_db(&db_path, &parent, &writer);

    let mut reconciler = EventReconciler::new_with_throttle_window(Duration::from_secs(60));
    reconciler.switch_to_live();
    let mut pending = HashSet::<String>::new();
    let path_str = file.to_string_lossy().to_string();

    // Leading edge at 1 KB.
    std::fs::write(&file, vec![b'x'; 1_000]).unwrap();
    reconciler.process_live_event(
        &make_event(&path_str, 1, modified_file_flags()),
        &conn,
        &writer,
        &mut pending,
    );
    writer.flush_blocking().unwrap();
    assert_eq!(db_child_size(&db_path, &parent, "grow.bin"), Some(1_000));

    // +2 MiB one step later, still well within the window: over the floor, so it
    // bypasses the throttle and lands in the DB with no sweep.
    let big = 1_000 + 2 * 1024 * 1024;
    std::fs::write(&file, vec![b'x'; big]).unwrap();
    reconciler.process_live_event(
        &make_event(&path_str, 2, modified_file_flags()),
        &conn,
        &writer,
        &mut pending,
    );
    writer.flush_blocking().unwrap();
    assert_eq!(
        db_child_size(&db_path, &parent, "grow.bin"),
        Some(big as u64),
        "significant jump bypassed the throttle mid-window"
    );

    writer.shutdown();
}
