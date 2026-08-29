//! The rollback engine's LIFECYCLE, split from the per-item invariants in
//! `tests.rs`: the op-level gate (`check_rollbackable`), the entry point that
//! sets `rolling_back` and hands the plan to the spawn hook, the startup
//! reconcile that resolves whatever a crash left mid-reversal, and the retention
//! race the `rolling_back` state closes.
//!
//! Same rig as its siblings (`test_support`), so a reversal is driven the same
//! way here as anywhere else.

use std::sync::Arc;

use super::test_support::*;
use super::*;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::operation_log::store::{open_read_connection, read_operation};
use crate::operation_log::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage,
};
use crate::operation_log::writer::{JournalItem, OpenOperation};

// ── The op-level gate ────────────────────────────────────────────────────────

#[test]
fn check_rollbackable_gates_state_and_connectivity() {
    let vm = VolumeManager::new();
    vm.register("src", Arc::new(InMemoryVolume::new("Src")) as Arc<dyn Volume>);
    vm.register("dst", Arc::new(InMemoryVolume::new("Dst")) as Arc<dyn Volume>);

    let base = |state: RollbackState, reason: Option<NotRollbackableReason>, dst: Option<&str>| OperationRow {
        op_id: "op".into(),
        kind: OpKind::Copy,
        archive_subkind: None,
        initiator: Initiator::User,
        execution_status: ExecutionStatus::Done,
        rollback_state: state,
        not_rollbackable_reason: reason,
        rolls_back_op_id: None,
        source_volume_id: Some("src".into()),
        dest_volume_id: dst.map(str::to_string),
        started_at: 1,
        ended_at: Some(2),
        item_count: 1,
        items_done: 1,
        bytes_total: 0,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        dev_summary: None,
    };

    // Rollbackable + all volumes present ⇒ Ok.
    assert!(check_rollbackable(&vm, &base(RollbackState::Rollbackable, None, Some("dst"))).is_ok());
    // Already rolling back ⇒ typed refusal (double-rollback guard).
    assert_eq!(
        check_rollbackable(&vm, &base(RollbackState::RollingBack, None, Some("dst"))),
        Err(RollbackRefusal::AlreadyRollingBack)
    );
    // Already rolled back ⇒ nothing to do.
    assert_eq!(
        check_rollbackable(&vm, &base(RollbackState::RolledBack, None, Some("dst"))),
        Err(RollbackRefusal::AlreadyRolledBack)
    );
    // Not rollbackable (a delete) ⇒ carries the stored reason.
    assert_eq!(
        check_rollbackable(
            &vm,
            &base(
                RollbackState::NotRollbackable,
                Some(NotRollbackableReason::PermanentDelete),
                Some("dst")
            )
        ),
        Err(RollbackRefusal::NotRollbackable(NotRollbackableReason::PermanentDelete))
    );
    // A move that overwrote ⇒ not rollbackable with the overwrote reason.
    assert_eq!(
        check_rollbackable(
            &vm,
            &base(
                RollbackState::NotRollbackable,
                Some(NotRollbackableReason::Overwrote),
                Some("dst")
            )
        ),
        Err(RollbackRefusal::NotRollbackable(NotRollbackableReason::Overwrote))
    );
    // A required volume isn't connected ⇒ typed unavailable naming the volume.
    assert_eq!(
        check_rollbackable(&vm, &base(RollbackState::Rollbackable, None, Some("backup"))),
        Err(RollbackRefusal::VolumeUnavailable {
            volume_id: "backup".into()
        })
    );
}

// ── The entry point: gate, set rolling_back, reset on spawn failure ───────────

/// A helper that seeds a minimal rollbackable copy op (one dst file) + registers
/// its volumes, returning the rig.
async fn rig_with_rollbackable_op(op_id: &str) -> Rig {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/f.txt", b"x").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst);
    rig.seed(
        op_id,
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "src", "/f.txt", "dst", "/f.txt", 1)],
    );
    rig
}

#[tokio::test]
async fn double_rollback_is_refused_with_already_rolling_back() {
    let rig = rig_with_rollbackable_op("op").await;
    // First rollback: gate passes, the op is set rolling_back.
    let first = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| Ok(()));
    assert!(first.is_ok(), "first rollback accepted");
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::RollingBack);
    // Second rollback while still rolling_back ⇒ typed refusal, and the spawn
    // closure is never reached.
    let second = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| {
        panic!("spawn must not run for an already-rolling-back op")
    });
    assert_eq!(second.unwrap_err(), RollbackRefusal::AlreadyRollingBack);
}

