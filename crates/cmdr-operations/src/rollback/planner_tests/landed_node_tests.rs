//! Undoing a copy or a move acts only on the node the operation landed, when its row
//! recorded which (`crates/cmdr-operation-log/DETAILS.md` § "A landed item must still
//! be the node that landed"). A file of the same size and date can take the landed
//! one's place, and a folder's recheck is existence alone, so without the node a
//! stranger under the recorded name is deleted (a copy) or carried off to the source
//! (a move).
//!
//! Each case comes in a pair: the row with an identity, and the same row without one
//! (written before the journal kept it, or landed where no stable identity exists),
//! which keeps exactly the checks it always had.

use std::path::Path;
use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::{InMemoryVolume, NodeIdentity, Volume};
use cmdr_operation_log::rollback::RollbackReport;
use cmdr_operation_log::types::{ItemOutcome, OpKind, RollbackState, SkipReason};
use cmdr_operation_log::writer::JournalItem;

const ORIGINAL: &str = "/DCIM/a.jpg";
const LANDED: &str = "/photos/a.jpg";

async fn identity_at(vol: &InMemoryVolume, path: &str) -> NodeIdentity {
    vol.node_identity(Path::new(path))
        .await
        .expect("stat")
        .expect("the double reports an identity")
}

/// Someone else's file takes the landed one's place: same name, same size, same date.
async fn replace_with_a_twin(vol: &InMemoryVolume, path: &str, bytes: &[u8]) {
    vol.delete(Path::new(path)).await.expect("the landed file leaves");
    put(vol, path, bytes).await;
}

fn only_skip_is(report: &RollbackReport, reason: SkipReason) -> bool {
    report.skips.len() == 1 && report.skips[0].reason == reason && report.skips[0].count == 1
}

/// The row a file landing at `LANDED` records, with the node it landed as when
/// `identity` is `Some`.
fn landed_file(identity: Option<NodeIdentity>) -> JournalItem {
    JournalItem {
        dest_identity: identity,
        ..file_unit(0, "v", ORIGINAL, "v", LANDED, 5)
    }
}

/// A finished copy of `ORIGINAL` to `LANDED` on one volume, the original still there.
/// Returns the node the copy landed as.
async fn copied() -> (Rig, Arc<InMemoryVolume>, NodeIdentity) {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, ORIGINAL, b"photo").await;
    put(&v, LANDED, b"photo").await;
    let landed = identity_at(&v, LANDED).await;
    rig.register("v", v.clone());
    (rig, v, landed)
}

fn seed_copy(rig: &Rig, identity: Option<NodeIdentity>) {
    rig.seed(
        "op",
        OpKind::Copy,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![landed_file(identity)],
    );
}

// ── Copy ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn undoing_a_copy_removes_the_node_it_landed() {
    let (rig, v, landed) = copied().await;
    seed_copy(&rig, Some(landed));

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(!exists(&v, LANDED).await, "the copy is gone");
    assert_eq!(read(&v, ORIGINAL).await, b"photo", "the original stays");
}

/// The copy was replaced by a different file that happens to share its size and date:
/// the snapshot matches, the node doesn't, and the stranger stays.
#[tokio::test]
async fn undoing_a_copy_keeps_a_twin_that_replaced_it() {
    let (rig, v, landed) = copied().await;
    replace_with_a_twin(&v, LANDED, b"other").await;
    seed_copy(&rig, Some(landed));

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 1), "{report:?}");
    assert!(only_skip_is(&report, SkipReason::Drift), "{:?}", report.skips);
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(read(&v, LANDED).await, b"other", "the stranger stays");
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::Skipped);
    assert_eq!(items[0].rollback_skip_reason, Some(SkipReason::Drift));
}

/// The same replacement under a row with no identity passes, as it always has: size
/// and date are all that row can check.
#[tokio::test]
async fn undoing_a_copy_recorded_without_an_identity_removes_what_matches_its_snapshot() {
    let (rig, v, _landed) = copied().await;
    replace_with_a_twin(&v, LANDED, b"other").await;
    seed_copy(&rig, None);

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert!(!exists(&v, LANDED).await);
}

/// The copy recorded an identity, but the volume answering now can't report one: no
/// proof it's the same file, so it stays.
#[tokio::test]
async fn undoing_a_copy_whose_volume_cannot_report_an_identity_keeps_it() {
    let (rig, v, landed) = copied().await;
    // `RootFenced` forwards stats and deletes but keeps the trait's "can't say" answer.
    rig.vm.register("v", RootFenced::over(&v, "/"));
    seed_copy(&rig, Some(landed));

    let report = rig.rollback("op").await;

    assert!(
        only_skip_is(&report, SkipReason::UnverifiablePrecondition),
        "{:?}",
        report.skips
    );
    assert_eq!(read(&v, LANDED).await, b"photo", "left where it is");
}

