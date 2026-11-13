//! What the card says of the log's answer: the operation's kind and starter in
//! the card's words, where the item came from, and every way of not knowing.

use std::path::Path;

use cmdr_about::fact::{Fact, UnknownWhy};
use cmdr_about::provenance::{CmdrOperation, LandedBy, StartedBy};
use cmdr_about::text::ItemText;
use cmdr_fs::volume::NodeIdentity;
use cmdr_operation_log::store::{OperationLogStore, operation_log_db_path};
use cmdr_operation_log::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage,
};
use cmdr_operation_log::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

use super::*;
use crate::test_support::TestDir;

const LANDED: &str = "/Users/me/Documents/report.pdf";

fn node(inode: u64) -> NodeIdentity {
    NodeIdentity {
        inode,
        birth_nanos: 1_700_000_000_000_000_000,
    }
}

/// A journal in a scratch folder. The folder lives as long as the value.
struct Journal {
    writer: OperationLogWriter,
    dir: TestDir,
}

impl Journal {
    fn new(label: &str) -> Self {
        let dir = TestDir::new(label);
        let db = operation_log_db_path(&dir);
        OperationLogStore::open(&db, &|_| None).expect("the store opens");
        Self {
            writer: OperationLogWriter::spawn(&db).expect("the writer starts"),
            dir,
        }
    }

    fn history(&self, volume_id: &str) -> History {
        History {
            db: operation_log_db_path(&self.dir),
            volume_id: volume_id.to_string(),
        }
    }

    /// One finished operation with one item, from `source` on `source_volume` to
    /// `dest` on `dest_volume`, ending at `started_at + 10`.
    #[allow(clippy::too_many_arguments, reason = "the fields of one journaled operation")]
    fn landed(
        &self,
        op_id: &str,
        kind: OpKind,
        initiator: Initiator,
        started_at: i64,
        (source_volume, source): (&str, &str),
        (dest_volume, dest): (&str, &str),
        landed: Option<NodeIdentity>,
    ) {
        let split = |path: &str| {
            let path = Path::new(path);
            (
                path.parent().expect("a folder").to_string_lossy().into_owned(),
                path.file_name().expect("a name").to_string_lossy().into_owned(),
            )
        };
        let (source_dir, source_name) = split(source);
        let (dest_dir, dest_name) = split(dest);
        self.writer
            .open_operation(OpenOperation {
                op_id: op_id.into(),
                kind,
                initiator,
                source_volume_id: Some(source_volume.to_string()),
                dest_volume_id: Some(dest_volume.to_string()),
                item_count: 1,
                started_at,
                rolls_back_op_id: None,
                execution_status: ExecutionStatus::Running,
            })
            .expect("open");
        self.writer
            .record_items(
                op_id,
                vec![JournalItem {
                    seq: 0,
                    entry_type: EntryType::File,
                    row_role: RowRole::RollbackUnit,
                    source_volume_id: source_volume.to_string(),
                    source_dir,
                    source_name,
                    dest_volume_id: Some(dest_volume.to_string()),
                    dest_dir: Some(dest_dir),
                    dest_name: Some(dest_name),
                    size: Some(1),
                    mtime: None,
                    outcome: ItemOutcome::Done,
                    overwrote: false,
                    dest_identity: landed,
                    tags: None,
                }],
            )
            .expect("record");
        self.writer
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
        self.writer.flush_blocking().expect("flush");
    }
}

fn names(volume_id: &str) -> Option<String> {
    (volume_id == "nas").then(|| "Naspolya".to_string())
}

fn ask(history: &History, path: &str, node: Option<NodeIdentity>) -> Fact<CmdrOperation> {
    cmdr_operation_naming(history, path, node, &names)
}

fn why(fact: &Fact<CmdrOperation>) -> Option<UnknownWhy> {
    match fact {
        Fact::Unknown(unknown) => Some(unknown.why()),
        _ => None,
    }
}

