//! Tests for the aggregation handlers ([`super`]).

use super::*;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::stress_test_helpers::check_db_consistency;
use crate::indexing::writer::tests::setup_db;
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Subtree-aggregate ancestor repair (Leak A) ───────────────────
//
// These replay the real message sequence a subtree scan emits
// (`DeleteDescendantsById` → `InsertEntriesV2` → `MarkDirsListed` →
// `ComputeSubtreeAggregates`) and assert the handler leaves the ancestor
// chain EXACT — sizes, counts, AND coverage — with no off-writer
// compensation. Pre-M2 the messages alone left ancestors stale (the
// subtree aggregate only rewrote rows INSIDE the subtree).

fn dir_row(id: i64, parent_id: i64, name: &str) -> EntryRow {
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

fn file_row(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
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

/// Force a dir_stats row's `min_subtree_epoch` to a value, keeping its
/// sizes/counts — simulates the coverage drop a live `UpsertEntryV2` inflicts
/// on ancestors when it creates a new (unlisted) dir.
fn set_epoch(db_path: &std::path::Path, entry_id: i64, epoch: u64) {
    let conn = IndexStore::open_write_connection(db_path).expect("open write conn");
    let mut row = IndexStore::get_dir_stats_by_id(&conn, entry_id)
        .expect("read dir_stats")
        .expect("dir_stats row exists");
    row.min_subtree_epoch = epoch;
    IndexStore::upsert_dir_stats_by_id(&conn, std::slice::from_ref(&row)).expect("upsert epoch");
}

/// A subtree rescan that GROWS the subtree must roll the extra size, files,
/// and dir count up the whole ancestor chain — and RESTORE coverage the
/// new-dir creation had dropped to 0.
#[test]
fn subtree_aggregate_grows_ancestors_and_restores_coverage() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT(1) → A(10) → S(20) → f1(21, 1000). Baseline at epoch 1.
    let entries = vec![
        dir_row(10, ROOT_ID, "A"),
        dir_row(20, 10, "S"),
        file_row(21, 20, "f1", 1000),
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10, 20],
            epoch: 1,
        })
        .unwrap();
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Simulate the ancestor coverage drop a new-dir `UpsertEntryV2` causes.
    set_epoch(&db_path, 10, 0);
    set_epoch(&db_path, ROOT_ID, 0);

    // The real subtree-scan sequence: wipe S's descendants, re-insert the
    // grown set (f1 + f2 + D/f3), stamp the listed dirs, aggregate the subtree.
    writer.send(WriteMessage::DeleteDescendantsById(20)).unwrap();
    writer
        .send(WriteMessage::InsertEntriesV2(vec![
            file_row(21, 20, "f1", 1000),
            file_row(22, 20, "f2", 500),
            dir_row(23, 20, "D"),
            file_row(24, 23, "f3", 300),
        ]))
        .unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![20, 23],
            epoch: 1,
        })
        .unwrap();
    writer
        .send(WriteMessage::ComputeSubtreeAggregates { root_id: 20 })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (a.recursive_logical_size, a.recursive_file_count, a.recursive_dir_count),
        (1800, 3, 2),
        "A must reflect the grown subtree (S + D), not the stale baseline"
    );
    assert_eq!(
        a.min_subtree_epoch, 1,
        "repair must restore A's coverage the new-dir drop zeroed"
    );
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(
        (
            root.recursive_logical_size,
            root.recursive_file_count,
            root.recursive_dir_count
        ),
        (1800, 3, 3),
    );
    assert_eq!(root.min_subtree_epoch, 1, "repair must restore ROOT's coverage too");
    check_db_consistency(&conn);

    writer.shutdown();
}

