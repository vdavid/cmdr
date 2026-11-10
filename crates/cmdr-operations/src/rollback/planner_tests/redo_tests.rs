//! A rollback is never itself rolled back (`cmdr-operation-log`'s `DETAILS.md` § "No
//! rollback of a rollback"): the gate refuses an inverse op typed, before anything is
//! set or started, whatever state the inverse stored, and the files and the journal
//! stay as the undo left them. The tag kind's cell is `tag_tests.rs`.

use std::sync::Arc;

use super::test_support::*;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_operation_log::rollback::RollbackRefusal;
use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind, RollbackState};
use cmdr_operation_log::writer::OpenOperation;

#[tokio::test]
async fn the_undo_of_a_move_rename_or_copy_is_never_rolled_back() {
    for (kind, from, to) in [
        (OpKind::Move, "/src/a.txt", "/dst/a.txt"),
        (OpKind::Rename, "/photo.jpg", "/image.jpg"),
        (OpKind::Copy, "/src/a.txt", "/dst/a.txt"),
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
        if kind == OpKind::Copy {
            rig.put_originals("op").await;
        }
        let undo = rig.rollback_as("op", "undo").await;
        assert_eq!(undo.final_state, RollbackState::RolledBack, "{kind:?}: {undo:?}");
        let undo_before = rig.read_op("undo");
        let undo_rows_before = rig.read_items("undo");
        let original_before = rig.read_op("op");

        assert_eq!(rig.refusal_of("undo"), RollbackRefusal::IsARollback, "{kind:?}");

        assert_eq!(
            rig.read_op("undo"),
            undo_before,
            "{kind:?}: the undo's header is untouched"
        );
        assert_eq!(rig.read_items("undo"), undo_rows_before, "{kind:?}: so are its rows");
        assert_eq!(rig.read_op("op"), original_before, "{kind:?}: and the original's state");
        assert!(exists(&v, from).await, "{kind:?}: nothing moved");
        assert!(!exists(&v, to).await, "{kind:?}: nothing came back");
    }
}

#[test]
fn an_undo_that_stored_rollbackable_is_still_refused_first() {
    // An inverse journaled before this rule may say `rollbackable`; the gate reads its
    // link to the original, never its stored state, and refuses ahead of every other
    // check (here, a volume that isn't connected).
    let rig = Rig::new();
    rig.seed(
        "op",
        OpKind::Move,
        "gone",
        Some("gone"),
        RollbackState::RolledBack,
        vec![],
    );
    rig.writer
        .open_operation(OpenOperation {
            op_id: "undo".into(),
            kind: OpKind::Move,
            initiator: Initiator::User,
            source_volume_id: Some("gone".to_string()),
            dest_volume_id: Some("gone".to_string()),
            item_count: 0,
            started_at: 100,
            rolls_back_op_id: Some("op".into()),
            execution_status: ExecutionStatus::Done,
        })
        .expect("open");
    rig.writer
        .set_rollback_state("undo", RollbackState::Rollbackable, None)
        .expect("store the old shape");

    assert_eq!(rig.refusal_of("undo"), RollbackRefusal::IsARollback);
}
