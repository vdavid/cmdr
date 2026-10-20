//! Undoing a trash moves back only the node the trash put there (`crates/cmdr-operation-log/DETAILS.md`
//! § "A trashed item must still be the one trashed"). The Trash hands a freed name to the
//! next item trashed under it, so once the trashed folder leaves the Trash, a different
//! folder can sit at its recorded in-Trash path (#535).

use std::path::Path;
use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::{InMemoryVolume, NodeIdentity, Volume};
use cmdr_operation_log::rollback::RollbackReport;
use cmdr_operation_log::types::{EntryType, ItemOutcome, OpKind, RollbackState, SkipReason};
use cmdr_operation_log::writer::JournalItem;

const IN_TRASH: &str = "/.Trash/Invoices";
const HOME: &str = "/Invoices";

/// The `rollback_unit` row a trash of `HOME` records: the in-Trash location as dest,
/// and the node it landed as when `identity` is `Some` (`None` is a row from before
/// the journal kept one).
fn trashed(entry_type: EntryType, size: i64, identity: Option<NodeIdentity>) -> JournalItem {
    JournalItem {
        entry_type,
        dest_identity: identity,
        ..file_unit(0, "v", HOME, "v", IN_TRASH, size)
    }
}

async fn identity_at(vol: &InMemoryVolume, path: &str) -> NodeIdentity {
    vol.node_identity(Path::new(path))
        .await
        .expect("stat")
        .expect("the double reports an identity")
}

/// A folder trashed to `IN_TRASH`, then gone from the Trash, and another folder of the
/// same name trashed since: it landed at the same in-Trash path. Returns the trashed
/// folder's identity, as the trash recorded it.
async fn stranger_took_the_name(v: &InMemoryVolume) -> NodeIdentity {
    mkdir(v, IN_TRASH).await;
    let trashed_folder = identity_at(v, IN_TRASH).await;
    v.delete(Path::new(IN_TRASH)).await.expect("gone from the Trash");
    mkdir(v, IN_TRASH).await;
    put(v, "/.Trash/Invoices/someone-elses.pdf", b"stranger").await;
    trashed_folder
}

fn only_skip_is(report: &RollbackReport, reason: SkipReason) -> bool {
    report.skips.len() == 1 && report.skips[0].reason == reason && report.skips[0].count == 1
}

#[tokio::test]
async fn undoing_a_trash_leaves_a_stranger_folder_that_took_the_name_in_the_trash() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    let recorded = stranger_took_the_name(&v).await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![trashed(EntryType::Dir, 64, Some(recorded))],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "{report:?}");
    assert!(only_skip_is(&report, SkipReason::Drift), "{:?}", report.skips);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert!(
        exists(&v, "/.Trash/Invoices/someone-elses.pdf").await,
        "the stranger stays in the Trash"
    );
    assert!(
        !exists(&v, HOME).await,
        "nothing came back under the trashed folder's name"
    );
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::Skipped);
    assert_eq!(items[0].rollback_skip_reason, Some(SkipReason::Drift));
}

/// A file gets the same check, since a size and date can match by chance.
#[tokio::test]
async fn undoing_a_trash_leaves_a_stranger_file_of_the_same_size_and_date() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, IN_TRASH, b"mine").await;
    let recorded = identity_at(&v, IN_TRASH).await;
    v.delete(Path::new(IN_TRASH)).await.expect("gone from the Trash");
    put(&v, IN_TRASH, b"them").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![trashed(EntryType::File, 4, Some(recorded))],
    );

    let report = rig.rollback("op").await;

    assert!(only_skip_is(&report, SkipReason::Drift), "{:?}", report.skips);
    assert_eq!(read(&v, IN_TRASH).await, b"them", "the stranger stays in the Trash");
    assert!(!exists(&v, HOME).await);
}

#[tokio::test]
async fn undoing_a_trash_restores_the_folder_it_put_there() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, IN_TRASH).await;
    put(&v, "/.Trash/Invoices/2026-10.pdf", b"mine").await;
    let recorded = identity_at(&v, IN_TRASH).await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![trashed(EntryType::Dir, 64, Some(recorded))],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert_eq!(
        read(&v, "/Invoices/2026-10.pdf").await,
        b"mine",
        "restored with its contents"
    );
    assert!(!exists(&v, IN_TRASH).await, "out of the Trash");
}

/// A row from before the journal kept identities can't tell a stranger from the folder
/// it trashed, and refusing every such undo would strand the user's older trashes, so
/// it keeps the path-only check it always had.
#[tokio::test]
async fn undoing_a_trash_recorded_without_an_identity_restores_what_sits_there() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, IN_TRASH).await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![trashed(EntryType::Dir, 64, None)],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert!(exists(&v, HOME).await);
}

/// The trash recorded an identity, but the volume answering now can't report one: no
/// proof it's the same folder, so it stays.
#[tokio::test]
async fn undoing_a_trash_whose_volume_cannot_report_an_identity_leaves_it() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, IN_TRASH).await;
    let recorded = identity_at(&v, IN_TRASH).await;
    // `RootFenced` forwards stats and renames but keeps the trait's "can't say" answer.
    rig.vm.register("v", RootFenced::over(&v, "/"));
    rig.seed(
        "op",
        OpKind::Trash,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![trashed(EntryType::Dir, 64, Some(recorded))],
    );

    let report = rig.rollback("op").await;

    assert!(
        only_skip_is(&report, SkipReason::UnverifiablePrecondition),
        "{:?}",
        report.skips
    );
    assert!(exists(&v, IN_TRASH).await, "left in the Trash");
    assert!(!exists(&v, HOME).await);
}
