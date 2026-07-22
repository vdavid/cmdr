//! Inode rename pre-pass (`detect_renames_by_inode`), removal-storm coalescing,
//! and the `process_live_batch` end-to-end rename, plus their shared fixtures.

use super::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::indexing::event_loop::live::detect_renames_by_inode;
use crate::indexing::reconciler::EventReconciler;
use crate::indexing::store::{DirStatsById, ROOT_ID};
use crate::indexing::writer::IndexWriter;

/// Create a temp dir under CARGO_MANIFEST_DIR (Linux's `should_exclude`
/// blocks `/tmp/`, but we don't actually scan here (the path just has
/// to exist on disk so `stat` succeeds and gives us a real inode).
fn rename_test_tempdir() -> tempfile::TempDir {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    tempfile::Builder::new()
        .prefix("cmdr-rename-test-")
        .tempdir_in(base)
        .expect("create temp dir")
}

/// Spawn a writer + DB and return everything callers need.
fn rename_test_setup() -> (IndexWriter, PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create db temp dir");
    let db_path = dir.path().join("rename-test.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    (writer, db_path, dir)
}

/// Insert each path component as a directory entry, returning the deepest
/// dir's entry_id. Mirrors `verifier::tests::ensure_path_in_db`.
fn insert_path_chain(db_path: &Path, path: &Path, writer: &IndexWriter) -> i64 {
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    let path_str = path.to_string_lossy();
    let components: Vec<&str> = path_str.split('/').filter(|c| !c.is_empty()).collect();
    let mut parent_id = ROOT_ID;
    for component in components {
        parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
            Ok(Some(id)) => id,
            _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None).unwrap(),
        };
    }
    let db_next_id = IndexStore::get_next_id(&conn).unwrap();
    writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    parent_id
}

/// Flags for a removed FILE (gone from disk → the reconciler would delete it).
fn removed_file() -> watcher::FsEventFlags {
    watcher::FsEventFlags {
        item_removed: true,
        item_is_file: true,
        ..Default::default()
    }
}

/// Flags for a removed DIRECTORY.
fn removed_dir() -> watcher::FsEventFlags {
    watcher::FsEventFlags {
        item_removed: true,
        item_is_dir: true,
        ..Default::default()
    }
}

/// Seed a directory chain plus `n` file rows under its deepest dir; returns the
/// deepest dir's entry id. The synthetic paths don't exist on disk, so a removal
/// event for one stats-fails and takes the delete path (what the storm coalesces).
fn seed_files_under(db_path: &Path, base: &str, n: usize, writer: &IndexWriter) -> i64 {
    let dir_id = insert_path_chain(db_path, Path::new(base), writer);
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    for i in 0..n {
        IndexStore::insert_entry_v2(
            &conn,
            dir_id,
            &format!("item{i}.dat"),
            false,
            false,
            Some(1),
            Some(1),
            None,
            None,
        )
        .unwrap();
    }
    let db_next_id = IndexStore::get_next_id(&conn).unwrap();
    writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    dir_id
}

/// Run `process_live_batch` inside a multi-thread runtime (its flushes use
/// `block_in_place`), returning the writer's `(DeleteEntryById, DeleteSubtreeById)`
/// counts after a final flush.
fn run_live_batch(
    pending_events: &mut HashMap<String, watcher::FsChangeEvent>,
    reconciler: &mut EventReconciler,
    writer: &IndexWriter,
    db_path: &Path,
) -> (u64, u64) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let mut pending_paths = HashSet::new();
        process_live_batch(
            pending_events,
            reconciler,
            &IndexPathSpace::root(),
            &conn,
            writer,
            &mut pending_paths,
            &mut crate::indexing::watch::churn_monitor::ChurnObserver::disabled(),
        );
    });
    writer.flush_blocking().unwrap();
    writer.delete_counts()
}

// ── Removal-storm coalescing (root cause 7) ──────────────────────