/// A subtree rescan that SHRINKS the subtree must debit the ancestor chain
/// exactly — the mirror of the grow case.
#[test]
fn subtree_aggregate_shrinks_ancestors() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT(1) → A(10) → S(20) → f1(21,1000), f2(22,1000), D(23) → f3(24,1000)
    let entries = vec![
        dir_row(10, ROOT_ID, "A"),
        dir_row(20, 10, "S"),
        file_row(21, 20, "f1", 1000),
        file_row(22, 20, "f2", 1000),
        dir_row(23, 20, "D"),
        file_row(24, 23, "f3", 1000),
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10, 20, 23],
            epoch: 1,
        })
        .unwrap();
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Rescan finds S shrank to just f1.
    writer.send(WriteMessage::DeleteDescendantsById(20)).unwrap();
    writer
        .send(WriteMessage::InsertEntriesV2(vec![file_row(21, 20, "f1", 1000)]))
        .unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![20],
            epoch: 1,
        })
        .unwrap();
    writer
        .send(WriteMessage::ComputeSubtreeAggregates { root_id: 20 })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (a.recursive_logical_size, a.recursive_file_count, a.recursive_dir_count),
        (1000, 1, 1),
        "A must shrink to the surviving f1 under S"
    );
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(
        (
            root.recursive_logical_size,
            root.recursive_file_count,
            root.recursive_dir_count
        ),
        (1000, 1, 2),
    );
    check_db_consistency(&conn);

    writer.shutdown();
}

/// Boundary: a `ComputeSubtreeAggregates` whose root has no in-index parent
/// (the volume-root sentinel) must repair no ancestors and not crash.
#[test]
fn subtree_aggregate_from_parentless_root_is_noop() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    let entries = vec![dir_row(10, ROOT_ID, "A"), file_row(11, 10, "f", 100)];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10],
            epoch: 1,
        })
        .unwrap();
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Aggregate the whole tree keyed at the ROOT sentinel: its parent is the
    // 0 boundary, so no ancestor repair fires.
    writer
        .send(WriteMessage::ComputeSubtreeAggregates { root_id: ROOT_ID })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(
        (
            root.recursive_logical_size,
            root.recursive_file_count,
            root.recursive_dir_count
        ),
        (100, 1, 1),
    );
    check_db_consistency(&conn);

    writer.shutdown();
}

// ── Skip-severity classification ─────────────────────────────────

#[test]
fn skip_severity_none_when_nothing_skipped() {
    assert_eq!(classify_skip_severity(5_000_000, 0), SkipSeverity::None);
}

#[test]
fn skip_severity_benign_for_sparse_dedup() {
    // A handful of firmlink double-visits / case-NFD siblings in a big scan: expected, not actionable.
    assert_eq!(classify_skip_severity(5_000_000, 3), SkipSeverity::Benign);
    assert_eq!(classify_skip_severity(5_000_000, 49), SkipSeverity::Benign);
}

#[test]
fn skip_severity_benign_when_below_absolute_floor_even_at_high_ratio() {
    // Tiny tree with a couple genuine sibling collisions: high ratio but few skips, stay quiet.
    assert_eq!(classify_skip_severity(20, 10), SkipSeverity::Benign);
}

#[test]
fn skip_severity_suspicious_for_racing_writer_signature() {
    // Two writers racing on one DB: the loser's inserts all conflict, so a large fraction skips.
    assert_eq!(classify_skip_severity(5_000_000, 5_000_000), SkipSeverity::Suspicious);
    // Just over both gates: 100 skips and >1% of the scan (100 / 9100 ≈ 1.1%).
    assert_eq!(classify_skip_severity(9_000, 100), SkipSeverity::Suspicious);
    // Exactly 1% does not trip it (the ratio gate is strict `>`): 100 / 10000.
    assert_eq!(classify_skip_severity(9_900, 100), SkipSeverity::Benign);
}

#[test]
fn skip_severity_benign_when_over_floor_but_under_ratio() {
    // 50 skips clears the floor but is a vanishing fraction of a 5M scan: still benign.
    assert_eq!(classify_skip_severity(5_000_000, 50), SkipSeverity::Benign);
}

// ── Partial aggregation tests ────────────────────────────────────

