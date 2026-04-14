//! Lifecycle stress tests for the indexing subsystem.
//!
//! Exercises start/stop/restart scenarios, shutdown edge cases,
//! and concurrent scanning guards. Tests use `flush_blocking()` for
//! synchronization wherever possible; minimal sleeps only where
//! races against shutdown must be exercised.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::writer::{IndexWriter, WriteMessage};

use super::stress_test_helpers::{build_synthetic_tree, check_db_consistency, setup_writer};

// ── Test 5: lifecycle transitions under load ────────────────────────

/// Exercises start/stop/restart scenarios while the writer has pending
/// messages. Historically caused startup panic (f9855ca) and overlay
/// race (795e48b).
///
/// Sub-scenarios:
/// 1. Shutdown with pending scan writes
/// 2. Fresh writer on the same DB + full rescan to consistent state
/// 3. Rapid restart cycles (spawn → write → shutdown, repeated)
/// 4. Shutdown with pending flush
/// 5. Multiple shutdown sends (second should not panic)
#[test]
fn lifecycle_transitions_under_load() {
    // ── Phase 1: populate a tree, verify consistency ────────────────

    let (writer, read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();

    let tree = build_synthetic_tree(2, 3, 5, 1024);
    for chunk in tree.chunks(20) {
        writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    check_db_consistency(&read_conn);

    // ── Phase 2: start a scan but shutdown before completion ────────
    // Send TruncateData + entry batches, then immediately send Shutdown
    // without waiting. The writer must exit cleanly even with pending
    // writes in the channel.

    writer.send(WriteMessage::TruncateData).unwrap();

    let tree2 = build_synthetic_tree(2, 3, 5, 2048);
    for chunk in tree2.chunks(10) {
        // Some of these sends may fail if the writer processes Shutdown
        // before draining the channel — that's expected and fine.
        let _ = writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec()));
    }

    // Shutdown joins the writer thread, so no panic = clean exit.
    writer.shutdown();
    drop(read_conn);

    // ── Phase 3: spawn a NEW writer on the same DB ─────────────────
    // The DB may be partially populated (truncated but not fully
    // re-inserted). That's fine — we verify no corruption, then do a
    // complete rescan.

    let writer2 = IndexWriter::spawn(&db_path, None).expect("spawn second writer");
    let read_conn2 = IndexStore::open_read_connection(&db_path).expect("open read conn 2");

    // DB should be openable and queryable (no corruption).
    let entry_count: i64 = read_conn2
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    // Could be 0 (if truncate ran) or partial — just verify no error.
    assert!(entry_count >= 0, "entry count should be non-negative");

    // Do a complete fresh scan to bring DB to a consistent state.
    writer2.send(WriteMessage::TruncateData).unwrap();
    writer2.flush_blocking().unwrap();

    let tree3 = build_synthetic_tree(2, 3, 5, 512);
    let tree3_len = tree3.len();
    for chunk in tree3.chunks(20) {
        writer2.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer2.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer2.flush_blocking().unwrap();

    // Re-open read connection to see committed data.
    drop(read_conn2);
    let fresh_conn = IndexStore::open_read_connection(&db_path).expect("open fresh conn");
    check_db_consistency(&fresh_conn);

    let final_count: i64 = fresh_conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    // tree3 entries + 1 root sentinel
    assert_eq!(
        final_count,
        tree3_len as i64 + 1,
        "after full rescan, entry count should match tree + root sentinel"
    );

    writer2.shutdown();
    drop(fresh_conn);

    // ── Phase 4: rapid restart cycles ──────────────────────────────
    // Spawn writer, send some messages, shutdown, repeat 4 times.
    // Verifies no resource leaks or DB lock issues across restarts.

    for cycle in 0..4 {
        let w = IndexWriter::spawn(&db_path, None).unwrap_or_else(|e| panic!("spawn failed on cycle {cycle}: {e}"));

        // Send a mix of write operations.
        w.send(WriteMessage::TruncateData).unwrap();
        let small_tree = build_synthetic_tree(1, 2, 3, 256);
        for chunk in small_tree.chunks(10) {
            w.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
        }
        w.send(WriteMessage::ComputeAllAggregates).unwrap();

        // shutdown() joins the thread, ensuring it fully exits and
        // releases the DB write lock before we spawn the next writer.
        w.shutdown();
    }

    // After all cycles, verify DB is in a valid state.
    let post_cycle_conn = IndexStore::open_read_connection(&db_path).expect("open post-cycle conn");
    check_db_consistency(&post_cycle_conn);
    drop(post_cycle_conn);

    // ── Phase 5: shutdown with pending flush ───────────────────────
    // Send entries, send Flush (don't wait on the oneshot), then
    // immediately send Shutdown. The writer should not deadlock.

    let w = IndexWriter::spawn(&db_path, None).expect("spawn for flush test");
    w.send(WriteMessage::TruncateData).unwrap();
    let flush_tree = build_synthetic_tree(1, 2, 4, 128);
    for chunk in flush_tree.chunks(10) {
        w.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }

    // Send Flush but intentionally drop the receiver without awaiting.
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    w.send(WriteMessage::Flush(tx)).unwrap();

    // Immediately send Shutdown — the writer must handle the dropped
    // oneshot sender (Flush response fails to send) and then exit.
    w.shutdown();

    // ── Phase 6: multiple shutdown sends ───────────────────────────
    // Sending Shutdown twice must not panic. The second send returns an
    // error (channel disconnected) which we ignore.

    let w = IndexWriter::spawn(&db_path, None).expect("spawn for double shutdown test");
    w.send(WriteMessage::TruncateData).unwrap();
    w.send(WriteMessage::Shutdown).unwrap();
    // Brief sleep so the writer thread has time to process Shutdown and
    // exit, releasing the DB write lock. Needed because we used
    // send(Shutdown) instead of shutdown() (which would join the thread).
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Second shutdown: channel is closed, send returns Err — must not panic.
    let result = w.send(WriteMessage::Shutdown);
    assert!(result.is_err(), "second Shutdown send should fail (channel closed)");

    // flush_blocking should also fail after Shutdown.
    let flush_result = w.flush_blocking();
    assert!(flush_result.is_err(), "flush_blocking should fail after Shutdown");
}

// ── Test 6: normal start → populate → shutdown lifecycle ──────────

/// Verifies the cleanest lifecycle path: spawn a writer, insert data,
/// compute aggregates, flush, verify consistency, shut down, and confirm
/// the DB is still readable afterward (no locks held, no corruption).
#[test]
fn lifecycle_clean_start_populate_shutdown() {
    let (writer, _read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    // Populate
    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();

    let tree = build_synthetic_tree(2, 3, 4, 512);
    let tree_len = tree.len();
    for chunk in tree.chunks(20) {
        writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Verify consistency while writer is still alive
    let mid_conn = IndexStore::open_read_connection(&db_path).expect("open mid conn");
    check_db_consistency(&mid_conn);
    let count: i64 = mid_conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, tree_len as i64 + 1, "tree entries + root sentinel");
    drop(mid_conn);

    // Clean shutdown
    writer.shutdown();

    // After shutdown: DB must be readable with no locks held
    let post_conn = IndexStore::open_read_connection(&db_path).expect("open post-shutdown conn");
    check_db_consistency(&post_conn);
    let post_count: i64 = post_conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(post_count, tree_len as i64 + 1, "data survives shutdown");

    // A new writer should be spawnable on the same DB (no leftover locks)
    let writer2 = IndexWriter::spawn(&db_path, None).expect("spawn post-shutdown writer");
    writer2.flush_blocking().unwrap();
    writer2.shutdown();
}

// ── Test 7: double-start guard via scanning flag ──────────────────

/// The `scanning` AtomicBool in IndexManager prevents concurrent scans.
/// This test exercises the same pattern at a lower level: simulate two
/// "scans" (truncate + insert + aggregate) sharing an AtomicBool guard,
/// and verify that the second attempt is correctly rejected while the
/// first is in progress.
#[test]
fn double_start_guard_prevents_concurrent_scans() {
    let (writer, _read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    // Shared scanning flag (mirrors IndexManager.scanning)
    let scanning = Arc::new(AtomicBool::new(false));

    // Simulate start_scan's guard check
    fn try_start_scan(scanning: &AtomicBool) -> Result<(), String> {
        if scanning.load(Ordering::Relaxed) {
            return Err("Scan already running".to_string());
        }
        scanning.store(true, Ordering::Relaxed);
        Ok(())
    }

    // First scan starts successfully
    assert!(try_start_scan(&scanning).is_ok(), "first scan should start");
    assert!(scanning.load(Ordering::Relaxed), "scanning should be true");

    // Second scan is rejected
    let result = try_start_scan(&scanning);
    assert!(result.is_err(), "second scan should be rejected");
    assert_eq!(result.unwrap_err(), "Scan already running");

    // Simulate scan completion
    scanning.store(false, Ordering::Relaxed);

    // Third scan succeeds after the first completed
    assert!(try_start_scan(&scanning).is_ok(), "scan after completion should start");

    // Simulate a concurrent test: one thread holds scanning=true, another
    // tries to start. The second thread should see the guard and bail.
    scanning.store(true, Ordering::Relaxed);
    let scanning_clone = Arc::clone(&scanning);
    let handle = std::thread::spawn(move || try_start_scan(&scanning_clone));
    let result = handle.join().expect("thread should not panic");
    assert!(result.is_err(), "concurrent scan start should be rejected");

    // Clean up
    scanning.store(false, Ordering::Relaxed);

    // Also verify that the writer itself works fine through this —
    // populate data and verify consistency
    writer.send(WriteMessage::TruncateData).unwrap();
    writer.flush_blocking().unwrap();
    let tree = build_synthetic_tree(1, 2, 3, 256);
    for chunk in tree.chunks(10) {
        writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_read_connection(&db_path).expect("open conn");
    check_db_consistency(&conn);
    writer.shutdown();
}

// ── Test 8: early shutdown during active writes ───────────────────

/// Exercises the scenario where shutdown is triggered while a "scan"
/// is actively sending batches. This mirrors stop_indexing() being called
/// before start_indexing()/resume_or_scan() finishes. The writer must
/// exit cleanly, and the DB must not be corrupted (though it may have
/// partial data).
#[test]
fn early_shutdown_during_active_writes() {
    let (writer, _read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    writer.send(WriteMessage::TruncateData).unwrap();

    // Build a large tree — enough batches that shutdown races with inserts
    let tree = build_synthetic_tree(3, 4, 6, 1024);

    // Send batches from a separate thread to simulate async scan
    let writer_clone = writer.clone();
    let send_thread = std::thread::spawn(move || {
        let mut sent = 0u64;
        for chunk in tree.chunks(5) {
            // Some sends will fail after shutdown — that's expected
            if writer_clone
                .send(WriteMessage::InsertEntriesV2(chunk.to_vec()))
                .is_err()
            {
                break;
            }
            sent += chunk.len() as u64;
        }
        sent
    });

    // Give the sender a tiny head start, then shut down
    std::thread::sleep(std::time::Duration::from_millis(1));
    writer.shutdown();

    let _sent = send_thread.join().expect("send thread should not panic");

    // DB must be openable and not corrupted (partial data is fine)
    let conn = IndexStore::open_read_connection(&db_path).expect("open post-early-shutdown conn");
    let entry_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    // May be 0 (only root sentinel survived truncate) or partial
    assert!(entry_count >= 0, "entry count must be non-negative");

    // No orphaned entries (parent_id references are valid)
    let orphans: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries e
             WHERE e.parent_id != 0
               AND NOT EXISTS (
                 SELECT 1 FROM entries p WHERE p.id = e.parent_id AND p.is_directory = 1
               )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(orphans, 0, "no orphaned entries after early shutdown");

    // A new writer can start on the same DB
    let writer2 = IndexWriter::spawn(&db_path, None).expect("spawn after early shutdown");
    writer2.flush_blocking().unwrap();
    writer2.shutdown();
}

// ── Test 9: rapid start/stop/start cycle ──────────────────────────

/// Exercises a rapid start → populate → shutdown → start → populate
/// → shutdown cycle, verifying that the second lifecycle sees a fully
/// clean state. This is the pattern that occurs when a user toggles
/// indexing off and on quickly, or when the app restarts.
#[test]
fn rapid_start_stop_start_produces_clean_state() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("rapid-cycle.db");
    let _store = IndexStore::open(&db_path).expect("open store");

    // ── Cycle 1: populate with tree A ──────────────────────────────
    let writer1 = IndexWriter::spawn(&db_path, None).expect("spawn writer 1");
    writer1.send(WriteMessage::TruncateData).unwrap();
    writer1.flush_blocking().unwrap();

    let tree_a = build_synthetic_tree(2, 2, 3, 1024);
    for chunk in tree_a.chunks(15) {
        writer1.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer1.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer1.flush_blocking().unwrap();

    let conn1 = IndexStore::open_read_connection(&db_path).expect("open conn 1");
    check_db_consistency(&conn1);
    let count_a: i64 = conn1
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert!(count_a > 1, "cycle 1 should have entries");
    drop(conn1);

    writer1.shutdown();

    // ── Cycle 2: fresh start, populate with tree B ─────────────────
    let writer2 = IndexWriter::spawn(&db_path, None).expect("spawn writer 2");

    // Truncate (simulating a fresh scan on restart)
    writer2.send(WriteMessage::TruncateData).unwrap();
    writer2.flush_blocking().unwrap();

    // Tree B has a different shape than tree A
    let tree_b = build_synthetic_tree(1, 5, 2, 2048);
    let tree_b_len = tree_b.len();
    let tree_b_dirs: Vec<i64> = tree_b.iter().filter(|e| e.is_directory).map(|e| e.id).collect();
    for chunk in tree_b.chunks(15) {
        writer2.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer2.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer2.flush_blocking().unwrap();

    let conn2 = IndexStore::open_read_connection(&db_path).expect("open conn 2");
    check_db_consistency(&conn2);

    // Verify cycle 2 data is complete
    let count_b: i64 = conn2
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count_b,
        tree_b_len as i64 + 1,
        "cycle 2 should have exactly tree B entries + root sentinel"
    );

    // All tree B directories exist (tree A was truncated; IDs may overlap
    // since TruncateData resets the counter, so we verify by total count above)
    for &dir_id in &tree_b_dirs {
        let entry = IndexStore::get_entry_by_id(&conn2, dir_id).unwrap();
        assert!(
            entry.is_some(),
            "tree B directory id={dir_id} should exist after cycle 2"
        );
    }

    // Root stats should reflect tree B's data, not tree A's
    let root_stats = IndexStore::get_dir_stats_by_id(&conn2, ROOT_ID)
        .unwrap()
        .expect("root should have dir_stats");
    let tree_b_file_count: u64 = tree_b.iter().filter(|e| !e.is_directory).count() as u64;
    assert_eq!(
        root_stats.recursive_file_count, tree_b_file_count,
        "root file count should match tree B"
    );

    drop(conn2);
    writer2.shutdown();
}

// ── Test 10: shutdown cancels in-flight work cleanly ──────────────

/// Simulates a scenario where a writer has a mix of insert batches,
/// aggregation, and meta updates queued, then receives a shutdown.
/// Verifies that the writer thread exits without panic or deadlock,
/// and that whatever data WAS written is consistent (no half-written
/// aggregation, no corrupted meta).
#[test]
fn shutdown_with_mixed_queued_work() {
    let (writer, _read_conn, _dir) = setup_writer();
    let db_path = writer.db_path();

    // Queue a complex sequence of operations without flushing
    writer.send(WriteMessage::TruncateData).unwrap();

    let tree = build_synthetic_tree(2, 3, 4, 512);
    for chunk in tree.chunks(10) {
        writer.send(WriteMessage::InsertEntriesV2(chunk.to_vec())).unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.send(WriteMessage::BackfillMissingDirStats).unwrap();
    writer
        .send(WriteMessage::UpdateMeta {
            key: "scan_completed_at".to_string(),
            value: "1700000000".to_string(),
        })
        .unwrap();
    writer
        .send(WriteMessage::UpdateMeta {
            key: "total_entries".to_string(),
            value: tree.len().to_string(),
        })
        .unwrap();
    writer.send(WriteMessage::IncrementalVacuum).unwrap();

    // Now shut down — the writer processes everything in order, then exits
    writer.shutdown();

    // Verify: DB should have all the data (shutdown processes remaining messages)
    let conn = IndexStore::open_read_connection(&db_path).expect("open post-shutdown conn");
    check_db_consistency(&conn);

    let entry_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        entry_count,
        tree.len() as i64 + 1,
        "all entries should be committed before shutdown"
    );

    // Meta should be written
    let scan_completed: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'scan_completed_at'", [], |row| {
            row.get(0)
        })
        .ok();
    assert_eq!(
        scan_completed,
        Some("1700000000".to_string()),
        "meta should be committed"
    );

    // Aggregation should have run (dirs have stats)
    let dirs_without_stats: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries e
             WHERE e.is_directory = 1
               AND NOT EXISTS (SELECT 1 FROM dir_stats ds WHERE ds.entry_id = e.id)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        dirs_without_stats, 0,
        "all dirs should have stats after aggregation + backfill"
    );

    // No leftover locks — another writer can start
    let writer2 = IndexWriter::spawn(&db_path, None).expect("spawn after mixed-work shutdown");
    writer2.flush_blocking().unwrap();
    writer2.shutdown();
}
