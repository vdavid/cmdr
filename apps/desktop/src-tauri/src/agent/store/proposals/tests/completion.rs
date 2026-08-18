//! What execution writes back, and the one interaction that made M4a load-bearing: a group
//! that finished must survive the next launch recovery sweep.

use super::super::*;
use super::{group_with_ops, migrated_conn, status_of};
use crate::agent::types::{OpStatus, ProposalStatus};

/// Approve a group the way the claim transaction does, so the completion path starts from
/// the state it will really see.
fn approved_group(conn: &rusqlite::Connection, ops: usize) -> i64 {
    let group_id = group_with_ops(conn, ops);
    record_acceptance(conn, group_id, &[], 200).expect("preflight");
    match claim_group_for_execution(conn, group_id, 200).expect("claim") {
        ClaimOutcome::Claimed(_) => group_id,
        other => panic!("the fixture group must claim cleanly, got {other:?}"),
    }
}

/// The whole reason this milestone exists. Before it, nothing wrote `completed`, so a group
/// that ran to its end before a quit came back `interrupted` and asked the user to re-approve
/// work that had already happened.
#[test]
fn a_completed_group_survives_the_recovery_sweep() {
    let conn = migrated_conn();
    let finished = approved_group(&conn, 2);
    let still_running = approved_group(&conn, 2);

    assert_eq!(
        mark_group_completed(&conn, finished).expect("mark"),
        CompleteOutcome::Completed
    );
    recover_interrupted_groups(&conn).expect("sweep");

    assert_eq!(
        status_of(&conn, finished),
        ProposalStatus::Completed,
        "a group that finished is not something the user should be asked about again"
    );
    assert_eq!(
        status_of(&conn, still_running),
        ProposalStatus::Interrupted,
        "one that was still approved at launch is exactly what interrupted means"
    );
}

/// Conditional on `approved`, the same shape as the claim and the rejection. A group the
/// recovery sweep already froze must not be resurrected by a late settle from the operation
/// that died with the last launch.
#[test]
fn completing_is_conditional_on_still_being_approved() {
    let conn = migrated_conn();
    let group_id = approved_group(&conn, 1);
    recover_interrupted_groups(&conn).expect("sweep");

    assert_eq!(
        mark_group_completed(&conn, group_id).expect("mark"),
        CompleteOutcome::NotApproved
    );
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Interrupted);
}

#[test]
fn completing_an_unknown_group_says_so_rather_than_reporting_success() {
    let conn = migrated_conn();
    assert_eq!(
        mark_group_completed(&conn, 4_242).expect("mark"),
        CompleteOutcome::Unknown
    );
}

/// A cross-filesystem move speaks twice for one source: `Done` when staging finishes, then
/// `Skipped` when the rename phase leaves it standing. The LAST word is the verdict, so this
/// overwrites rather than refusing the second write.
#[test]
fn the_last_outcome_written_for_an_op_is_the_one_that_sticks() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 1);
    let op = page_ops(&conn, group_id, 10, 0).expect("ops").remove(0);

    assert!(record_op_outcome(&conn, op.id, OpStatus::Done).expect("first"));
    assert!(record_op_outcome(&conn, op.id, OpStatus::Skipped).expect("second"));

    let after = page_ops(&conn, group_id, 10, 0).expect("ops").remove(0);
    assert_eq!(after.status, OpStatus::Skipped);
}

/// An op the user deselected was never in the accepted set, so nothing that ran can be about
/// it, and a stray path match must not overwrite the record of what was offered.
#[test]
fn an_excluded_op_is_never_overwritten_by_an_outcome() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    let excluded = ops[0].id;
    record_acceptance(&conn, group_id, &[excluded], 200).expect("preflight");

    assert!(!record_op_outcome(&conn, excluded, OpStatus::Done).expect("write"));

    let after = page_ops(&conn, group_id, 10, 0).expect("ops");
    assert_eq!(after[0].status, OpStatus::Excluded);
}

/// The indicator is always mounted, so the count it renders has to be cheap and it has to be
/// right: only PENDING groups, and only their live ops. A group the user already answered
/// must not keep a badge on screen.
#[test]
fn the_pending_count_sees_only_what_the_user_still_has_to_answer() {
    let conn = migrated_conn();
    let waiting = group_with_ops(&conn, 3);
    let answered = approved_group(&conn, 5);

    let (groups, ops) = count_pending(&conn, ProposalStatus::Pending).expect("count");
    assert_eq!(
        groups, 1,
        // allowed-pluralize-noun: "counts" is the verb here, not a plural noun.
        "only the group still waiting: {waiting} counts, {answered} does not"
    );
    assert_eq!(ops, 3, "and only that group's ops");
}

/// A deselected op stays as a row so the decision record says what was OFFERED, but it is not
/// something the user still has to answer, so it must not inflate the badge.
#[test]
fn a_deselected_op_does_not_count_toward_the_badge() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    let first = page_ops(&conn, group_id, 10, 0).expect("ops")[0].id;
    record_acceptance(&conn, group_id, &[first], 200).expect("preflight");

    let (groups, ops) = count_pending(&conn, ProposalStatus::Pending).expect("count");
    assert_eq!(groups, 1);
    assert_eq!(ops, 2, "the excluded op keeps its row but leaves the live set");
}