/// A removal burst over one prefix coalesces into ONE subtree rescan at the
/// group's deepest common ancestor, and every per-file removal is DROPPED (zero
/// `DeleteEntryById`). Pre-fix, the seeded rows fired N per-file deletes.
#[test]
fn removal_storm_coalesces_into_one_rescan_and_drops_per_file_deletes() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    // Base at depth 8 so files (depth 9) share one depth-8 grouping prefix.
    let base = "/Users/cmdrstorm/ws/p1/p2/p3/deep/bulk";
    let n = storm::REMOVAL_STORM_THRESHOLD + 1;
    seed_files_under(&db_path, base, n, &writer);

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    for i in 0..n {
        let p = format!("{base}/item{i}.dat");
        pending_events.insert(p.clone(), make_event(&p, 100 + i as u64, removed_file()));
    }

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();
    // Pre-set rescan_active so the queued anchor STAYS in the set (deterministic
    // RED trick: no rescan spawns, so the pending-set assertion is stable).
    reconciler.set_rescan_active_for_test(true);

    let (del_entry, del_subtree) = run_live_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    let queued = reconciler.pending_rescans_snapshot();
    assert_eq!(
        queued,
        vec![PathBuf::from(base)],
        "one rescan at the group's deepest common ancestor"
    );
    assert_eq!(del_entry, 0, "every storm removal is dropped, not deleted per-file");
    assert_eq!(del_subtree, 0);

    writer.shutdown();
}

/// Below the threshold, removals process per-file (no coalescing, no rescan).
#[test]
fn below_threshold_removals_process_per_file() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    let base = "/Users/cmdrstorm/ws2/p1/p2/p3/deep/bulk";
    let n = 5usize;
    seed_files_under(&db_path, base, n, &writer);

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    for i in 0..n {
        let p = format!("{base}/item{i}.dat");
        pending_events.insert(p.clone(), make_event(&p, 200 + i as u64, removed_file()));
    }

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (del_entry, _del_subtree) = run_live_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert!(
        reconciler.pending_rescans_snapshot().is_empty(),
        "no rescan below the threshold"
    );
    assert_eq!(del_entry, n as u64, "each below-threshold removal deletes per-file");

    writer.shutdown();
}

/// Parent-first ordering: a dir removal and its file children in ONE batch become
/// a single `DeleteSubtreeById` (the dir sorts first, its subtree delete lands,
/// then the children resolve to nothing and skip) — not N `DeleteEntryById`.
#[test]
fn parent_first_sort_collapses_children_into_one_subtree_delete() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    let base = "/Users/cmdrstorm/ws3/p1/p2/sub";
    let n = 5usize;
    seed_files_under(&db_path, base, n, &writer);

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    // Children (files) first in the map; the sort must reorder the dir ahead.
    for i in 0..n {
        let p = format!("{base}/item{i}.dat");
        pending_events.insert(p.clone(), make_event(&p, 300 + i as u64, removed_file()));
    }
    pending_events.insert(base.to_string(), make_event(base, 400, removed_dir()));

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    let (del_entry, del_subtree) = run_live_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert_eq!(del_subtree, 1, "the dir sorts first → one subtree delete");
    assert_eq!(
        del_entry, 0,
        "children resolve to nothing after the subtree delete and skip"
    );

    writer.shutdown();
}

/// The scope's OWN removal event is never dropped (it must take the cheap
/// `DeleteSubtreeById` path), and a dropped strict-descendant re-queues the
/// anchor into `pending_rescans` so a tail batch can't strand stale rows.
#[test]
fn scope_root_removal_survives_and_descendant_requeues_the_anchor() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    let scope = "/Users/cmdrstorm/ws4/p1/p2/scope";
    seed_files_under(&db_path, scope, 1, &writer);

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();
    // An active rescan already covers `scope`: seed the set + mark active so the
    // drop rule sees the scope, and no new rescan spawns.
    reconciler.set_rescan_active_for_test(true);
    reconciler.insert_pending_rescan_for_test(PathBuf::from(scope));

    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    // The scope's own rmdir (kept) plus a strict-descendant unlink (dropped).
    pending_events.insert(scope.to_string(), make_event(scope, 500, removed_dir()));
    let child = format!("{scope}/item0.dat");
    pending_events.insert(child.clone(), make_event(&child, 501, removed_file()));

    let (del_entry, del_subtree) = run_live_batch(&mut pending_events, &mut reconciler, &writer, &db_path);

    assert_eq!(
        del_subtree, 1,
        "the scope's own removal takes the cheap subtree-delete path"
    );
    assert_eq!(del_entry, 0, "the strict descendant is dropped, not deleted per-file");
    assert!(
        reconciler.pending_rescans_snapshot().contains(&PathBuf::from(scope)),
        "a dropped descendant re-queues the anchor"
    );

    writer.shutdown();
}

