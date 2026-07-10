//! Writer tests: the full open→record→finalize→read round-trip, per-`row_role`
//! finalize counts, and the retention prune + dir GC mechanism.

use super::*;
use crate::operation_log::store::{
    OperationLogStore, operation_log_db_path, read_operation, read_operation_items, reconstruct_dir_path,
};

/// Open a fresh store + writer over one temp-dir DB. The store owns the schema
/// lifecycle; the writer opens its own write connection.
fn fresh() -> (OperationLogStore, OperationLogWriter, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let store = OperationLogStore::open(&path).expect("open store");
    let writer = OperationLogWriter::spawn(&path).expect("spawn writer");
    (store, writer, dir)
}

fn file_item(seq: i64, src_dir: &str, name: &str, dst_dir: &str) -> JournalItem {
    JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "vol-1".to_string(),
        source_dir: src_dir.to_string(),
        source_name: name.to_string(),
        dest_volume_id: Some("vol-1".to_string()),
        dest_dir: Some(dst_dir.to_string()),
        dest_name: Some(name.to_string()),
        size: Some(1024),
        mtime: Some(1_700_000_000),
        outcome: ItemOutcome::Done,
        overwrote: false,
    }
}

/// The headline M1 round-trip: open a grouped copy, record files + a created
/// dir, finalize, and read the whole thing back — one operation row plus its
/// items in `seq` order, dir prefixes interned and reconstructable, leaf names
/// folded. Everything else in the subsystem builds on this working.
#[test]
fn open_record_finalize_round_trips_one_operation() {
    let (store, writer, _dir) = fresh();

    writer
        .open_operation(OpenOperation {
            op_id: "op-1".to_string(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("vol-1".to_string()),
            dest_volume_id: Some("vol-1".to_string()),
            item_count: 3,
            started_at: 100,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");

    // Two copied files under /dst, plus the /dst dir the copy created (sequenced
    // AFTER its contents so a seq DESC rollback removes files before the dir).
    let created_dir = JournalItem {
        seq: 2,
        entry_type: EntryType::Dir,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "vol-1".to_string(),
        source_dir: "/src".to_string(),
        source_name: "dst".to_string(),
        dest_volume_id: Some("vol-1".to_string()),
        dest_dir: Some("/".to_string()),
        dest_name: Some("dst".to_string()),
        size: None,
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
    };
    writer
        .record_items(
            "op-1",
            vec![
                file_item(0, "/src", "Photo.JPG", "/dst"),
                file_item(1, "/src", "notes.txt", "/dst"),
                created_dir,
            ],
        )
        .expect("record");

    let outcome = writer
        .finalize_operation(FinalizeOperation {
            op_id: "op-1".to_string(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 200,
            items_done: 3,
            bytes_total: 2048,
            dev_summary: Some("Copy 3 items".to_string()),
        })
        .expect("finalize");
    assert_eq!(
        outcome,
        FinalizeOutcome {
            rollback_unit_rows: 3,
            search_only_rows: 0
        },
        "finalize reports the durable per-row_role counts"
    );
    writer.flush_blocking().expect("flush");

    // Read the header back.
    let op = read_operation(store.conn(), "op-1").expect("read op").expect("present");
    assert_eq!(op.kind, OpKind::Copy);
    assert_eq!(op.initiator, Initiator::User);
    assert_eq!(op.execution_status, ExecutionStatus::Done);
    assert_eq!(op.rollback_state, RollbackState::Rollbackable);
    assert_eq!(op.not_rollbackable_reason, None);
    assert_eq!(op.item_count, 3, "planned total");
    assert_eq!(op.items_done, 3);
    assert_eq!(op.bytes_total, 2048);
    assert_eq!(op.started_at, 100);
    assert_eq!(op.ended_at, Some(200));
    assert_eq!(op.search_coverage, SearchCoverage::Full);
    assert_eq!(op.dev_summary.as_deref(), Some("Copy 3 items"));

    // Read the items back in seq order.
    let items = read_operation_items(store.conn(), "op-1", 100).expect("read items");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].seq, 0);
    assert_eq!(items[0].source_name, "Photo.JPG");
    assert_eq!(items[0].entry_type, EntryType::File);
    assert_eq!(items[2].seq, 2, "the created dir is sequenced last");
    assert_eq!(items[2].entry_type, EntryType::Dir);

    // Dir prefixes interned + reconstructable; the two files share one /src dir.
    assert_eq!(
        items[0].source_dir_id, items[1].source_dir_id,
        "shared source dir interned once"
    );
    assert_eq!(
        reconstruct_dir_path(store.conn(), items[0].source_dir_id).expect("path"),
        "/src"
    );
    let dest_dir_id = items[0].dest_dir_id.expect("dest dir");
    assert_eq!(reconstruct_dir_path(store.conn(), dest_dir_id).expect("path"), "/dst");

    // The folded search key is stored (lowercased) alongside the display name.
    let folded: String = store
        .conn()
        .query_row(
            "SELECT source_name_folded FROM operation_items WHERE op_id = 'op-1' AND seq = 0",
            [],
            |row| row.get(0),
        )
        .expect("folded");
    assert_eq!(folded, "photo.jpg", "the leaf name is folded for search");

    writer.shutdown();
}

/// Finalize returns counts split by `row_role`: a trash-shaped op with a
/// top-level rollback unit plus search-only leaves reports both populations
/// separately (the D4 completeness input the M2 layer splits its checks on).
#[test]
fn finalize_counts_split_by_row_role() {
    let (_store, writer, _dir) = fresh();
    writer
        .open_operation(OpenOperation {
            op_id: "op-trash".to_string(),
            kind: OpKind::Trash,
            initiator: Initiator::User,
            source_volume_id: Some("vol-1".to_string()),
            dest_volume_id: None,
            item_count: 1,
            started_at: 10,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");

    let unit = JournalItem {
        seq: 0,
        entry_type: EntryType::Dir,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "vol-1".to_string(),
        source_dir: "/home".to_string(),
        source_name: "photos".to_string(),
        dest_volume_id: None,
        dest_dir: None,
        dest_name: None,
        size: None,
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
    };
    let leaf = |seq: i64, name: &str| JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: RowRole::SearchOnly,
        source_volume_id: "vol-1".to_string(),
        source_dir: "/home/photos".to_string(),
        source_name: name.to_string(),
        dest_volume_id: None,
        dest_dir: None,
        dest_name: None,
        size: Some(1),
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
    };
    writer
        .record_items("op-trash", vec![unit, leaf(1, "a.jpg"), leaf(2, "b.jpg")])
        .expect("record");

    let outcome = writer
        .finalize_operation(FinalizeOperation {
            op_id: "op-trash".to_string(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 20,
            items_done: 1,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    assert_eq!(
        outcome,
        FinalizeOutcome {
            rollback_unit_rows: 1,
            search_only_rows: 2
        }
    );
    writer.shutdown();
}

/// Retention prunes whole operations older than the age cutoff, GCs the interned
/// dirs that only the pruned op referenced, and keeps a recent op (and its dirs)
/// intact. The mechanism M4 will wire to a periodic timer + size budget.
#[test]
fn prune_removes_old_operation_and_gcs_its_dirs() {
    let (store, writer, _dir) = fresh();

    // An old op (ended long ago) touching /shared/oldtree, and a recent op
    // touching /shared/recenttree. The GC must drop /shared/oldtree but keep
    // /shared — it's the ancestor chain of the still-referenced recent dir, and
    // path reconstruction walks it.
    for (op_id, dir, ended) in [
        ("old", "/shared/oldtree", 1_000i64),
        ("recent", "/shared/recenttree", 9_000i64),
    ] {
        writer
            .open_operation(OpenOperation {
                op_id: op_id.to_string(),
                kind: OpKind::Delete,
                initiator: Initiator::User,
                source_volume_id: Some("vol-1".to_string()),
                dest_volume_id: None,
                item_count: 1,
                started_at: ended - 1,
                rolls_back_op_id: None,
                execution_status: ExecutionStatus::Running,
            })
            .expect("open");
        writer
            .record_items(
                op_id,
                vec![JournalItem {
                    seq: 0,
                    entry_type: EntryType::File,
                    row_role: RowRole::RollbackUnit,
                    source_volume_id: "vol-1".to_string(),
                    source_dir: dir.to_string(),
                    source_name: "leaf".to_string(),
                    dest_volume_id: None,
                    dest_dir: None,
                    dest_name: None,
                    size: Some(1),
                    mtime: None,
                    outcome: ItemOutcome::Done,
                    overwrote: false,
                }],
            )
            .expect("record");
        writer
            .finalize_operation(FinalizeOperation {
                op_id: op_id.to_string(),
                execution_status: ExecutionStatus::Done,
                rollback_state: RollbackState::NotRollbackable,
                not_rollbackable_reason: Some(NotRollbackableReason::PermanentDelete),
                archive_subkind: None,
                search_coverage: SearchCoverage::Full,
                search_coverage_reason: None,
                ended_at: ended,
                items_done: 1,
                bytes_total: 1,
                dev_summary: None,
            })
            .expect("finalize");
    }
    writer.flush_blocking().expect("flush");

    let dir_count = |folded: &str| -> i64 {
        store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM dirs WHERE name_folded = ?1",
                rusqlite::params![folded],
                |row| row.get(0),
            )
            .expect("count")
    };
    assert_eq!(dir_count("oldtree"), 1, "oldtree exists before prune");
    assert_eq!(dir_count("recenttree"), 1);
    assert_eq!(dir_count("shared"), 1, "the shared ancestor exists before prune");

    // Prune everything that ended before now(10_000) - age(5_000) = 5_000: the
    // old op (ended 1_000) goes, the recent op (ended 9_000) stays.
    writer
        .prune(PruneRequest {
            max_age_secs: Some(5_000),
            now_secs: 10_000,
            vacuum: true,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    assert!(
        read_operation(store.conn(), "old").expect("read").is_none(),
        "old op pruned"
    );
    assert!(
        read_operation(store.conn(), "recent").expect("read").is_some(),
        "recent op kept"
    );
    assert_eq!(dir_count("oldtree"), 0, "the old op's unique dir is GC'd");
    assert_eq!(dir_count("recenttree"), 1, "the recent op's dir survives");
    assert_eq!(
        dir_count("shared"),
        1,
        "the shared ancestor survives — it's still on a referenced dir's parent chain"
    );

    writer.shutdown();
}

/// Pruning an op that a SURVIVING op references via `rolls_back_op_id` nulls the
/// survivor's link instead of tripping the self-FK on delete. Guards the
/// ordering: null-before-delete. (M4 expands retention tests; this one defends
/// the FK trap the M3 rollback linkage will exercise.)
#[test]
fn prune_nulls_a_survivors_rollback_link_to_a_pruned_op() {
    let (store, writer, _dir) = fresh();

    // An old original op, and a recent inverse op that rolled it back.
    let open = |op_id: &str, ended: i64, rolls_back: Option<String>| OpenOperation {
        op_id: op_id.to_string(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("vol-1".to_string()),
        dest_volume_id: Some("vol-1".to_string()),
        item_count: 0,
        started_at: ended - 1,
        rolls_back_op_id: rolls_back,
        execution_status: ExecutionStatus::Running,
    };
    let finalize = |op_id: &str, ended: i64| FinalizeOperation {
        op_id: op_id.to_string(),
        execution_status: ExecutionStatus::Done,
        rollback_state: RollbackState::RolledBack,
        not_rollbackable_reason: None,
        archive_subkind: None,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        ended_at: ended,
        items_done: 0,
        bytes_total: 0,
        dev_summary: None,
    };
    writer.open_operation(open("original", 1_000, None)).expect("open orig");
    writer
        .finalize_operation(finalize("original", 1_000))
        .expect("finalize orig");
    writer
        .open_operation(open("inverse", 9_000, Some("original".to_string())))
        .expect("open inverse");
    writer
        .finalize_operation(finalize("inverse", 9_000))
        .expect("finalize inverse");
    writer.flush_blocking().expect("flush");

    writer
        .prune(PruneRequest {
            max_age_secs: Some(5_000),
            now_secs: 10_000,
            vacuum: false,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    assert!(
        read_operation(store.conn(), "original").expect("read").is_none(),
        "old original pruned"
    );
    let inverse = read_operation(store.conn(), "inverse")
        .expect("read")
        .expect("inverse kept");
    assert_eq!(
        inverse.rolls_back_op_id, None,
        "the survivor's link to the pruned op is nulled, not left dangling (no FK violation)"
    );

    writer.shutdown();
}

/// The tiered vacuum policy: skip below MIN, steady cap for a modest freelist,
/// ramp for a real backlog. Regressing this would thrash the writer lock or let
/// the freelist grow unbounded.
#[test]
fn vacuum_cap_tiers() {
    assert_eq!(pick_vacuum_cap(0), None);
    assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST - 1), None);
    assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST), Some(VACUUM_STEADY_CAP));
    assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD - 1), Some(VACUUM_STEADY_CAP));
    assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD), Some(VACUUM_BACKLOG_CAP));
    assert_eq!(pick_vacuum_cap(1_000_000), Some(VACUUM_BACKLOG_CAP));
}
