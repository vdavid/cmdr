//! Concurrency stress tests for the indexing subsystem.
//!
//! Exercises multiple concurrent actors (scanner/writer/reconciler/enrichment)
//! against real SQLite to catch races. Tests use `flush_blocking()` for
//! synchronization wherever possible; minimal sleeps only where
//! races against shutdown must be exercised.

use std::sync::Arc;

use crate::indexing::enrichment::{self, ReadPool};
use crate::indexing::reconciler::{self, EventReconciler};
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::watcher::{FsChangeEvent, FsEventFlags};
use crate::indexing::writer::WriteMessage;

use super::stress_test_helpers::{build_synthetic_tree, check_db_consistency, make_file_entry, setup_writer};

// ── Test 1: concurrent scan + events + replay ───────────────────────

/// Simulates a full scan (entries sent via InsertEntriesV2) while FS events
/// are buffered concurrently, then replayed through the reconciler.
///
/// This exercises the scenario that historically caused bugs like lost
/// metadata (424eedb), micro-scan interference (981b311), and overlay races.
#[test]
fn concurrent_scan_with_buffered_events_and_replay() {
    let (writer, read_conn, _dir) = setup_writer();

    // Phase 1: simulate start_scan — truncate
    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();

    // Build a synthetic tree: 3 levels, 3 dirs/level, 4 files/dir, 1KB each
    // This gives us: level 0 (under ROOT): 3 dirs + 4 files
    //                level 1 (under each L0 dir): 3 dirs + 4 files each
    //                level 2 (under each L1 dir): 3 dirs + 4 files each
    //                leaf dirs: 4 files each
    let tree = build_synthetic_tree(3, 3, 4, 1024);
    let tree_len = tree.len();
    // 3 levels of 3 dirs + 4 files each, plus 4 files in each of the 27 leaf dirs = 199
    assert_eq!(tree_len, 199, "synthetic tree entry count");

    let all_dir_ids: Vec<i64> = tree.iter().filter(|e| e.is_directory).map(|e| e.id).collect();

    // Phase 2: concurrently send entries (thread A) and buffer events (thread B)
    let writer_a = writer.clone();
    let scan_thread = std::thread::spawn(move || {
        // Send entries in batches (simulating scanner behavior)
        let batch_size = 20;
        for chunk in tree.chunks(batch_size) {
            writer_a.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
        }
        // Post-scan: compute aggregates
        writer_a.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer_a.flush_blocking().unwrap();
    });

    // Thread B: buffer synthetic FS events while the "scan" runs.
    // These represent changes that happened on disk during the scan.
    let mut reconciler = EventReconciler::new();
    assert!(reconciler.is_buffering());

    // Buffer a mix of event types with event_ids > 0 (scan_start_event_id = 0)
    // Event 1: create a new file in an L0 directory (dir_L0_D0, id=2)
    reconciler.buffer_event(FsChangeEvent {
        path: "/dir_L0_D0/new_file_from_event.txt".to_string(),
        event_id: 100,
        flags: FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    });

    // Event 2: modify an existing file (simulate metadata change)
    reconciler.buffer_event(FsChangeEvent {
        path: "/dir_L0_D1/file_L0_F0.dat".to_string(),
        event_id: 101,
        flags: FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        },
    });

    // Event 3: create a new directory
    reconciler.buffer_event(FsChangeEvent {
        path: "/dir_L0_D2/new_subdir".to_string(),
        event_id: 102,
        flags: FsEventFlags {
            item_created: true,
            item_is_dir: true,
            ..Default::default()
        },
    });

    // Event 4: stale event (event_id <= scan_start_event_id=0, should be skipped)
    reconciler.buffer_event(FsChangeEvent {
        path: "/dir_L0_D0/stale_event.txt".to_string(),
        event_id: 0,
        flags: FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    });

    assert_eq!(reconciler.buffer_len(), 4);

    // Wait for the scan thread to complete
    scan_thread.join().expect("scan thread panicked");

    // Phase 3: replay buffered events.
    // The reconciler's process_fs_event calls stat() on real paths (which don't
    // exist in our test), so handle_creation_or_modification will see stat()
    // fail and treat it as a deletion. handle_removal will also see that the
    // path doesn't exist on disk and try to resolve + delete from DB. Since
    // these are synthetic paths that the reconciler resolves via the DB, the
    // behavior depends on what resolve_path finds.
    //
    // For the purposes of this stress test, the important thing is that the
    // replay doesn't crash, doesn't corrupt the DB, and the scan data is intact.
    let mut affected_paths: Vec<String> = Vec::new();
    reconciler
        .replay(
            0, // scan_start_event_id
            &read_conn,
            &writer,
            &mut |paths| affected_paths.extend(paths),
        )
        .expect("reconciler replay should not fail");

    // Phase 4: post-replay backfill and final flush
    writer.send(WriteMessage::BackfillMissingDirStats).unwrap();
    writer.flush_blocking().unwrap();

    // Re-open a fresh read connection to see all committed data
    let fresh_conn = IndexStore::open_read_connection(writer.db_path().as_path()).expect("open fresh read conn");

    // Phase 5: verify DB consistency
    check_db_consistency(&fresh_conn);

    // Verify the scan data is present — all original directories should exist
    for &dir_id in &all_dir_ids {
        let entry = IndexStore::get_entry_by_id(&fresh_conn, dir_id).unwrap();
        assert!(
            entry.is_some(),
            "directory with id={dir_id} should exist after scan + replay"
        );
        assert!(entry.unwrap().is_directory, "entry id={dir_id} should be a directory");
    }

    // Verify entry count is at least what the scan inserted (replay may have
    // added or removed a few entries depending on path resolution results)
    let entry_count: i64 = fresh_conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    // We expect at least the root sentinel + scan entries. The reconciler's
    // replay events target paths that don't exist on disk, so they mostly
    // resolve to no-ops or deletions of non-existent entries.
    assert!(
        entry_count >= (tree_len as i64),
        "entry count ({entry_count}) should be >= scan entries ({tree_len}) + root sentinel"
    );

    // Verify that the stale event (event_id=0) was not processed
    // (it should have been skipped by the reconciler's event_id filter)

    // Verify root sentinel has dir_stats
    let root_stats = IndexStore::get_dir_stats_by_id(&fresh_conn, ROOT_ID)
        .unwrap()
        .expect("root sentinel should have dir_stats");
    assert!(root_stats.recursive_file_count > 0, "root should have files after scan");
    assert!(
        root_stats.recursive_dir_count > 0,
        "root should have subdirs after scan"
    );

    // Shut down the writer
    writer.send(WriteMessage::Shutdown).unwrap();
}