#[tokio::test]
async fn synchronous_spawn_failure_resets_to_rollbackable_and_a_retry_is_accepted() {
    let rig = rig_with_rollbackable_op("op").await;
    // The spawn fails synchronously (a volume dropped between the gate and spawn).
    let failed = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| {
        Err(RollbackRefusal::VolumeUnavailable {
            volume_id: "dst".into(),
        })
    });
    assert_eq!(
        failed.unwrap_err(),
        RollbackRefusal::VolumeUnavailable {
            volume_id: "dst".into()
        }
    );
    // NOT wedged: the op was reset to rollbackable, so an immediate retry is taken.
    assert_eq!(
        rig.read_op("op").rollback_state,
        RollbackState::Rollbackable,
        "a failed spawn must not leave the op stuck rolling_back"
    );
    let retry = rollback_operation(&rig.vm, &rig.writer, "op", |_plan| Ok(()));
    assert!(retry.is_ok(), "the retry is accepted after the reset");
}

#[tokio::test]
async fn entry_refuses_unknown_and_not_rollbackable_and_disconnected() {
    let rig = Rig::new();
    rig.register("v", Arc::new(InMemoryVolume::new("V")));
    // Unknown op.
    assert_eq!(
        rollback_operation(&rig.vm, &rig.writer, "nope", |_| Ok(())).unwrap_err(),
        RollbackRefusal::UnknownOperation
    );
    // A delete is never rollbackable — refused with the stored reason.
    rig.seed(
        "del",
        OpKind::Delete,
        "v",
        None,
        RollbackState::NotRollbackable,
        vec![file_unit(0, "v", "/gone.txt", "v", "/gone.txt", 1)],
    );
    // (Delete finalizes not_rollbackable via the pipeline; seed sets the state, but
    // the reason column is nulled by seed, so the gate reports a default reason.)
    assert!(matches!(
        rollback_operation(&rig.vm, &rig.writer, "del", |_| Ok(())).unwrap_err(),
        RollbackRefusal::NotRollbackable(_)
    ));
    // A rollbackable op whose volume isn't registered ⇒ unavailable.
    rig.seed(
        "x",
        OpKind::Copy,
        "gonevol",
        Some("gonevol"),
        RollbackState::Rollbackable,
        vec![file_unit(0, "gonevol", "/f", "gonevol", "/f", 1)],
    );
    assert_eq!(
        rollback_operation(&rig.vm, &rig.writer, "x", |_| Ok(())).unwrap_err(),
        RollbackRefusal::VolumeUnavailable {
            volume_id: "gonevol".into()
        }
    );
}

// ── Startup reconcile ────────────────────────────────────────────────────────

