//! Bulk-reconcile delta-propagation suppression: the per-entry ancestor walk
//! stays off until the reconcile's single final aggregate.

use super::*;

/// Large-delta regression guard (the test the original wedge needed):
///
/// The FULL reconcile sends thousands of `UpsertEntryV2` per pass. The bug was
/// that EACH one walked the ancestor `dir_stats` chain (O(entries × depth)),
/// wedging the writer for hours on a 270k→6M delta. `SetDeltaPropagation(false)`
/// suppresses that per-entry walk; the reconcile's single `ComputeAllAggregates`
/// recomputes every dir from the entries table instead.
///
/// This drives the writer with the SAME message stream a reconcile emits — bulk
/// mode ON, then thousands of `UpsertEntryV2`, then one `ComputeAllAggregates` —
/// and asserts BOTH halves of the contract on its OWN db (so it's immune to
/// other concurrent test writers, unlike a global counter would be):
///
/// 1. MID-WALK (after every upsert is flushed, BEFORE the aggregate) every dir's
///    `dir_stats` is still its zero-valued init row: the per-entry propagation did
///    NOT run. With propagation left ON this is where it FAILS — each dir would
///    already read its `FILES_PER_DIR` files (RED).
/// 2. POST-AGGREGATE the recomputed `dir_stats` are exactly correct, proving the
///    suppression is invisible to the final result (and that skipping the
///    aggregate would leave them wrong — the other RED).
#[test]
fn bulk_reconcile_suppresses_per_entry_propagation_until_final_aggregate() {
    const DIR_COUNT: i64 = 30;
    const FILES_PER_DIR: i64 = 100;
    const FILE_SIZE: u64 = 100;

    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Enter bulk-reconcile mode: per-entry ancestor propagation is now OFF.
    writer.send(WriteMessage::SetDeltaPropagation(false)).unwrap();

    // Wave 1: create DIR_COUNT directories directly under ROOT. Their ids aren't
    // known yet (UpsertEntryV2 lets the writer allocate), so flush + resolve.
    for i in 0..DIR_COUNT {
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: format!("dir{i}"),
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
    writer.flush_blocking().unwrap();

    let dir_ids: Vec<i64> = {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        (0..DIR_COUNT)
            .map(|i| {
                IndexStore::resolve_component(&conn, ROOT_ID, &format!("dir{i}"))
                    .unwrap()
                    .expect("dir resolved")
            })
            .collect()
    };

    // Wave 2: FILES_PER_DIR files in each directory — the bulk of the delta.
    for &dir_id in &dir_ids {
        for f in 0..FILES_PER_DIR {
            writer
                .send(WriteMessage::UpsertEntryV2 {
                    parent_id: dir_id,
                    name: format!("f{f}.dat"),
                    is_directory: false,
                    is_symlink: false,
                    logical_size: Some(FILE_SIZE),
                    physical_size: Some(FILE_SIZE),
                    modified_at: None,
                    inode: None,
                    nlink: None,
                })
                .unwrap();
        }
    }
    writer.flush_blocking().unwrap();

    // 1. MID-WALK: propagation suppressed, so every dir still shows its
    //    zero-valued init row despite holding FILES_PER_DIR files. (RED here if
    //    propagation is left on: each dir would read FILES_PER_DIR.)
    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        for &dir_id in &dir_ids {
            let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
            assert_eq!(
                stats.recursive_file_count, 0,
                "bulk mode must NOT propagate the files into dir {dir_id}'s dir_stats"
            );
            assert_eq!(
                stats.recursive_logical_size, 0,
                "bulk mode must NOT propagate file sizes into dir {dir_id}'s dir_stats"
            );
        }
        // ROOT was never touched by propagation either.
        let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap();
        assert!(
            root.map(|s| s.recursive_file_count).unwrap_or(0) == 0,
            "bulk mode must NOT propagate anything into ROOT's dir_stats"
        );
    }

    // 2. The single final aggregate recomputes everything correctly.
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();
    writer.send(WriteMessage::SetDeltaPropagation(true)).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        for &dir_id in &dir_ids {
            let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
            assert_eq!(
                stats.recursive_file_count, FILES_PER_DIR as u64,
                "aggregate must fill dir {dir_id}'s file count"
            );
            assert_eq!(
                stats.recursive_logical_size,
                FILE_SIZE * FILES_PER_DIR as u64,
                "aggregate must fill dir {dir_id}'s recursive size"
            );
        }
        let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(
            root.recursive_file_count,
            (DIR_COUNT * FILES_PER_DIR) as u64,
            "ROOT must total every file across every dir"
        );
        assert_eq!(
            root.recursive_dir_count, DIR_COUNT as u64,
            "ROOT must count every directory"
        );
        assert_eq!(
            root.recursive_logical_size,
            FILE_SIZE * (DIR_COUNT * FILES_PER_DIR) as u64,
            "ROOT must total every file's size"
        );
    }

    writer.shutdown();
}