/// A fresh writer with no inserts has empty accumulator maps, so a partial
/// pass must be a no-op: no `dir_stats` rows, and the writer's mutation
/// counter unchanged (partial passes are not "mutations" for search-staleness
/// purposes — they change no `entries` rows). The counter is asserted as a
/// before/after delta on this one writer (nothing else sends to it), never as
/// an absolute value and never via the global `WRITER_GENERATION`.
#[test]
fn partial_aggregates_no_op_on_empty_maps() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    let gen_before = writer.mutation_count();

    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let dir_stats_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM dir_stats", [], |row| row.get(0))
        .unwrap();
    assert_eq!(dir_stats_count, 0, "empty maps must produce no dir_stats rows");

    let gen_after = writer.mutation_count();
    assert_eq!(
        gen_before, gen_after,
        "a partial pass must not bump the writer's mutation counter"
    );

    writer.shutdown();
}

/// Partial sums show up at shallow depth and grow across batches. A 3-level
/// tree is inserted in two batches; a partial pass after batch 1 writes
/// dir_stats reflecting only batch-1 contents, and a pass after batch 2 grows
/// them.
#[test]
fn partial_aggregates_shallow_sums_grow_across_batches() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Tree (depths from ROOT_ID): /a (id=10, depth 1) -> /a/b (id=11, depth 2)
    //                             /a/b/c (id=12, depth 3) -> /a/b/c/f1 (file)
    // Batch 1 inserts /a, /a/b, /a/b/c and one 100-byte file under /a/b/c.
    let batch1 = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "b".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 12,
            parent_id: 11,
            name: "c".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 13,
            parent_id: 12,
            name: "f1.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(batch1)).unwrap();
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        // Depth ≤ 3 dirs (ROOT_ID=0, /a=1, /a/b=2, /a/b/c=3) all get rows.
        let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a.recursive_logical_size, 100, "/a should sum batch-1 file");
        assert_eq!(a.recursive_file_count, 1);
        assert_eq!(a.recursive_dir_count, 2, "/a has /a/b and /a/b/c beneath it");
        let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
        assert_eq!(c.recursive_logical_size, 100, "/a/b/c holds the file directly");
    }

    // Batch 2 adds a second 50-byte file under /a/b/c.
    let batch2 = vec![EntryRow {
        id: 14,
        parent_id: 12,
        name: "f2.dat".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(50),
        physical_size: Some(50),
        modified_at: None,
        inode: None,
    }];
    writer.send(WriteMessage::InsertEntriesV2(batch2)).unwrap();
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(a.recursive_logical_size, 150, "/a should grow to 100 + 50");
        assert_eq!(a.recursive_file_count, 2);
        let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
        assert_eq!(c.recursive_logical_size, 150);
        assert_eq!(c.recursive_file_count, 2);
    }

    writer.shutdown();
}

/// Dirs deeper than `PARTIAL_AGG_MAX_DEPTH` get no rows from a partial pass,
/// but DO get rows from the final `ComputeAllAggregates`.
#[test]
fn partial_aggregates_depth_limiting() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Chain: /a(10,d1) -> /a/b(11,d2) -> /a/b/c(12,d3) -> /a/b/c/d(13,d4)
    // with a file under the depth-4 dir. d4 = MAX_DEPTH + 1.
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "b".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 12,
            parent_id: 11,
            name: "c".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 13,
            parent_id: 12,
            name: "d".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 14,
            parent_id: 13,
            name: "deep.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(70),
            physical_size: Some(70),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        // /a/b/c is at depth 3 (≤ MAX_DEPTH) — gets a row reflecting the file.
        let c = IndexStore::get_dir_stats_by_id(&conn, 12).unwrap().unwrap();
        assert_eq!(c.recursive_logical_size, 70, "depth-3 dir should sum the deep file");
        // /a/b/c/d is at depth 4 (> MAX_DEPTH) — no partial row.
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().is_none(),
            "depth-4 dir must get no partial row"
        );
    }

    // The final pass writes every dir, including the depth-4 one.
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let d = IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().unwrap();
        assert_eq!(d.recursive_logical_size, 70, "final pass fills the depth-4 dir");
    }

    writer.shutdown();
}

