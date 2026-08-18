//! Rename-proposal tests: the plan boundary (scope, validation, the evidence guardrail),
//! what staging into `main.db` makes true, and the preflight engine's blocks and warnings.

use super::plan::{
    ProposalRefusal, RenameInput, check_row_evidence, missing_local_child, refusal_content, scoped_files,
    validate_destination_name,
};
use super::preflight::{
    allowed_rows, initial_rows, mark_cycle_warnings, mark_duplicate_destinations, preflight_local, rename_warnings,
};
use super::revise::revise_staged_row;
use super::store::{RenameDraft, RenameDraftRow};
use super::{
    AcceptedPreflight, AcceptedRenamePreflights, BulkRenameBlockReason, BulkRenamePreflightStatus, BulkRenameWarning,
    RenameProposal, RenameProposalRow,
};
use crate::agent::store::proposals::{ClaimOutcome, ClaimRefusal, claim_group_for_execution, record_acceptance};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::tools::propose::evidence::{
    EvidenceProblem, EvidenceScope, EvidenceSource, ImageFactsLedger, RenameEvidence,
};
use crate::mcp::pane_state::{PaneFileEntry, PaneState};
use rusqlite::Connection;

/// The chat thread these tests deliver into and propose from.
const THREAD: EvidenceScope = EvidenceScope::Thread(11);

#[test]
fn cycle_warnings_mark_only_closed_dependency_components() {
    let proposal = RenameProposal {
        proposal_id: "proposal".into(),
        rows: vec![
            proposal_row("chain-a", "/x/a", "b"),
            proposal_row("chain-b", "/x/b", "free"),
            proposal_row("cycle-a", "/x/c", "d"),
            proposal_row("cycle-b", "/x/d", "c"),
        ],
    };
    let allowed_ids: Vec<String> = proposal.rows.iter().map(|row| row.row_id.clone()).collect();
    let mut statuses = initial_rows(&proposal, &allowed_ids);
    let allowed = allowed_rows(&proposal, &allowed_ids, &mut statuses);

    mark_cycle_warnings(&allowed, &mut statuses);

    assert!(statuses["chain-a"].warnings.is_empty());
    assert!(statuses["chain-b"].warnings.is_empty());
    assert_eq!(statuses["cycle-a"].warnings, vec![BulkRenameWarning::Cycle]);
    assert_eq!(statuses["cycle-b"].warnings, vec![BulkRenameWarning::Cycle]);
}

/// A staged row with the evidence a real one always carries. Preflight and the store
/// don't read evidence (the tool boundary already checked it), so a filename source
/// keeps these fixtures about the thing under test.
fn proposal_row(row_id: &str, source_path: &str, destination_name: &str) -> RenameProposalRow {
    RenameProposalRow {
        row_id: row_id.into(),
        source_path: source_path.into(),
        volume_id: "root".into(),
        destination_name: destination_name.into(),
        evidence: RenameEvidence {
            source: EvidenceSource::Filename,
            detail: "the old name".into(),
        },
        coverage: None,
    }
}

#[test]
fn extension_warnings_cover_changes_additions_removals_and_filename_edges() {
    for (source, destination) in [
        ("photo.png", "photo.jpg"),
        ("photo.png", "photo"),
        ("README", "README.md"),
        (".env", ".env.txt"),
        ("archive.tar.gz", "archive.tar.zip"),
        ("trailing.", "trailing"),
    ] {
        assert_eq!(
            rename_warnings(source, destination),
            vec![BulkRenameWarning::ExtensionChanged],
            "expected an extension warning for {source:?} -> {destination:?}"
        );
    }

    for (source, destination) in [
        ("photo.png", "renamed.png"),
        ("photo.PNG", "renamed.png"),
        (".env", ".config"),
        ("archive.tar.gz", "renamed.gz"),
    ] {
        assert!(
            rename_warnings(source, destination).is_empty(),
            "did not expect an extension warning for {source:?} -> {destination:?}"
        );
    }
}

#[test]
fn local_preflight_blocks_a_source_that_no_longer_exists() {
    let temp = tempfile::tempdir().expect("temp directory");
    let missing = temp.path().join("missing.png");
    let proposal = RenameProposal {
        proposal_id: "proposal".into(),
        rows: vec![proposal_row(
            "row",
            missing.to_str().expect("UTF-8 temp path"),
            "renamed.png",
        )],
    };

    let outcome = preflight_local(&proposal, &["row".into()]);

    assert_eq!(outcome.status, BulkRenamePreflightStatus::Blocked);
    assert_eq!(
        outcome.response.rows[0].reason,
        Some(BulkRenameBlockReason::SourceMissing)
    );
    assert!(outcome.fingerprints.is_empty());
}