#[test]
fn a_copy_off_another_volume_names_where_it_came_from() {
    let journal = Journal::new("about-history-copy");
    journal.landed(
        "op",
        OpKind::Copy,
        Initiator::User,
        100,
        ("nas", "smb://nas/share/reports/report.pdf"),
        ("root", LANDED),
        Some(node(7)),
    );

    assert_eq!(
        ask(&journal.history("root"), LANDED, Some(node(7))),
        Fact::Known(CmdrOperation {
            kind: LandedBy::Copy,
            at: 110,
            initiator: StartedBy::You,
            source_path: Some(ItemText::new("smb://nas/share/reports/report.pdf")),
            source_volume: Some(ItemText::new("Naspolya")),
        })
    );
}

#[test]
fn a_move_on_the_same_volume_names_no_volume() {
    let journal = Journal::new("about-history-move");
    journal.landed(
        "op",
        OpKind::Move,
        Initiator::Agent,
        100,
        ("root", "/Users/me/Downloads/report.pdf"),
        ("root", LANDED),
        Some(node(7)),
    );

    let Fact::Known(operation) = ask(&journal.history("root"), LANDED, Some(node(7))) else {
        panic!("the move landed this node");
    };
    assert_eq!(operation.kind, LandedBy::Move);
    assert_eq!(operation.initiator, StartedBy::AskCmdr);
    assert_eq!(
        operation.source_path,
        Some(ItemText::new("/Users/me/Downloads/report.pdf"))
    );
    assert_eq!(operation.source_volume, None);
}

/// A source volume that isn't connected now has no name to give; the path still
/// says where the item came from.
#[test]
fn a_source_volume_that_is_not_connected_goes_unnamed() {
    let journal = Journal::new("about-history-gone");
    journal.landed(
        "op",
        OpKind::Copy,
        Initiator::AiClient,
        100,
        ("usb-gone", "/Volumes/Stick/report.pdf"),
        ("root", LANDED),
        Some(node(7)),
    );

    let Fact::Known(operation) = ask(&journal.history("root"), LANDED, Some(node(7))) else {
        panic!("the copy landed this node");
    };
    assert_eq!(operation.source_volume, None);
    assert_eq!(operation.source_path, Some(ItemText::new("/Volumes/Stick/report.pdf")));
    assert_eq!(operation.initiator, StartedBy::AiClient);
}

/// An extract is journaled as a copy whose source sits inside an archive.
#[test]
fn a_copy_out_of_an_archive_is_an_extract() {
    let journal = Journal::new("about-history-extract");
    journal.landed(
        "op",
        OpKind::Copy,
        Initiator::User,
        100,
        ("root", "/Users/me/Downloads/reports.zip/2026/report.pdf"),
        ("root", LANDED),
        Some(node(7)),
    );
    // A copy OF an archive is a copy.
    journal.landed(
        "op-zip",
        OpKind::Copy,
        Initiator::User,
        200,
        ("root", "/Users/me/Downloads/reports.zip"),
        ("root", "/Users/me/Documents/reports.zip"),
        Some(node(8)),
    );

    let history = journal.history("root");
    let kind = |path: &str, node: NodeIdentity| match ask(&history, path, Some(node)) {
        Fact::Known(operation) => operation.kind,
        other => panic!("landed: {other:?}"),
    };
    assert_eq!(kind(LANDED, node(7)), LandedBy::Extract);
    assert_eq!(kind("/Users/me/Documents/reports.zip", node(8)), LandedBy::Copy);
}

#[test]
fn an_item_in_the_trash_says_where_it_was_trashed_from() {
    let journal = Journal::new("about-history-trash");
    journal.landed(
        "op",
        OpKind::Trash,
        Initiator::User,
        100,
        ("root", LANDED),
        ("root", "/Users/me/.Trash/report.pdf"),
        Some(node(7)),
    );

    let Fact::Known(operation) = ask(&journal.history("root"), "/Users/me/.Trash/report.pdf", Some(node(7))) else {
        panic!("the trash landed this node");
    };
    assert_eq!(operation.kind, LandedBy::Trash);
    assert_eq!(operation.source_path, Some(ItemText::new(LANDED)));
}

