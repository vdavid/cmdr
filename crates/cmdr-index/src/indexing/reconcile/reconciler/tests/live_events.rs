//! The live FS-event path: which events are skipped, what a create/remove/modify
//! writes, false removals that must not delete, the mount-rooted path strip, and
//! the size throttle that collapses rapid rewrites.

use super::*;

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
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
    assert!(result.is_none());
    writer.shutdown();
}

#[test]
#[cfg(target_os = "macos")]
fn system_paths_without_firmlink_are_skipped() {
    // /System/foo paths that aren't firmlinked should be excluded
    let event = make_event("/System/Library/Frameworks/foo", 1, created_file_flags());
    let (writer, _dir, conn) = setup_test_writer();
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
    assert!(result.is_none());
    writer.shutdown();
}

#[test]
fn history_done_events_are_skipped() {
    let event = make_event("/test/file.txt", 1, history_done_flags());
    let (writer, _dir, conn) = setup_test_writer();
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
    assert!(result.is_none());
    writer.shutdown();
}

// ── Event processing with real files ────────────────────────────

/// A live change reports ONLY the directory whose own listing changed — its
/// immediate parent — never the ancestor chain up to `/`.
///
/// Pre-fix this returned every ancestor, and the importance scheduler (which reads
/// the same set off the dir-changed bus and expands each entry DOWNWARD into its
/// whole subtree) saw `/Users` in every batch and rescored ~90,000 folders a minute
/// for a two-folder change. The recursive-size refresh set is rebuilt from these
/// origins by `with_ancestor_closure` at the drain point instead.
#[test]
fn a_change_reports_only_the_dir_whose_listing_changed() {
    let (writer, dir, conn) = setup_test_writer();

    let test_dir = non_excluded_tempdir();
    let deep = test_dir.path().join("a/b/c");
    std::fs::create_dir_all(&deep).unwrap();
    let file_path = deep.join("built.o");
    std::fs::write(&file_path, "hello").unwrap();

    let db_path = dir.path().join("test-reconciler.db");
    ensure_path_in_db(&db_path, &deep.to_string_lossy(), &writer);

    let space = IndexPathSpace::root();
    let event = make_event(&file_path.to_string_lossy(), 80, created_file_flags());
    let origins = process_fs_event(&event, &space, &conn, &writer, None, &mut None).expect("the event is processed");
    writer.shutdown();

    assert_eq!(
        origins,
        vec![space.absolute(&deep.to_string_lossy())],
        "only the file's own directory changed its listing; its ancestors did not"
    );
}

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

    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
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
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
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

    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
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
    process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);

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
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
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
    process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);

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
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
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
    let result = process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
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

// ── Mount-rooted path space (external drive) ─────────────────────

/// A live create under a MOUNT-ROOTED index resolves its parent only after the
/// mount-relative strip: with `root` space (the pre-strip behavior) the
/// mount-absolute parent is walked from `ROOT_ID` and misses, so the change is
/// dropped; with the drive's `mount_rooted` space the mount root strips to `/` and
/// the parent resolves, so the file is indexed. Pins that the strip at the
/// `resolve_path` argument is load-bearing (dropping it silently drops live events).
#[test]
fn live_create_under_mount_rooted_index_resolves_via_strip() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    // A tempdir stands in for the drive's mount root (`/Volumes/X`). The index is
    // MOUNT-ROOTED: `ROOT_ID` IS the mount, and a scanned "sub" dir hangs off
    // `ROOT_ID` by its mount-relative name — NOT the absolute path chain a `/`-rooted
    // index would seed.
    let mount = non_excluded_tempdir();
    let mount_root = mount.path().to_string_lossy().to_string();
    let sub_id = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let id = IndexStore::insert_entry_v2(&wconn, ROOT_ID, "sub", true, false, None, None, None, None).unwrap();
        let next = IndexStore::get_next_id(&wconn).unwrap();
        writer.next_id().fetch_max(next, Ordering::Relaxed);
        id
    };

    // A real file on disk at `<mount>/sub/new.txt`, and the absolute FS event for it.
    std::fs::create_dir(mount.path().join("sub")).unwrap();
    let file_path = mount.path().join("sub/new.txt");
    std::fs::write(&file_path, "hello mount").unwrap();
    let event = make_event(&file_path.to_string_lossy(), 50, created_file_flags());

    // `root` space (pre-strip): the absolute parent `<mount>/sub` is walked from
    // `ROOT_ID` (which holds only "sub") and misses, so the create is dropped.
    process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
    writer.flush_blocking().unwrap();
    assert_eq!(
        IndexStore::list_children_on(sub_id, &conn).unwrap().len(),
        0,
        "root space can't resolve the mount-absolute parent, so the create is dropped (the pre-fix miss)"
    );

    // `mount_rooted` space: `<mount>/sub` strips to `/sub`, resolves to `sub_id`, upserts.
    let space = IndexPathSpace::mount_rooted(mount_root);
    process_fs_event(&event, &space, &conn, &writer, None, &mut None);
    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let children = store.list_children(sub_id).unwrap();
    assert_eq!(
        children.len(),
        1,
        "the mount-relative strip resolves the parent, so the file is indexed"
    );
    assert_eq!(children[0].name, "new.txt");
    assert!(children[0].logical_size.unwrap_or(0) > 0);
}