#[test]
fn only_a_missing_direct_child_can_enter_review_without_a_pane_entry() {
    let temp = tempfile::tempdir().expect("temp directory");
    let state = PaneState {
        path: temp.path().to_string_lossy().into_owned(),
        ..PaneState::default()
    };
    let missing = temp.path().join("imagined.png");
    let nested = temp.path().join("nested").join("imagined.png");
    let existing = temp.path().join("existing.png");
    std::fs::write(&existing, b"present").expect("write fixture");

    assert!(missing_local_child(
        &state,
        "root",
        missing.to_str().expect("UTF-8 path")
    ));
    assert!(!missing_local_child(
        &state,
        "root",
        nested.to_str().expect("UTF-8 path")
    ));
    assert!(!missing_local_child(
        &state,
        "root",
        existing.to_str().expect("UTF-8 path")
    ));
    assert!(!missing_local_child(
        &state,
        "mtp-device",
        missing.to_str().expect("UTF-8 path")
    ));
}

#[test]
fn destination_names_reject_paths_and_dot_entries() {
    for name in ["", ".", "..", "folder/name.png", "folder\\name.png"] {
        assert!(validate_destination_name(name).is_err(), "{name}");
    }
    assert!(validate_destination_name("2026-07-20 - Receipt.png").is_ok());
}
// ── Where a staged proposal lives ─────────────────────────────────────────────

/// A migrated in-memory `main.db`.
fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// Stage `rows` as one group and read the proposal back, the way every later step does.
fn staged(conn: &Connection, rows: Vec<RenameDraftRow>) -> RenameProposal {
    let draft = RenameDraft {
        volume_id: "root".into(),
        parent: "/shots".into(),
        rows,
    };
    // No conversation id: `proposal_sets.conversation_id` is a real foreign key, and these
    // fixtures are about the proposal, not the thread it came out of.
    super::store::stage(conn, None, &draft, 100)
        .expect("stage")
        .expect("a group was created");
    let proposal_id = last_group_id(conn).to_string();
    super::store::load(conn, &proposal_id)
        .expect("load")
        .expect("the staged proposal is reviewable")
}

fn last_group_id(conn: &Connection) -> i64 {
    conn.query_row("SELECT MAX(id) FROM proposals", [], |row| row.get(0))
        .expect("a staged group")
}

#[test]
fn a_staged_proposal_reads_back_the_same_every_time() {
    let conn = migrated_conn();
    let proposal = staged(
        &conn,
        vec![draft_row(
            "/shots/a.png",
            "b.png",
            EvidenceSource::Filename,
            "the old name",
        )],
    );

    let snapshot = proposal.snapshot();
    assert_eq!(snapshot.rows[0].source_name, "a.png");
    assert_eq!(snapshot.rows[0].destination_name, "b.png");
    assert!(
        super::store::load(&conn, &proposal.proposal_id)
            .expect("load")
            .is_some()
    );
    assert!(
        super::store::load(&conn, &proposal.proposal_id)
            .expect("load")
            .is_some(),
        "reading a review never consumes it"
    );
}

/// The reason the proposal moved into `main.db` at all: it has no expiry, so it is still
/// there for the user to answer after a quit. What the agent proposed and what the user was
/// asked outlives the process that asked it.
#[test]
fn a_staged_proposal_survives_a_store_reopen() {
    let dir = crate::test_support::TestDir::new("rename-proposal-reopen");
    let db_path = dir.join("main.db");
    let proposal_id = {
        let conn = crate::agent::store::open_write_connection(&db_path).expect("open");
        let proposal = staged(
            &conn,
            vec![draft_row(
                "/shots/one.png",
                "Invoice 4021.png",
                EvidenceSource::Metadata,
                "Taken 2026-07-20",
            )],
        );
        proposal.proposal_id
    };

    let conn = crate::agent::store::open_write_connection(&db_path).expect("reopen");
    let reopened = super::store::load(&conn, &proposal_id)
        .expect("load")
        .expect("the proposal is still there");
    assert_eq!(reopened.rows.len(), 1);
    assert_eq!(reopened.rows[0].destination_name, "Invoice 4021.png");
    assert_eq!(
        reopened.rows[0].evidence.source,
        EvidenceSource::Metadata,
        "and it still says where its name came from"
    );
}