fn renamed_event(path: &str, event_id: u64) -> watcher::FsChangeEvent {
    make_event(
        path,
        event_id,
        watcher::FsEventFlags {
            item_renamed: true,
            item_is_dir: true,
            ..Default::default()
        },
    )
}

/// Same-parent rename: dir created on disk under a known parent. The DB
/// has an entry under the same parent at a *different* name with the
/// dir's inode pre-populated. The pre-pass should rename the row in
/// place, preserving its `dir_stats`.
#[test]
fn detect_renames_by_inode_same_parent_uses_move_and_preserves_stats() {
    let fs_root = rename_test_tempdir();
    let new_dir_path = fs_root.path().join("Bar");
    std::fs::create_dir(&new_dir_path).expect("create renamed dir");

    let inode =
        std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&new_dir_path).expect("stat renamed dir"));

    let (writer, db_path, _db_dir) = rename_test_setup();
    let parent_id = insert_path_chain(&db_path, fs_root.path(), &writer);

    // Insert the "old name" entry with the renamed dir's inode and pre-populate
    // its dir_stats. This is what the pre-pass should preserve.
    let foo_id = {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let id =
            IndexStore::insert_entry_v2(&conn, parent_id, "Foo", true, false, None, None, None, Some(inode)).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: id,
                recursive_logical_size: 12_345,
                recursive_physical_size: 12_345,
                recursive_file_count: 9,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
        id
    };

    let mut events = vec![(
        new_dir_path.to_string_lossy().to_string(),
        renamed_event(&new_dir_path.to_string_lossy(), 100),
    )];
    let mut pending_paths = HashSet::new();
    let mut max_event_id = 0u64;

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let handled = detect_renames_by_inode(
        &mut events,
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &mut pending_paths,
        &mut max_event_id,
    );
    writer.flush_blocking().unwrap();

    assert_eq!(handled, 1, "should detect the rename and emit one MoveEntryV2");
    assert_eq!(events.len(), 0, "matched event should be removed from the batch");
    assert_eq!(max_event_id, 100);
    assert!(pending_paths.contains(&fs_root.path().to_string_lossy().to_string()));

    let read_conn = IndexStore::open_write_connection(&db_path).unwrap();
    let entry = IndexStore::get_entry_by_id(&read_conn, foo_id).unwrap().unwrap();
    assert_eq!(entry.name, "Bar", "row should be renamed in place");
    assert_eq!(entry.parent_id, parent_id);

    let stats = IndexStore::get_dir_stats_by_id(&read_conn, foo_id).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 12_345, "dir_stats preserved");
    assert_eq!(stats.recursive_file_count, 9);

    writer.shutdown();
}

