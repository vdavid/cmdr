//! Rename-proposal tests: the plan boundary (scope, validation, the evidence guardrail),
//! the store's staging lifetime, and the preflight engine's blocks and warnings.

use super::plan::{
    ProposalRefusal, RenameInput, collect_evidence_rejections, missing_local_child, refusal_content, scoped_files,
    validate_destination_name,
};
use super::preflight::{
    allowed_rows, initial_rows, mark_cycle_warnings, mark_duplicate_destinations, preflight_local, rename_warnings,
};
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
    let rows = vec![
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

    let rejections = collect_evidence_rejections(&ledger, THREAD, &rows);

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
        rows: vec![],
    });
    assert!(store.record_accepted_preflight(
        "proposal",
        AcceptedPreflight {
            allowed_row_ids: vec!["row".into()],
            fingerprints: vec![],
        },
    ));
    assert!(store.accepted_preflight("proposal", &["row".into()]).is_some());
    assert!(store.accepted_preflight("proposal", &["other".into()]).is_none());
}