#[test]
fn every_way_of_not_knowing_has_its_reason() {
    let journal = Journal::new("about-history-unknown");
    journal.landed(
        "op",
        OpKind::Copy,
        Initiator::User,
        100,
        ("nas", "smb://nas/share/report.pdf"),
        ("root", LANDED),
        Some(node(7)),
    );
    journal.landed(
        "op-old",
        OpKind::Copy,
        Initiator::User,
        50,
        ("nas", "smb://nas/share/old.pdf"),
        ("root", "/Users/me/Documents/old.pdf"),
        None,
    );
    let history = journal.history("root");

    // Replaced since: the honest answer, and a notable one.
    let replaced = ask(&history, LANDED, Some(node(8)));
    assert_eq!(why(&replaced), Some(UnknownWhy::NodeChangedSince));
    // A row that recorded no node, a path nothing landed at, and an item whose
    // filesystem gives no node to ask about: no record, ❌ never the operation.
    assert_eq!(
        why(&ask(&history, "/Users/me/Documents/old.pdf", Some(node(7)))),
        Some(UnknownWhy::NoRecord)
    );
    assert_eq!(
        why(&ask(&history, "/Users/me/Desktop/x.pdf", Some(node(7)))),
        Some(UnknownWhy::NoRecord)
    );
    assert_eq!(why(&ask(&history, LANDED, None)), Some(UnknownWhy::NoRecord));
}

/// No database yet (a fresh install that never journaled) is no record; a file
/// that isn't a journal is unreadable, said as such.
#[test]
fn a_missing_log_is_no_record_and_a_broken_one_is_unreadable() {
    let dir = TestDir::new("about-history-no-db");
    let missing = History {
        db: operation_log_db_path(&dir),
        volume_id: "root".to_string(),
    };
    assert_eq!(why(&ask(&missing, LANDED, Some(node(7)))), Some(UnknownWhy::NoRecord));

    std::fs::write(&missing.db, b"not a database").expect("write junk");
    assert_eq!(
        why(&ask(&missing, LANDED, Some(node(7)))),
        Some(UnknownWhy::Unparseable)
    );
}

/// An item on a volume of its own (an external disk) is looked up under the local
/// disk's id, where a local copy journals it, AND under its volume's id, where a
/// transfer off a share journals it. The newest operation wins.
#[test]
fn an_item_on_another_local_volume_is_found_under_either_id() {
    let journal = Journal::new("about-history-two-ids");
    let on_the_disk = "/Volumes/Photos/trip/a.jpg";
    journal.landed(
        "op-local",
        OpKind::Copy,
        Initiator::User,
        100,
        ("root", "/Users/me/Pictures/a.jpg"),
        ("root", on_the_disk),
        Some(node(7)),
    );
    let history = journal.history("disk-photos");
    let Fact::Known(found) = ask(&history, on_the_disk, Some(node(7))) else {
        panic!("found under the local disk's id");
    };
    assert_eq!(found.kind, LandedBy::Copy);
    assert_eq!(found.source_volume, None, "both ends were journaled under one id");

    journal.landed(
        "op-volume",
        OpKind::Move,
        Initiator::User,
        500,
        ("nas", "smb://nas/share/a.jpg"),
        ("disk-photos", on_the_disk),
        Some(node(7)),
    );
    let Fact::Known(newest) = ask(&history, on_the_disk, Some(node(7))) else {
        panic!("found under the volume's own id");
    };
    assert_eq!((newest.kind, newest.at), (LandedBy::Move, 510));
    assert_eq!(newest.source_volume, Some(ItemText::new("Naspolya")));

    // An operation under one id wins over "replaced since" under the other.
    journal.landed(
        "op-other-node",
        OpKind::Copy,
        Initiator::User,
        900,
        ("root", "/Users/me/Pictures/b.jpg"),
        ("root", on_the_disk),
        Some(node(9)),
    );
    assert!(matches!(ask(&history, on_the_disk, Some(node(7))), Fact::Known(_)));
    assert_eq!(
        why(&ask(&history, on_the_disk, Some(node(1)))),
        Some(UnknownWhy::NodeChangedSince)
    );
}
