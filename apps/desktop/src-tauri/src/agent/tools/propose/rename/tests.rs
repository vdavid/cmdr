//! Rename-proposal tests: the plan boundary (scope, validation, the evidence guardrail),
//! the store's staging lifetime, and the preflight engine's blocks and warnings.

use super::plan::{
    ProposalRefusal, RenameInput, check_row_evidence, missing_local_child, refusal_content, scoped_files,
    validate_destination_name,
};
use super::preflight::{
    allowed_rows, initial_rows, mark_cycle_warnings, mark_duplicate_destinations, preflight_local, rename_warnings,
};
use super::revise::revise_staged_row;
use super::{
    AcceptedPreflight, BulkRenameBlockReason, BulkRenamePreflightStatus, BulkRenameWarning, RenameProposal,
    RenameProposalRow, RenameProposalStore,
};
use crate::agent::tools::propose::evidence::{
    EvidenceProblem, EvidenceScope, EvidenceSource, ImageFactsLedger, RenameEvidence,
};
use crate::mcp::pane_state::{PaneFileEntry, PaneState};

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
#[test]
fn store_returns_an_immutable_snapshot_and_consumes_once() {
    let store = RenameProposalStore::default();
    let proposal = RenameProposal {
        proposal_id: "proposal".into(),
        rows: vec![proposal_row("row", "/x/a.png", "b.png")],
    };
    let snapshot = store.stage(proposal);
    assert_eq!(snapshot.rows[0].source_name, "a.png");
    assert!(store.get("proposal").is_some());
    assert!(store.get("proposal").is_some());
    assert!(store.consume("proposal").is_some());
    assert!(store.get("proposal").is_none());
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
        evidence_row(
            "backed",
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021",
        ),
        evidence_row(
            "fabricated",
            "/shots/two.png",
            "hello-world-output.png",
            EvidenceSource::ImageText,
            "hello world output",
        ),
        evidence_row(
            "dated",
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
    let store = RenameProposalStore::default();
    let snapshot = store.stage(RenameProposal {
        proposal_id: "proposal".into(),
        rows: vec![evidence_row(
            "row",
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021",
        )],
    });

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
        evidence_row(
            "quoted",
            "/shots/one.png",
            "Klarna payment.png",
            EvidenceSource::ImageText,
            "payment confirmation",
        ),
        evidence_row(
            "dated",
            "/shots/two.png",
            "2026-07-20.png",
            EvidenceSource::Metadata,
            "Taken 2026-07-20",
        ),
    ];

    check_row_evidence(&ledger, THREAD, &mut rows).expect("both rows check out");
    let store = RenameProposalStore::default();
    let snapshot = store.stage(RenameProposal {
        proposal_id: "proposal".into(),
        rows,
    });

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

fn evidence_row(
    row_id: &str,
    source_path: &str,
    destination_name: &str,
    source: EvidenceSource,
    detail: &str,
) -> RenameProposalRow {
    RenameProposalRow {
        evidence: RenameEvidence {
            source,
            detail: detail.into(),
        },
        ..proposal_row(row_id, source_path, destination_name)
    }
}

#[test]
fn accepted_preflight_requires_the_exact_allowed_subset() {
    let store = RenameProposalStore::default();
    store.stage(RenameProposal {
        proposal_id: "proposal".into(),
        rows: vec![proposal_row("row", "/x/a.png", "b.png")],
    });
    assert!(store.record_accepted_preflight(
        "proposal",
        AcceptedPreflight {
            allowed_row_ids: vec!["row".into()],
            allowed_destination_names: vec!["b.png".into()],
            fingerprints: vec![],
        },
    ));
    assert!(store.accepted_preflight("proposal", &["row".into()]).is_some());
    assert!(store.accepted_preflight("proposal", &["other".into()]).is_none());
}

// ── Revising one row's name, as the user typed it ─────────────────────────────

/// Stages one row and records the accepted preflight a Ready review would have left behind.
fn staged_with_accepted_preflight(rows: Vec<RenameProposalRow>) -> RenameProposalStore {
    let store = RenameProposalStore::default();
    let allowed_row_ids: Vec<String> = rows.iter().map(|row| row.row_id.clone()).collect();
    let allowed_destination_names: Vec<String> = rows.iter().map(|row| row.destination_name.clone()).collect();
    store.stage(RenameProposal {
        proposal_id: "proposal".into(),
        rows,
    });
    assert!(store.record_accepted_preflight(
        "proposal",
        AcceptedPreflight {
            allowed_row_ids,
            allowed_destination_names,
            fingerprints: vec![],
        },
    ));
    store
}

/// The user's name is the first destination name that crosses IPC, so the server validates it
/// exactly as it validates the model's. Nothing downstream looks at a name again: apply resolves
/// it from this stored row, never from the client.
#[test]
fn revising_a_row_validates_the_name_on_the_server() {
    let store = staged_with_accepted_preflight(vec![proposal_row("row", "/x/a.png", "b.png")]);

    for name in ["", "   ", ".", "..", "folder/name.png", "folder\\name.png"] {
        assert!(
            revise_staged_row(&store, "proposal", "row", name).is_err(),
            "{name:?} must be refused"
        );
    }
    assert!(revise_staged_row(&store, "proposal", "unknown-row", "fine.png").is_err());
    assert!(revise_staged_row(&store, "other-proposal", "row", "fine.png").is_err());

    let revised = revise_staged_row(&store, "proposal", "row", "Receipt 2026-07-20.png").expect("a valid filename");
    assert_eq!(revised.destination_name, "Receipt 2026-07-20.png");
    assert_eq!(
        store.get("proposal").expect("still staged").rows[0].destination_name,
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
    let store = staged_with_accepted_preflight(vec![proposal_row("row", "/x/a.png", "b.png")]);
    assert!(
        store.accepted_preflight("proposal", &["row".into()]).is_some(),
        "the review starts from a Ready preflight"
    );

    revise_staged_row(&store, "proposal", "row", "typed-by-hand.png").expect("a valid filename");

    assert!(
        store.accepted_preflight("proposal", &["row".into()]).is_none(),
        "an edited name must be preflighted again before it can reach the filesystem"
    );
    assert!(
        store.take_accepted_preflight("proposal", &["row".into()]).is_none(),
        "and apply must not be able to consume the stale acceptance either"
    );
}

/// Belt and braces for the same failure: the acceptance records the names it checked, so a
/// lookup whose names have moved on refuses even if some future path forgets to clear it.
/// Apply then falls back to a fresh authoritative preflight instead of trusting the old one.
#[test]
fn an_accepted_preflight_is_bound_to_the_names_it_checked() {
    let store = RenameProposalStore::default();
    store.stage(RenameProposal {
        proposal_id: "proposal".into(),
        rows: vec![proposal_row("row", "/x/a.png", "b.png")],
    });
    assert!(store.record_accepted_preflight(
        "proposal",
        AcceptedPreflight {
            allowed_row_ids: vec!["row".into()],
            allowed_destination_names: vec!["a-different-name.png".into()],
            fingerprints: vec![],
        },
    ));

    assert!(
        store.accepted_preflight("proposal", &["row".into()]).is_none(),
        "row ids alone don't say which names were checked"
    );
    assert!(store.take_accepted_preflight("proposal", &["row".into()]).is_none());
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
    let mut rows = vec![evidence_row(
        "row",
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
    let store = staged_with_accepted_preflight(rows);

    let revised = revise_staged_row(&store, "proposal", "row", "Klarna payment 2026-07-20.png").expect("valid");

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
        evidence_row(
            "quoted",
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021 total",
        ),
        evidence_row(
            "neighbour",
            "/shots/two.png",
            "Order summary.png",
            EvidenceSource::ImageText,
            "Order summary 1,299",
        ),
    ];
    check_row_evidence(&ledger, THREAD, &mut rows).expect("both quotes check out");
    let store = staged_with_accepted_preflight(rows.clone());

    // The prompt dropped that result after the fact, so the ledger no longer vouches for
    // either row: a re-staged plan would now be refused in full.
    ledger.revoke_call("call-1");
    let mut restaged = rows.clone();
    assert!(
        check_row_evidence(&ledger, THREAD, &mut restaged).is_err(),
        "the counterfactual: the whole-plan rule would refuse this plan now"
    );

    revise_staged_row(&store, "proposal", "quoted", "Invoice 4021 (Klarna).png").expect("the edit still lands");

    let stored = store.get("proposal").expect("the review survives");
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
    let mut rows = vec![evidence_row(
        "row",
        "/shots/one.png",
        "hand-typed.png",
        EvidenceSource::UserEdited,
        "the user typed this",
    )];

    let rejections = check_row_evidence(&ImageFactsLedger::default(), THREAD, &mut rows).expect_err("refused");

    assert_eq!(rejections.len(), 1);
    assert_eq!(rejections[0].problem, EvidenceProblem::SourceReservedForUser);
}
