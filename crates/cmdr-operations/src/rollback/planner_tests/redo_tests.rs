//! Rolling back an undo (a redo) of a move or a rename, as it behaves today: the
//! inverse op opens `rollbackable` (`inverse_kind` makes it a move or a rename), yet
//! its rows carry no dest (`bookkeeping::inverse_item_row`), so `restore_move` holds
//! every row back as `Failed`. It fails safe: nothing moves, and the undo lands
//! partial.
//!
//! ❗ This pins a known gap, not a promise. The fix (record the original's source as
//! the inverse row's dest) turns on a redo that has never worked; when it lands, these
//! assertions flip on purpose.

use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_operation_log::types::{ItemOutcome, OpKind, RollbackState, SkipReason};

#[tokio::test]
async fn rolling_back_the_undo_of_a_move_or_rename_holds_every_row_back_and_moves_nothing() {
    for (kind, from, to) in [
        (OpKind::Move, "/src/a.txt", "/dst/a.txt"),
        (OpKind::Rename, "/photo.jpg", "/image.jpg"),
    ] {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        put(&v, to, b"data").await;
        rig.register("v", v.clone());
        rig.seed(
            "op",
            kind,
            "v",
            Some("v"),
            RollbackState::Rollbackable,
            vec![file_unit(0, "v", from, "v", to, 4)],
        );
        let undo = rig.rollback_as("op", "undo").await;
        assert_eq!(undo.final_state, RollbackState::RolledBack, "{kind:?}");
        assert!(exists(&v, from).await, "{kind:?}: the undo put it back");

        let undo_op = rig.read_op("undo");
        assert_eq!(undo_op.kind, kind, "{kind:?}: the undo is itself a {kind:?}");
        assert_eq!(
            undo_op.rollback_state,
            RollbackState::Rollbackable,
            "{kind:?}: and offers to be rolled back"
        );

        let redo = rig.rollback_as("undo", "redo").await;

        assert_eq!((redo.reversed, redo.skipped), (0, 1), "{kind:?}: {redo:?}");
        assert!(
            redo.skips.iter().all(|s| s.reason == SkipReason::Failed),
            "{kind:?}: {redo:?}"
        );
        assert_eq!(redo.final_state, RollbackState::PartiallyRolledBack, "{kind:?}");
        let rows = rig.read_items("undo");
        assert!(
            rows.iter()
                .all(|r| r.outcome == ItemOutcome::Skipped && r.rollback_skip_reason == Some(SkipReason::Failed)),
            "{kind:?}: {rows:?}"
        );
        assert!(exists(&v, from).await, "{kind:?}: nothing moved");
        assert!(!exists(&v, to).await, "{kind:?}: nothing moved");
    }
}
