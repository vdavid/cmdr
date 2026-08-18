//! The tool boundary: the pane's effective scope, per-row validation, and the evidence
//! guardrail that refuses a WHOLE plan rather than staging part of one.

use super::super::plan::{
    ProposalRefusal, RenameInput, check_row_evidence, missing_local_child, refusal_content, scoped_files,
    validate_destination_name,
};
use super::{THREAD, draft_row};
use crate::agent::tools::propose::evidence::{EvidenceProblem, EvidenceSource, ImageFactsLedger};
use crate::mcp::pane_state::{PaneFileEntry, PaneState};

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
fn one_unbacked_content_claim_rejects_that_row_and_refuses_the_whole_plan() {
    // The incident's shape at the plan boundary: one row cites image text nobody delivered,
    // its neighbours are fine. Every offending row comes back named, and the caller refuses
    // the WHOLE plan rather than staging a partial one the user would read as complete.
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

/// The tool can't be called without evidence at all: an old-shaped plan is a param refusal,
/// not a plan with an empty justification.
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