/// A deep dir listed in `hot_paths` punches through the depth limit: it gets
/// its own row plus rows for its direct children. An unresolvable hot path is
/// skipped without error.
#[test]
fn partial_aggregates_hot_paths_punch_through_depth() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // /a(10,d1)/b(11,d2)/c(12,d3)/d(13,d4)/e(14,d5, child dir of d)
    // plus a 60-byte file under e. /a/b/c/d is the hot path (depth 4).
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "b".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 12,
            parent_id: 11,
            name: "c".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 13,
            parent_id: 12,
            name: "d".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 14,
            parent_id: 13,
            name: "e".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 15,
            parent_id: 14,
            name: "x.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(60),
            physical_size: Some(60),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputePartialAggregates {
            // The hot dir (depth 4) and one unresolvable path.
            hot_paths: vec!["/a/b/c/d".into(), "/does/not/exist".into()],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    // /a/b/c/d (hot, depth 4) gets a row despite the cap.
    let d = IndexStore::get_dir_stats_by_id(&conn, 13).unwrap().unwrap();
    assert_eq!(d.recursive_logical_size, 60, "hot dir punches through the depth cap");
    // Its direct child /a/b/c/d/e (depth 5) also gets a row.
    let e = IndexStore::get_dir_stats_by_id(&conn, 14).unwrap().unwrap();
    assert_eq!(e.recursive_logical_size, 60, "hot dir's direct child gets a row");
    // The unresolvable hot path produced no error and no spurious rows: the
    // flush above returned cleanly, which is the assertion.

    writer.shutdown();
}

// ── SQL-sourced partial aggregation (source: Sql) ────────────────

