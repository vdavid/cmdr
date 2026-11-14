//! `landed_at`: an operation is named only for the node it recorded landing, the
//! lookup is served by the destination-name index, and every way of not knowing
//! (replaced since, no node recorded, nothing at the path) has its own answer.

use std::path::Path;

use cmdr_fs::volume::NodeIdentity;
use rusqlite::Connection;

use super::*;
use crate::store::{OperationLogStore, operation_log_db_path};
use crate::types::{EntryType, ExecutionStatus, ItemOutcome, RollbackState, RowRole, SearchCoverage};
use crate::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

const LANDED: &str = "/Users/me/Documents/report.pdf";

fn fresh() -> (OperationLogStore, OperationLogWriter, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let store = OperationLogStore::open(&path, &|_| None).expect("open store");
    let writer = OperationLogWriter::spawn(&path).expect("spawn writer");
    (store, writer, dir)
}

fn node(inode: u64) -> NodeIdentity {
    NodeIdentity {
        inode,
        birth_nanos: 1_700_000_000_000_000_000 + inode as i64,
    }
}

fn split(path: &str) -> (String, String) {
    let path = Path::new(path);
    (
        path.parent().expect("a folder").to_string_lossy().into_owned(),
        path.file_name().expect("a name").to_string_lossy().into_owned(),
    )
}

/// One item that came from `source` on `nas` and landed at `dest` on `root`, as
/// the node `landed` when the row recorded one.
fn item(source: &str, dest: &str, landed: Option<NodeIdentity>) -> JournalItem {
    let (source_dir, source_name) = split(source);
    let (dest_dir, dest_name) = split(dest);
    JournalItem {
        seq: 0,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: "nas".to_string(),
        source_dir,
        source_name,
        dest_volume_id: Some("root".to_string()),
        dest_dir: Some(dest_dir),
        dest_name: Some(dest_name),
        size: Some(1),
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
        dest_identity: landed,
        tags: None,
    }
}

fn open(writer: &OperationLogWriter, op_id: &str, kind: OpKind, initiator: Initiator, started_at: i64) {
    writer
        .open_operation(OpenOperation {
            op_id: op_id.into(),
            kind,
            initiator,
            source_volume_id: Some("nas".to_string()),
            dest_volume_id: Some("root".to_string()),
            item_count: 1,
            started_at,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
}

/// Journal one finished operation that ended ten seconds after it started.
fn journal(
    writer: &OperationLogWriter,
    op_id: &str,
    kind: OpKind,
    initiator: Initiator,
    started_at: i64,
    items: Vec<JournalItem>,
) {
    open(writer, op_id, kind, initiator, started_at);
    writer.record_items(op_id, items).expect("record");
    writer
        .finalize_operation(FinalizeOperation {
            op_id: op_id.to_string(),
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::Rollbackable,
            not_rollbackable_reason: None,
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: started_at + 10,
            item_count: None,
            items_done: 1,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
    writer.flush_blocking().expect("flush");
}

fn copied_from_the_nas(writer: &OperationLogWriter, op_id: &str, started_at: i64, landed: Option<NodeIdentity>) {
    journal(
        writer,
        op_id,
        OpKind::Copy,
        Initiator::User,
        started_at,
        vec![item("smb://nas/share/reports/report.pdf", LANDED, landed)],
    );
}

fn ask(conn: &Connection, path: &str, node: NodeIdentity) -> LandedAt {
    landed_at(conn, "root", Path::new(path), node).expect("the read")
}

#[test]
fn the_node_an_operation_landed_names_that_operation() {
    let (store, writer, _dir) = fresh();
    journal(
        &writer,
        "op",
        OpKind::Copy,
        Initiator::Agent,
        100,
        vec![item("smb://nas/share/reports/report.pdf", LANDED, Some(node(7)))],
    );

    assert_eq!(
        ask(store.conn(), LANDED, node(7)),
        LandedAt::ByOperation {
            kind: OpKind::Copy,
            initiator: Initiator::Agent,
            at: 110,
            source_volume_id: "nas".to_string(),
            source_path: "smb://nas/share/reports/report.pdf".to_string(),
            dates_changed: false,
        }
    );
    writer.shutdown();
}

/// On APFS, setting a file's modified time before its birth moves the birth back
/// (`touch -t`, `rsync -t`, a sync client restoring dates): same inode, earlier
/// birth (verified on macOS 27, APFS, `stat` before and after `touch -t
/// 202001010000`, 2026-11-14). That's the node Cmdr landed with its dates changed,
/// ❌ never "replaced since". A birth that moved LATER is a new node: nothing
/// moves a birth time forward.
#[test]
fn a_node_whose_birth_moved_earlier_is_the_same_item_with_its_dates_changed() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, Some(node(7)));
    let backdated = NodeIdentity {
        inode: 7,
        birth_nanos: node(7).birth_nanos - 86_400 * 1_000_000_000,
    };

    let LandedAt::ByOperation { dates_changed, .. } = ask(store.conn(), LANDED, backdated) else {
        panic!("the backdated node is the one the copy landed");
    };
    assert!(dates_changed);
    writer.shutdown();
}

/// Cmdr moved the item here, an undo carried it back, and something else moved
/// it here again: the node matches the move's row, but that landing was undone,
/// so the move doesn't explain how it's here now.
#[test]
fn a_landing_an_undo_reversed_credits_nothing() {
    let (store, writer, _dir) = fresh();
    let mut moved = item("/Users/me/Inbox/report.pdf", LANDED, Some(node(7)));
    moved.outcome = ItemOutcome::RolledBack;
    journal(&writer, "op", OpKind::Move, Initiator::User, 100, vec![moved]);

    assert_eq!(ask(store.conn(), LANDED, node(7)), LandedAt::NoRecord);
    writer.shutdown();
}

/// The honesty case: the file was replaced since, so the name still matches and
/// the node doesn't. Naming the copy here would say a stranger came from the NAS.
#[test]
fn a_file_replaced_since_is_never_the_operations() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, Some(node(7)));

    assert_eq!(ask(store.conn(), LANDED, node(8)), LandedAt::NodeChangedSince);
    // The same inode reused by a later file is another node: the birth time differs.
    let reused = NodeIdentity {
        inode: 7,
        birth_nanos: node(7).birth_nanos + 1,
    };
    assert_eq!(ask(store.conn(), LANDED, reused), LandedAt::NodeChangedSince);
    writer.shutdown();
}

