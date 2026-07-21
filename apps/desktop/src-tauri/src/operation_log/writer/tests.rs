//! Writer tests: the full open→record→finalize→read round-trip, per-`row_role`
//! finalize counts, and the rollback reads + state-mutation messages. The
//! retention prune + dir GC + vacuum tests live in the sibling
//! [`retention_tests`](super::retention_tests) module (split out to keep each test
//! file under the length budget); they share [`fresh`] from here.

use super::*;
use crate::operation_log::store::{
    OperationLogStore, operation_log_db_path, ops_in_rolling_back, read_inverse_op, read_operation,
    read_operation_items, read_rollback_units_page, reconstruct_dir_path,
};

/// Open a fresh store + writer over one temp-dir DB. The store owns the schema
/// lifecycle; the writer opens its own write connection. Shared with
/// `retention_tests`.
pub(super) fn fresh() -> (OperationLogStore, OperationLogWriter, tempfile::TempDir) {
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

/// The headline durable-store round-trip: open a grouped copy, record files + a created
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
            item_count: None,
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
/// separately (the D4 completeness input the capture layer splits its checks on).
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
            item_count: None,
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

// ── Rollback reads + state-mutation messages ─────────────────────────────

/// `read_rollback_units_page` streams `rollback_unit` rows newest-`seq`-first,
/// resolves interned dirs to full paths + real volume ids, EXCLUDES `search_only`
/// leaves, and pages via the `before_seq` cursor without materializing the op.
#[test]
fn rollback_units_page_streams_reverse_and_excludes_search_leaves() {
    let (store, writer, _dir) = fresh();
    writer
        .open_operation(OpenOperation {
            op_id: "op".to_string(),
            kind: OpKind::Move,
            initiator: Initiator::User,
            source_volume_id: Some("vol-1".to_string()),
            dest_volume_id: Some("vol-2".to_string()),
            item_count: 0,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    // Three completed rollback units (seq 0,1,2), one skipped rollback unit,
    // plus one search-only leaf.
    let unit = |seq: i64, name: &str| JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "vol-1".to_string(),
        source_dir: "/src".to_string(),
        source_name: name.to_string(),
        dest_volume_id: Some("vol-2".to_string()),
        dest_dir: Some("/dst".to_string()),
        dest_name: Some(name.to_string()),
        size: Some(10),
        mtime: Some(1_700_000_000),
        outcome: ItemOutcome::Done,
        overwrote: false,
    };
    let leaf = JournalItem {
        row_role: RowRole::SearchOnly,
        ..unit(3, "inner.txt")
    };
    let skipped = JournalItem {
        outcome: ItemOutcome::Skipped,
        ..unit(4, "untouched.txt")
    };
    writer
        .record_items("op", vec![unit(0, "a"), unit(1, "b"), unit(2, "c"), leaf, skipped])
        .expect("record");
    writer.flush_blocking().expect("flush");

    let conn = store.conn();
    // First page of 2: newest seq first (2, 1).
    let page1 = read_rollback_units_page(conn, "op", i64::MAX, 2).expect("page1");
    assert_eq!(page1.iter().map(|u| u.seq).collect::<Vec<_>>(), vec![2, 1]);
    assert_eq!(page1[0].source_path, PathBuf::from("/src/c"));
    assert_eq!(page1[0].dest_path, Some(PathBuf::from("/dst/c")));
    assert_eq!(page1[0].source_volume_id, "vol-1");
    assert_eq!(page1[0].dest_volume_id.as_deref(), Some("vol-2"));

    // Next page: everything with seq < 1 ⇒ just seq 0. The search-only leaf (seq
    // 3) never appears in any page.
    let cursor = page1.last().expect("last").seq;
    let page2 = read_rollback_units_page(conn, "op", cursor, 2).expect("page2");
    assert_eq!(page2.iter().map(|u| u.seq).collect::<Vec<_>>(), vec![0]);
    let all: Vec<_> = read_rollback_units_page(conn, "op", i64::MAX, 100)
        .expect("all")
        .into_iter()
        .map(|u| u.seq)
        .collect();
    assert_eq!(
        all,
        vec![2, 1, 0],
        "search-only and non-Done rollback rows are excluded from every page"
    );

    writer.shutdown();
}

/// `set_rollback_state` transitions the two-axis rollback state (+ reason) and
/// acts as a barrier; `set_item_outcomes` flips per-item outcomes by `(op_id, seq)`.
/// These back the rollback engine's rolling_back transitions and the "mark reversed items
/// rolled_back" step.
#[test]
fn set_rollback_state_and_item_outcomes_persist() {
    let (store, writer, _dir) = fresh();
    writer
        .open_operation(OpenOperation {
            op_id: "op".to_string(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("vol-1".to_string()),
            dest_volume_id: Some("vol-1".to_string()),
            item_count: 0,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    writer
        .record_items("op", vec![file_item(0, "/s", "a", "/d"), file_item(1, "/s", "b", "/d")])
        .expect("record");
    writer
        .finalize_operation(FinalizeOperation {
            op_id: "op".to_string(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: 2,
            item_count: None,
            items_done: 2,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");

    // Transition to rolling_back, then to partially_rolled_back.
    writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("set rolling_back");
    let op = read_operation(store.conn(), "op").expect("read").expect("row");
    assert_eq!(op.rollback_state, RollbackState::RollingBack);

    writer
        .set_item_outcomes("op", vec![(0, ItemOutcome::RolledBack), (1, ItemOutcome::Skipped)])
        .expect("set outcomes");
    writer
        .set_rollback_state("op", RollbackState::PartiallyRolledBack, None)
        .expect("set partial");

    let op = read_operation(store.conn(), "op").expect("read").expect("row");
    assert_eq!(op.rollback_state, RollbackState::PartiallyRolledBack);
    let items = read_operation_items(store.conn(), "op", 100).expect("items");
    assert_eq!(items[0].outcome, ItemOutcome::RolledBack);
    assert_eq!(items[1].outcome, ItemOutcome::Skipped);

    writer.shutdown();
}

/// `ops_in_rolling_back` returns exactly the ops left mid-rollback (the reconcile
/// input), and `read_inverse_op` finds the op reversing a given original by its
/// `rolls_back_op_id` link.
#[test]
fn reconcile_reads_find_rolling_back_ops_and_their_inverse() {
    let (store, writer, _dir) = fresh();
    let open = |op_id: &str, rolls_back: Option<String>| OpenOperation {
        op_id: op_id.to_string(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("vol-1".to_string()),
        dest_volume_id: Some("vol-1".to_string()),
        item_count: 0,
        started_at: 1,
        rolls_back_op_id: rolls_back,
        execution_status: ExecutionStatus::Running,
    };
    writer.open_operation(open("orig", None)).expect("open orig");
    writer
        .open_operation(open("inv", Some("orig".to_string())))
        .expect("open inv");
    // Leave `orig` mid-rollback; `inv` is the (unfinalized) inverse.
    writer
        .set_rollback_state("orig", RollbackState::RollingBack, None)
        .expect("set");

    let rolling: Vec<_> = ops_in_rolling_back(store.conn())
        .expect("rolling")
        .into_iter()
        .map(|o| o.op_id)
        .collect();
    assert_eq!(rolling, vec!["orig".to_string()], "only the mid-rollback op");
    let inverse = read_inverse_op(store.conn(), "orig")
        .expect("inverse")
        .expect("present");
    assert_eq!(inverse.op_id, "inv");
    assert!(
        read_inverse_op(store.conn(), "never-rolled-back")
            .expect("none")
            .is_none(),
        "no inverse ⇒ None"
    );

    writer.shutdown();
}
