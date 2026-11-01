//! The preflight engine: what blocks a row, what only warns, and how it reads a rename
//! graph. All of it runs over an in-memory proposal, so nothing here needs a store.

use super::super::check::{CheckTally, RenameChecks};
use super::super::preflight::{
    allowed_rows, initial_rows, mark_cycle_warnings, mark_duplicate_destinations, preflight_local, rename_warnings,
};
use super::super::{
    BulkRenameBlockReason, BulkRenamePreflightStatus, BulkRenameWarning, RenameProposal, RenameProposalRow,
};
use crate::tools::propose::evidence::{EvidenceSource, RenameEvidence};

#[test]
fn cycle_warnings_mark_only_closed_dependency_components() {
    let proposal = RenameProposal {
        proposal_id: "proposal".into(),
        revision: 0,
        volume_id: "root".into(),
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
        // `tar.gz` is one extension (`cmdr_fs::file_extension`), so this is a format change.
        ("archive.tar.gz", "renamed.gz"),
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
        // Neither side has an extension: a trailing dot isn't one, and a dot
        // followed by words is part of the name.
        ("trailing.", "trailing"),
        ("Smlouva v. 2 final", "Smlouva v. 3 final"),
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
        revision: 0,
        volume_id: "root".into(),
        rows: vec![proposal_row(
            "row",
            missing.to_str().expect("UTF-8 temp path"),
            "renamed.png",
        )],
    };

    let outcome = preflight_local(&proposal, &["row".into()], &CheckTally::default()).expect("nothing stopped it");

    assert_eq!(outcome.status, BulkRenamePreflightStatus::Blocked);
    assert_eq!(
        outcome.response.rows[0].reason,
        Some(BulkRenameBlockReason::SourceMissing)
    );
    assert!(outcome.fingerprints.is_empty());
}

/// A stopped check answers nothing about its rows, ❌ never a partial list that reads as "these
/// were fine": everything apply trusts comes from an outcome that saw every allowed row.
#[test]
fn a_check_stopped_before_its_first_row_answers_nothing() {
    let proposal = RenameProposal {
        proposal_id: "proposal".into(),
        revision: 0,
        volume_id: "root".into(),
        rows: vec![proposal_row("row", "/definitely/not/there.png", "renamed.png")],
    };
    let checks = RenameChecks::default();
    let running = checks.begin("proposal");
    assert!(checks.cancel("proposal"));

    assert!(preflight_local(&proposal, &["row".into()], &running.tally).is_none());
}

#[test]
fn duplicate_final_targets_block_every_row_in_the_group() {
    let first = proposal_row("first", "/ignored/a.png", "same.png");
    let second = proposal_row("second", "/ignored/b.png", "same.png");
    let mut statuses = initial_rows(
        &RenameProposal {
            proposal_id: "proposal".into(),
            revision: 0,
            volume_id: "root".into(),
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
