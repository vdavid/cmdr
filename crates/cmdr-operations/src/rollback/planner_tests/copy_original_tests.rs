//! Undoing a copy deletes the copy permanently, so it first proves the original is
//! still where it was copied from (`crates/cmdr-operation-log/DETAILS.md` § "A copy's
//! original must still be there"). Copy photos off a card, delete them there, then
//! roll the copy back: without this check the undo removed the only copy left.

use std::path::Path;
use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_operation_log::types::{ItemOutcome, OpKind, RollbackState, SkipReason};

/// A finished copy of `/DCIM/a.jpg` from the card to `/photos/a.jpg` on the disk, with
/// the copy still exactly as the operation wrote it. The card holds the original
/// only when `original` is `Some`.
async fn copied_photo(original: Option<&[u8]>) -> (Rig, Arc<InMemoryVolume>, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let card = Arc::new(InMemoryVolume::new("Card"));
    let disk = Arc::new(InMemoryVolume::new("Disk"));
    if let Some(bytes) = original {
        put(&card, "/DCIM/a.jpg", bytes).await;
    }
    put(&disk, "/photos/a.jpg", b"photo").await;
    rig.register("card", card.clone());
    rig.register("disk", disk.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "card",
        Some("disk"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "card", "/DCIM/a.jpg", "disk", "/photos/a.jpg", 5)],
    );
    (rig, card, disk)
}

fn only_skip_is(report: &cmdr_operation_log::rollback::RollbackReport, reason: SkipReason) -> bool {
    report.skips.len() == 1 && report.skips[0].reason == reason && report.skips[0].count == 1
}

#[tokio::test]
async fn undoing_a_copy_whose_original_is_gone_keeps_the_copy() {
    let (rig, _card, disk) = copied_photo(None).await;

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "{report:?}");
    assert!(only_skip_is(&report, SkipReason::OriginalGone), "{:?}", report.skips);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(read(&disk, "/photos/a.jpg").await, b"photo", "the only copy left stays");
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::Skipped);
    assert_eq!(items[0].rollback_skip_reason, Some(SkipReason::OriginalGone));
}

/// The original is still at its path but is no longer the file that was copied (its
/// size changed): the copy is the only one left of what was copied, so it stays.
#[tokio::test]
async fn undoing_a_copy_whose_original_changed_size_keeps_the_copy() {
    let (rig, _card, disk) = copied_photo(Some(b"edited-since")).await;

    let report = rig.rollback("op").await;

    assert!(only_skip_is(&report, SkipReason::OriginalGone), "{:?}", report.skips);
    assert!(exists(&disk, "/photos/a.jpg").await);
}

#[tokio::test]
async fn undoing_a_copy_whose_original_is_still_there_removes_the_copy() {
    let (rig, card, disk) = copied_photo(Some(b"photo")).await;

    let report = rig.rollback("op").await;

    assert_eq!(report.final_state, RollbackState::RolledBack, "{report:?}");
    assert!(!exists(&disk, "/photos/a.jpg").await);
    assert_eq!(read(&card, "/DCIM/a.jpg").await, b"photo", "the original is untouched");
}

/// A source volume the planner can't reach for this item can't prove the original
/// is there, so the copy stays: never delete on doubt. (The op-level gate refuses an
/// unregistered volume before any item runs; this is the planner's own floor.)
#[tokio::test]
async fn undoing_a_copy_whose_source_volume_is_missing_keeps_the_copy() {
    let rig = Rig::new();
    let disk = Arc::new(InMemoryVolume::new("Disk"));
    put(&disk, "/photos/a.jpg", b"photo").await;
    rig.register("disk", disk.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "card",
        Some("disk"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "card", "/DCIM/a.jpg", "disk", "/photos/a.jpg", 5)],
    );

    let report = rig.rollback("op").await;

    assert!(
        only_skip_is(&report, SkipReason::UnverifiablePrecondition),
        "{:?}",
        report.skips
    );
    assert!(exists(&disk, "/photos/a.jpg").await);
}

/// A source volume whose root was narrowed answers `NotFound` for a path above it.
/// That's not proof the original is gone, only that it's out of reach: unverifiable,
/// and the copy stays.
#[tokio::test]
async fn undoing_a_copy_whose_original_is_out_of_the_source_roots_reach_keeps_the_copy() {
    let rig = Rig::new();
    let server = Arc::new(InMemoryVolume::new("NAS"));
    put(&server, "/share/old/a.jpg", b"photo").await;
    let disk = Arc::new(InMemoryVolume::new("Disk"));
    put(&disk, "/photos/a.jpg", b"photo").await;
    rig.vm.register("nas", RootFenced::over(&server, "/share/new"));
    rig.register("disk", disk.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "nas",
        Some("disk"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "nas", "/share/old/a.jpg", "disk", "/photos/a.jpg", 5)],
    );

    let report = rig.rollback("op").await;

    assert!(
        only_skip_is(&report, SkipReason::UnverifiablePrecondition),
        "{:?}",
        report.skips
    );
    assert!(exists(&disk, "/photos/a.jpg").await);
}

/// "Finish rolling back" rechecks the original: once it's back, the copy goes.
#[tokio::test]
async fn finishing_the_rollback_removes_the_copy_once_the_original_is_back() {
    let (rig, card, disk) = copied_photo(None).await;
    let first = rig.rollback_as("op", "inv-1").await;
    assert_eq!(first.final_state, RollbackState::PartiallyRolledBack);

    card.create_directory(Path::new("/DCIM")).await.ok();
    put(&card, "/DCIM/a.jpg", b"photo").await;
    let second = rig.rollback_as("op", "inv-2").await;

    assert_eq!(second.final_state, RollbackState::RolledBack, "{second:?}");
    assert!(!exists(&disk, "/photos/a.jpg").await);
}