/// Seed an op left `rolling_back`, plus an optional unfinalized inverse op with
/// the given per-item outcomes, and run the reconcile.
fn seed_rolling_back(rig: &Rig, op_id: &str, inverse: Option<(&str, &[ItemOutcome])>) {
    rig.writer
        .open_operation(OpenOperation {
            op_id: op_id.to_string(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 1,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Done,
        })
        .expect("open orig");
    rig.writer
        .set_rollback_state(op_id, RollbackState::RollingBack, None)
        .expect("set rolling_back");
    if let Some((inv_id, outcomes)) = inverse {
        rig.writer
            .open_operation(OpenOperation {
                op_id: inv_id.to_string(),
                kind: OpKind::Delete,
                initiator: Initiator::User,
                source_volume_id: Some("dst".into()),
                dest_volume_id: None,
                item_count: outcomes.len() as u64,
                started_at: 2,
                rolls_back_op_id: Some(op_id.to_string()),
                execution_status: ExecutionStatus::Running,
            })
            .expect("open inverse");
        let items: Vec<_> = outcomes
            .iter()
            .enumerate()
            .map(|(i, &outcome)| JournalItem {
                seq: i as i64,
                entry_type: EntryType::File,
                row_role: RowRole::RollbackUnit,
                source_volume_id: "dst".into(),
                source_dir: "/".into(),
                source_name: format!("f{i}"),
                dest_volume_id: None,
                dest_dir: None,
                dest_name: None,
                size: Some(1),
                mtime: Some(MT as i64),
                outcome,
                overwrote: false,
            })
            .collect();
        rig.writer.record_items(inv_id, items).expect("record inverse items");
        // Deliberately NOT finalized — it crashed mid-stream.
    }
    rig.writer.flush_blocking().expect("flush");
}

#[tokio::test]
async fn reconcile_resolves_from_inverse_outcomes() {
    // (i) inverse reversed something ⇒ partially_rolled_back.
    {
        let rig = Rig::new();
        seed_rolling_back(&rig, "op", Some(("inv", &[ItemOutcome::Done, ItemOutcome::Skipped])));
        reconcile_rolling_back_on_open(&rig.writer);
        assert_eq!(rig.read_op("op").rollback_state, RollbackState::PartiallyRolledBack);
    }
    // (i') inverse reversed nothing (all skipped) ⇒ back to rollbackable.
    {
        let rig = Rig::new();
        seed_rolling_back(&rig, "op", Some(("inv", &[ItemOutcome::Skipped])));
        reconcile_rolling_back_on_open(&rig.writer);
        assert_eq!(rig.read_op("op").rollback_state, RollbackState::Rollbackable);
    }
}

#[tokio::test]
async fn reconcile_with_no_inverse_row_returns_to_rollbackable_and_a_reissue_resumes() {
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/f.txt", b"x").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    // Crashed AFTER setting rolling_back but before the inverse op opened, and the
    // op still has real rollback_unit rows to reverse.
    rig.writer
        .open_operation(OpenOperation {
            op_id: "op".into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: Some("src".into()),
            dest_volume_id: Some("dst".into()),
            item_count: 1,
            started_at: 1,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Done,
        })
        .expect("open");
    rig.writer
        .record_items("op", vec![file_unit(0, "src", "/f.txt", "dst", "/f.txt", 1)])
        .expect("record");
    rig.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("set");
    rig.writer.flush_blocking().expect("flush");

    // No inverse op ⇒ reconcile returns it straight to rollbackable.
    reconcile_rolling_back_on_open(&rig.writer);
    assert_eq!(rig.read_op("op").rollback_state, RollbackState::Rollbackable);

    // A re-issued rollback now resumes and finishes idempotently.
    let report = rig.rollback("op").await;
    assert_eq!(report.final_state, RollbackState::RolledBack);
    assert!(
        !exists(&dst, "/f.txt").await,
        "the re-issued rollback reversed the copy"
    );
}

#[tokio::test]
async fn retention_cannot_prune_a_rollbacks_source_mid_stream() {
    use crate::operation_log::writer::PruneRequest;
    let rig = Rig::new();
    let dst = Arc::new(InMemoryVolume::new("Dst"));
    put(&dst, "/one.txt", b"1").await;
    put(&dst, "/two.txt", b"22").await;
    rig.register("src", Arc::new(InMemoryVolume::new("Src")));
    rig.register("dst", dst.clone());
    rig.seed(
        "op",
        OpKind::Copy,
        "src",
        Some("dst"),
        RollbackState::Rollbackable,
        vec![
            file_unit(0, "src", "/one.txt", "dst", "/one.txt", 1),
            file_unit(1, "src", "/two.txt", "dst", "/two.txt", 2),
        ],
    );
    // The op is mid-rollback...
    rig.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("set");
    // ...and a retention pass runs that WOULD prune it by age (ended_at 200 is well
    // before the cutoff). It must skip a `rolling_back` op so the source rows a live
    // rollback is streaming can't vanish out from under it.
    rig.writer
        .prune(PruneRequest {
            max_age_secs: Some(0),
            max_size_bytes: None,
            now_secs: 1_000_000,
            vacuum: true,
        })
        .expect("prune");
    rig.writer.flush_blocking().expect("flush");

    // The op and every item survived the prune.
    let conn = open_read_connection(rig.writer.db_path()).expect("conn");
    assert!(
        read_operation(&conn, "op").expect("read").is_some(),
        "the rolling_back op is not pruned"
    );
    assert_eq!(
        read_operation_items(&conn, "op", 100).expect("items").len(),
        2,
        "its source rows survive"
    );
    drop(conn);

    // Reset to rollbackable (as the reconcile would) and run the rollback to
    // completion: it restores every item because the rows were never pruned.
    rig.writer
        .set_rollback_state("op", RollbackState::Rollbackable, None)
        .expect("reset");
    let report = rig.rollback("op").await;
    assert_eq!(report.reversed, 2, "both source rows were still there to reverse");
    assert!(!exists(&dst, "/one.txt").await);
    assert!(!exists(&dst, "/two.txt").await);
}
