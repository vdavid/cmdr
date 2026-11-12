//! A server group: zero ops, a draft beside it, and a claim of its own.
//!
//! Every test here is about the two kinds of group staying apart (a file claim can't take a
//! server group, nor the server claim a file group), or about the draft being exactly what the
//! user read: written with the group, replaced with its revision, and never read as a guess.

use std::sync::Barrier;

use rusqlite::{Connection, params};

use super::super::*;
use super::{group_with_ops, migrated_conn, status_of};
use crate::agent::store::{AgentStoreError, ServerDraftProblem, open_write_connection};
use crate::agent::types::{DraftProtocol, ProposalStatus, ProposalVerb, Reversibility, ServerSaveNote};

fn sftp_draft(host: &str) -> ServerDraft {
    ServerDraft {
        display_name: Some("build-box".to_string()),
        host: Some(host.to_string()),
        port: Some(22),
        username: Some("test_user_13".to_string()),
        remote_root: Some("/srv/builds".to_string()),
        ..ServerDraft::new(DraftProtocol::Sftp)
    }
}

fn server_group(edit: Option<ServerEdit>, draft: ServerDraft) -> NewGroup {
    NewGroup {
        intent: GroupIntent::SaveServer { draft, edit },
        // A server group has no sources; the spine decides what this column holds.
        source_volume_id: "ignored".to_string(),
        display_name: "build-box".to_string(),
        rationale: Some("You copy builds there every day.".to_string()),
        selector: None,
    }
}

fn add_group(conn: &Connection) -> i64 {
    let set_id = create_sweep(conn, &NewSweep::default(), 100).expect("sweep");
    create_group(conn, set_id, &server_group(None, sftp_draft("build.lan")), 100).expect("group")
}

fn edit() -> ServerEdit {
    ServerEdit {
        editing: "sftp-build-box".to_string(),
        previous: ServerDraft {
            display_name: Some("old name".to_string()),
            ..sftp_draft("build.lan")
        },
    }
}

fn proposal(conn: &Connection, group_id: i64) -> ServerProposal {
    read_server_proposal(conn, group_id)
        .expect("read")
        .expect("a server group has a proposal")
}

fn claimed(outcome: ServerClaimOutcome) -> ClaimedServer {
    match outcome {
        ServerClaimOutcome::Claimed(claimed) => claimed,
        other => panic!("expected a claim, got {other:?}"),
    }
}

#[test]
fn an_add_is_a_zero_op_group_whose_draft_is_stored_beside_it() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);

    let group = get_group(&conn, group_id).expect("read").expect("exists");
    assert_eq!(group.verb, ProposalVerb::AddServer);
    assert_eq!(group.reversible, Reversibility::ServerSettings);
    assert_eq!(group.source_volume_id, "", "an add has no place yet");
    assert_eq!(group.destination, None);
    assert_eq!(count_ops(&conn, group_id, None).expect("count"), 0);

    let stored = proposal(&conn, group_id);
    assert_eq!(stored.draft, sftp_draft("build.lan"));
    assert_eq!(stored.edit, None);
    assert_eq!(stored.note, None);
}

#[test]
fn an_edit_stores_the_place_it_changes_and_how_that_place_looked() {
    let conn = migrated_conn();
    let set_id = create_sweep(&conn, &NewSweep::default(), 100).expect("sweep");
    let group_id =
        create_group(&conn, set_id, &server_group(Some(edit()), sftp_draft("build.lan")), 100).expect("group");

    let group = get_group(&conn, group_id).expect("read").expect("exists");
    assert_eq!(group.verb, ProposalVerb::EditServer);
    assert_eq!(group.source_volume_id, "sftp-build-box");
    assert_eq!(proposal(&conn, group_id).edit, Some(edit()));
}

#[test]
fn a_file_group_has_no_server_proposal() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);
    assert_eq!(read_server_proposal(&conn, group_id).expect("read"), None);
}

/// The amend is the one way a draft changes, so it bumps the revision AND replaces the draft,
/// together.
#[test]
fn an_amend_replaces_the_draft_and_bumps_the_revision() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);

    let outcome = repropose_group(&conn, group_id, &server_group(None, sftp_draft("build2.lan")), 200).expect("amend");

    assert_eq!(outcome, ReproposeOutcome::Reproposed);
    assert_eq!(get_group(&conn, group_id).expect("read").expect("exists").revision, 1);
    assert_eq!(proposal(&conn, group_id).draft.host.as_deref(), Some("build2.lan"));
}

/// ⚠️ The draft and the revision commit together or not at all. A crash between them that left
/// the OLD draft under the NEW revision would let a click on the new card save the old server.
#[test]
fn an_amend_that_fails_writing_the_draft_leaves_the_old_draft_and_the_old_revision() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    conn.execute_batch(
        "CREATE TEMP TRIGGER fail_draft_write BEFORE UPDATE ON proposal_server_drafts
         BEGIN SELECT RAISE(ABORT, 'the disk went away'); END;",
    )
    .expect("trigger");

    let failed = repropose_group(&conn, group_id, &server_group(None, sftp_draft("build2.lan")), 200);

    assert!(failed.is_err(), "the amend reports the failure: {failed:?}");
    assert_eq!(get_group(&conn, group_id).expect("read").expect("exists").revision, 0);
    assert_eq!(proposal(&conn, group_id).draft.host.as_deref(), Some("build.lan"));
}

