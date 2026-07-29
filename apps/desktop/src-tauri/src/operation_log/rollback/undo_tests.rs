//! Undoing several operations as one action: the ORDER, and why it is data safety.
//!
//! Apart from `tests.rs` (the per-kind reversal engine) because this is one rule with
//! one consequence, and the consequence is the test that matters: reversed in the
//! wrong order, a batch that reused a freed name leaves a file unrestored, silently.
//! Shares `tests.rs`'s rig via `test_support`.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::operation_log::store::OperationRow;
use crate::operation_log::types::{OpKind, RollbackState};

/// Seed a rename batch that renamed `src` → `dst` and put the file where the
/// batch left it.
async fn seed_rename_batch(rig: &Rig, op_id: &str, started_at: i64, v: &InMemoryVolume, src: &str, dst: &str) {
    put(v, dst, b"data").await;
    rig.seed_at(
        op_id,
        OpKind::Rename,
        "v",
        Some("v"),
        RollbackState::Rollbackable,
        started_at,
        vec![file_unit(0, "v", src, "v", dst, 4)],
    );
}

/// **Multi-batch undo runs NEWEST BATCH FIRST**, and this is why: batch two
/// renamed a file INTO the name batch one freed. Reversed newest-first, batch two
/// vacates that name before batch one needs it back and both restore. Reversed
/// oldest-first, batch one finds its old name occupied, skips (never overwrites),
/// and the user is left with a file that undo silently failed to restore.
///
/// `undo_order` is what puts the two in that order; this test pins the
/// consequence, so deleting the ordering can't pass.
#[tokio::test]
async fn a_batch_that_reused_a_freed_name_only_undoes_cleanly_newest_first() {
    // Newest batch first: everything comes back.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        rig.register("v", v.clone());
        seed_rename_batch(&rig, "batch-1", 100, &v, "/a.txt", "/b.txt").await;
        seed_rename_batch(&rig, "batch-2", 200, &v, "/c.txt", "/a.txt").await;

        for op_id in undo_order(vec![rig.read_op("batch-1"), rig.read_op("batch-2")])
            .iter()
            .map(|op| op.op_id.clone())
        {
            let report = rig.rollback_as(&op_id, &format!("inv-{op_id}")).await;
            assert_eq!(
                report.final_state,
                RollbackState::RolledBack,
                "clean reversal for {op_id}"
            );
        }
        assert!(exists(&v, "/a.txt").await, "batch one's original name is back");
        assert!(exists(&v, "/c.txt").await, "batch two's original name is back");
        assert!(!exists(&v, "/b.txt").await);
    }
    // Oldest first — the order this must never use: batch one's name is taken.
    {
        let rig = Rig::new();
        let v = Arc::new(InMemoryVolume::new("V"));
        rig.register("v", v.clone());
        seed_rename_batch(&rig, "batch-1", 100, &v, "/a.txt", "/b.txt").await;
        seed_rename_batch(&rig, "batch-2", 200, &v, "/c.txt", "/a.txt").await;

        let first = rig.rollback_as("batch-1", "inv-1").await;

        assert_eq!(
            first.final_state,
            RollbackState::PartiallyRolledBack,
            "oldest-first hits an occupied restore target"
        );
        assert_eq!(first.skipped, 1);
        assert!(exists(&v, "/b.txt").await, "and leaves the file under its new name");
    }
}

/// The order itself: newest batch first, by start time. A job's batches can share
/// a second (the journal's clock is whole seconds), so ties fall back to the
/// reverse of the order the caller applied them in — never an arbitrary one.
#[test]
fn undo_order_puts_the_newest_batch_first_and_breaks_ties_by_reverse_apply_order() {
    let row = |op_id: &str, started_at: i64| OperationRow {
        op_id: op_id.to_string(),
        started_at,
        ..blank_op_row()
    };
    let ids = |ops: Vec<OperationRow>| undo_order(ops).into_iter().map(|op| op.op_id).collect::<Vec<_>>();

    assert_eq!(
        ids(vec![row("first", 100), row("second", 200), row("third", 300)]),
        vec!["third", "second", "first"]
    );
    // All three in the same second: the caller passed them in apply order, so the
    // reverse of that is the newest-first order.
    assert_eq!(
        ids(vec![row("first", 100), row("second", 100), row("third", 100)]),
        vec!["third", "second", "first"]
    );
    // A mix: the later second wins outright, the tied pair reverses.
    assert_eq!(
        ids(vec![row("first", 100), row("second", 100), row("third", 50)]),
        vec!["second", "first", "third"]
    );
}
