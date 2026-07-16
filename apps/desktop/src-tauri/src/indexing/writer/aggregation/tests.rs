//! Tests for the aggregation handlers ([`super`]).

use super::*;
use crate::indexing::store::{DirStatsById, EntryRow, IndexStore, ROOT_ID};
use crate::indexing::stress_test_helpers::check_db_consistency;
use crate::indexing::writer::tests::setup_db;
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

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
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
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
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
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
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
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

// ── Leak C: backfill repairs ancestors above the filled rows ──────

/// A dir chain has entries but a MISSING `dir_stats` row mid-chain, and a stale,
/// under-credited ancestor above it (the incident's shape: a delta never walked
/// through the missing dir, so ancestors were never credited for its subtree).
/// Backfill must fill the missing row AND repair the stale ancestor chain to
/// exact — not just write the missing row and leave ancestors lying low.
#[test]
fn backfill_repairs_stale_ancestors_above_filled_rows() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT(1) → A(10) → M(20) → f(21, 800). M and its file exist as entries,
    // but M has no `dir_stats` row (Leak C), and A + ROOT carry stale zeroed
    // rows (never credited for M's 800 bytes).
    let entries = vec![
        dir_row(10, ROOT_ID, "A"),
        dir_row(20, 10, "M"),
        file_row(21, 20, "f", 800),
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10, 20],
            epoch: 1,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Seed A and ROOT with stale under-credited rows; leave M with NO row.
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        for id in [ROOT_ID, 10] {
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: id,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 1,
                }],
            )
            .unwrap();
        }
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().is_none(),
            "M starts with no dir_stats row (Leak C)"
        );
    }

    writer.send(WriteMessage::BackfillMissingDirStats).unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    // M filled from its file child.
    let m = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(
        (m.recursive_logical_size, m.recursive_file_count, m.recursive_dir_count),
        (800, 1, 0),
        "backfill fills M from its committed child"
    );
    // A and ROOT repaired to reflect M's now-filled subtree.
    let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (a.recursive_logical_size, a.recursive_file_count, a.recursive_dir_count),
        (800, 1, 1),
        "backfill must repair the stale ancestor A, not leave it under-credited at 0"
    );
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(
        (
            root.recursive_logical_size,
            root.recursive_file_count,
            root.recursive_dir_count
        ),
        (800, 1, 2),
        "ROOT repaired to reflect A → M"
    );
    // The strong oracle: the whole tree agrees with a recompute from `entries`.
    check_db_consistency(&conn);

    writer.shutdown();
}

// ── Leak D: a polluted accumulator can't poison a full aggregate ──
//
// A verification subtree scan's `InsertEntriesV2` batches leave the writer's
// accumulator maps holding SUBTREE-ONLY data. A `finish_reconcile`-shaped
// `ComputeAllAggregates` must NOT roll every out-of-subtree dir up from those
// partial maps — it declares `source: Sql` and recomputes from committed rows.

/// A subtree batch pollutes the maps, then a reconcile-finish `ComputeAllAggregates`
/// lands. Dirs OUTSIDE the rescanned subtree must keep their correct stats.
#[test]
fn reconcile_finish_ignores_polluted_maps() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT(1) → A(10) → fa(11, 500)   [external branch, must stay 500]
    //         → B(20) → fb(21, 700)   [the subtree a verification rescans]
    let entries = vec![
        dir_row(10, ROOT_ID, "A"),
        file_row(11, 10, "fa", 500),
        dir_row(20, ROOT_ID, "B"),
        file_row(21, 20, "fb", 700),
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10, 20],
            epoch: 1,
        })
        .unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // A verification subtree rescan of B: wipe its descendants, re-insert a grown
    // set. The `InsertEntriesV2` leaves the accumulator maps holding ONLY B's
    // subtree (parent_id=20) — the pollution.
    writer.send(WriteMessage::DeleteDescendantsById(20)).unwrap();
    writer
        .send(WriteMessage::InsertEntriesV2(vec![
            file_row(22, 20, "fb", 700),
            file_row(23, 20, "fb2", 300),
        ]))
        .unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![20],
            epoch: 1,
        })
        .unwrap();

    // The reconcile finish's full aggregate lands while the maps are polluted.
    // It declares `Sql`, so it recomputes from committed rows and ignores them.
    writer
        .send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (a.recursive_logical_size, a.recursive_file_count),
        (500, 1),
        "the external branch A must keep its stats — the polluted maps must not zero it"
    );
    check_db_consistency(&conn);

    writer.shutdown();
}

/// The interleaved variant: the reconcile-finish full aggregate lands BETWEEN a
/// verification's subtree `InsertEntriesV2` batches and their
/// `ComputeSubtreeAggregates`. This is why the source parameter (not a
/// clear-the-maps-in-the-subtree-handler fix) is the real fix: the full
/// aggregate must ignore the maps even mid-verification.
#[test]
fn reconcile_finish_interleaved_with_subtree_scan_stays_exact() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT(1) → A(10) → fa(11, 500)   [external]
    //         → B(20) → fb(21, 700)   [rescanned subtree]
    let entries = vec![
        dir_row(10, ROOT_ID, "A"),
        file_row(11, 10, "fa", 500),
        dir_row(20, ROOT_ID, "B"),
        file_row(21, 20, "fb", 700),
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10, 20],
            epoch: 1,
        })
        .unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Verification subtree scan of B: destructive prep + fresh inserts (maps now
    // polluted with B's subtree only).
    writer.send(WriteMessage::DeleteDescendantsById(20)).unwrap();
    writer
        .send(WriteMessage::InsertEntriesV2(vec![
            file_row(22, 20, "fb", 700),
            file_row(23, 20, "fb2", 300),
            dir_row(24, 20, "sub"),
            file_row(25, 24, "fs", 100),
        ]))
        .unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![20, 24],
            epoch: 1,
        })
        .unwrap();

    // The reconcile finish's full aggregate lands HERE — between the subtree
    // batch and its subtree aggregate. It declares `Sql` (Leak-D defense).
    writer
        .send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql })
        .unwrap();

    // The verification's own subtree aggregate lands afterwards.
    writer
        .send(WriteMessage::ComputeSubtreeAggregates { root_id: 20 })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (a.recursive_logical_size, a.recursive_file_count),
        (500, 1),
        "external branch A untouched by the interleaved aggregate"
    );
    let b = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(
        (b.recursive_logical_size, b.recursive_file_count, b.recursive_dir_count),
        (1100, 3, 1),
        "B reflects its rescanned subtree (fb + fb2 + sub/fs)"
    );
    check_db_consistency(&conn);

    writer.shutdown();
}
