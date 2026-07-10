//! Query-API tests: the D8 name-search benchmark (index-served, asserted via
//! `EXPLAIN QUERY PLAN`), leaf search through `search_only` rows, the
//! `top_level_only` known-gap flag, composed filters, stable paging, and the
//! paged operation detail.

use rusqlite::Connection;

use super::*;
use crate::operation_log::store::{OperationLogStore, operation_log_db_path};
use crate::operation_log::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState, RowRole,
    SearchCoverage, SearchCoverageReason,
};
use crate::operation_log::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

/// A fresh store + writer over one temp DB. Reads run on `store.conn()` (a write
/// connection reads fine); the writer owns inserts.
fn fresh() -> (OperationLogStore, OperationLogWriter, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let store = OperationLogStore::open(&path).expect("open store");
    let writer = OperationLogWriter::spawn(&path).expect("spawn writer");
    (store, writer, dir)
}

/// Journal one whole operation (open → record → finalize), then block for
/// durability. `items` are recorded as given; the header's timings come from the
/// args so tests can order ops deterministically.
#[allow(
    clippy::too_many_arguments,
    reason = "test builder: one arg per journaled field, clearer inline than a struct"
)]
fn journal_op(
    writer: &OperationLogWriter,
    op_id: &str,
    kind: OpKind,
    initiator: Initiator,
    started_at: i64,
    ended_at: i64,
    coverage: SearchCoverage,
    coverage_reason: Option<SearchCoverageReason>,
    items: Vec<JournalItem>,
) {
    writer
        .open_operation(OpenOperation {
            op_id: op_id.to_string(),
            kind,
            initiator,
            source_volume_id: Some("vol-1".to_string()),
            dest_volume_id: None,
            item_count: items.len() as u64,
            started_at,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    let count = items.len() as u64;
    if !items.is_empty() {
        writer.record_items(op_id, items).expect("record");
    }
    writer
        .finalize_operation(FinalizeOperation {
            op_id: op_id.to_string(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::NotRollbackable,
            not_rollbackable_reason: Some(NotRollbackableReason::PermanentDelete),
            archive_subkind: None,
            search_coverage: coverage,
            search_coverage_reason: coverage_reason,
            ended_at,
            items_done: count,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    writer.flush_blocking().expect("flush");
}

fn leaf(seq: i64, dir: &str, name: &str, role: RowRole) -> JournalItem {
    JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: role,
        source_volume_id: "vol-1".to_string(),
        source_dir: dir.to_string(),
        source_name: name.to_string(),
        dest_volume_id: None,
        dest_dir: None,
        dest_name: None,
        size: Some(1),
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
    }
}

fn delete_trash_filter(name: &str) -> OperationSearchFilters {
    OperationSearchFilters {
        name: Some(NameFilter {
            text: name.to_string(),
            match_kind: NameMatch::Exact,
        }),
        kinds: vec![OpKind::Delete, OpKind::Trash],
        ..Default::default()
    }
}

/// "When did I delete dog.jpg" returns exactly the delete op, matched
/// case-insensitively on the folded name.
#[test]
fn delete_dog_jpg_returns_the_op() {
    let (store, writer, _dir) = fresh();
    journal_op(
        &writer,
        "op-del",
        OpKind::Delete,
        Initiator::User,
        10,
        20,
        SearchCoverage::Full,
        None,
        vec![
            leaf(0, "/home/pics", "Dog.JPG", RowRole::RollbackUnit),
            leaf(1, "/home/pics", "cat.png", RowRole::RollbackUnit),
        ],
    );
    // A copy of a same-named file must NOT match the delete/trash kind filter.
    journal_op(
        &writer,
        "op-copy",
        OpKind::Copy,
        Initiator::User,
        30,
        40,
        SearchCoverage::Full,
        None,
        vec![leaf(0, "/home/pics", "dog.jpg", RowRole::RollbackUnit)],
    );

    let hits = search_operations(store.conn(), &delete_trash_filter("dog.jpg"), 50, 0).expect("search");
    assert_eq!(hits.len(), 1, "only the delete op matches");
    assert_eq!(hits[0].op_id, "op-del");
    assert_eq!(hits[0].kind, OpKind::Delete);

    writer.shutdown();
}

/// The benchmark query is index-served: its `EXPLAIN QUERY PLAN` uses the
/// `operation_items_source_name` index and the `operations` PK, with no full table
/// scan of either table.
#[test]
fn delete_dog_jpg_is_index_served() {
    let (store, writer, _dir) = fresh();
    journal_op(
        &writer,
        "op-del",
        OpKind::Delete,
        Initiator::User,
        10,
        20,
        SearchCoverage::Full,
        None,
        vec![leaf(0, "/home/pics", "dog.jpg", RowRole::RollbackUnit)],
    );

    let plan = explain_plan(store.conn(), &delete_trash_filter("dog.jpg"));
    assert!(
        plan.contains("operation_items_source_name"),
        "the folded-name index serves the item lookup; plan was:\n{plan}"
    );
    // A full table scan is a bare "SCAN <table>" with no index. A "SEARCH" always
    // uses an index or the PK, and a covering-index scan is "SCAN ... USING ...
    // INDEX" — both fine. So only a `SCAN` with no `USING` clause is the scan we
    // reject. (`USE TEMP B-TREE FOR DISTINCT/ORDER BY` are sorts, not scans.)
    for line in plan.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("SCAN") && !trimmed.contains("USING") {
            panic!("benchmark query does a full table scan — offending step:\n{trimmed}\nfull plan:\n{plan}");
        }
    }

    writer.shutdown();
}

/// "When did I trash dog.jpg" hits even when dog.jpg sat inside a trashed folder:
/// the leaf is a `search_only` row under the top-level `rollback_unit`, and search
/// spans every `row_role`.
#[test]
fn trash_dog_jpg_hits_via_search_only_leaf() {
    let (store, writer, _dir) = fresh();
    // Trash of the folder `photos`: one top-level rollback unit, plus its leaves as
    // search_only rows (dog.jpg among them).
    journal_op(
        &writer,
        "op-trash",
        OpKind::Trash,
        Initiator::User,
        10,
        20,
        SearchCoverage::Full,
        None,
        vec![
            JournalItem {
                entry_type: EntryType::Dir,
                ..leaf(0, "/home", "photos", RowRole::RollbackUnit)
            },
            leaf(1, "/home/photos", "dog.jpg", RowRole::SearchOnly),
            leaf(2, "/home/photos", "sunset.jpg", RowRole::SearchOnly),
        ],
    );

    let hits = search_operations(store.conn(), &delete_trash_filter("dog.jpg"), 50, 0).expect("search");
    assert_eq!(hits.len(), 1, "the trashed-folder op is found via its search_only leaf");
    assert_eq!(hits[0].op_id, "op-trash");
    assert!(coverage_is_complete(&hits[0]), "this op indexed its whole subtree");

    writer.shutdown();
}

/// An op that couldn't enumerate its subtree is `top_level_only`: searching a leaf
/// name that WOULD have been inside it returns nothing (the leaf was never
/// recorded), but the op is a queryable known gap via its coverage flag — not a
/// silent false "never happened".
#[test]
fn top_level_only_coverage_is_a_known_gap() {
    let (store, writer, _dir) = fresh();
    // A capped trash: only the top-level `bigtree` row exists, no leaves.
    journal_op(
        &writer,
        "op-capped",
        OpKind::Trash,
        Initiator::User,
        10,
        20,
        SearchCoverage::TopLevelOnly,
        Some(SearchCoverageReason::Capped),
        vec![JournalItem {
            entry_type: EntryType::Dir,
            ..leaf(0, "/home", "bigtree", RowRole::RollbackUnit)
        }],
    );

    // Searching a leaf that would have been inside the capped subtree finds nothing.
    let leaf_hits = search_operations(store.conn(), &delete_trash_filter("inner.jpg"), 50, 0).expect("search");
    assert!(leaf_hits.is_empty(), "the capped subtree's leaves were never recorded");

    // But the op is present and flagged as a known gap, so a consumer can surface it
    // instead of asserting the leaf was never trashed.
    let top = search_operations(store.conn(), &delete_trash_filter("bigtree"), 50, 0).expect("search");
    assert_eq!(top.len(), 1);
    assert!(!coverage_is_complete(&top[0]), "coverage is a known gap, not full");
    assert_eq!(top[0].search_coverage, SearchCoverage::TopLevelOnly);

    writer.shutdown();
}

/// Filters compose: kind + initiator + time range together select the one op that
/// satisfies all three, excluding ops that fail any single predicate.
#[test]
fn filters_compose() {
    let (store, writer, _dir) = fresh();
    // The target: a user delete at t=100.
    journal_op(
        &writer,
        "target",
        OpKind::Delete,
        Initiator::User,
        100,
        110,
        SearchCoverage::Full,
        None,
        vec![leaf(0, "/a", "x", RowRole::RollbackUnit)],
    );
    // Wrong initiator.
    journal_op(
        &writer,
        "ai",
        OpKind::Delete,
        Initiator::AiClient,
        101,
        111,
        SearchCoverage::Full,
        None,
        vec![leaf(0, "/a", "x", RowRole::RollbackUnit)],
    );
    // Wrong kind.
    journal_op(
        &writer,
        "copy",
        OpKind::Copy,
        Initiator::User,
        102,
        112,
        SearchCoverage::Full,
        None,
        vec![leaf(0, "/a", "x", RowRole::RollbackUnit)],
    );
    // Out of the time range.
    journal_op(
        &writer,
        "old",
        OpKind::Delete,
        Initiator::User,
        1,
        2,
        SearchCoverage::Full,
        None,
        vec![leaf(0, "/a", "x", RowRole::RollbackUnit)],
    );

    let filters = OperationSearchFilters {
        kinds: vec![OpKind::Delete],
        initiator: Some(Initiator::User),
        since: Some(50),
        until: Some(200),
        ..Default::default()
    };
    let hits = search_operations(store.conn(), &filters, 50, 0).expect("search");
    assert_eq!(
        hits.iter().map(|o| o.op_id.as_str()).collect::<Vec<_>>(),
        vec!["target"],
        "only the op satisfying kind AND initiator AND time range"
    );

    writer.shutdown();
}

/// Paging over the recent feed is stable: page 1 then page 2 cover every op once,
/// with no duplicates or skips across the boundary.
#[test]
fn recent_paging_is_stable() {
    let (store, writer, _dir) = fresh();
    for i in 0..5 {
        journal_op(
            &writer,
            &format!("op-{i}"),
            OpKind::Delete,
            Initiator::User,
            i,
            i + 1,
            SearchCoverage::Full,
            None,
            vec![],
        );
    }

    let page1 = recent_operations(store.conn(), 2, 0).expect("page1");
    let page2 = recent_operations(store.conn(), 2, 2).expect("page2");
    let page3 = recent_operations(store.conn(), 2, 4).expect("page3");
    let ids: Vec<String> = page1
        .iter()
        .chain(&page2)
        .chain(&page3)
        .map(|o| o.op_id.clone())
        .collect();
    // Newest-first, every op exactly once.
    assert_eq!(ids, vec!["op-4", "op-3", "op-2", "op-1", "op-0"]);

    writer.shutdown();
}

/// `get_operation` returns the header plus a page of items in seq order with dir
/// prefixes resolved to full paths, and reports the total item count so a paged UI
/// knows more remain.
#[test]
fn get_operation_returns_header_and_paged_items() {
    let (store, writer, _dir) = fresh();
    journal_op(
        &writer,
        "op",
        OpKind::Delete,
        Initiator::User,
        10,
        20,
        SearchCoverage::Full,
        None,
        vec![
            leaf(0, "/home/docs", "a.txt", RowRole::RollbackUnit),
            leaf(1, "/home/docs", "b.txt", RowRole::RollbackUnit),
            leaf(2, "/home/docs", "c.txt", RowRole::RollbackUnit),
        ],
    );

    let detail = get_operation(store.conn(), "op", 2, 0).expect("get").expect("present");
    assert_eq!(detail.operation.op_id, "op");
    assert_eq!(detail.total_items, 3, "reports the full count, not the page size");
    assert_eq!(detail.items.len(), 2, "first page of two");
    assert_eq!(detail.items[0].seq, 0);
    assert_eq!(detail.items[0].source_path, "/home/docs/a.txt", "dir prefix resolved");
    assert_eq!(detail.items[0].source_volume_id, "vol-1");

    let page2 = get_operation(store.conn(), "op", 2, 2).expect("get").expect("present");
    assert_eq!(page2.items.len(), 1, "remaining item");
    assert_eq!(page2.items[0].seq, 2);
    assert_eq!(page2.items[0].source_path, "/home/docs/c.txt");

    assert!(
        get_operation(store.conn(), "absent", 10, 0).expect("get").is_none(),
        "absent op ⇒ None"
    );

    writer.shutdown();
}

/// A prefix name match spans the folded b-tree range: `report` matches
/// `report-2026.pdf` but not `annual.pdf`.
#[test]
fn prefix_name_match() {
    let (store, writer, _dir) = fresh();
    journal_op(
        &writer,
        "op",
        OpKind::Delete,
        Initiator::User,
        10,
        20,
        SearchCoverage::Full,
        None,
        vec![
            leaf(0, "/a", "Report-2026.pdf", RowRole::RollbackUnit),
            leaf(1, "/a", "annual.pdf", RowRole::RollbackUnit),
        ],
    );

    let filters = OperationSearchFilters {
        name: Some(NameFilter {
            text: "report".to_string(),
            match_kind: NameMatch::Prefix,
        }),
        ..Default::default()
    };
    let hits = search_operations(store.conn(), &filters, 50, 0).expect("search");
    assert_eq!(hits.len(), 1, "the op containing report-2026.pdf matches");
    assert_eq!(hits[0].op_id, "op");

    // A prefix that matches nothing returns nothing.
    let none = OperationSearchFilters {
        name: Some(NameFilter {
            text: "zzz".to_string(),
            match_kind: NameMatch::Prefix,
        }),
        ..Default::default()
    };
    assert!(
        search_operations(store.conn(), &none, 50, 0)
            .expect("search")
            .is_empty()
    );

    writer.shutdown();
}

/// Run `EXPLAIN QUERY PLAN` over the exact search SQL, joined into one string of
/// `detail` lines.
fn explain_plan(conn: &Connection, filters: &OperationSearchFilters) -> String {
    let (sql, params) = build_search_query(filters, 50, 0);
    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&explain_sql).expect("prepare explain");
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| row.get::<_, String>(3))
        .expect("explain rows");
    rows.map(|r| r.expect("detail")).collect::<Vec<_>>().join("\n")
}
