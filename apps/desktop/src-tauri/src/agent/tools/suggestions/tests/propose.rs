//! The write path, against a real migrated in-memory `main.db`, so the store's own guards
//! are in the loop rather than mocked away.

use rusqlite::Connection;
use serde_json::json;

use super::{FakeIndex, apply, indexed, migrated_conn, selector_call, trash_call};
use crate::agent::store::proposals::{count_ops, get_group, page_ops};
use crate::agent::suggested_ops::SelectorRefusal;
use crate::agent::tools::suggestions::propose::{ApplyRefusal, GroupOutcome};
use crate::agent::types::{OpStatus, ProposalStatus, ProposalVerb, Reversibility};

// ── The write path ────────────────────────────────────────────────────────────

#[test]
fn a_named_list_becomes_a_sweep_with_one_reviewable_group() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let report = apply(&conn, &index, &trash_call()).expect("staged");

    assert!(report.ready_for_review);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].outcome, GroupOutcome::Created);
    assert_eq!(report.groups[0].op_count, 2);
    assert_eq!(report.groups[0].verb, ProposalVerb::Trash);

    let group = get_group(&conn, report.groups[0].group_id)
        .expect("read")
        .expect("the group exists");
    assert_eq!(group.status, ProposalStatus::Pending);
    assert_eq!(group.reversible, Reversibility::RestoreMove);
    assert_eq!(group.selector, None, "a hand-listed group carries no pattern");
    assert_eq!(count_ops(&conn, group.id, Some(OpStatus::Pending)).expect("count"), 2);
    // The index was never asked: there was no pattern to resolve.
    assert_eq!(index.calls.get(), 0);
}

#[test]
fn a_selector_resolves_once_at_creation_and_freezes_what_it_found() {
    let conn = migrated_conn();
    let index = FakeIndex::with(vec![
        indexed("/Users/someone/Downloads/a.dmg"),
        indexed("/Users/someone/Downloads/b.dmg"),
    ]);
    let report = apply(&conn, &index, &selector_call()).expect("staged");

    assert_eq!(index.calls.get(), 1, "a selector is resolved exactly once");
    let group_id = report.groups[0].group_id;
    let group = get_group(&conn, group_id).expect("read").expect("exists");
    // The pattern survives as the group's display text and as stored JSON, for the dialog.
    assert!(
        group.display_name.ends_with("/Downloads/*.dmg"),
        "{}",
        group.display_name
    );
    assert!(group.selector.is_some(), "the pattern rides along for display");
    assert_eq!(group.source_volume_id, "root", "the selector's root names the volume");

    // The resolved rows carry the index's snapshot, which is what M2 checks against at
    // apply time.
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].snapshot_size, Some(4_096));
    assert_eq!(ops[0].snapshot_inode, Some(12));
}

#[test]
fn a_selector_that_matches_nothing_stages_nothing_and_says_so() {
    // An empty group is a review the user can't act on, and "nothing matched" is a fact the
    // agent should relay rather than a group it should stage.
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let refusal = apply(&conn, &index, &selector_call()).expect_err("nothing matched");
    assert!(matches!(refusal, ApplyRefusal::SelectorMatchedNothing { group: 0, .. }));
    assert_eq!(sweep_count(&conn), 0, "nothing was written");
}

#[test]
fn an_unreadable_index_refuses_differently_from_an_empty_one() {
    // "I can't see that drive" and "nothing matched" read the same as an empty list and
    // mean opposite things, so they stay two typed refusals.
    let conn = migrated_conn();
    let index = FakeIndex::refusing(SelectorRefusal::NotIndexed {
        volume_id: "nas".to_string(),
    });
    let refusal = apply(&conn, &index, &selector_call()).expect_err("no index");
    assert!(matches!(
        refusal,
        ApplyRefusal::Selector {
            group: 0,
            refusal: SelectorRefusal::NotIndexed { .. }
        }
    ));
    assert_eq!(sweep_count(&conn), 0);
}