/// A copy already gone is the end state the undo wanted, identity or not.
#[tokio::test]
async fn undoing_a_copy_that_is_already_gone_counts_it_reversed() {
    let (rig, v, landed) = copied().await;
    v.delete(Path::new(LANDED)).await.expect("gone");
    seed_copy(&rig, Some(landed));

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::RolledBack);
}

// ── Move ─────────────────────────────────────────────────────────────────────

/// A finished move of `ORIGINAL` to `LANDED`. Returns the node that landed.
async fn moved_file() -> (Rig, Arc<InMemoryVolume>, NodeIdentity) {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, "/DCIM").await;
    put(&v, LANDED, b"photo").await;
    let landed = identity_at(&v, LANDED).await;
    rig.register("v", v.clone());
    (rig, v, landed)
}

fn seed_move(rig: &Rig, row: JournalItem) {
    rig.seed(
        "op",
        OpKind::Move,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![row],
    );
}

#[tokio::test]
async fn undoing_a_move_takes_back_the_node_it_landed() {
    let (rig, v, landed) = moved_file().await;
    seed_move(&rig, landed_file(Some(landed)));

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(read(&v, ORIGINAL).await, b"photo", "back where it came from");
    assert!(!exists(&v, LANDED).await);
}

#[tokio::test]
async fn undoing_a_move_leaves_a_twin_that_replaced_the_moved_file() {
    let (rig, v, landed) = moved_file().await;
    replace_with_a_twin(&v, LANDED, b"other").await;
    seed_move(&rig, landed_file(Some(landed)));

    let report = rig.rollback("op").await;

    assert!(only_skip_is(&report, SkipReason::Drift), "{:?}", report.skips);
    assert_eq!(read(&v, LANDED).await, b"other", "the stranger stays put");
    assert!(!exists(&v, ORIGINAL).await, "nothing was carried to the source");
}

#[tokio::test]
async fn undoing_a_move_recorded_without_an_identity_takes_back_what_matches_its_snapshot() {
    let (rig, v, _landed) = moved_file().await;
    replace_with_a_twin(&v, LANDED, b"other").await;
    seed_move(&rig, landed_file(None));

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert_eq!(read(&v, ORIGINAL).await, b"other");
}

/// A folder's snapshot is existence alone, so the node is the only thing that tells
/// the moved folder from another one made under its name since.
#[tokio::test]
async fn undoing_a_move_leaves_another_folder_made_under_the_moved_ones_name() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, "/album").await;
    let landed = identity_at(&v, "/album").await;
    v.delete(Path::new("/album")).await.expect("the moved folder leaves");
    mkdir(&v, "/album").await;
    put(&v, "/album/someone-elses.jpg", b"stranger").await;
    rig.register("v", v.clone());
    seed_move(
        &rig,
        JournalItem {
            dest_identity: Some(landed),
            ..folder_unit(0, "v", "/trip", "/album")
        },
    );

    let report = rig.rollback("op").await;

    assert!(only_skip_is(&report, SkipReason::Drift), "{:?}", report.skips);
    assert!(exists(&v, "/album/someone-elses.jpg").await, "the stranger stays put");
    assert!(!exists(&v, "/trip").await);
}

#[tokio::test]
async fn undoing_a_folder_move_recorded_without_an_identity_takes_back_what_sits_there() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, "/album").await;
    rig.register("v", v.clone());
    seed_move(&rig, folder_unit(0, "v", "/trip", "/album"));

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (1, 0), "{report:?}");
    assert!(exists(&v, "/trip").await);
}

// ── A folder the operation created ───────────────────────────────────────────

/// No record point writes an identity on a created folder's row today, but a row that
/// carries one is held to it like any other: an identity is never ignored.
#[tokio::test]
async fn undoing_a_copy_leaves_another_empty_folder_made_under_the_created_ones_name() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    mkdir(&v, "/album").await;
    let created = identity_at(&v, "/album").await;
    v.delete(Path::new("/album")).await.expect("the created folder leaves");
    mkdir(&v, "/album").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![JournalItem {
            dest_identity: Some(created),
            ..dir_unit(0, "v", "/album")
        }],
    );

    let report = rig.rollback("op").await;

    assert!(only_skip_is(&report, SkipReason::Drift), "{:?}", report.skips);
    assert!(exists(&v, "/album").await, "the other folder stays");
}