/// A live delete under a MOUNT-ROOTED index resolves the target only after the
/// strip: `root` space misses (leaving the stale row), `mount_rooted` space
/// resolves and removes it.
#[test]
fn live_delete_under_mount_rooted_index_resolves_via_strip() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let mount = non_excluded_tempdir();
    let mount_root = mount.path().to_string_lossy().to_string();
    let (sub_id, _gone_id) = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let sub = IndexStore::insert_entry_v2(&wconn, ROOT_ID, "sub", true, false, None, None, None, None).unwrap();
        let gone =
            IndexStore::insert_entry_v2(&wconn, sub, "gone.txt", false, false, Some(9), Some(9), None, None).unwrap();
        let next = IndexStore::get_next_id(&wconn).unwrap();
        writer.next_id().fetch_max(next, Ordering::Relaxed);
        (sub, gone)
    };

    // The file is truly gone on disk (never created under the mount), so a removal
    // event should delete the row — but only once the path resolves.
    let gone_abs = mount.path().join("sub/gone.txt");
    let event = make_event(&gone_abs.to_string_lossy(), 60, removed_file_flags());

    // `root` space misses the mount-absolute path, so the stale row survives.
    process_fs_event(&event, &IndexPathSpace::root(), &conn, &writer, None, &mut None);
    writer.flush_blocking().unwrap();
    assert_eq!(
        IndexStore::list_children_on(sub_id, &conn).unwrap().len(),
        1,
        "root space can't resolve the mount-absolute path, so the stale row survives (the pre-fix miss)"
    );

    // `mount_rooted` space strips and resolves, so the delete lands.
    let space = IndexPathSpace::mount_rooted(mount_root);
    process_fs_event(&event, &space, &conn, &writer, None, &mut None);
    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    assert!(
        store.list_children(sub_id).unwrap().is_empty(),
        "the mount-relative strip resolves the target, so the delete removes the row"
    );
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
/// `sweep_throttle`.
///
/// **Gotcha**: the window has to outlast the rewrite loop by a wide margin, and the
/// sweep has to be driven by an injected `now`. The live path stamps `last_applied_at`
/// from the wall clock, so with a short window a slow machine finishes the loop AFTER
/// the window elapses, one mid-loop rewrite re-applies as a fresh leading edge, and the
/// suppression assertion fails on timing rather than on behavior. A production-length
/// window plus a synthetic sweep instant makes both halves independent of how long the
/// loop takes.
#[test]
fn live_throttle_collapses_rapid_rewrites_and_trailing_flushes_last_size() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    let file = test_dir.path().join("hot.log");
    ensure_path_in_db(&db_path, &parent, &writer);

    let window = Duration::from_secs(60);
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
    // `sweep_throttle` takes its clock as an argument, so a `now` past the window is
    // what "the window elapsed" means here: no sleeping, and no dependence on how long
    // the rewrites above took (this `now` is computed after them, so it's past the
    // window whatever the loop cost).
    let past_the_window = Instant::now() + window + Duration::from_secs(1);
    let affected = reconciler.sweep_throttle(&writer, past_the_window);
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
