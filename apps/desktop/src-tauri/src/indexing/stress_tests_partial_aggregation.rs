//! Partial-aggregation correctness tests.
//!
//! The centerpiece is the differential test: the final `dir_stats` must be
//! identical whether or not partial passes ran in between. Partial passes borrow
//! the accumulator maps read-only; if they ever corrupted those maps, the final
//! aggregation (which consumes the same maps) would be wrong. These tests pin
//! that the partial path can't poison the final state.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::indexing::store::{DirStatsById, EntryRow, IndexStore, ROOT_ID, resolve_path};
use crate::indexing::writer::{IndexWriter, PartialAggSource, WriteMessage};

use super::stress_test_helpers::{
    build_synthetic_tree_with_symlinks_and_hardlinks, check_db_consistency, check_recursive_has_symlinks, setup_writer,
};

/// Read every `dir_stats` row keyed by `entry_id`, comparing value columns only
/// (rowids/order are irrelevant).
fn snapshot_dir_stats(conn: &Connection) -> HashMap<i64, DirStatsById> {
    let mut stmt = conn
        .prepare(
            "SELECT entry_id, recursive_logical_size, recursive_physical_size,
                    recursive_file_count, recursive_dir_count, recursive_has_symlinks, min_subtree_epoch
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
                min_subtree_epoch: row.get(6)?,
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
        assert_eq!(
            sa.min_subtree_epoch, sb.min_subtree_epoch,
            "min_subtree_epoch differs for id={id}"
        );
    }
}

/// The directory ids of an entry list, for stamping `listed_epoch` before the
/// final aggregate (mirroring the scanner's mark-before-aggregate sequence).
fn dir_ids(entries: &[EntryRow]) -> Vec<i64> {
    let mut ids: Vec<i64> = entries.iter().filter(|e| e.is_directory).map(|e| e.id).collect();
    // The synthetic root sentinel (ROOT_ID) isn't in the entry list; the scanner
    // marks it too, so include it.
    ids.push(ROOT_ID);
    ids
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

    // Every directory is "successfully listed" at this epoch, stamped after the
    // final insert and before the final aggregate — exactly the scanner's
    // mark-before-aggregate sequence. So the final state has non-zero
    // `min_subtree_epoch` everywhere, making the equality + oracle meaningful.
    let listed_epoch: u64 = 7;
    let marks = dir_ids(&entries);

    // Arm (a): inserts, mark all listed, then final aggregation only.
    let stats_a = {
        let (writer, read_conn, _dir) = setup_writer();
        for batch in &batches {
            writer.send(WriteMessage::InsertEntriesV2(batch.clone())).unwrap();
        }
        writer
            .send(WriteMessage::MarkDirsListed {
                ids: marks.clone(),
                epoch: listed_epoch,
            })
            .unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();
        let snap = snapshot_dir_stats(&read_conn);
        writer.shutdown();
        snap
    };

    // Arm (b): inserts with a partial pass after each batch, then mark + final
    // aggregation. Mid-run the partial passes legitimately differ (marks land only
    // at the end), so the equality below is on the END state only.
    let (writer, read_conn, _dir) = setup_writer();
    for batch in &batches {
        writer.send(WriteMessage::InsertEntriesV2(batch.clone())).unwrap();
        writer
            .send(WriteMessage::ComputePartialAggregates {
                hot_paths: hot_paths.clone(),
                source: PartialAggSource::Maps,
            })
            .unwrap();
    }
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: marks.clone(),
            epoch: listed_epoch,
        })
        .unwrap();
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

/// Upsert one entry by parent path (reconcile-style `UpsertEntryV2`) and block
/// until it's committed, so a follow-up `resolve_path` on the read connection
/// sees it.
fn upsert(
    writer: &IndexWriter,
    read_conn: &Connection,
    parent_path: &str,
    name: &str,
    is_dir: bool,
    size: Option<u64>,
) {
    let parent_id = resolve_path(read_conn, parent_path)
        .unwrap()
        .unwrap_or_else(|| panic!("parent path '{parent_path}' resolves"));
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id,
            name: name.into(),
            is_directory: is_dir,
            is_symlink: false,
            logical_size: size,
            physical_size: size,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();
}