/// Cross-parent move: the inode lives in a new parent on disk, but the
/// DB has it under a different parent. The pre-pass should issue a
/// `MoveEntryV2` that propagates the moved subtree's totals from the
/// old ancestor chain to the new one.
#[test]
fn detect_renames_by_inode_cross_parent_propagates_deltas() {
    let fs_root = rename_test_tempdir();
    let dir_a = fs_root.path().join("A");
    let dir_b = fs_root.path().join("B");
    std::fs::create_dir(&dir_a).unwrap();
    std::fs::create_dir(&dir_b).unwrap();
    let new_dir_path = dir_b.join("D");
    std::fs::create_dir(&new_dir_path).unwrap();

    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&new_dir_path).expect("stat new dir"));

    let (writer, db_path, _db_dir) = rename_test_setup();
    let _root_id = insert_path_chain(&db_path, fs_root.path(), &writer);
    let dir_a_id = insert_path_chain(&db_path, &dir_a, &writer);
    let dir_b_id = insert_path_chain(&db_path, &dir_b, &writer);

    // Pre-populate stats so we can observe the propagation deltas.
    // A starts with the moved dir's contribution, B starts empty.
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: dir_a_id,
                recursive_logical_size: 2048,
                recursive_physical_size: 4096,
                recursive_file_count: 3,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
            DirStatsById {
                entry_id: dir_b_id,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
        ],
    )
    .unwrap();

    // Insert D under A (the OLD location) with the inode of B/D and pre-populated stats.
    let d_id = IndexStore::insert_entry_v2(&conn, dir_a_id, "D", true, false, None, None, None, Some(inode)).unwrap();
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: d_id,
            recursive_logical_size: 2048,
            recursive_physical_size: 4096,
            recursive_file_count: 3,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        }],
    )
    .unwrap();
    drop(conn);

    let mut events = vec![(
        new_dir_path.to_string_lossy().to_string(),
        renamed_event(&new_dir_path.to_string_lossy(), 200),
    )];
    let mut pending_paths = HashSet::new();
    let mut max_event_id = 0u64;

    let read_conn = IndexStore::open_write_connection(&db_path).unwrap();
    let handled = detect_renames_by_inode(
        &mut events,
        &IndexPathSpace::root(),
        &read_conn,
        &writer,
        &mut pending_paths,
        &mut max_event_id,
    );
    writer.flush_blocking().unwrap();

    assert_eq!(handled, 1);
    assert_eq!(events.len(), 0);

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let d = IndexStore::get_entry_by_id(&conn, d_id).unwrap().unwrap();
    assert_eq!(d.parent_id, dir_b_id, "D should now live under B");

    let a_stats = IndexStore::get_dir_stats_by_id(&conn, dir_a_id).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0, "A loses the moved subtree's bytes");
    assert_eq!(a_stats.recursive_file_count, 0);
    assert_eq!(a_stats.recursive_dir_count, 0);

    let b_stats = IndexStore::get_dir_stats_by_id(&conn, dir_b_id).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 2048);
    assert_eq!(b_stats.recursive_file_count, 3);
    assert_eq!(b_stats.recursive_dir_count, 1, "B gains D itself in its dir count");

    writer.shutdown();
}

/// Inode-unstable filesystems (exFAT/FAT) report a different inode for
/// the renamed dir than the DB has. The pre-pass leaves the event in
/// the batch so Phase 2 falls through to today's create/delete path,
/// no regression from current behaviour.
#[test]
fn detect_renames_by_inode_no_match_keeps_event() {
    let fs_root = rename_test_tempdir();
    let new_dir_path = fs_root.path().join("Bar");
    std::fs::create_dir(&new_dir_path).unwrap();

    let (writer, db_path, _db_dir) = rename_test_setup();
    let parent_id = insert_path_chain(&db_path, fs_root.path(), &writer);

    // Old DB entry with an inode that doesn't match what's on disk.
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::insert_entry_v2(&conn, parent_id, "Foo", true, false, None, None, None, Some(99_999_999)).unwrap();
    drop(conn);

    let mut events = vec![(
        new_dir_path.to_string_lossy().to_string(),
        renamed_event(&new_dir_path.to_string_lossy(), 50),
    )];
    let mut pending_paths = HashSet::new();
    let mut max_event_id = 0u64;

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let handled = detect_renames_by_inode(
        &mut events,
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &mut pending_paths,
        &mut max_event_id,
    );

    assert_eq!(handled, 0, "no inode match → no rename detected");
    assert_eq!(events.len(), 1, "event remains for Phase 2");
    assert_eq!(max_event_id, 0, "max_event_id only bumped on matches");
    assert!(pending_paths.is_empty());

    writer.shutdown();
}