/// The other half of no-expiry, and the one that matters for data safety: an APPROVAL does
/// not survive. The fingerprints an acceptance pairs with describe files as they were before
/// the app died, so a restart has to force a fresh preflight rather than resurrect it.
#[test]
fn an_accepted_preflight_does_not_survive_a_store_reopen() {
    let dir = crate::test_support::TestDir::new("rename-acceptance-reopen");
    let db_path = dir.join("main.db");
    let accepted = AcceptedRenamePreflights::default();
    let (proposal_id, allowed) = {
        let conn = crate::agent::store::open_write_connection(&db_path).expect("open");
        let proposal = staged(
            &conn,
            vec![draft_row("/shots/a.png", "b.png", EvidenceSource::Filename, "old name")],
        );
        let allowed: Vec<String> = proposal.rows.iter().map(|row| row.row_id.clone()).collect();
        record_acceptance(&conn, last_group_id(&conn), &[], 200).expect("preflight");
        accepted.record(
            &proposal.proposal_id,
            AcceptedPreflight {
                allowed_row_ids: allowed.clone(),
                fingerprints: vec![],
            },
        );
        (proposal.proposal_id, allowed)
    };

    // What a restart leaves behind: the durable half only.
    let restarted = AcceptedRenamePreflights::default();
    let conn = crate::agent::store::open_write_connection(&db_path).expect("reopen");
    assert!(
        super::store::load(&conn, &proposal_id).expect("load").is_some(),
        "the proposal itself is still reviewable"
    );
    assert!(
        restarted.matching(&proposal_id, &allowed).is_none(),
        "but nothing may apply it without preflighting it again"
    );
    assert!(restarted.take_matching(&proposal_id, &allowed).is_none());
}