/// Resolve `parent_path`, send an `UpsertEntryV2`, and flush so the next
/// resolve sees it. Mirrors how the reconciler builds the tree one entry at a
/// time WITHOUT touching the accumulator maps (which only `InsertEntriesV2`
/// populates), so the `Sql` partial path is exercised with empty maps.
fn upsert_and_flush(
    writer: &IndexWriter,
    db_path: &std::path::Path,
    parent_path: &str,
    name: &str,
    is_dir: bool,
    size: Option<u64>,
) {
    let parent_id = {
        let conn = IndexStore::open_read_connection(db_path).unwrap();
        crate::indexing::store::resolve_path(&conn, parent_path)
            .unwrap()
            .expect("parent path resolves")
    };
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

/// The unified `Sql` source works when the accumulator maps are EMPTY (the
/// reconcile / network reality). The tree is built entirely with
/// `UpsertEntryV2` under `SetDeltaPropagation(false)` — so the maps stay
/// empty and ancestor `dir_stats` stay at their zero-init values until a real
/// aggregate runs. A `Sql` partial pass on a hot path freshens just that dir +
/// its direct children; the final `ComputeAllAggregates` then makes everything
/// byte-exact (validated by the independent recompute-from-`entries` oracle).
#[test]
fn sql_partial_works_on_empty_maps_then_final_is_exact() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Reconcile-style: no delta propagation; the final aggregate recomputes.
    writer.send(WriteMessage::SetDeltaPropagation(false)).unwrap();

    // /a/b/{c -> f1(100), deep -> f2(200)}, plus unrelated /x/y -> f3(500).
    upsert_and_flush(&writer, &db_path, "/", "a", true, None);
    upsert_and_flush(&writer, &db_path, "/a", "b", true, None);
    upsert_and_flush(&writer, &db_path, "/a/b", "c", true, None);
    upsert_and_flush(&writer, &db_path, "/a/b/c", "f1.dat", false, Some(100));
    upsert_and_flush(&writer, &db_path, "/a/b", "deep", true, None);
    upsert_and_flush(&writer, &db_path, "/a/b/deep", "f2.dat", false, Some(200));
    upsert_and_flush(&writer, &db_path, "/", "x", true, None);
    upsert_and_flush(&writer, &db_path, "/x", "y", true, None);
    upsert_and_flush(&writer, &db_path, "/x/y", "f3.dat", false, Some(500));

    // Resolve the ids we'll assert on.
    let (a, b, c, deep, x) = {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        let r = |p: &str| crate::indexing::store::resolve_path(&conn, p).unwrap().unwrap();
        (r("/a"), r("/a/b"), r("/a/b/c"), r("/a/b/deep"), r("/x"))
    };

    // A Sql partial pass routed through the handler, hot path /a/b.
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec!["/a/b".into()],
            source: PartialAggSource::Sql,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        // Hot dir + direct children freshened to correct recursive sizes.
        let b_stats = IndexStore::get_dir_stats_by_id(&conn, b).unwrap().unwrap();
        assert_eq!(b_stats.recursive_logical_size, 300, "/a/b sums its subtree");
        assert_eq!(b_stats.recursive_file_count, 2);
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, c)
                .unwrap()
                .unwrap()
                .recursive_logical_size,
            100
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, deep)
                .unwrap()
                .unwrap()
                .recursive_logical_size,
            200
        );
        // Ancestor /a is a zero-init row (UpsertEntryV2 created it; no
        // propagation, and the Sql pass writes only the hot dir + children).
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, a)
                .unwrap()
                .unwrap()
                .recursive_logical_size,
            0,
            "the ancestor stays stale until the final aggregate"
        );
        // Unrelated /x likewise untouched (stale zero).
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, x)
                .unwrap()
                .unwrap()
                .recursive_logical_size,
            0
        );
    }

    // The final aggregate fills everything to byte-exact totals.
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, a)
                .unwrap()
                .unwrap()
                .recursive_logical_size,
            300,
            "final pass fills the ancestor"
        );
        // Independent recompute-from-entries oracle: every dir_stats row exact.
        check_db_consistency(&conn);
    }

    writer.shutdown();
}

/// Late-race safety: a `Sql` partial pass that lands AFTER the final
/// `ComputeAllAggregates` recomputes the SAME exact values from the SAME
/// committed rows, so it can't corrupt the final `dir_stats` — and a `Maps`
/// pass after the final aggregate is a no-op (the final pass cleared the maps).
#[test]
fn partial_after_final_aggregate_is_safe_for_both_sources() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Build with InsertEntriesV2 so the maps are populated (the fresh-scan
    // path); the final aggregate then clears them.
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "b".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 12,
            parent_id: 11,
            name: "f.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(123),
            physical_size: Some(123),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();

    // Snapshot the exact final state of /a and /a/b.
    let (a_before, b_before) = {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        (
            IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap(),
            IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap(),
        )
    };
    assert_eq!(b_before.recursive_logical_size, 123);

    // A LATE Sql partial pass (maps are now empty) hitting the same dirs.
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec!["/a".into(), "/a/b".into()],
            source: PartialAggSource::Sql,
        })
        .unwrap();
    // ...and a LATE Maps pass, which must no-op on the cleared maps.
    writer
        .send(WriteMessage::ComputePartialAggregates {
            hot_paths: vec!["/a".into()],
            source: PartialAggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let a_after = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    let b_after = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    // The late Sql pass recomputed identical values (idempotent, not corrupt);
    // the late Maps pass changed nothing.
    assert_eq!(a_after.recursive_logical_size, a_before.recursive_logical_size);
    assert_eq!(a_after.recursive_file_count, a_before.recursive_file_count);
    assert_eq!(a_after.recursive_dir_count, a_before.recursive_dir_count);
    assert_eq!(b_after.recursive_logical_size, b_before.recursive_logical_size);

    writer.shutdown();
}