/// The corruption-prevention lock for inode-untrusted filesystems (FAT/exFAT):
/// a delete+create that REUSES a freed inode must NEVER be mistaken for a rename.
///
/// On FAT/exFAT `st_ino` is derived from the file's first data cluster, so a
/// delete+create can alias a fresh, unrelated file onto a freed inode. If that
/// inode were stored in the index, the live rename pre-pass would match the new
/// file against the deleted file's row and emit a `MoveEntryV2`, silently
/// re-homing the old entry's `dir_stats` onto the unrelated file (index
/// corruption). The fix stores `inode: None` for every entry on such a volume,
/// so `find_entry_by_inode` never matches and the change falls back to a safe
/// delete+create.
///
/// This pins BOTH directions at the pre-pass decision point, feeding the DB the
/// value each volume kind would store for the OLD entry: with the inode STORED (a
/// trusted volume) the same inode still detects a genuine move; with it NULLED (an
/// untrusted volume) the reused inode finds no row, so no false `MoveEntryV2`.
#[test]
fn inode_reuse_is_never_a_false_move_when_inodes_are_nulled() {
    // One on-disk entry whose real inode we reuse to simulate cluster aliasing:
    // on disk it's a brand-new "FreshFile", but the DB's OLD entry may carry the
    // same inode value (what a trusted volume would have stored).
    let fs_root = rename_test_tempdir();
    let reused_path = fs_root.path().join("FreshFile");
    std::fs::create_dir(&reused_path).expect("create the fresh on-disk entry");
    let reused_inode =
        std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&reused_path).expect("stat fresh entry"));

    // Seed a DB whose OLD entry "Deleted" carries `stored_inode`, then run the
    // rename pre-pass for a rename event on the fresh entry. Returns how many
    // renames it handled (each emits a `MoveEntryV2`).
    let handled_for = |stored_inode: Option<u64>| -> usize {
        let (writer, db_path, _db_dir) = rename_test_setup();
        let parent_id = insert_path_chain(&db_path, fs_root.path(), &writer);
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::insert_entry_v2(&conn, parent_id, "Deleted", true, false, None, None, None, stored_inode)
                .unwrap();
        }
        let mut events = vec![(
            reused_path.to_string_lossy().to_string(),
            renamed_event(&reused_path.to_string_lossy(), 100),
        )];
        let mut pending_paths = HashSet::new();
        let mut max_event_id = 0u64;
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let handled = detect_renames_by_inode(
            &mut events,
            &IndexPathSpace::root(),
            &conn,
            &writer,
            &mut pending_paths,
            &mut max_event_id,
        );
        writer.shutdown();
        handled
    };

    // Trusted volume (inode stored): a stable inode genuinely IS the same file, so
    // the pre-pass matches and emits one MoveEntryV2 (correct rename detection).
    assert_eq!(
        handled_for(Some(reused_inode)),
        1,
        "with the inode stored, a real move is still detected",
    );

    // Untrusted volume (inode nulled on FAT/exFAT): the reused inode finds no DB
    // row, so NO MoveEntryV2 — the delete+create is not corrupted into a move.
    assert_eq!(
        handled_for(None),
        0,
        "with the inode nulled, inode reuse never becomes a false move",
    );
}

/// Events without `item_renamed` set are passed through untouched even
/// if their inode would happen to match a DB row.
#[test]
fn detect_renames_by_inode_ignores_non_renamed_events() {
    let fs_root = rename_test_tempdir();
    let new_dir_path = fs_root.path().join("Bar");
    std::fs::create_dir(&new_dir_path).unwrap();

    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&new_dir_path).unwrap());

    let (writer, db_path, _db_dir) = rename_test_setup();
    let parent_id = insert_path_chain(&db_path, fs_root.path(), &writer);
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::insert_entry_v2(&conn, parent_id, "Foo", true, false, None, None, None, Some(inode)).unwrap();
    drop(conn);

    // Non-renamed event (item_modified): the pre-pass must ignore it.
    let modified = make_event(
        &new_dir_path.to_string_lossy(),
        42,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_dir: true,
            ..Default::default()
        },
    );
    let mut events = vec![(new_dir_path.to_string_lossy().to_string(), modified)];
    let mut pending_paths = HashSet::new();
    let mut max_event_id = 0u64;

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let handled = detect_renames_by_inode(
        &mut events,
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &mut pending_paths,
        &mut max_event_id,
    );

    assert_eq!(handled, 0);
    assert_eq!(events.len(), 1, "non-renamed event is passed through");

    writer.shutdown();
}

/// `item_renamed` event whose path is gone (the OLD-path side of a
/// rename pair) stays in the batch. The pre-pass only handles new-path
/// events. Phase 2 will resolve the old path; if a `MoveEntryV2` already
/// landed for the same inode, `resolve_path` returns None and Phase 2
/// silently no-ops.
#[test]
fn detect_renames_by_inode_keeps_old_path_event_when_path_is_gone() {
    let (writer, db_path, _db_dir) = rename_test_setup();
    let _ = insert_path_chain(&db_path, Path::new("/some/parent"), &writer);

    // Path doesn't exist on disk, symlink_metadata will fail.
    let gone_path = "/some/parent/RemovedOrRenamedAway";
    let mut events = vec![(gone_path.to_string(), renamed_event(gone_path, 7))];
    let mut pending_paths = HashSet::new();
    let mut max_event_id = 0u64;

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let handled = detect_renames_by_inode(
        &mut events,
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &mut pending_paths,
        &mut max_event_id,
    );

    assert_eq!(handled, 0);
    assert_eq!(events.len(), 1, "gone-path event must remain for Phase 2 to handle");

    writer.shutdown();
}

