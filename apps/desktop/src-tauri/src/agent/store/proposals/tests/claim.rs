//! The claim transaction: the one place a bug applies ops to real files twice.
//!
//! Every test here is about a claim that must NOT go through, or about two claims where
//! exactly one must.

use std::sync::Barrier;

use rusqlite::params;

use super::super::*;
use super::{group_with_ops, migrated_conn, status_of};
use crate::agent::store::open_write_connection;
use crate::agent::types::{OpStatus, ProposalStatus};

/// Preflight a whole group and claim it: the happy path, so the refusals below mean
/// something.
#[test]
fn a_preflighted_group_claims_once_and_becomes_approved() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);

    let accepted = record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    let AcceptanceOutcome::Accepted { binding } = accepted else {
        panic!("preflight refused a pending group: {accepted:?}");
    };
    assert_eq!(binding.op_count, 3);

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    let ClaimOutcome::Claimed(claimed) = outcome else {
        panic!("a preflighted pending group was refused: {outcome:?}");
    };
    assert_eq!(claimed.binding, binding, "the claim binds what preflight accepted");
    assert_eq!(claimed.group.status, ProposalStatus::Approved);
    assert_eq!(claimed.group.decided_at, Some(300));
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Approved);
}

/// A second claim of an already-approved group is refused for STALE STATUS, distinctly from
/// a binding mismatch: nothing about the op list changed, the answer is simply already given.
#[test]
fn claiming_an_approved_group_again_is_refused_as_stale_status() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    claim_group_for_execution(&conn, group_id, 300).expect("first claim");

    let outcome = claim_group_for_execution(&conn, group_id, 400).expect("second claim");
    assert!(
        matches!(
            outcome,
            ClaimOutcome::Refused(ClaimRefusal::StaleStatus {
                found: ProposalStatus::Approved
            })
        ),
        "a second claim is stale status, never a binding mismatch: {outcome:?}"
    );
}

/// An op whose VALUES changed after preflight refuses the claim with the binding-mismatch
/// variant. This is the variant that stops an amended op list riding an older approval onto
/// the filesystem, and the user-facing recovery ("review it again") differs from stale
/// status, so the two must never collapse into one refusal.
#[test]
fn a_claim_whose_op_values_changed_refuses_as_binding_mismatch() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    let AcceptanceOutcome::Accepted { binding: accepted } =
        record_acceptance(&conn, group_id, &[], 200).expect("preflight")
    else {
        panic!("preflight refused");
    };

    // The agent amends one op's source between preflight and claim.
    conn.execute(
        "UPDATE proposal_ops SET source_path = '/Users/someone/Documents/taxes.pdf'
         WHERE group_id = ?1 AND seq = 1",
        params![group_id],
    )
    .expect("amend one op");

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    let ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { accepted: was, live }) = outcome else {
        panic!("an amended op set was claimed: {outcome:?}");
    };
    assert_eq!(was, Some(accepted.clone()), "the refusal carries what preflight accepted");
    assert_eq!(live.op_count, accepted.op_count, "the COUNT alone would not have caught this");
    assert_ne!(live.digest, accepted.digest, "the digest is what caught it");
    assert_eq!(
        status_of(&conn, group_id),
        ProposalStatus::Pending,
        "a refused claim leaves the group where it was"
    );
}

/// An op ADDED after preflight refuses too. The count catches this one, which is why the
/// binding is a hash PLUS a count rather than either alone.
#[test]
fn a_claim_whose_op_set_grew_refuses_as_binding_mismatch() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");

    conn.execute(
        "INSERT INTO proposal_ops (group_id, seq, source_path, status, created_at)
         VALUES (?1, 99, '/Users/someone/Documents/taxes.pdf', ?2, 250)",
        params![group_id, OpStatus::Pending.as_token()],
    )
    .expect("smuggle an op in");

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    let ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { live, .. }) = outcome else {
        panic!("a grown op set was claimed: {outcome:?}");
    };
    assert_eq!(live.op_count, 4);
}

/// A group that was never preflighted is refused. The acceptance record is server-owned and
/// the client presents only ids, so "no record" means nobody ever checked this op set.
#[test]
fn claiming_without_a_preflight_is_refused() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    assert!(
        matches!(
            outcome,
            ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { accepted: None, .. })
        ),
        "an unpreflighted group must not claim: {outcome:?}"
    );
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Pending);
}