/// An amend may change a group's kind; the draft row follows the kind, so a file group never
/// carries a stale draft and a server group never lacks one.
#[test]
fn an_amend_that_changes_the_kind_moves_the_draft_with_it() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    let to_trash = NewGroup {
        intent: GroupIntent::Trash {
            sources: vec![NewOp {
                source_path: "/Users/test_user_13/old.dmg".to_string(),
                snapshot: None,
            }],
        },
        source_volume_id: "root".to_string(),
        display_name: "an installer".to_string(),
        rationale: None,
        selector: None,
    };

    repropose_group(&conn, group_id, &to_trash, 200).expect("to a file group");
    assert_eq!(read_server_proposal(&conn, group_id).expect("read"), None);
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM proposal_server_drafts", [], |row| row.get(0))
        .expect("count");
    assert_eq!(rows, 0, "a file group keeps no draft row");

    repropose_group(&conn, group_id, &server_group(None, sftp_draft("back.lan")), 300).expect("back to a server");
    assert_eq!(count_ops(&conn, group_id, None).expect("count"), 0);
    assert_eq!(proposal(&conn, group_id).draft.host.as_deref(), Some("back.lan"));
}

#[test]
fn the_server_claim_approves_and_hands_back_exactly_the_stored_draft() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);

    let claim = claimed(claim_server_group(&conn, group_id, 0, 300).expect("claim"));

    assert_eq!(claim.draft, sftp_draft("build.lan"));
    assert_eq!(claim.edit, None);
    assert_eq!(claim.group.status, ProposalStatus::Approved);
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Approved);
}

/// The revision IS a server group's binding: a card read before an amend never approves the
/// amended draft.
#[test]
fn a_server_claim_on_an_old_revision_refuses_and_leaves_the_group_pending() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    repropose_group(
        &conn,
        group_id,
        &server_group(None, sftp_draft("elsewhere.example")),
        200,
    )
    .expect("amend");

    let outcome = claim_server_group(&conn, group_id, 0, 300).expect("claim");

    assert_eq!(outcome, ServerClaimOutcome::RevisionChanged { shown: 0, found: 1 });
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Pending);
}

#[test]
fn two_concurrent_server_claims_leave_exactly_one_winner() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = crate::agent::store::main_db_path(dir.path());
    let setup = open_write_connection(&db_path).expect("open");
    let group_id = add_group(&setup);
    drop(setup);

    let barrier = Barrier::new(2);
    let outcomes: Vec<ServerClaimOutcome> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let barrier = &barrier;
                let db_path = &db_path;
                scope.spawn(move || {
                    let conn = open_write_connection(db_path).expect("open");
                    barrier.wait();
                    claim_server_group(&conn, group_id, 0, 300).expect("claim did not error")
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("thread")).collect()
    });

    let winners = outcomes
        .iter()
        .filter(|o| matches!(o, ServerClaimOutcome::Claimed(_)))
        .count();
    assert_eq!(winners, 1, "exactly one claim wins: {outcomes:?}");
    assert!(
        outcomes.contains(&ServerClaimOutcome::NotPending {
            found: ProposalStatus::Approved
        }),
        "the loser is told the group was already answered: {outcomes:?}"
    );
}

#[test]
fn the_server_claim_refuses_a_file_group() {
    let conn = migrated_conn();
    let group_id = group_with_ops(&conn, 2);

    assert_eq!(
        claim_server_group(&conn, group_id, 0, 300).expect("claim"),
        ServerClaimOutcome::NotAServerGroup
    );
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Pending);
}

/// The file path refuses a server group at preflight, typed, and with no acceptance on record
/// the file claim refuses it too: neither path can take the other's kind.
#[test]
fn file_approval_refuses_a_server_group() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);

    assert_eq!(
        record_acceptance(&conn, group_id, 0, &OpSelection::all_except(&[]), 200).expect("preflight"),
        AcceptanceOutcome::NotAFileGroup
    );
    assert!(matches!(
        claim_group_for_execution(&conn, group_id, 300).expect("claim"),
        ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { accepted: None, .. })
    ));
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Pending);
}

#[test]
fn an_unknown_group_is_unknown_to_the_server_claim() {
    let conn = migrated_conn();
    assert_eq!(
        claim_server_group(&conn, 999, 0, 300).expect("claim"),
        ServerClaimOutcome::Unknown
    );
}

