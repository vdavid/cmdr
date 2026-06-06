//! Partial-aggregation correctness tests.
//!
//! The centerpiece is the differential test: the final `dir_stats` must be
//! identical whether or not partial passes ran in between. Partial passes borrow
//! the accumulator maps read-only; if they ever corrupted those maps, the final
//! aggregation (which consumes the same maps) would be wrong. These tests pin
//! that the partial path can't poison the final state.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::indexing::store::{DirStatsById, EntryRow};
use crate::indexing::writer::WriteMessage;

use super::stress_test_helpers::{
    build_synthetic_tree_with_symlinks_and_hardlinks, check_db_consistency, check_recursive_has_symlinks, setup_writer,
};

/// Read every `dir_stats` row keyed by `entry_id`, comparing value columns only
/// (rowids/order are irrelevant).
fn snapshot_dir_stats(conn: &Connection) -> HashMap<i64, DirStatsById> {
    let mut stmt = conn
        .prepare(
            "SELECT entry_id, recursive_logical_size, recursive_physical_size,
                    recursive_file_count, recursive_dir_count, recursive_has_symlinks
             FROM dir_stats",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok(DirStatsById {
                entry_id: row.get(0)?,
                recursive_logical_size: row.get(1)?,
                recursive_physical_size: row.get(2)?,
                recursive_file_count: row.get(3)?,
                recursive_dir_count: row.get(4)?,
                recursive_has_symlinks: row.get::<_, i32>(5)? != 0,
            })
        })
        .unwrap();
    rows.map(|r| {
        let s = r.unwrap();
        (s.entry_id, s)
    })
    .collect()
}

fn assert_dir_stats_equal(a: &HashMap<i64, DirStatsById>, b: &HashMap<i64, DirStatsById>) {
    let a_ids: std::collections::HashSet<i64> = a.keys().copied().collect();
    let b_ids: std::collections::HashSet<i64> = b.keys().copied().collect();
    assert_eq!(a_ids, b_ids, "the two arms must have dir_stats for the same entry ids");
    for (id, sa) in a {
        let sb = &b[id];
        assert_eq!(
            sa.recursive_logical_size, sb.recursive_logical_size,
            "logical size differs for id={id}"
        );
        assert_eq!(
            sa.recursive_physical_size, sb.recursive_physical_size,
            "physical size differs for id={id}"
        );
        assert_eq!(
            sa.recursive_file_count, sb.recursive_file_count,
            "file count differs for id={id}"
        );
        assert_eq!(
            sa.recursive_dir_count, sb.recursive_dir_count,
            "dir count differs for id={id}"
        );
        assert_eq!(
            sa.recursive_has_symlinks, sb.recursive_has_symlinks,
            "recursive_has_symlinks differs for id={id}"
        );
    }
}

/// Split a flat entry list into deterministic batches for streaming inserts.
fn chunk_entries(entries: &[EntryRow], chunk_size: usize) -> Vec<Vec<EntryRow>> {
    entries.chunks(chunk_size).map(|c| c.to_vec()).collect()
}

/// THE differential test. Same inserts; arm (a) runs only the final aggregation,
/// arm (b) interleaves a partial pass (with hot paths) after every batch. After
/// the final aggregation + flush, both must agree.
///
/// Primary oracle: `check_db_consistency` on arm (b) — an independent
/// recompute-from-`entries` that doesn't share state with the code under test.
/// This catches the nightmare bug: if the partial handler corrupted the shared
/// `AccumulatorMaps`, both arms' final passes would be identically wrong and an
/// (a)==(b) comparison alone would pass green. `check_recursive_has_symlinks`
/// extends the consistency check to the symlink flag (which
/// `check_db_consistency` doesn't validate).
///
/// Secondary oracle: full `dir_stats` of (a) == (b) row-for-row — catches subtler
/// divergences like leftover rows the final pass didn't cover.
#[test]
fn partial_passes_never_change_final_state() {
    // 4 levels, 2 dirs/level, 2 files/dir, plus symlinks + hardlink pairs at the
    // leaves. Several thousand entries.
    let entries = build_synthetic_tree_with_symlinks_and_hardlinks(4, 2, 2, 1024);
    let batches = chunk_entries(&entries, 64);

    // Hot paths reaching dirs deeper than the depth cap, exercising the
    // punch-through branch every pass.
    let hot_paths = vec![
        "/dir_L0_D0/dir_L1_D0/dir_L2_D0/dir_L3_D0".to_string(),
        "/dir_L0_D1/dir_L1_D1/dir_L2_D1/dir_L3_D1".to_string(),
    ];

    // Arm (a): inserts then final aggregation only.
    let stats_a = {
        let (writer, read_conn, _dir) = setup_writer();
        for batch in &batches {
            writer.send(WriteMessage::InsertEntriesV2(batch.clone())).unwrap();
        }
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();
        let snap = snapshot_dir_stats(&read_conn);
        writer.shutdown();
        snap
    };

    // Arm (b): inserts with a partial pass after each batch, then final aggregation.
    let (writer, read_conn, _dir) = setup_writer();
    for batch in &batches {
        writer.send(WriteMessage::InsertEntriesV2(batch.clone())).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates {
                hot_paths: hot_paths.clone(),
            })
            .unwrap();
    }
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Primary oracle: ground-truth recompute on the partial-pass arm.
    check_db_consistency(&read_conn);
    check_recursive_has_symlinks(&read_conn);

    // Secondary oracle: row-for-row equality with the no-partial arm.
    let stats_b = snapshot_dir_stats(&read_conn);
    assert_dir_stats_equal(&stats_a, &stats_b);

    writer.shutdown();
}

/// Two consecutive partial passes with no inserts between produce identical rows.
#[test]
fn partial_passes_are_idempotent() {
    let entries = build_synthetic_tree_with_symlinks_and_hardlinks(3, 2, 2, 512);
    let (writer, read_conn, _dir) = setup_writer();

    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
        .unwrap();
    writer.flush_blocking().unwrap();
    let first = snapshot_dir_stats(&read_conn);

    writer
        .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
        .unwrap();
    writer.flush_blocking().unwrap();
    let second = snapshot_dir_stats(&read_conn);

    assert!(!first.is_empty(), "the first partial pass should write some rows");
    assert_dir_stats_equal(&first, &second);

    writer.shutdown();
}

/// Ordering vs `TruncateData`: a partial pass after a truncate writes nothing
/// (maps cleared), in both orderings.
#[test]
fn partial_pass_after_truncate_is_no_op() {
    // Truncate first, then partial pass: maps already empty.
    {
        let (writer, read_conn, _dir) = setup_writer();
        writer.send(WriteMessage::TruncateData).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();
        let count: i64 = read_conn
            .query_row("SELECT COUNT(*) FROM dir_stats", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "partial pass after truncate (empty maps) writes nothing");
        writer.shutdown();
    }

    // Inserts, then truncate (clears maps), then partial pass: still nothing.
    {
        let (writer, read_conn, _dir) = setup_writer();
        let entries = build_synthetic_tree_with_symlinks_and_hardlinks(2, 2, 2, 256);
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::TruncateData).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates { hot_paths: vec![] })
            .unwrap();
        writer.flush_blocking().unwrap();
        let count: i64 = read_conn
            .query_row("SELECT COUNT(*) FROM dir_stats", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "truncate clears the maps, so a following partial pass writes nothing"
        );
        // The only entry left is the root sentinel re-created by TruncateData.
        let entry_count: i64 = read_conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(entry_count, 1, "only the root sentinel survives a truncate");
        writer.shutdown();
    }
}
