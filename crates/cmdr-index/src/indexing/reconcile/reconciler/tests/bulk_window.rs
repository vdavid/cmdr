//! The bulk window's coverage ledger: an interrupted, a killed, and a cleanly
//! finished walk each have to leave the ancestors' sizes honest.

use super::*;

// ── The bulk window's unpaid ledger ──────────────────────────────

/// Seed `ROOT → A → B`, list every dir at epoch 7, and aggregate, so the whole
/// tree reads exact (`min_subtree_epoch = 7`) before the window opens. Returns
/// `(a_id, b_id)`.
fn seed_listed_tree(db_path: &Path, writer: &IndexWriter) -> (i64, i64) {
    const EPOCH: u64 = 7;
    let entries = vec![
        store::EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "A".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        store::EntryRow {
            id: 20,
            parent_id: 10,
            name: "B".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![ROOT_ID, 10, 20],
            epoch: EPOCH,
        })
        .unwrap();
    // Arm the latch so this seeding aggregate marks the ledger PAID, the state a
    // real index is in before a bulk walk opens a window.
    writer.send(WriteMessage::ArmLedgerHealLatch).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_read_connection(db_path).unwrap();
    assert!(
        IndexStore::ledger_heal_done(&conn).unwrap(),
        "seed: the ledger must start out paid"
    );
    for id in [ROOT_ID, 10, 20] {
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, id)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            EPOCH,
            "seed: dir {id} must start out exact"
        );
    }
    (10, 20)
}

/// Send the message stream a bulk walk emits when it discovers ONE new
/// directory: an `UpsertEntryV2` for it, with no mark (the walk never listed
/// its contents) and no finish.
fn discover_new_dir_under(parent_id: i64, writer: &IndexWriter) {
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id,
            name: "fresh".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
}

/// A directory discovered mid-walk is genuinely unlisted (`listed_epoch = 0`),
/// so every ancestor must stop claiming an exact size. Inside the bulk window
/// per-entry propagation is deliberately off, so the ONLY thing that pays that
/// debt is the terminal `ComputeAllAggregates` — and a walk that never finishes
/// (cancel, error, budget bail) never sends one. Closing the window must pay it.
///
/// Pre-fix, this is the production leak measured on 2026-07-21: 249 directories
/// claiming exact sizes over a descendant at `listed_epoch = 0`.
#[test]
fn an_interrupted_bulk_window_pays_the_coverage_debt_when_it_closes() {
    let (writer, dir, _conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");
    let (a_id, b_id) = seed_listed_tree(&db_path, &writer);

    {
        let _guard = BulkReconcileGuard::begin(&writer);
        discover_new_dir_under(b_id, &writer);
        // No `finish_reconcile`: the walk is interrupted here.
    }
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    for id in [ROOT_ID, a_id, b_id] {
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, id)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "an unlisted descendant must drag ancestor {id} to incomplete"
        );
    }
    check_db_consistency(&conn);

    writer.shutdown();
}

/// The process-death case: the guard's `Drop` never runs, so nothing in this
/// session can pay the debt. The DB must say so on disk (`LEDGER_HEAL_KEY`
/// cleared), which is what makes the next launch arm the heal latch and rebuild
/// the aggregates.
#[test]
fn a_bulk_window_that_dies_mid_walk_leaves_the_ledger_unpaid_for_the_next_launch() {
    let (writer, dir, _conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");
    let (a_id, b_id) = seed_listed_tree(&db_path, &writer);

    let guard = BulkReconcileGuard::begin(&writer);
    discover_new_dir_under(b_id, &writer);
    writer.flush_blocking().unwrap();
    // The app is killed mid-walk: no unwind, so no `Drop`.
    std::mem::forget(guard);

    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        assert!(
            !IndexStore::ledger_heal_done(&conn).unwrap(),
            "a window that never closed must leave the ledger unpaid on disk"
        );
    }
    writer.shutdown();

    // Relaunch: the manager arms the latch for an unpaid ledger, and the launch
    // flow's aggregate consumes it.
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();
    writer.send(WriteMessage::ArmLedgerHealLatch).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    for id in [ROOT_ID, a_id, b_id] {
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, id)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "the relaunch heal must make ancestor {id} honest"
        );
    }
    assert!(
        IndexStore::ledger_heal_done(&conn).unwrap(),
        "the heal must mark the ledger paid again"
    );
    check_db_consistency(&conn);

    writer.shutdown();
}

/// The happy path stays cheap: a walk that finishes pays the debt with its own
/// single aggregate, so the ledger reads paid and no relaunch heal is queued.
#[test]
fn a_bulk_window_that_finishes_cleanly_leaves_the_ledger_paid() {
    let (writer, dir, _conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");
    let (_a_id, b_id) = seed_listed_tree(&db_path, &writer);

    {
        let _guard = BulkReconcileGuard::begin(&writer);
        discover_new_dir_under(b_id, &writer);
        writer.flush_blocking().unwrap();
        let new_id = {
            let conn = IndexStore::open_read_connection(&db_path).unwrap();
            IndexStore::resolve_component(&conn, b_id, "fresh").unwrap().unwrap()
        };
        // The walk listed the new dir too, then finished.
        finish_reconcile(&[ROOT_ID, 10, b_id, new_id], 7, &writer).unwrap();
    }
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert!(
        IndexStore::ledger_heal_done(&conn).unwrap(),
        "a clean finish pays the ledger with its own aggregate"
    );
    assert_eq!(
        IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
            .unwrap()
            .unwrap()
            .min_subtree_epoch,
        7,
        "a fully-listed tree stays exact"
    );
    check_db_consistency(&conn);

    writer.shutdown();
}
