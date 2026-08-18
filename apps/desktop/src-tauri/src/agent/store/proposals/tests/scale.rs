//! A 60 000-op group, which is a legitimate group: there is no cap, so "delete every
//! installer you've already opened" can genuinely be that large.

use super::super::*;
use super::{group_with_ops, migrated_conn};
use crate::agent::types::OpStatus;

/// How large a group this file exercises. The plan's headline number, so the store is proven
/// at the size the dialog and the tools have to hold too.
const HUGE: usize = 60_000;

/// A 60 000-op group preflights and claims WITHOUT materializing its op rows.
///
/// The structural half of the assertion is the point: `page_ops` is the only function that
/// builds op rows in memory, and neither preflight nor the claim may call it. If a future
/// change compares op sets by loading them, the claim goes from constant memory to 60 000
/// rows twice over, and this test fails rather than the machine quietly swapping.
#[test]
fn a_60000_op_group_claims_without_materializing_its_rows() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, HUGE);

    // Counts come from COUNT(*), so asking how big the group is costs no rows either.
    assert_eq!(count_ops(&conn, group_id, None).expect("count"), HUGE as u64);

    let before = read::tests_support::page_ops_calls();
    let accepted = record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    let AcceptanceOutcome::Accepted { binding } = accepted else {
        panic!("preflight refused: {accepted:?}");
    };
    assert_eq!(binding.op_count, HUGE as u64);

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    assert!(matches!(outcome, ClaimOutcome::Claimed(_)), "{outcome:?}");
    assert_eq!(
        read::tests_support::page_ops_calls(),
        before,
        "preflight and the claim must never load op rows; they stream a hash plus a count"
    );
}

/// One changed op out of 60 000 still refuses the claim: the digest covers every row, not a
/// prefix, and not a sample.
#[test]
fn one_changed_op_out_of_60000_still_refuses_the_claim() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, HUGE);
    record_acceptance(&conn, group_id, &[], 200).expect("preflight");

    conn.execute(
        "UPDATE proposal_ops SET source_path = '/Users/someone/Documents/taxes.pdf'
         WHERE group_id = ?1 AND seq = ?2",
        rusqlite::params![group_id, (HUGE - 1) as i64],
    )
    .expect("amend the last op");

    let outcome = claim_group_for_execution(&conn, group_id, 300).expect("claim");
    assert!(
        matches!(outcome, ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { .. })),
        "{outcome:?}"
    );
}

/// The review surface reads a huge group one page at a time, in `seq` order, with no gap and
/// no overlap between pages.
#[test]
fn a_huge_group_pages_in_seq_order_without_gaps() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, HUGE);

    let first = page_ops(&conn, group_id, 100, 0).expect("first page");
    let second = page_ops(&conn, group_id, 100, 100).expect("second page");
    let last = page_ops(&conn, group_id, 100, (HUGE - 50) as u32).expect("last page");

    assert_eq!(first.len(), 100);
    assert_eq!(second.len(), 100);
    assert_eq!(last.len(), 50, "the last page is short, not wrapped");
    assert_eq!(first[0].seq, 0);
    assert_eq!(second[0].seq, 100, "page two starts where page one ended");
    assert_eq!(last[49].seq, (HUGE - 1) as i64);
    assert!(first.iter().all(|op| op.status == OpStatus::Pending));
}