/// A draft that won't decode is never shown, listed, or claimed as a guess.
#[test]
fn a_draft_that_wont_decode_fails_every_read_and_the_claim() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    // A secret-shaped key is exactly what the type refuses to read back.
    conn.execute(
        "UPDATE proposal_server_drafts SET draft = ?2 WHERE group_id = ?1",
        params![
            group_id,
            r#"{"protocol":"sftp","host":"build.lan","password":"hunter2"}"#
        ],
    )
    .expect("corrupt");

    let undecodable = |result: Result<_, AgentStoreError>| {
        matches!(
            result,
            Err(AgentStoreError::ServerDraft {
                problem: ServerDraftProblem::Undecodable,
                ..
            })
        )
    };
    assert!(undecodable(read_server_proposal(&conn, group_id).map(|_| ())));
    assert!(undecodable(list_groups(&conn, None).map(|_| ())));
    assert!(undecodable(claim_server_group(&conn, group_id, 0, 300).map(|_| ())));
    assert_eq!(
        status_of(&conn, group_id),
        ProposalStatus::Pending,
        "a failed claim rolls back"
    );
}

#[test]
fn a_server_group_without_its_draft_row_fails_closed() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    conn.execute(
        "DELETE FROM proposal_server_drafts WHERE group_id = ?1",
        params![group_id],
    )
    .expect("delete");

    assert!(matches!(
        read_server_proposal(&conn, group_id),
        Err(AgentStoreError::ServerDraft {
            problem: ServerDraftProblem::Missing,
            ..
        })
    ));
}

#[test]
fn the_draft_row_goes_with_its_sweep() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    let set_id = get_group(&conn, group_id).expect("read").expect("exists").set_id;

    conn.execute("DELETE FROM proposal_sets WHERE id = ?1", params![set_id])
        .expect("delete sweep");

    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM proposal_server_drafts", [], |row| row.get(0))
        .expect("count");
    assert_eq!(rows, 0);
}

/// A partial save completes WITH its note, in one write, and the note reads back.
#[test]
fn a_completed_server_group_carries_what_its_save_left_undone() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    claimed(claim_server_group(&conn, group_id, 0, 300).expect("claim"));

    let outcome = mark_server_group_completed(&conn, group_id, Some(ServerSaveNote::ShareNotSaved)).expect("complete");

    assert_eq!(outcome, CompleteOutcome::Completed);
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Completed);
    assert_eq!(proposal(&conn, group_id).note, Some(ServerSaveNote::ShareNotSaved));
}

#[test]
fn completing_an_unclaimed_server_group_writes_no_note() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);

    let outcome = mark_server_group_completed(&conn, group_id, Some(ServerSaveNote::ShareNotSaved)).expect("complete");

    assert_eq!(outcome, CompleteOutcome::NotApproved);
    assert_eq!(proposal(&conn, group_id).note, None);
}

/// A claim goes back to `pending` only while nothing has been written. Once a save completed,
/// the group never returns to approvable: a second click would save twice.
#[test]
fn a_server_claim_goes_back_only_before_anything_was_written() {
    let conn = migrated_conn();
    let refused_save = add_group(&conn);
    claimed(claim_server_group(&conn, refused_save, 0, 300).expect("claim"));
    assert!(release_claim(&conn, refused_save).expect("release"));
    assert_eq!(status_of(&conn, refused_save), ProposalStatus::Pending);

    let saved = add_group(&conn);
    claimed(claim_server_group(&conn, saved, 0, 300).expect("claim"));
    mark_server_group_completed(&conn, saved, None).expect("complete");
    assert!(!release_claim(&conn, saved).expect("release"));
    assert_eq!(status_of(&conn, saved), ProposalStatus::Completed);
}

/// An app that died mid-save can't know whether the save landed, so the group is frozen
/// `interrupted`, never offered again as pending.
#[test]
fn recovery_freezes_a_server_group_the_app_died_saving() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    claimed(claim_server_group(&conn, group_id, 0, 300).expect("claim"));

    assert_eq!(recover_interrupted_groups(&conn).expect("recover"), 1);
    assert_eq!(status_of(&conn, group_id), ProposalStatus::Interrupted);
}

/// The badge counts groups, so a server card waiting on the user shows in it.
#[test]
fn a_pending_server_group_counts_toward_the_badge() {
    let conn = migrated_conn();
    add_group(&conn);
    group_with_ops(&conn, 3);

    assert_eq!(count_pending(&conn, ProposalStatus::Pending).expect("count"), (2, 3));
}

#[test]
fn a_listed_server_group_carries_its_draft() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);

    let listed = list_groups(&conn, Some(ProposalStatus::Pending)).expect("list");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].group.id, group_id);
    assert_eq!(listed[0].live_op_count, 0);
    assert_eq!(
        listed[0].server.as_ref().map(|s| &s.draft),
        Some(&sftp_draft("build.lan"))
    );
}

/// The draft has no field a secret could ride in, and the stored JSON proves it.
#[test]
fn a_stored_draft_has_no_secret_shaped_key() {
    let conn = migrated_conn();
    let group_id = add_group(&conn);
    let stored: String = conn
        .query_row(
            "SELECT draft FROM proposal_server_drafts WHERE group_id = ?1",
            params![group_id],
            |row| row.get(0),
        )
        .expect("read");

    let lower = stored.to_lowercase();
    for key in ["password", "secret", "keyfile", "useagent", "autoreconnect"] {
        assert!(!lower.contains(key), "{key} in {stored}");
    }
}