#[test]
fn current_folder_entries_are_a_valid_rename_scope_while_listing_updates() {
    let state = PaneState {
        total_files: 2,
        files: vec![PaneFileEntry {
            name: "a.png".into(),
            path: "/ignored/a.png".into(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
            recursive_size_pending: None,
            tags: vec![],
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(scoped_files(&state).expect("current entries are usable").len(), 1);
}

/// `selected_indices` are GLOBAL listing indices; `files` holds only the loaded window
/// from `loaded_start`. Reading the window with a global index scopes the plan to the
/// wrong files, so a rename the user reviewed as "these two" would land on two others.
/// Same conversion as `read::pane_listing` and `mcp::executor`.
#[test]
fn a_scrolled_pane_scopes_the_plan_to_the_rows_the_user_picked() {
    let entry = |global: usize| PaneFileEntry {
        name: format!("shot-{global}.png"),
        path: format!("/shots/shot-{global}.png"),
        is_directory: false,
        size: None,
        recursive_size: None,
        modified: None,
        recursive_size_pending: None,
        tags: vec![],
        ..Default::default()
    };
    let state = PaneState {
        path: "/shots".into(),
        // Global rows 100..104 are loaded; the user selected 101 and 103.
        files: (100..104).map(entry).collect(),
        loaded_start: 100,
        loaded_end: 104,
        selected_indices: vec![101, 103],
        total_files: 5_000,
        ..Default::default()
    };

    let scoped = scoped_files(&state).expect("the selected rows are inside the loaded window");

    let mut names: Vec<&str> = scoped.values().map(|entry| entry.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["shot-101.png", "shot-103.png"]);
}

/// A row scrolled out BELOW the window must stay unresolvable. With `saturating_sub` it
/// would collapse onto the window's first entry, renaming a file the user never picked
/// while the plan still looked complete.
#[test]
fn a_selected_row_outside_the_loaded_window_refuses_the_scope() {
    let state = PaneState {
        path: "/shots".into(),
        files: vec![PaneFileEntry {
            name: "shot-100.png".into(),
            path: "/shots/shot-100.png".into(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
            recursive_size_pending: None,
            tags: vec![],
            ..Default::default()
        }],
        loaded_start: 100,
        loaded_end: 101,
        selected_indices: vec![7],
        total_files: 5_000,
        ..Default::default()
    };

    assert!(
        scoped_files(&state).is_err(),
        "an out-of-window row must refuse, never resolve to a neighbour"
    );
}

#[test]
fn duplicate_final_targets_block_every_row_in_the_group() {
    let first = proposal_row("first", "/ignored/a.png", "same.png");
    let second = proposal_row("second", "/ignored/b.png", "same.png");
    let mut statuses = initial_rows(
        &RenameProposal {
            proposal_id: "proposal".into(),
            rows: vec![first.clone(), second.clone()],
        },
        &["first".into(), "second".into()],
    );
    mark_duplicate_destinations(&[&first, &second], &mut statuses);
    assert!(
        statuses
            .values()
            .all(|row| row.reason == Some(BulkRenameBlockReason::DuplicateDestination))
    );
}

/// The incident's shape at the plan boundary: one row cites image text nobody
/// delivered, its neighbours are fine. Every offending row comes back named, and the
/// caller refuses the WHOLE plan rather than staging a partial one the user would read
/// as complete.
#[test]
fn one_unbacked_content_claim_rejects_that_row_and_refuses_the_whole_plan() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &serde_json::json!({ "status": "ok", "facts": [
            { "path": "/shots/one.png", "state": "indexed", "text": "Invoice 4021 total 250 SEK" }
        ] }),
    );
    let mut rows = vec![
        draft_row(
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021",
        ),
        draft_row(
            "/shots/two.png",
            "hello-world-output.png",
            EvidenceSource::ImageText,
            "hello world output",
        ),
        draft_row(
            "/shots/three.png",
            "2026-07-20.png",
            EvidenceSource::Metadata,
            "Taken 2026-07-20",
        ),
    ];

    let rejections = check_row_evidence(&ledger, THREAD, &mut rows).expect_err("the plan is refused");

    assert_eq!(rejections.len(), 1, "only the unbacked row is rejected");
    assert_eq!(rejections[0].source_path, "/shots/two.png");
    assert_eq!(rejections[0].proposed_name, "hello-world-output.png");
    assert_eq!(rejections[0].problem, EvidenceProblem::FactsNotDelivered);

    // A rejected row is never silently dropped: the model gets the typed verdict, and
    // nothing was staged, so the user sees no plan at all.
    let content = refusal_content(&ProposalRefusal::Evidence(rejections));
    assert_eq!(content["readyForReview"], false);
    assert_eq!(content["evidenceRejected"][0]["problem"], "factsNotDelivered");
    assert_eq!(content["evidenceRejected"][0]["evidenceSource"], "imageText");
    assert!(
        content["guidance"].is_string(),
        "the model gets something it can act on"
    );
}

/// Evidence rides the snapshot into the review dialog, so the reviewer can see what
/// each name is based on rather than only old-name → new-name.
#[test]
fn the_review_snapshot_carries_each_rows_evidence() {
    let conn = migrated_conn();
    let snapshot = staged(
        &conn,
        vec![draft_row(
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021",
        )],
    )
    .snapshot();

    assert_eq!(snapshot.rows[0].evidence.source, EvidenceSource::ImageText);
    assert_eq!(snapshot.rows[0].evidence.detail, "Invoice 4021");
    let wire = serde_json::to_value(&snapshot).expect("serializes");
    assert_eq!(wire["rows"][0]["evidence"]["source"], "imageText");
    assert_eq!(wire["rows"][0]["evidence"]["detail"], "Invoice 4021");
}

/// The review dialog can't show how thin a match is unless the numbers get there, and it
/// can't show the file unless the path does. Both ride the row snapshot: the offset and the
/// delivered length turn a bare quote into "matched 20 of 61 characters", and the path is
/// what the thumbnail and the full viewer open.
#[test]
fn the_review_snapshot_carries_the_matchs_coverage_and_the_previewable_path() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &serde_json::json!({ "status": "ok", "facts": [
            { "path": "/shots/one.png", "state": "indexed",
              "text": "Order summary\nKlarna payment confirmation 1,299 SEK\nThank you" }
        ] }),
    );
    let mut rows = vec![
        draft_row(
            "/shots/one.png",
            "Klarna payment.png",
            EvidenceSource::ImageText,
            "payment confirmation",
        ),
        draft_row(
            "/shots/two.png",
            "2026-07-20.png",
            EvidenceSource::Metadata,
            "Taken 2026-07-20",
        ),
    ];

    check_row_evidence(&ledger, THREAD, &mut rows).expect("both rows check out");
    let conn = migrated_conn();
    let snapshot = staged(&conn, rows).snapshot();

    let wire = serde_json::to_value(&snapshot).expect("serializes");
    let quoted = &wire["rows"][0];
    assert_eq!(quoted["sourcePath"], "/shots/one.png");
    assert_eq!(quoted["volumeId"], "root");
    assert_eq!(quoted["coverage"]["matchOffset"], 21);
    assert_eq!(quoted["coverage"]["matchedChars"], 20);
    assert_eq!(quoted["coverage"]["deliveredChars"], 61);
    assert_eq!(quoted["coverage"]["matchedText"], "payment confirmation");
    assert_eq!(quoted["coverage"]["contextBefore"], "Klarna ");
    assert_eq!(quoted["coverage"]["contextAfter"], " 1,299 SEK");
    assert!(
        wire["rows"][1]["coverage"].is_null(),
        "a metadata row measures no span of delivered text"
    );
}

/// The tool can't be called without evidence at all: an old-shaped plan is a param
/// refusal, not a plan with an empty justification.
#[test]
fn a_rename_row_without_evidence_does_not_parse() {
    let without = serde_json::json!({ "sourcePath": "/x/a.png", "volumeId": "root", "destinationName": "b.png" });
    assert!(serde_json::from_value::<RenameInput>(without).is_err());

    let with = serde_json::json!({
        "sourcePath": "/x/a.png", "volumeId": "root", "destinationName": "b.png",
        "evidence": { "source": "filename", "detail": "the old name" }
    });
    assert!(serde_json::from_value::<RenameInput>(with).is_ok());
}

/// One row of a plan on its way in: no id yet, because the store hands those out.
fn draft_row(source_path: &str, destination_name: &str, source: EvidenceSource, detail: &str) -> RenameDraftRow {
    RenameDraftRow {
        source_path: source_path.into(),
        destination_name: destination_name.into(),
        evidence: RenameEvidence {
            source,
            detail: detail.into(),
        },
        coverage: None,
    }
}

#[test]
fn accepted_preflight_requires_the_exact_allowed_subset() {
    let accepted = AcceptedRenamePreflights::default();
    accepted.record(
        "7",
        AcceptedPreflight {
            allowed_row_ids: vec!["row".into()],
            fingerprints: vec![],
        },
    );

    assert!(accepted.matching("7", &["row".into()]).is_some());
    assert!(
        accepted.matching("7", &["other".into()]).is_none(),
        "an acceptance describes the rows it checked and no others"
    );
    assert!(accepted.matching("8", &["row".into()]).is_none());
}

// ── Revising one row's name, as the user typed it ─────────────────────────────

/// Stages `rows` and records both halves of the acceptance a Ready review leaves behind: the
/// spine's own record of the values, and this process's row ids plus fingerprints.
fn staged_with_accepted_preflight(
    conn: &Connection,
    accepted: &AcceptedRenamePreflights,
    rows: Vec<RenameDraftRow>,
) -> RenameProposal {
    let proposal = staged(conn, rows);
    let allowed_row_ids: Vec<String> = proposal.rows.iter().map(|row| row.row_id.clone()).collect();
    record_acceptance(conn, last_group_id(conn), &[], 200).expect("preflight");
    accepted.record(
        &proposal.proposal_id,
        AcceptedPreflight {
            allowed_row_ids,
            fingerprints: vec![],
        },
    );
    proposal
}

/// The user's name is the first destination name that crosses IPC, so the server validates it
/// exactly as it validates the model's. Nothing downstream looks at a name again: apply resolves
/// it from this stored row, never from the client.
#[test]
fn revising_a_row_validates_the_name_on_the_server() {
    let conn = migrated_conn();
    let accepted = AcceptedRenamePreflights::default();
    let proposal = staged_with_accepted_preflight(
        &conn,
        &accepted,
        vec![draft_row(
            "/shots/a.png",
            "b.png",
            EvidenceSource::Filename,
            "the old name",
        )],
    );
    let id = proposal.proposal_id.as_str();
    let row_id = proposal.rows[0].row_id.as_str();

    for name in ["", "   ", ".", "..", "folder/name.png", "folder\\name.png"] {
        assert!(
            revise_staged_row(&conn, &accepted, id, row_id, name).is_err(),
            "{name:?} must be refused"
        );
    }
    assert!(revise_staged_row(&conn, &accepted, id, "unknown-row", "fine.png").is_err());
    assert!(revise_staged_row(&conn, &accepted, "9999", row_id, "fine.png").is_err());

    let revised = revise_staged_row(&conn, &accepted, id, row_id, "Receipt 2026-07-20.png").expect("a valid filename");
    assert_eq!(revised.destination_name, "Receipt 2026-07-20.png");
    assert_eq!(
        super::store::load(&conn, id).expect("load").expect("still staged").rows[0].destination_name,
        "Receipt 2026-07-20.png",
        "the stored row is what apply reads, so the edit has to land there"
    );
}

/// The data-safety case. Apply skips its own re-check when the allowed row ids match the
/// accepted preflight, and duplicate-destination, cycle, and case-only detection all live in
/// preflight. So edit → preflight → edit again → apply would put a name on disk that none of
/// those checks ever saw. Any revise clears the acceptance (invariant 10).
#[test]
fn revising_a_row_clears_the_accepted_preflight() {
    let conn = migrated_conn();
    let accepted = AcceptedRenamePreflights::default();
    let proposal = staged_with_accepted_preflight(
        &conn,
        &accepted,
        vec![draft_row(
            "/shots/a.png",
            "b.png",
            EvidenceSource::Filename,
            "the old name",
        )],
    );
    let allowed: Vec<String> = proposal.rows.iter().map(|row| row.row_id.clone()).collect();
    assert!(
        accepted.matching(&proposal.proposal_id, &allowed).is_some(),
        "the review starts from a Ready preflight"
    );

    revise_staged_row(
        &conn,
        &accepted,
        &proposal.proposal_id,
        &proposal.rows[0].row_id,
        "typed-by-hand.png",
    )
    .expect("a valid filename");

    assert!(
        accepted.matching(&proposal.proposal_id, &allowed).is_none(),
        "an edited name must be preflighted again before it can reach the filesystem"
    );
    assert!(
        accepted.take_matching(&proposal.proposal_id, &allowed).is_none(),
        "and apply must not be able to consume the stale acceptance either"
    );
}

/// Belt and braces for the same failure, and the half the client can't reach: the spine's
/// acceptance record binds the VALUES the ops carried, so a revised name makes the claim
/// refuse with a binding mismatch even if some future path forgets to drop the fingerprints.
/// Apply then falls back to a fresh authoritative preflight instead of trusting the old one.
#[test]
fn a_revised_name_makes_the_claim_refuse() {
    let conn = migrated_conn();
    let accepted = AcceptedRenamePreflights::default();
    staged_with_accepted_preflight(
        &conn,
        &accepted,
        vec![draft_row(
            "/shots/a.png",
            "b.png",
            EvidenceSource::Filename,
            "the old name",
        )],
    );
    assert!(
        matches!(
            claim_group_for_execution(&conn, last_group_id(&conn), 300).expect("claim"),
            ClaimOutcome::Claimed(_)
        ),
        "the counterfactual: the plan the user preflighted claims cleanly"
    );

    // Same plan, same acceptance, one name typed over.
    let conn = migrated_conn();
    let accepted = AcceptedRenamePreflights::default();
    let proposal = staged_with_accepted_preflight(
        &conn,
        &accepted,
        vec![draft_row(
            "/shots/a.png",
            "b.png",
            EvidenceSource::Filename,
            "the old name",
        )],
    );
    revise_staged_row(
        &conn,
        &accepted,
        &proposal.proposal_id,
        &proposal.rows[0].row_id,
        "typed-by-hand.png",
    )
    .expect("a valid filename");

    assert!(
        matches!(
            claim_group_for_execution(&conn, last_group_id(&conn), 300).expect("claim"),
            ClaimOutcome::Refused(ClaimRefusal::BindingMismatch { .. })
        ),
        "a name preflight never checked can't ride an older approval onto the filesystem"
    );
}

/// Invariant 10: a user-edited name needs no evidence, never claims any, and never inherits
/// the model's. Evidence rides the row snapshot into the dialog, so keeping the model's quote
/// beside the user's own name would credit the model for a name it didn't choose.
#[test]
fn a_revised_row_reports_user_edited_and_keeps_no_evidence() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &serde_json::json!({ "status": "ok", "facts": [
            { "path": "/shots/one.png", "state": "indexed", "text": "Klarna payment confirmation 1,299 SEK" }
        ] }),
    );
    let mut rows = vec![draft_row(
        "/shots/one.png",
        "Klarna invoice.png",
        EvidenceSource::ImageText,
        "payment confirmation",
    )];
    check_row_evidence(&ledger, THREAD, &mut rows).expect("the quote checks out");
    assert!(
        rows[0].coverage.is_some(),
        "the model's row carries its match's coverage"
    );
    let conn = migrated_conn();
    let accepted = AcceptedRenamePreflights::default();
    let proposal = staged_with_accepted_preflight(&conn, &accepted, rows);

    let revised = revise_staged_row(
        &conn,
        &accepted,
        &proposal.proposal_id,
        &proposal.rows[0].row_id,
        "Klarna payment 2026-07-20.png",
    )
    .expect("valid");

    assert_eq!(revised.evidence.source, EvidenceSource::UserEdited);
    assert_eq!(revised.evidence.detail, "", "a typed name quotes nothing");
    assert!(revised.coverage.is_none(), "and measures no span of delivered text");
    let wire = serde_json::to_value(&revised).expect("serializes");
    assert_eq!(wire["evidence"]["source"], "userEdited");
    assert!(
        !wire.to_string().contains("payment confirmation"),
        "the model's quote must not survive beside the user's name"
    );
}