/// A row from before the journal kept nodes, or one landed where no lasting
/// identity exists, can't prove anything about the file at its path.
#[test]
fn a_row_that_recorded_no_node_answers_no_record() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, None);

    assert_eq!(ask(store.conn(), LANDED, node(7)), LandedAt::NoRecord);
    writer.shutdown();
}

/// Renamed after it landed: the node is the one Cmdr copied, but no row lands at
/// the new name, and the read never goes looking for a node by itself.
#[test]
fn an_item_renamed_after_it_landed_has_no_record_at_its_new_name() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, Some(node(7)));

    assert_eq!(
        ask(store.conn(), "/Users/me/Documents/report-final.pdf", node(7)),
        LandedAt::NoRecord
    );
    writer.shutdown();
}

#[test]
fn the_same_name_elsewhere_is_no_record() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, Some(node(7)));
    let conn = store.conn();

    // Another folder, a folder nothing was ever recorded in, another volume, and
    // a path with no folder at all.
    assert_eq!(ask(conn, "/Users/me/report.pdf", node(7)), LandedAt::NoRecord);
    assert_eq!(ask(conn, "/Users/me/Elsewhere/report.pdf", node(7)), LandedAt::NoRecord);
    assert_eq!(
        landed_at(conn, "usb", Path::new(LANDED), node(7)).expect("the read"),
        LandedAt::NoRecord
    );
    assert_eq!(ask(conn, "/", node(7)), LandedAt::NoRecord);
    writer.shutdown();
}

/// The folder is matched by its exact names (two spellings can be two folders on
/// a case-sensitive disk), the item's own name folded, as the index stores it.
#[test]
fn the_item_name_matches_folded_and_its_folder_exactly() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, Some(node(7)));
    let conn = store.conn();

    assert!(matches!(
        ask(conn, "/Users/me/Documents/Report.PDF", node(7)),
        LandedAt::ByOperation { .. }
    ));
    assert_eq!(ask(conn, "/Users/me/documents/report.pdf", node(7)), LandedAt::NoRecord);
    writer.shutdown();
}

/// Two operations landed the same node at the path (moved away and back): the
/// newest one is the story of how it got here.
#[test]
fn the_newest_operation_that_landed_the_node_wins() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op-old", 100, Some(node(7)));
    journal(
        &writer,
        "op-new",
        OpKind::Move,
        Initiator::User,
        500,
        vec![item("/Users/me/Desktop/report.pdf", LANDED, Some(node(7)))],
    );

    let LandedAt::ByOperation { kind, at, .. } = ask(store.conn(), LANDED, node(7)) else {
        panic!("the node is the one both operations landed");
    };
    assert_eq!((kind, at), (OpKind::Move, 510));
    writer.shutdown();
}

