use super::*;

fn make_event(path: &str, event_id: u64, flags: watcher::FsEventFlags) -> watcher::FsChangeEvent {
    watcher::FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags,
    }
}

/// Merging created+removed: `item_removed` wins (priority-based merge),
/// dropping `item_created`. The reconciler's stat-before-delete in
/// `handle_removal` compensates: if the file still exists on disk, it
/// upserts instead of deleting. Regression coverage for f0c225f.
#[test]
fn merge_created_then_removed_prioritizes_removed() {
    let created = make_event(
        "/test/file.txt",
        100,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let removed = make_event(
        "/test/file.txt",
        200,
        watcher::FsEventFlags {
            item_removed: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    let merged = merge_fs_events(&created, &removed);

    assert!(merged.flags.item_removed, "item_removed should be set");
    assert!(!merged.flags.item_created, "item_created is dropped; removed wins");
    assert!(merged.flags.item_is_file, "item_is_file should be preserved");
    assert_eq!(merged.event_id, 200, "higher event_id should be kept");
}

/// Same as above but with events in reverse order: removed first, then
/// created. `item_removed` still wins.
#[test]
fn merge_removed_then_created_prioritizes_removed() {
    let removed = make_event(
        "/test/file.txt",
        100,
        watcher::FsEventFlags {
            item_removed: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let created = make_event(
        "/test/file.txt",
        200,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    let merged = merge_fs_events(&removed, &created);

    assert!(merged.flags.item_removed, "item_removed should be set");
    assert!(!merged.flags.item_created, "item_created is dropped; removed wins");
    assert!(merged.flags.item_is_file, "item_is_file should be preserved");
    assert_eq!(merged.event_id, 200, "higher event_id should be kept");
}

/// When merging, the higher event_id should always win regardless of
/// which event is "existing" vs "incoming".
#[test]
fn merge_keeps_higher_event_id() {
    let older = make_event(
        "/test/file.txt",
        300,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let newer = make_event(
        "/test/file.txt",
        100,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    // existing=older (300), incoming=newer (100)
    let merged = merge_fs_events(&older, &newer);
    assert_eq!(merged.event_id, 300, "higher event_id should be kept");

    // existing=newer (100), incoming=older (300)
    let merged = merge_fs_events(&newer, &older);
    assert_eq!(merged.event_id, 300, "higher event_id should be kept");
}

// ── merge_fs_events dedup/flag tests ─────────────────────────────

/// Three events for the same path merge into one with the highest
/// priority flag and the highest event_id.
#[test]
fn merge_three_events_same_path_keeps_highest_priority() {
    let modified = make_event(
        "/test/file.txt",
        10,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let created = make_event(
        "/test/file.txt",
        20,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    );
    let modified2 = make_event(
        "/test/file.txt",
        30,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    );

    // Simulate HashMap-style dedup: fold sequentially
    let merged = merge_fs_events(&modified, &created);
    let merged = merge_fs_events(&merged, &modified2);

    assert!(
        merged.flags.item_created,
        "item_created should survive (higher priority than modified)"
    );
    assert!(!merged.flags.item_modified, "item_modified is subsumed by item_created");
    assert_eq!(merged.event_id, 30, "highest event_id should be kept");
}

/// Events for different paths are preserved independently when stored
/// in a HashMap keyed by path (the live event loop's dedup strategy).
#[test]
fn distinct_paths_are_all_preserved() {
    let paths = ["/a.txt", "/b.txt", "/c.txt", "/d/e.txt", "/f/g/h.txt"];
    let mut map = HashMap::<String, watcher::FsChangeEvent>::new();

    for (i, path) in paths.iter().enumerate() {
        let event = make_event(
            path,
            (i + 1) as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        map.entry(path.to_string())
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &event);
            })
            .or_insert(event);
    }

    assert_eq!(map.len(), paths.len(), "each distinct path should have its own entry");
    for path in &paths {
        assert!(map.contains_key(*path), "map should contain {path}");
    }
}

/// `must_scan_sub_dirs` always wins when merged with other flags.
#[test]
fn merge_must_scan_sub_dirs_wins_over_modified() {
    let modified = make_event(
        "/test/dir",
        10,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_dir: true,
            ..Default::default()
        },
    );
    let must_scan = make_event(
        "/test/dir",
        20,
        watcher::FsEventFlags {
            must_scan_sub_dirs: true,
            item_is_dir: true,
            ..Default::default()
        },
    );

    // must_scan_sub_dirs incoming
    let merged = merge_fs_events(&modified, &must_scan);
    assert!(merged.flags.must_scan_sub_dirs, "must_scan_sub_dirs should win");
    assert_eq!(merged.event_id, 20);

    // must_scan_sub_dirs existing
    let merged = merge_fs_events(&must_scan, &modified);
    assert!(
        merged.flags.must_scan_sub_dirs,
        "must_scan_sub_dirs should win regardless of order"
    );
    assert_eq!(merged.event_id, 20);
}

/// `must_scan_sub_dirs` wins even when the other event has `item_removed`.
#[test]
fn merge_must_scan_sub_dirs_wins_over_removed() {
    let removed = make_event(
        "/test/dir",
        10,
        watcher::FsEventFlags {
            item_removed: true,
            item_is_dir: true,
            ..Default::default()
        },
    );
    let must_scan = make_event(
        "/test/dir",
        20,
        watcher::FsEventFlags {
            must_scan_sub_dirs: true,
            item_is_dir: true,
            ..Default::default()
        },
    );

    let merged = merge_fs_events(&removed, &must_scan);
    assert!(
        merged.flags.must_scan_sub_dirs,
        "must_scan_sub_dirs should win over item_removed"
    );
}

// ── EventReconciler buffer overflow tests ────────────────────────

/// Buffering exactly MAX_BUFFER_CAPACITY (500K) events does NOT
/// trigger overflow. Adding one more does.
#[test]
fn buffer_capacity_boundary() {
    // MAX_BUFFER_CAPACITY is 500_000 (private to reconciler.rs)
    let cap = 500_000usize;
    let mut reconciler = EventReconciler::new();

    for i in 0..cap {
        reconciler.buffer_event(make_event(
            "/test/file.txt",
            i as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
    }

    assert_eq!(
        reconciler.buffer_len(),
        cap,
        "buffer should hold exactly MAX_BUFFER_CAPACITY events"
    );
    assert!(
        !reconciler.did_buffer_overflow(),
        "should not overflow at exactly MAX_BUFFER_CAPACITY"
    );

    // One more triggers overflow
    reconciler.buffer_event(make_event(
        "/test/overflow.txt",
        cap as u64,
        watcher::FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    ));

    assert!(
        reconciler.did_buffer_overflow(),
        "should overflow after exceeding MAX_BUFFER_CAPACITY"
    );
    assert_eq!(reconciler.buffer_len(), 0, "buffer should be cleared on overflow");
}

/// After overflow, subsequent buffer_event calls are no-ops.
#[test]
fn buffer_overflow_drops_further_events() {
    let cap = 500_000usize;
    let mut reconciler = EventReconciler::new();

    // Fill to capacity + 1 to trigger overflow
    for i in 0..=cap {
        reconciler.buffer_event(make_event(
            "/test/file.txt",
            i as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
    }
    assert!(reconciler.did_buffer_overflow());

    // Further events are silently dropped
    reconciler.buffer_event(make_event(
        "/test/new.txt",
        999_999,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    ));
    assert_eq!(reconciler.buffer_len(), 0, "buffer should remain empty after overflow");
}

/// `did_buffer_overflow()` returns true after overflow, but
/// `switch_to_live()` resets it. This matches the production flow:
/// overflow is checked (and acted on) BEFORE `switch_to_live()`.
#[test]
fn overflow_flag_is_readable_before_switch_to_live() {
    let cap = 500_000usize;
    let mut reconciler = EventReconciler::new();

    for i in 0..=cap {
        reconciler.buffer_event(make_event(
            "/test/file.txt",
            i as u64,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ));
    }

    // Overflow is observable before switch_to_live
    assert!(reconciler.did_buffer_overflow(), "overflow flag should be set");

    // switch_to_live resets it (by design: the caller already consumed the flag)
    reconciler.switch_to_live();
    assert!(
        !reconciler.did_buffer_overflow(),
        "switch_to_live should reset overflow flag"
    );
    assert!(!reconciler.is_buffering(), "should be in live mode");
}

/// Buffering mode transitions: new -> buffering, switch_to_live ->
/// live, buffer_event is no-op in live mode.
#[test]
fn buffering_mode_transitions() {
    let mut reconciler = EventReconciler::new();

    // Starts in buffering mode
    assert!(reconciler.is_buffering());

    // Buffer works
    reconciler.buffer_event(make_event(
        "/a.txt",
        1,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    ));
    assert_eq!(reconciler.buffer_len(), 1);

    // Switch to live
    reconciler.switch_to_live();
    assert!(!reconciler.is_buffering());
    assert_eq!(reconciler.buffer_len(), 0, "buffer cleared on switch");

    // buffer_event is no-op in live mode
    reconciler.buffer_event(make_event(
        "/b.txt",
        2,
        watcher::FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    ));
    assert_eq!(reconciler.buffer_len(), 0, "buffer_event should be no-op in live mode");
}

// ── Replay dedup tests ───────────────────────────────────────────

/// Replay dedup: 500 removal events for the same path (like a SQLite
/// journal file) collapse to a single merged event.
#[test]
fn replay_dedup_collapses_duplicate_events() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();

    for i in 0..500 {
        let path = "/Users/test/Library/peewee-sqlite.db-journal".to_string();
        let event = make_event(
            &path,
            1000 + i,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        pending
            .entry(path)
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &event);
            })
            .or_insert(event);
    }

    assert_eq!(pending.len(), 1, "500 events for same path should collapse to 1");
    let merged = pending.values().next().unwrap();
    assert_eq!(merged.event_id, 1499, "highest event_id should be kept");
    assert!(merged.flags.item_removed, "item_removed flag should be preserved");
}

/// Replay dedup: events for different paths are all preserved while
/// duplicates within each path are merged.
#[test]
fn replay_dedup_preserves_distinct_paths_merges_duplicates() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();

    // 100 events: 10 paths x 10 events each
    for path_idx in 0..10u64 {
        for event_idx in 0..10u64 {
            let path = format!("/path/{path_idx}/file.txt");
            let event = make_event(
                &path,
                path_idx * 10 + event_idx,
                watcher::FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|existing| {
                    *existing = merge_fs_events(existing, &event);
                })
                .or_insert(event);
        }
    }

    assert_eq!(pending.len(), 10, "10 unique paths should be preserved");
    for path_idx in 0..10u64 {
        let path = format!("/path/{path_idx}/file.txt");
        let event = &pending[&path];
        assert_eq!(
            event.event_id,
            path_idx * 10 + 9,
            "each path should keep its highest event_id"
        );
    }
}

/// Replay dedup: mixed create/modify/remove events for the same path
/// merge with correct flag priority (removed wins).
#[test]
fn replay_dedup_mixed_events_merge_correctly() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();
    let path = "/test/file.txt".to_string();

    let events = [
        (
            1,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
        (
            2,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
        (
            3,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
        (
            4,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        ),
    ];

    for (id, flags) in events {
        let event = make_event(&path, id, flags);
        pending
            .entry(path.clone())
            .and_modify(|existing| {
                *existing = merge_fs_events(existing, &event);
            })
            .or_insert(event);
    }

    assert_eq!(pending.len(), 1);
    let merged = &pending[&path];
    assert!(merged.flags.item_removed, "removed should win over created+modified");
    assert!(
        !merged.flags.item_created,
        "created should be dropped when removed wins"
    );
    assert_eq!(merged.event_id, 4, "highest event_id should be kept");
}

/// Replay dedup: simulates realistic event storm with a mix of high-churn
/// paths (SQLite journals, Chrome cache) and unique paths. Verifies the
/// dedup ratio matches expectations.
#[test]
fn replay_dedup_realistic_event_storm() {
    let mut pending = HashMap::<String, watcher::FsChangeEvent>::new();
    let mut raw_count = 0u64;

    // 500 events for a SQLite journal (same path, rapid create/delete)
    for i in 0..500 {
        let path = "/Users/test/Library/aw-server/peewee-sqlite.db-journal".to_string();
        let event = make_event(
            &path,
            i,
            watcher::FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        pending
            .entry(path)
            .and_modify(|e| *e = merge_fs_events(e, &event))
            .or_insert(event);
        raw_count += 1;
    }

    // 200 events for Chrome cache (20 different todelete_ files, 10 events each)
    for file_idx in 0..20 {
        for event_idx in 0..10 {
            let path = format!("/Users/test/Library/Chrome/todelete_{file_idx:04x}");
            let event = make_event(
                &path,
                500 + file_idx * 10 + event_idx,
                watcher::FsEventFlags {
                    item_removed: true,
                    item_is_file: true,
                    ..Default::default()
                },
            );
            pending
                .entry(path)
                .and_modify(|e| *e = merge_fs_events(e, &event))
                .or_insert(event);
            raw_count += 1;
        }
    }

    // 50 unique file modifications (no duplicates)
    for i in 0..50 {
        let path = format!("/Users/test/projects/file_{i}.rs");
        let event = make_event(
            &path,
            700 + i,
            watcher::FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        );
        pending
            .entry(path)
            .and_modify(|e| *e = merge_fs_events(e, &event))
            .or_insert(event);
        raw_count += 1;
    }

    assert_eq!(raw_count, 750, "should have 750 raw events");
    // 1 (journal) + 20 (chrome) + 50 (unique) = 71 unique paths
    assert_eq!(pending.len(), 71, "should deduplicate to 71 unique paths");
}

// ── split_parent_and_name tests (pure helper) ────────────────────

#[test]
fn split_parent_and_name_handles_normal_paths() {
    assert_eq!(
        split_parent_and_name("/a/b/c"),
        Some(("/a/b".to_string(), "c".to_string()))
    );
    assert_eq!(
        split_parent_and_name("/Users/foo/bar.txt"),
        Some(("/Users/foo".to_string(), "bar.txt".to_string()))
    );
}

#[test]
fn split_parent_and_name_handles_root_child() {
    assert_eq!(
        split_parent_and_name("/foo"),
        Some(("/".to_string(), "foo".to_string()))
    );
}

#[test]
fn split_parent_and_name_strips_trailing_slash() {
    assert_eq!(
        split_parent_and_name("/a/b/c/"),
        Some(("/a/b".to_string(), "c".to_string()))
    );
}

#[test]
fn split_parent_and_name_rejects_root_only() {
    assert_eq!(split_parent_and_name("/"), None);
    assert_eq!(split_parent_and_name(""), None);
}

// ── detect_renames_by_inode integration tests ────────────────────

use crate::indexing::store::{DirStatsById, ROOT_ID};

/// Create a temp dir under CARGO_MANIFEST_DIR (Linux's `should_exclude`
/// blocks `/tmp/`, but we don't actually scan here (the path just has
/// to exist on disk so `stat` succeeds and gives us a real inode).
fn rename_test_tempdir() -> tempfile::TempDir {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    tempfile::Builder::new()
        .prefix("cmdr-rename-test-")
        .tempdir_in(base)
        .expect("create temp dir")
}

/// Spawn a writer + DB and return everything callers need.
fn rename_test_setup() -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
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
    let handled = detect_renames_by_inode(&mut events, &conn, &writer, &mut pending_paths, &mut max_event_id);
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
    let handled = detect_renames_by_inode(&mut events, &read_conn, &writer, &mut pending_paths, &mut max_event_id);
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
    let handled = detect_renames_by_inode(&mut events, &conn, &writer, &mut pending_paths, &mut max_event_id);

    assert_eq!(handled, 0, "no inode match → no rename detected");
    assert_eq!(events.len(), 1, "event remains for Phase 2");
    assert_eq!(max_event_id, 0, "max_event_id only bumped on matches");
    assert!(pending_paths.is_empty());

    writer.shutdown();
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
    let handled = detect_renames_by_inode(&mut events, &conn, &writer, &mut pending_paths, &mut max_event_id);

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
    let handled = detect_renames_by_inode(&mut events, &conn, &writer, &mut pending_paths, &mut max_event_id);

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
        process_live_batch(&mut pending_events, &mut reconciler, &conn, &writer, &mut pending_paths);
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