/// One revoked `call_id` refuses a WHOLE plan under the shipped evidence rule, so routing an
/// edit through the proposal path would let fixing row two destroy all 50 rows of a review.
/// Revise is its own narrow operation and consults no ledger.
#[test]
fn revising_a_row_does_not_re_run_the_whole_plan_evidence_rule() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &serde_json::json!({ "status": "ok", "facts": [
            { "path": "/shots/one.png", "state": "indexed", "text": "Invoice 4021 total 250 SEK" },
            { "path": "/shots/two.png", "state": "indexed", "text": "Order summary 1,299 SEK" }
        ] }),
    );
    let mut rows = vec![
        draft_row(
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021 total",
        ),
        draft_row(
            "/shots/two.png",
            "Order summary.png",
            EvidenceSource::ImageText,
            "Order summary 1,299",
        ),
    ];
    check_row_evidence(&ledger, THREAD, &mut rows).expect("both quotes check out");
    let conn = migrated_conn();
    let accepted = AcceptedRenamePreflights::default();
    let proposal = staged_with_accepted_preflight(&conn, &accepted, rows.clone());

    // The prompt dropped that result after the fact, so the ledger no longer vouches for
    // either row: a re-staged plan would now be refused in full.
    ledger.revoke_call("call-1");
    let mut restaged = rows.clone();
    assert!(
        check_row_evidence(&ledger, THREAD, &mut restaged).is_err(),
        "the counterfactual: the whole-plan rule would refuse this plan now"
    );

    revise_staged_row(
        &conn,
        &accepted,
        &proposal.proposal_id,
        &proposal.rows[0].row_id,
        "Invoice 4021 (Klarna).png",
    )
    .expect("the edit still lands");

    let stored = super::store::load(&conn, &proposal.proposal_id)
        .expect("load")
        .expect("the review survives");
    assert_eq!(stored.rows[0].destination_name, "Invoice 4021 (Klarna).png");
    assert_eq!(
        stored.rows[1].evidence.source,
        EvidenceSource::ImageText,
        "the neighbour keeps its own evidence"
    );
    assert_eq!(stored.rows[1].destination_name, "Order summary.png");
    assert!(stored.rows[1].coverage.is_some(), "and its coverage");
}

/// `userEdited` is the review dialog's word for a name the USER typed, so the model may never
/// send it: a plan that did would put "You typed this name" beside a name it invented itself.
#[test]
fn a_plan_cannot_claim_the_user_typed_a_name() {
    let mut rows = vec![draft_row(
        "/shots/one.png",
        "hand-typed.png",
        EvidenceSource::UserEdited,
        "the user typed this",
    )];

    let rejections = check_row_evidence(&ImageFactsLedger::default(), THREAD, &mut rows).expect_err("refused");

    assert_eq!(rejections.len(), 1);
    assert_eq!(rejections[0].problem, EvidenceProblem::SourceReservedForUser);
}