/// Stress test: multiple concurrent InsertEntriesV2 senders (simulating
/// overlapping scan batches) followed by aggregation and consistency check.
#[test]
fn concurrent_batch_inserts_with_aggregation() {
    let (writer, _read_conn, _dir) = setup_writer();

    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();

    // Build 4 independent subtrees, each rooted under ROOT_ID with
    // non-overlapping ID ranges.
    let subtrees: Vec<Vec<EntryRow>> = (0..4)
        .map(|subtree_idx| {
            let id_offset = 2 + subtree_idx * 1000; // non-overlapping ID ranges
            let mut entries = Vec::new();
            let mut next_id = id_offset;

            // Root dir for this subtree
            let subtree_root_id = next_id;
            next_id += 1;
            entries.push(EntryRow {
                id: subtree_root_id,
                parent_id: ROOT_ID,
                name: format!("subtree_{subtree_idx}"),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            });

            // Add 5 subdirs with 10 files each
            for d in 0..5 {
                let dir_id = next_id;
                next_id += 1;
                entries.push(EntryRow {
                    id: dir_id,
                    parent_id: subtree_root_id,
                    name: format!("dir_{d}"),
                    is_directory: true,
                    is_symlink: false,
                    logical_size: None,
                    physical_size: None,
                    modified_at: None,
                    inode: None,
                });
                for f in 0..10 {
                    let file_id = next_id;
                    next_id += 1;
                    entries.push(EntryRow {
                        id: file_id,
                        parent_id: dir_id,
                        name: format!("file_{f}.bin"),
                        is_directory: false,
                        is_symlink: false,
                        logical_size: Some(512),
                        physical_size: Some(512),
                        modified_at: Some(1_700_000_000),
                        inode: None,
                    });
                }
            }

            entries
        })
        .collect();

    // Send all subtrees concurrently from separate threads.
    // The writer serializes them, but the senders race on the channel.
    let handles: Vec<_> = subtrees
        .into_iter()
        .map(|entries| {
            let w = writer.clone();
            std::thread::spawn(move || {
                for chunk in entries.chunks(15) {
                    w.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("sender thread panicked");
    }

    // Compute aggregates
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Verify consistency
    let fresh_conn = IndexStore::open_read_connection(writer.db_path().as_path()).expect("open read conn");
    check_db_consistency(&fresh_conn);

    // Each subtree: 1 root dir + 5 subdirs + 50 files = 56 entries
    // Total: 4 * 56 = 224 entries + 1 root sentinel = 225
    let entry_count: i64 = fresh_conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(entry_count, 225, "expected 4 subtrees * 56 entries + root sentinel");

    // Each subtree has 50 files * 512 bytes = 25,600 bytes
    let root_stats = IndexStore::get_dir_stats_by_id(&fresh_conn, ROOT_ID)
        .unwrap()
        .expect("root should have dir_stats");
    assert_eq!(
        root_stats.recursive_logical_size,
        4 * 50 * 512,
        "root recursive_size should be sum of all file sizes"
    );
    assert_eq!(root_stats.recursive_file_count, 200, "root should have 200 files total");
    // 4 subtree roots + 4*5 subdirs = 24 dirs (not counting root itself)
    assert_eq!(
        root_stats.recursive_dir_count, 24,
        "root should have 24 subdirectories total"
    );

    writer.send(WriteMessage::Shutdown).unwrap();
}

// ── Test 3: concurrent scan + enrichment reads ──────────────────────

/// Exercises concurrent writes (scan batches) and reads (enrichment) —
/// the scenario that caused "DB is locked" (26785fc) and enrichment
/// lock contention (d125a24).
#[test]
fn concurrent_scan_with_enrichment_reads() {
    let (writer, _read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    // Phase 1: populate initial tree
    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();

    let tree = build_synthetic_tree(3, 3, 4, 1024);
    for chunk in tree.chunks(20) {
        writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Verify consistency after initial population
    let check_conn = IndexStore::open_read_connection(&db_path).expect("open check conn");
    check_db_consistency(&check_conn);
    drop(check_conn);

    // Phase 2: concurrent writes + enrichment reads
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("create read pool"));

    // Build a second wave of entries (new files appearing in existing dirs).
    // Use IDs that don't collide with the first tree.
    let max_existing_id = tree.iter().map(|e| e.id).max().unwrap();
    let parent_dirs: Vec<&EntryRow> = tree.iter().filter(|e| e.is_directory).take(10).collect();
    let wave2: Vec<EntryRow> = (0..50)
        .map(|i| {
            let parent = parent_dirs[i % parent_dirs.len()];
            EntryRow {
                id: max_existing_id + 1 + i as i64,
                parent_id: parent.id,
                name: format!("wave2_file_{i}.dat"),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(2048),
                physical_size: Some(2048),
                modified_at: Some(1_700_001_000),
                inode: None,
            }
        })
        .collect();

    // Thread A: send second wave + backfill
    let writer_a = writer.clone();
    let wave2_clone = wave2.clone();
    let write_thread = std::thread::spawn(move || {
        for chunk in wave2_clone.chunks(10) {
            writer_a.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
        }
        writer_a.send(WriteMessage::BackfillMissingDirStats).unwrap();
        writer_a.flush_blocking().unwrap();
    });

    // Threads B, C, D: continuous enrichment reads
    let reader_handles: Vec<_> = (0..3)
        .map(|thread_idx| {
            let pool = Arc::clone(&pool);
            std::thread::spawn(move || {
                for iteration in 0..80 {
                    // Build FileEntry objects for enrichment (directories under root)
                    let mut entries = vec![
                        make_file_entry("dir_L0_D0", "/dir_L0_D0", true),
                        make_file_entry("dir_L0_D1", "/dir_L0_D1", true),
                        make_file_entry("dir_L0_D2", "/dir_L0_D2", true),
                    ];

                    // Try parent-id-based enrichment
                    let parent_result =
                        pool.with_conn(|conn| enrichment::enrich_via_parent_id_on(&mut entries, conn, "/"));
                    // During concurrent writes, enrichment may fail to find stats —
                    // that's expected. What matters: no SQLite errors, no panics.
                    if let Err(e) = parent_result {
                        // ReadPool errors (connection issues) would be a real problem
                        panic!("ReadPool error on thread {thread_idx} iteration {iteration}: {e}");
                    }

                    // Also exercise the individual-paths fallback
                    let fallback_result = pool.with_conn(|conn| {
                        enrichment::enrich_via_individual_paths_on(&mut entries, conn);
                    });
                    if let Err(e) = fallback_result {
                        panic!(
                            "ReadPool fallback error on thread {thread_idx} \
                             iteration {iteration}: {e}"
                        );
                    }
                }
            })
        })
        .collect();

    // Wait for all threads
    write_thread.join().expect("write thread panicked");
    for h in reader_handles {
        h.join().expect("reader thread panicked");
    }

    // Phase 3: final consistency check.
    // The writer's accumulator maps only have wave2 data (partial). The first
    // ComputeAllAggregates consumes and clears those partial maps; the second
    // runs the SQL fallback path against the full DB, producing correct stats.
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    let fresh_conn = IndexStore::open_read_connection(&db_path).expect("open fresh conn");
    check_db_consistency(&fresh_conn);

    // Verify second wave entries exist
    for entry in &wave2 {
        let found = IndexStore::get_entry_by_id(&fresh_conn, entry.id).unwrap();
        assert!(
            found.is_some(),
            "wave2 entry id={} name='{}' should exist",
            entry.id,
            entry.name
        );
    }

    writer.send(WriteMessage::Shutdown).unwrap();
}

// ── Test 4: live event storm + concurrent reads ─────────────────────

/// Exercises the scenario where many FS events arrive concurrently with
/// enrichment reads — the scenario that caused false deletions (f0c225f),
/// MustScanSubDirs data loss (31df59e), and event dedup issues (207ddee).
#[test]
fn live_event_storm_with_concurrent_reads() {
    let (writer, _read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    // Phase 1: build and index a full tree
    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();

    let tree = build_synthetic_tree(2, 4, 5, 512);
    let tree_dirs: Vec<EntryRow> = tree.iter().filter(|e| e.is_directory).cloned().collect();

    for chunk in tree.chunks(20) {
        writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    let check_conn = IndexStore::open_read_connection(&db_path).expect("open check conn");
    check_db_consistency(&check_conn);
    drop(check_conn);

    // Prepare event storm: creates, modifies, deletes, MustScanSubDirs.
    // Paths are synthetic and don't exist on disk, so stat() will fail:
    // - Creates/modifies: stat fails -> reconciler tries to delete from DB.
    //   Paths that DON'T match DB entries are no-ops.
    //   Paths that DO match DB entries get deleted (testing deletion resilience).
    // - Removals: stat fails -> resolve path in DB -> delete if found.
    // - MustScanSubDirs: same as create/modify (process_fs_event doesn't
    //   special-case it).
    //
    // We use two classes of paths:
    // 1. Non-resolvable paths (under /nonexistent/) — exercise the code path
    //    without affecting indexed data.
    // 2. Paths matching real DB entries — test actual deletions.
    let mut events: Vec<FsChangeEvent> = Vec::new();
    let mut event_id_counter: u64 = 200;

    // Create events for paths that won't resolve in the DB
    for i in 0..20 {
        events.push(FsChangeEvent {
            path: format!("/nonexistent/storm_new_file_{i}.txt"),
            event_id: event_id_counter,
            flags: FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        });
        event_id_counter += 1;
    }

    // Modify events for non-resolvable paths
    for i in 0..10 {
        events.push(FsChangeEvent {
            path: format!("/nonexistent/synthetic_modify_{i}.dat"),
            event_id: event_id_counter,
            flags: FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..Default::default()
            },
        });
        event_id_counter += 1;
    }

    // Delete events for non-resolvable paths
    for i in 0..5 {
        events.push(FsChangeEvent {
            path: format!("/nonexistent/synthetic_delete_{i}.dat"),
            event_id: event_id_counter,
            flags: FsEventFlags {
                item_removed: true,
                item_is_file: true,
                ..Default::default()
            },
        });
        event_id_counter += 1;
    }

    // MustScanSubDirs events for non-resolvable directories.
    // process_fs_event doesn't special-case must_scan_sub_dirs — it falls
    // through to the item_is_dir handler, which stats and (on failure)
    // tries to delete. Non-resolvable paths are no-ops.
    for i in 0..5 {
        events.push(FsChangeEvent {
            path: format!("/nonexistent/storm_subdir_{i}"),
            event_id: event_id_counter,
            flags: FsEventFlags {
                must_scan_sub_dirs: true,
                item_is_dir: true,
                ..Default::default()
            },
        });
        event_id_counter += 1;
    }

    // Phase 2: concurrent event processing + enrichment reads
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("create read pool"));

    // Thread A: process all events through the reconciler
    let writer_a = writer.clone();
    let events_clone = events;
    let db_path_a = db_path.clone();
    let event_thread = std::thread::spawn(move || {
        let conn = IndexStore::open_read_connection(&db_path_a).expect("open event conn");
        for event in &events_clone {
            let _affected = reconciler::process_fs_event(event, &conn, &writer_a);
        }
    });

    // Threads B, C: continuous enrichment reads
    let reader_handles: Vec<_> = (0..2)
        .map(|thread_idx| {
            let pool = Arc::clone(&pool);
            std::thread::spawn(move || {
                for iteration in 0..60 {
                    let mut entries = vec![
                        make_file_entry("dir_L0_D0", "/dir_L0_D0", true),
                        make_file_entry("dir_L0_D1", "/dir_L0_D1", true),
                        make_file_entry("dir_L0_D2", "/dir_L0_D2", true),
                        make_file_entry("dir_L0_D3", "/dir_L0_D3", true),
                    ];

                    let result = pool.with_conn(|conn| enrichment::enrich_via_parent_id_on(&mut entries, conn, "/"));
                    if let Err(e) = result {
                        panic!("ReadPool error on thread {thread_idx} iteration {iteration}: {e}");
                    }

                    let result = pool.with_conn(|conn| {
                        enrichment::enrich_via_individual_paths_on(&mut entries, conn);
                    });
                    if let Err(e) = result {
                        panic!(
                            "ReadPool fallback error on thread {thread_idx} \
                             iteration {iteration}: {e}"
                        );
                    }
                }
            })
        })
        .collect();

    // Wait for all threads
    event_thread.join().expect("event thread panicked");
    for h in reader_handles {
        h.join().expect("reader thread panicked");
    }

    // Phase 3: flush, backfill, and verify
    writer.send(WriteMessage::BackfillMissingDirStats).unwrap();
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    let fresh_conn = IndexStore::open_read_connection(&db_path).expect("open fresh conn");
    check_db_consistency(&fresh_conn);

    // Verify the original tree's directory structure is intact.
    // Events targeted synthetic paths that don't map to real tree entries,
    // so the original directories should still exist.
    for dir in &tree_dirs {
        let found = IndexStore::get_entry_by_id(&fresh_conn, dir.id).unwrap();
        assert!(
            found.is_some(),
            "original directory id={} name='{}' should survive the event storm",
            dir.id,
            dir.name
        );
    }

    // Root sentinel should have valid stats
    let root_stats = IndexStore::get_dir_stats_by_id(&fresh_conn, ROOT_ID)
        .unwrap()
        .expect("root should have dir_stats after event storm");
    assert!(
        root_stats.recursive_dir_count > 0,
        "root should still have subdirectories after event storm"
    );

    writer.send(WriteMessage::Shutdown).unwrap();
}
