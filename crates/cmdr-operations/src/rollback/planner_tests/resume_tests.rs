//! Finishing a partial rollback: a second run over a `partially_rolled_back` op
//! re-attempts what the first run held back, with a fresh recheck, and never claims
//! the whole op came back on a pass that proved nothing.

use std::path::Path;
use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_operation_log::types::{ItemOutcome, OpKind, RollbackState, SkipReason};
use cmdr_operation_log::writer::JournalItem;

/// A rename row the operation itself skipped (its destination was taken), with no
/// rollback verdict: a log entry, never something to reverse.
fn skipped_by_the_operation(seq: i64, from: &str, to: &str) -> JournalItem {
    JournalItem {
        outcome: ItemOutcome::Skipped,
        ..file_unit(seq, "v", from, "v", to, 4)
    }
}

/// A rename of `/old.txt` to `/new.txt` whose undo finds `/old.txt` taken, next to a
/// row the rename itself skipped. The first rollback leaves the op partial.
async fn partially_rolled_back_rename() -> (Rig, Arc<InMemoryVolume>) {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, "/new.txt", b"data").await;
    put(&v, "/old.txt", b"someone-elses").await;
    put(&v, "/taken.txt", b"was-already-here").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "v", "/old.txt", "v", "/new.txt", 4),
            skipped_by_the_operation(1, "/never-moved.txt", "/taken.txt"),
        ],
    );

    let first = rig.rollback_as("op", "inv-1").await;
    assert_eq!(
        first.skipped, 1,
        "the occupied restore target holds the item back: {first:?}"
    );
    assert_eq!(first.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
    (rig, v)
}

#[tokio::test]
async fn finishing_a_partial_rollback_reverses_the_item_a_cleared_blocker_held_back() {
    let (rig, v) = partially_rolled_back_rename().await;
    v.delete(Path::new("/old.txt")).await.expect("clear the blocker");

    let second = rig.rollback_as("op", "inv-2").await;

    assert_eq!(second.reversed, 1, "the held-back item is re-attempted: {second:?}");
    assert_eq!(
        second.skipped, 0,
        "the operation's own skip is never retried: {second:?}"
    );
    assert_eq!(second.final_state, RollbackState::RolledBack);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::RolledBack);
    assert_eq!(
        read(&v, "/old.txt").await,
        b"data",
        "the file is back under its old name"
    );
    assert!(!exists(&v, "/new.txt").await);
    assert_eq!(
        read(&v, "/taken.txt").await,
        b"was-already-here",
        "the row the rename skipped moves nothing"
    );

    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::RolledBack, "history says it came back");
    assert_eq!(items[0].rollback_skip_reason, None, "and no longer explains a skip");
    assert_eq!(
        items[1].outcome,
        ItemOutcome::Skipped,
        "the operation's own skip stays a log entry"
    );
    assert_eq!(items[1].rollback_skip_reason, None);
}

#[tokio::test]
async fn finishing_a_partial_rollback_whose_blocker_remains_stays_partial() {
    let (rig, v) = partially_rolled_back_rename().await;

    let second = rig.rollback_as("op", "inv-2").await;

    assert_eq!(second.reversed, 0, "{second:?}");
    assert_eq!(
        second.skipped, 1,
        "the item is rechecked and held back again: {second:?}"
    );
    assert_eq!(second.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
    assert_eq!(
        read(&v, "/old.txt").await,
        b"someone-elses",
        "the blocker is never overwritten"
    );
    let items = rig.read_items("op");
    assert_eq!(items[0].outcome, ItemOutcome::Skipped);
    assert_eq!(items[0].rollback_skip_reason, Some(SkipReason::RestoreTargetOccupied));
}

/// A partial op whose held-back rows the run can't see (a skip recorded before the
/// journal kept a reason reads like the operation's own skip) gives the run nothing to
/// do. An empty pass proves nothing came back, so the op stays partial.
#[tokio::test]
async fn an_empty_pass_over_a_partial_rollback_stays_partial() {
    let rig = Rig::new();
    let v = Arc::new(InMemoryVolume::new("V"));
    put(&v, "/a.txt", b"data").await;
    put(&v, "/held-back.txt", b"data").await;
    rig.register("v", v.clone());
    rig.seed(
        "op",
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::PartiallyRolledBack,
        vec![
            JournalItem {
                outcome: ItemOutcome::RolledBack,
                ..file_unit(0, "v", "/a.txt", "v", "/b.txt", 4)
            },
            skipped_by_the_operation(1, "/c.txt", "/held-back.txt"),
        ],
    );

    let report = rig.rollback("op").await;

    assert_eq!((report.reversed, report.skipped), (0, 0), "{report:?}");
    assert_eq!(report.final_state, RollbackState::PartiallyRolledBack);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
}