/// A partial approval: the user deselects ops, preflight binds only what's left, and the
/// claim goes through. The deselected ops keep their ROWS — the decision record says what
/// was offered, not only what ran.
#[test]
fn partial_approval_by_deselection_binds_only_the_selected_ops() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 5);
    let all = page_ops(&conn, group_id, 100, 0).expect("ops");
    let deselected = vec![all[1].id, all[3].id];

    let AcceptanceOutcome::Accepted { binding } =
        record_acceptance(&conn, group_id, &deselected, 200).expect("preflight")
    else {
        panic!("preflight refused");
    };
    assert_eq!(binding.op_count, 3, "the binding covers only the selected ops");

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    assert!(matches!(outcome, ClaimOutcome::Claimed(_)), "{outcome:?}");

    assert_eq!(count_ops(&conn, group_id, None).expect("count"), 5, "every row stays");
    assert_eq!(
        count_ops(&conn, group_id, Some(OpStatus::Excluded)).expect("count"),
        2,
        "the deselected ops are excluded, not deleted"
    );
}

/// Re-selecting an op a previous preflight excluded puts it back in the live set: a second
/// preflight describes what the user is looking at NOW, never the union of every review.
#[test]
fn a_second_preflight_reselects_what_the_first_deselected() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 4);
    let all = page_ops(&conn, group_id, 100, 0).expect("ops");

    record_acceptance(&conn, group_id, &[all[0].id, all[1].id], 200).expect("first preflight");
    let AcceptanceOutcome::Accepted { binding } =
        record_acceptance(&conn, group_id, &[all[0].id], 210).expect("second preflight")
    else {
        panic!("preflight refused");
    };
    assert_eq!(binding.op_count, 3, "op 1 came back into the live set");
    assert_eq!(count_ops(&conn, group_id, Some(OpStatus::Excluded)).expect("count"), 1);
}

/// TWO CONCURRENT CLAIMS: exactly one wins, and the loser gets a TYPED refusal rather than
/// `SQLITE_BUSY`. `BEGIN IMMEDIATE` plus the busy timeout serializes them, so the loser
/// reaches its conditional `UPDATE` after the winner committed and finds the group already
/// approved.
#[test]
fn two_concurrent_claims_leave_exactly_one_winner_and_a_typed_refusal() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = crate::agent::store::main_db_path(dir.path());
    let setup = open_write_connection(&db_path).expect("open");
    let group_id = group_with_ops(&setup, 200);
    record_acceptance(&setup, group_id, &[], 200).expect("preflight");
    drop(setup);

    let barrier = Barrier::new(2);
    let outcomes: Vec<ClaimOutcome> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = &barrier;
                let db_path = &db_path;
                scope.spawn(move || {
                    let conn = open_write_connection(db_path).expect("open");
                    barrier.wait();
                    claim_group_for_execution(&conn, group_id, 300).expect("claim did not error")
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("thread")).collect()
    });

    let claimed = outcomes
        .iter()
        .filter(|o| matches!(o, ClaimOutcome::Claimed(_)))
        .count();
    assert_eq!(claimed, 1, "exactly one claim wins: {outcomes:?}");
    assert!(
        outcomes.iter().any(|o| matches!(
            o,
            ClaimOutcome::Refused(ClaimRefusal::StaleStatus {
                found: ProposalStatus::Approved
            })
        )),
        "the loser gets a typed stale-status refusal, never a busy error: {outcomes:?}"
    );
}

/// Rejecting takes the same conditional shape as claiming: only a pending group moves, and a
/// group that already left `pending` is refused with its status rather than silently
/// overwritten.
#[test]
fn rejecting_is_conditional_on_pending_too() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);

    assert_eq!(
        reject_group(&conn, group_id, 300).expect("reject"),
        RejectOutcome::Rejected
    );
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Rejected);

    assert_eq!(
        reject_group(&conn, group_id, 400).expect("second reject"),
        RejectOutcome::NotPending {
            found: ProposalStatus::Rejected
        },
        "a rejected group can't be rejected twice"
    );
}

/// A rejected group can't then be claimed — the same stale-status refusal, from the other
/// terminal state.
#[test]
fn a_rejected_group_cannot_be_claimed() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    reject_group(&conn, group_id, 300).expect("reject");

    let outcome = claim_group_for_execution(&conn, group_id, 400).expect("claim");
    assert!(
        matches!(
            outcome,
            ClaimOutcome::Refused(ClaimRefusal::StaleStatus {
                found: ProposalStatus::Rejected
            })
        ),
        "{outcome:?}"
    );
}
