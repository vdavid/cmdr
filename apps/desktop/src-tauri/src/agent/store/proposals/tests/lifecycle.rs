//! Who may still change a group: re-propose's pending-only guard, and the recovery sweep
//! that freezes what the app died on.

use super::super::*;
use super::{group_with_ops, migrated_conn, status_of};
use crate::agent::types::{ProposalStatus, ProposalVerb};
use crate::location::Location;

/// A move group over one source, for re-proposing a group into something else.
fn amended_group(source: &str) -> NewGroup {
    NewGroup {
        intent: GroupIntent::Move {
            destination: Location {
                volume_id: "root".to_string(),
                path: "/Users/someone/Documents/Invoices".to_string(),
            },
            sources: vec![NewOp {
                source_path: source.to_string(),
                snapshot: None,
            }],
        },
        source_volume_id: "root".to_string(),
        display_name: "one invoice".to_string(),
        rationale: None,
        selector: None,
    }
}

/// A pending group is its author's to amend: a re-propose replaces its ops and its text.
#[test]
fn re_proposing_a_pending_group_replaces_its_ops() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 5);

    let outcome =
        repropose_group(&conn, group_id, &amended_group("/Users/someone/Downloads/inv.pdf"), 200).expect("re-propose");
    assert_eq!(outcome, ReproposeOutcome::Reproposed);

    let ops = page_ops(&conn, group_id, 100, 0).expect("ops");
    assert_eq!(ops.len(), 1, "the old op rows are gone, not merged with");
    assert_eq!(ops[0].source_path, "/Users/someone/Downloads/inv.pdf");
    let group = get_group(&conn, group_id).expect("read").expect("exists");
    assert_eq!(group.verb, ProposalVerb::Move);
    assert_eq!(group.destination.as_deref(), Some("/Users/someone/Documents/Invoices"));
}

/// A re-propose TEARS UP the acceptance record. Without this, a preflight taken against the
/// old op list would still bind, and an amended list could ride an older approval onto the
/// filesystem.
#[test]
fn re_proposing_tears_up_the_acceptance_record() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");

    repropose_group(&conn, group_id, &amended_group("/Users/someone/Downloads/inv.pdf"), 210).expect("re-propose");

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    assert!(
        matches!(
            outcome,
            ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { accepted: None, .. })
        ),
        "the old acceptance must not survive the amendment: {outcome:?}"
    );
}

/// An approved group is frozen to its author: the user answered, and an agent that could
/// still rewrite it could rewrite what the user said yes to.
#[test]
fn re_proposing_an_approved_group_is_refused() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    claim_group_for_execution(&conn, group_id, 300).expect("claim");

    let outcome = repropose_group(&conn, group_id, &amended_group("/tmp/other"), 400).expect("re-propose");
    assert_eq!(
        outcome,
        ReproposeOutcome::NotPending {
            found: ProposalStatus::Approved
        }
    );
    assert_eq!(
        count_ops(&conn, group_id, None).expect("count"),
        3,
        "the ops are untouched"
    );
}

/// `interrupted` counts as frozen too. It is the state where nothing knows which ops already
/// ran, so it is the user's to re-approve or discard — never the agent's to rewrite.
#[test]
fn re_proposing_an_interrupted_group_is_refused() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    claim_group_for_execution(&conn, group_id, 300).expect("claim");
    recover_interrupted_groups(&conn).expect("sweep");

    let outcome = repropose_group(&conn, group_id, &amended_group("/tmp/other"), 400).expect("re-propose");
    assert_eq!(
        outcome,
        ReproposeOutcome::NotPending {
            found: ProposalStatus::Interrupted
        }
    );
}

/// Reopening the app with an approved group in the store yields `interrupted`: execution was
/// in flight when the app died, so nothing knows what ran.
#[test]
fn reopening_an_approved_group_yields_interrupted() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 3);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    claim_group_for_execution(&conn, group_id, 300).expect("claim");

    assert_eq!(recover_interrupted_groups(&conn).expect("sweep"), 1);
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Interrupted);
}

/// The sweep is idempotent and minds its own business: a second run finds nothing, and
/// pending, rejected, and completed groups are left exactly where they are.
#[test]
fn the_recovery_sweep_is_idempotent_and_leaves_other_statuses_alone() {
    let conn = migrated_conn();
    let approved = group_with_ops(&conn, 1);
    let pending = group_with_ops(&conn, 1);
    let rejected = group_with_ops(&conn, 1);
    let completed = group_with_ops(&conn, 1);
    record_acceptance(&conn, approved, &[], 200).expect("preflight");
    claim_group_for_execution(&conn, approved, 300).expect("claim");
    reject_group(&conn, rejected, 300).expect("reject");
    conn.execute(
        "UPDATE proposals SET status = ?2 WHERE id = ?1",
        rusqlite::params![completed, ProposalStatus::Completed.as_token()],
    )
    .expect("mark completed");

    assert_eq!(recover_interrupted_groups(&conn).expect("first sweep"), 1);
    assert_eq!(recover_interrupted_groups(&conn).expect("second sweep"), 0);

    assert_eq!(status_of(&conn, approved), ProposalStatus::Interrupted);
    assert_eq!(status_of(&conn, pending), ProposalStatus::Pending);
    assert_eq!(status_of(&conn, rejected), ProposalStatus::Rejected);
    assert_eq!(
        status_of(&conn, completed),
        ProposalStatus::Completed,
        "a finished group must never be swept: re-approving it would run its ops twice"
    );
}

/// An interrupted group can't be claimed straight back into execution. Re-approval mints a
/// NEW group with a fresh preflight, so the old decision record stays whole.
#[test]
fn an_interrupted_group_cannot_be_claimed() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    claim_group_for_execution(&conn, group_id, 300).expect("claim");
    recover_interrupted_groups(&conn).expect("sweep");

    let outcome = claim_group_for_execution(&conn, group_id, 400).expect("claim");
    assert!(
        matches!(
            outcome,
            ClaimOutcome::Refused(ClaimRefusal::StaleStatus {
                found: ProposalStatus::Interrupted
            })
        ),
        "{outcome:?}"
    );
}

/// Preflighting a frozen group is refused too, so a review surface that opened before the
/// sweep can't quietly re-arm it.
#[test]
fn preflighting_a_frozen_group_is_refused() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);
    reject_group(&conn, group_id, 300).expect("reject");

    assert_eq!(
        record_acceptance(&conn, group_id, &[], 400).expect("preflight"),
        AcceptanceOutcome::NotPending {
            found: ProposalStatus::Rejected
        }
    );
}

/// A group summary counts its ops without loading them, and separates the live set from
/// what the group offered.
#[test]
fn group_summaries_count_live_and_total_ops() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 4);
    let all = page_ops(&conn, group_id, 100, 0).expect("ops");
    record_acceptance(&conn, group_id, &[all[0].id], 200).expect("preflight");

    let summaries = list_groups(&conn, Some(ProposalStatus::Pending)).expect("list");
    let summary = summaries
        .iter()
        .find(|s| s.group.id == group_id)
        .expect("the group is listed");
    assert_eq!(summary.live_op_count, 3);
    assert_eq!(summary.total_op_count, 4);
}