/// Recursive size of a directory by path (0 if it has no `dir_stats` row yet).
fn recursive_size(read_conn: &Connection, path: &str) -> u64 {
    let id = resolve_path(read_conn, path)
        .unwrap()
        .unwrap_or_else(|| panic!("'{path}' resolves"));
    IndexStore::get_dir_stats_by_id(read_conn, id)
        .unwrap()
        .map(|s| s.recursive_logical_size)
        .unwrap_or(0)
}

/// `min_subtree_epoch` of a directory by path (panics if it has no row).
fn min_epoch(read_conn: &Connection, path: &str) -> u64 {
    let id = resolve_path(read_conn, path)
        .unwrap()
        .unwrap_or_else(|| panic!("'{path}' resolves"));
    IndexStore::get_dir_stats_by_id(read_conn, id)
        .unwrap()
        .unwrap_or_else(|| panic!("dir_stats exists for '{path}'"))
        .min_subtree_epoch
}

/// Every directory entry id (for `MarkDirsListed`), including the ROOT sentinel.
fn all_dir_ids(read_conn: &Connection) -> Vec<i64> {
    let mut stmt = read_conn
        .prepare("SELECT id FROM entries WHERE is_directory = 1")
        .unwrap();
    stmt.query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// THE reconcile-with-large-ADD-delta differential test — the regression this
/// whole unified-partials work fixes.
///
/// A reconcile rescan builds the tree with `UpsertEntryV2` under
/// `SetDeltaPropagation(false)` (the `BulkReconcileGuard` reality), so the
/// writer's accumulator maps stay EMPTY the whole walk. Before this change the
/// only mid-scan partial source was `Maps`, which no-ops on empty maps — so a
/// reconcile that GAINS thousands of entries showed flat placeholder sizes for
/// the entire (multi-minute) walk, every size popping in at the final aggregate.
/// The `Sql` source (the timer now sends on a reconcile) recomputes from
/// committed rows, so the hot dir's size GROWS pass over pass.
///
/// Drives `ComputePartialAggregates { source: Sql }` directly, mimicking the
/// timer. The hot path is index-relative (`/a/b`); for the local `root` volume
/// that's the absolute path unchanged, and `compute_partial_aggregates_sql`
/// resolves it under `ROOT_ID` — exactly what a network volume gets after
/// `routing::index_read_path` strips its mount root.
///
/// Asserts:
/// (a) the hot dir `/a/b` recursive size APPEARS and strictly GROWS across the
///     mid-scan partial passes (flat → fails without the wiring),
/// (b) a genuinely-new subtree reads `min_subtree_epoch == 0` mid-scan (the
///     "≥ X" render — nothing is marked listed until the final reconcile),
/// (c) after the final `MarkDirsListed` + `ComputeAllAggregates`, the result is
///     byte-identical to the SAME reconcile run WITHOUT any partial passes, with
///     the independent recompute-from-`entries` oracle confirming the partials
///     never corrupted the final state.
#[test]
fn sql_partial_passes_grow_sizes_mid_reconcile_without_corrupting_final() {
    /// Rounds of "add a new subdir + files under the hot dir", matching a
    /// reconcile that discovers new directories one walk-step at a time.
    const ROUNDS: usize = 6;
    const FILES_PER_ROUND: u64 = 4;
    const FILE_SIZE: u64 = 1000;
    /// The epoch the final reconcile stamps onto every listed dir.
    const LISTED_EPOCH: u64 = 9;

    // Run one reconcile arm. With `run_partials`, fire a `Sql` partial pass after
    // each round and record the hot dir's size (to prove growth). Returns the
    // final `dir_stats` snapshot and the per-round size samples.
    let run_arm = |run_partials: bool| -> (HashMap<i64, DirStatsById>, Vec<u64>) {
        let (writer, read_conn, _dir) = setup_writer();
        // Reconcile bracket: no per-entry propagation, maps stay empty.
        writer.send(WriteMessage::SetDeltaPropagation(false)).unwrap();

        // The hot dir a pane is showing throughout the rescan.
        upsert(&writer, &read_conn, "/", "a", true, None);
        upsert(&writer, &read_conn, "/a", "b", true, None);

        let mut samples = Vec::new();
        for r in 0..ROUNDS {
            let sub = format!("sub_{r}");
            upsert(&writer, &read_conn, "/a/b", &sub, true, None);
            for f in 0..FILES_PER_ROUND {
                upsert(
                    &writer,
                    &read_conn,
                    &format!("/a/b/{sub}"),
                    &format!("f{f}.dat"),
                    false,
                    Some(FILE_SIZE),
                );
            }

            if run_partials {
                writer
                    .send(WriteMessage::ComputePartialAggregates {
                        hot_paths: vec!["/a/b".to_string()],
                        source: PartialAggSource::Sql,
                    })
                    .unwrap();
                writer.flush_blocking().unwrap();
                samples.push(recursive_size(&read_conn, "/a/b"));

                // (b) Mid-scan, before any `MarkDirsListed`, a freshly-added
                // subtree is uncovered: its `min_subtree_epoch` is 0, which the FE
                // renders as "≥ X". The `Sql` pass writes the hot dir's direct
                // children, so this row exists.
                assert_eq!(
                    min_epoch(&read_conn, &format!("/a/b/{sub}")),
                    0,
                    "a genuinely-new subtree reads min_subtree_epoch == 0 mid-scan"
                );
            }
        }

        // Final reconcile: stamp coverage, then the single bulk aggregate.
        let mut listed = all_dir_ids(&read_conn);
        listed.push(ROOT_ID); // harmless if already present; mirrors the scanner.
        writer
            .send(WriteMessage::MarkDirsListed {
                ids: listed,
                epoch: LISTED_EPOCH,
            })
            .unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        let snap = snapshot_dir_stats(&read_conn);
        if run_partials {
            // Primary oracle: ground-truth recompute on the partial-pass arm.
            check_db_consistency(&read_conn);
        }
        writer.shutdown();
        (snap, samples)
    };

    // Arm without partials (baseline) and with partials (under test).
    let (stats_no_partials, _) = run_arm(false);
    let (stats_with_partials, samples) = run_arm(true);

    // (a) The hot dir's size appeared and strictly grew across the passes. This
    // is the regression: with the old `Maps`-only timer on a reconcile, every
    // sample would be 0 (empty maps no-op) and this would fail.
    assert_eq!(samples.len(), ROUNDS, "one sample per round");
    assert!(samples[0] > 0, "the hot dir size appears on the first partial pass");
    for w in samples.windows(2) {
        assert!(
            w[1] > w[0],
            "the hot dir recursive size must GROW across passes, got {samples:?}"
        );
    }
    // Sanity: the final sample equals the full delta (every file under /a/b).
    assert_eq!(
        *samples.last().unwrap(),
        ROUNDS as u64 * FILES_PER_ROUND * FILE_SIZE,
        "the last partial pass sees the whole hot subtree"
    );

    // (c) Byte-identical final state with vs without the partial passes.
    assert_dir_stats_equal(&stats_no_partials, &stats_with_partials);
}

/// Two consecutive partial passes with no inserts between produce identical rows.
#[test]
fn partial_passes_are_idempotent() {
    let entries = build_synthetic_tree_with_symlinks_and_hardlinks(3, 2, 2, 512);
    let (writer, read_conn, _dir) = setup_writer();

    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();
    let first = snapshot_dir_stats(&read_conn);

    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: PartialAggSource::Maps,
        })
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
            .send(WriteMessage::ComputePartialAggregates {
                hot_paths: vec![],
                source: PartialAggSource::Maps,
            })
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
            .send(WriteMessage::ComputePartialAggregates {
                hot_paths: vec![],
                source: PartialAggSource::Maps,
            })
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