// ── process_live_batch end-to-end rename ─────────────────────────

/// Full pipeline test: a rename produces two FSEvents in one batch
/// (old-path gone, new-path exists). `process_live_batch` should pair
/// them via the inode pre-pass, emit a single `MoveEntryV2`, and the
/// OLD-path event must silent-no-op in Phase 2 (because `resolve_path`
/// no longer finds the row at the old name after the flush).
///
/// This is the test the rename fix has to pass for the end-to-end
/// "renamed dir keeps its size" property to hold.
#[test]
fn process_live_batch_rename_preserves_dir_stats_and_old_path_no_ops() {
    let fs_root = rename_test_tempdir();
    let new_dir_path = fs_root.path().join("Bar");
    std::fs::create_dir(&new_dir_path).expect("create renamed dir");

    let inode = std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(&new_dir_path).unwrap());

    let (writer, db_path, _db_dir) = rename_test_setup();
    let parent_id = insert_path_chain(&db_path, fs_root.path(), &writer);

    // The renamed-from row, with the renamed dir's inode and pre-populated stats.
    let foo_id = {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let id =
            IndexStore::insert_entry_v2(&conn, parent_id, "Foo", true, false, None, None, None, Some(inode)).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: id,
                recursive_logical_size: 42_000,
                recursive_physical_size: 42_000,
                recursive_file_count: 17,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
        id
    };

    // Build the batch the way the live loop would: HashMap keyed by
    // path, both halves of the rename pair present.
    let mut pending_events: HashMap<String, watcher::FsChangeEvent> = HashMap::new();
    let new_path_str = new_dir_path.to_string_lossy().to_string();
    let old_path_str = fs_root.path().join("Foo").to_string_lossy().to_string();
    pending_events.insert(new_path_str.clone(), renamed_event(&new_path_str, 200));
    pending_events.insert(old_path_str.clone(), renamed_event(&old_path_str, 201));

    let mut reconciler = EventReconciler::new();
    reconciler.switch_to_live();

    // process_live_batch flushes via tokio::task::block_in_place, which
    // requires being inside a multi-thread runtime.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let mut pending_paths = HashSet::new();
        process_live_batch(
            &mut pending_events,
            &mut reconciler,
            &IndexPathSpace::root(),
            &conn,
            &writer,
            &mut pending_paths,
            &mut crate::indexing::watch::churn_monitor::ChurnObserver::disabled(),
        );
    });
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // The original row survives: same id, renamed in place.
    let entry = IndexStore::get_entry_by_id(&conn, foo_id).unwrap().unwrap();
    assert_eq!(entry.name, "Bar", "row should be renamed in place");
    assert_eq!(entry.parent_id, parent_id);

    // dir_stats preserved: the whole point of the fix.
    let stats = IndexStore::get_dir_stats_by_id(&conn, foo_id).unwrap().unwrap();
    assert_eq!(
        stats.recursive_logical_size, 42_000,
        "dir_stats preserved across rename"
    );
    assert_eq!(stats.recursive_file_count, 17);

    // No second row was created at the new name (delete+insert would
    // have left a fresh entry_id with zero stats). Query by name_folded
    // so the assertion is platform-agnostic (macOS folds case + NFD).
    let bar_folded = store::normalize_for_comparison("Bar");
    let row_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE parent_id = ?1 AND name_folded = ?2",
            rusqlite::params![parent_id, bar_folded],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(row_count, 1, "exactly one row should match (parent, 'Bar')");

    // No leftover row at the old name either.
    let foo_folded = store::normalize_for_comparison("Foo");
    let leftover: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE parent_id = ?1 AND name_folded = ?2",
            rusqlite::params![parent_id, foo_folded],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(leftover, 0, "old name should be gone after the rename");

    writer.shutdown();
}