#[test]
fn one_bad_group_stages_none_of_them() {
    // A half-applied sweep leaves the user reading a mix of what the agent meant and what
    // it managed, so the check runs over every group before the first write.
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let call = json!({
        "groups": [
            { "verb": "trash", "sourceVolumeId": "root", "displayName": "fine", "paths": ["/Users/someone/a.dmg"] },
            { "verb": "trash", "selector": { "root": { "volumeId": "root", "path": "/Users/someone/Downloads" } } },
        ]
    });
    let refusal = apply(&conn, &index, &call).expect_err("the second group matched nothing");
    assert!(matches!(refusal, ApplyRefusal::SelectorMatchedNothing { group: 1, .. }));
    assert_eq!(sweep_count(&conn), 0, "the good group didn't land either");
}

#[test]
fn re_proposing_replaces_a_pending_groups_ops_without_minting_a_new_group() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");
    let group_id = first.groups[0].group_id;

    let amendment = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "groupId": group_id,
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "One old installer",
            "paths": ["/Users/someone/Downloads/a.dmg"],
        }]
    });
    let second = apply(&conn, &index, &amendment).expect("amended");
    assert_eq!(second.sweep_id, first.sweep_id, "the amendment stays in its sweep");
    assert_eq!(second.groups[0].group_id, group_id, "same group, new contents");
    assert_eq!(second.groups[0].outcome, GroupOutcome::Amended);
    assert_eq!(count_ops(&conn, group_id, None).expect("count"), 1);
    assert_eq!(sweep_count(&conn), 1);
}

#[test]
fn an_answered_group_is_the_users_and_cant_be_rewritten() {
    // `pending` is the only mutable status. An agent that could still rewrite an approved
    // group could rewrite what the user already said yes to.
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");
    let group_id = first.groups[0].group_id;
    conn.execute(
        "UPDATE proposals SET status = ?2 WHERE id = ?1",
        rusqlite::params![group_id, ProposalStatus::Approved.as_token()],
    )
    .expect("approve it behind the tool's back");

    let amendment = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "groupId": group_id,
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "Sneaky rewrite",
            "paths": ["/Users/someone/Downloads/c.dmg"],
        }]
    });
    let refusal = apply(&conn, &index, &amendment).expect_err("approved is frozen");
    assert!(matches!(
        refusal,
        ApplyRefusal::GroupNotPending {
            group: 0,
            status: ProposalStatus::Approved,
            ..
        }
    ));
    // The ops the user approved are untouched.
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    assert_eq!(ops.len(), 2);
}

#[test]
fn a_group_from_another_sweep_cant_be_amended_through_this_one() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");
    let other = apply(&conn, &index, &trash_call()).expect("a second sweep");

    let amendment = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "groupId": other.groups[0].group_id,
            "verb": "trash",
            "sourceVolumeId": "root",
            "displayName": "x",
            "paths": ["/Users/someone/Downloads/a.dmg"],
        }]
    });
    let refusal = apply(&conn, &index, &amendment).expect_err("wrong sweep");
    assert!(matches!(refusal, ApplyRefusal::GroupNotInSweep { group: 0, .. }));
}

#[test]
fn a_new_group_can_join_an_existing_sweep() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let first = apply(&conn, &index, &trash_call()).expect("staged");

    let addition = json!({
        "sweepId": first.sweep_id,
        "groups": [{
            "verb": "move",
            "sourceVolumeId": "root",
            "displayName": "Screenshots to Pictures",
            "destination": { "volumeId": "root", "path": "/Users/someone/Pictures" },
            "paths": ["/Users/someone/Downloads/shot.png"],
        }]
    });
    let second = apply(&conn, &index, &addition).expect("added");
    assert_eq!(second.sweep_id, first.sweep_id);
    assert_eq!(second.groups[0].outcome, GroupOutcome::Created);
    assert_eq!(sweep_count(&conn), 1, "no second sweep was opened");
}

#[test]
fn an_unknown_sweep_is_refused_before_anything_is_written() {
    let conn = migrated_conn();
    let index = FakeIndex::with(Vec::new());
    let mut call = trash_call();
    call["sweepId"] = json!(4_242);
    let refusal = apply(&conn, &index, &call).expect_err("no such sweep");
    assert!(matches!(refusal, ApplyRefusal::UnknownSweep { sweep_id: 4_242 }));
    assert_eq!(sweep_count(&conn), 0);
}

fn sweep_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM proposal_sets", [], |row| row.get(0))
        .expect("count sweeps")
}