/// A later operation landed another file at the path, which then left, and the
/// earlier one's file is back: the earlier operation is still how THIS node got
/// here, and the later row doesn't hide it.
#[test]
fn an_older_row_for_this_node_is_found_behind_a_newer_row_for_another() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op-old", 100, Some(node(7)));
    copied_from_the_nas(&writer, "op-new", 500, Some(node(9)));
    copied_from_the_nas(&writer, "op-newest", 900, None);

    let LandedAt::ByOperation { at, .. } = ask(store.conn(), LANDED, node(7)) else {
        panic!("the older operation landed this node");
    };
    assert_eq!(at, 110);
    // And a node none of them recorded was replaced since, whatever the newest
    // row (which recorded nothing) would say by itself.
    assert_eq!(ask(store.conn(), LANDED, node(1)), LandedAt::NodeChangedSince);
    writer.shutdown();
}

/// An operation still running has no end yet; its start stands in.
#[test]
fn an_unfinished_operation_answers_with_its_start() {
    let (store, writer, _dir) = fresh();
    open(&writer, "op", OpKind::Copy, Initiator::User, 100);
    writer
        .record_items(
            "op",
            vec![item("smb://nas/share/reports/report.pdf", LANDED, Some(node(7)))],
        )
        .expect("record");
    writer.flush_blocking().expect("flush");

    let LandedAt::ByOperation { at, .. } = ask(store.conn(), LANDED, node(7)) else {
        panic!("the row is there");
    };
    assert_eq!(at, 100);
    writer.shutdown();
}

/// A rollback's own rows never answer: its original carries the story.
#[test]
fn a_rollbacks_row_is_never_how_an_item_got_somewhere() {
    let (store, writer, _dir) = fresh();
    // The move the rollback undoes took the file AWAY from `LANDED`; the rollback
    // puts it back, and its row is the only one landing there.
    journal(
        &writer,
        "original",
        OpKind::Move,
        Initiator::User,
        50,
        vec![item(LANDED, "/Users/me/Desktop/report.pdf", Some(node(7)))],
    );
    writer
        .open_operation(OpenOperation {
            op_id: "inverse".into(),
            kind: OpKind::Move,
            initiator: Initiator::User,
            source_volume_id: Some("nas".to_string()),
            dest_volume_id: Some("root".to_string()),
            item_count: 1,
            started_at: 100,
            rolls_back_op_id: Some(Into::into("original".to_string())),
            execution_status: ExecutionStatus::Running,
        })
        .expect("open the inverse");
    writer
        .record_items(
            "inverse",
            vec![item("/Users/me/Desktop/report.pdf", LANDED, Some(node(7)))],
        )
        .expect("record");
    writer.flush_blocking().expect("flush");
    let rows_landing_there: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM operation_items WHERE op_id = 'inverse' AND dest_name_folded = 'report.pdf'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(rows_landing_there, 1, "the rollback's row is in the journal");

    assert_eq!(ask(store.conn(), LANDED, node(7)), LandedAt::NoRecord);
    writer.shutdown();
}

/// The lookup is served by the destination-name index and the `operations` key,
/// never a walk of every item row: a journal holds millions.
#[test]
fn the_lookup_is_index_served() {
    let (store, writer, _dir) = fresh();
    copied_from_the_nas(&writer, "op", 100, Some(node(7)));

    let mut stmt = store
        .conn()
        .prepare(&format!("EXPLAIN QUERY PLAN {LANDED_AT_SQL}"))
        .expect("prepare explain");
    let plan = stmt
        .query_map(rusqlite::params!["report.pdf", 1, 64, "rolled_back"], |row| {
            row.get::<_, String>(3)
        })
        .expect("explain rows")
        .map(|row| row.expect("detail"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plan.contains("operation_items_dest_name"),
        "the folded destination name's index serves the item lookup; plan was:\n{plan}"
    );
    // A full table scan is a bare "SCAN <table>" with no index (`query::tests`'
    // `delete_dog_jpg_is_index_served` has the reasoning).
    for line in plan.lines() {
        let step = line.trim();
        assert!(
            !step.starts_with("SCAN") || step.contains("USING"),
            "a full table scan: {step}\nfull plan:\n{plan}"
        );
    }
    writer.shutdown();
}
