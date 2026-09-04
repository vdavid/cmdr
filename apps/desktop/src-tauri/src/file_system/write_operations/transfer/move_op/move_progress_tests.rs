//! What a local move REPORTS while it runs: the phases it announces and the
//! counts under them.
//!
//! Split out of `move_op_tests.rs`, which owns what a move DOES to the files.
//! The subjects are separate enough that neither suite is read for the other's
//! reason, and the sibling `volume/` engine draws the same line
//! (`volume/move_progress_tests.rs`).
//!
//! Both engines report, and both are covered here: `move_with_staging` for a
//! move between filesystems (scan, copy, flush, then the source sweep) and
//! `move_with_rename` for one within a filesystem (which only flushes the
//! directories the renames touched).

use super::cross_fs::move_with_staging;
use super::test_support::{make_state, run_same_fs_move};
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationPhase};

/// Cross-FS local move of a single file must emit at least one `Copying`-
/// phase progress event with `files_done == N`. `move_with_staging` doesn't
/// use the transfer driver — it has its own copy loop calling
/// `copy_single_item` per file. The per-file milestone has to live inside
/// `copy_single_item` so both this loop and the driver-driven loop see it.
///
/// Uses `progress_interval_ms: 200` (production default) to keep the
/// throttle window active. Pre-fix the test reliably sees zero Copying
/// events with `files_done = 1` (the chunked progress callback absorbs the
/// throttle, the milestone is missing); post-fix `copy_single_item` fires
/// the milestone unconditionally so the assertion holds.
#[test]
fn cross_fs_local_move_single_file_reaches_files_done_n() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 1_048_576]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-files-n",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
        0,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Move semantics: source gone, dest has the file.
    assert!(!src_file.exists(), "source should be removed after move");
    let dst_file = dst_dir.join("file.bin");
    assert!(dst_file.exists(), "destination should hold the moved file");

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    let saw_files_done_n = copying.iter().any(|p| p.files_done == 1);
    assert!(
        saw_files_done_n,
        "cross-FS local move: expected at least one Copying event with files_done = 1, got {:?}",
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );
    // The milestone event accounts for the full file.
    let milestone = copying
        .iter()
        .find(|p| p.files_done == 1)
        .expect("at least one Copying event with files_done = 1");
    assert_eq!(milestone.bytes_done, 1_048_576);

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 1);
    assert_eq!(complete[0].bytes_processed, 1_048_576);
}

/// A cross-FS local move must emit a `Flushing`-phase progress event before
/// `write-complete`. The staging copy lands real bytes that must be durable
/// before we delete the sources — on a move, a non-durable "complete" is
/// data loss (gone from source, not yet on disk at dest). The Flushing event
/// is the user-visible "Writing the last piece…" state and the observable
/// proxy for the end-of-op `fdatasync` over the moved destinations.
#[test]
fn cross_fs_local_move_emits_flushing_phase_before_complete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-flushing",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
        0,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert!(!src_file.exists(), "source should be removed after move");
    let dst_file = dst_dir.join("file.bin");
    assert!(dst_file.exists(), "destination should hold the moved file");

    let progress = events.progress.lock().unwrap();
    let saw_flushing = progress.iter().any(|p| p.phase == WriteOperationPhase::Flushing);
    assert!(
        saw_flushing,
        "cross-FS move: expected a Flushing-phase progress event, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "exactly one write-complete");
}

/// Phase 4 — deleting the originals, once the destinations are durable — must
/// report itself, because it is real, unbounded work: one `remove_file` or
/// `remove_dir_all` per top-level source, over however large a tree.
///
/// While it ran silently the last progress the frontend had was the copy phase's
/// `files_done == files_total`, so the dialog sat at "100%" (and, if the user
/// pressed Pause, at "Paused" over a full bar) with the whole source sweep still
/// ahead. Its own `Deleting` phase gives the readout a denominator that means
/// something and gives the dialog a phase to name.
#[test]
fn cross_fs_local_move_reports_the_source_deletion_instead_of_sitting_at_full() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // Three top-level sources, so the cleanup bar has somewhere to travel.
    let sources: Vec<_> = ["a.bin", "b.bin", "c.bin"]
        .iter()
        .map(|name| {
            let path = src_dir.join(name);
            fs::write(&path, vec![0u8; 4096]).unwrap();
            path
        })
        .collect();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = move_with_staging(
        &*events,
        "op-cross-fs-move-cleanup",
        &state,
        &sources,
        &dst_dir,
        &config,
        0,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let deleting: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Deleting)
        .collect();
    assert!(
        !deleting.is_empty(),
        "cross-FS move: expected Deleting-phase progress for the source sweep, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );

    // The denominator is the top-level sources it iterates, and the sweep opens
    // at zero rather than inheriting the copy's full bar.
    assert!(
        deleting.iter().all(|p| p.files_total == sources.len()),
        "every cleanup tick counts against the sources it will remove, got {:?}",
        deleting
            .iter()
            .map(|p| (p.files_done, p.files_total))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        deleting.first().map(|p| p.files_done),
        Some(0),
        "the sweep opens at zero"
    );
    assert_eq!(
        deleting.last().map(|p| p.files_done),
        Some(sources.len()),
        "and closes having accounted for every source",
    );

    // No bytes move in this phase, so the readout drops its size bar rather than
    // showing one frozen at whatever the copy left.
    assert!(
        deleting.iter().all(|p| p.bytes_total == 0 && p.bytes_done == 0),
        "the cleanup sweep transfers no bytes",
    );

    // Ordering: the flush that makes the destinations durable comes first.
    let flushing_at = progress
        .iter()
        .position(|p| p.phase == WriteOperationPhase::Flushing)
        .expect("a Flushing-phase event");
    let deleting_at = progress
        .iter()
        .position(|p| p.phase == WriteOperationPhase::Deleting)
        .expect("a Deleting-phase event");
    assert!(
        flushing_at < deleting_at,
        "the originals go only after the destinations are durable",
    );
}

/// A same-FS move announces its closing flush too, so the FE's state machine
/// shows "Writing the last piece…" for both move kinds. What it flushes is the
/// directories the renames touched, not the moved files (`touched_directories`),
/// and this event is the observable proxy for that pass.
#[test]
fn same_fs_local_move_emits_flushing_phase_before_complete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = run_same_fs_move(
        std::slice::from_ref(&src_file),
        &dst_dir,
        ConflictResolution::Stop,
        "op-same-fs-move-flushing",
    )
    .expect("the move should land");

    assert!(!src_file.exists(), "the rename took the source with it");
    assert!(dst_dir.join("file.bin").exists(), "and the destination holds it");

    let progress = events.progress.lock().unwrap();
    assert!(
        progress.iter().any(|p| p.phase == WriteOperationPhase::Flushing),
        "same-FS move: expected a Flushing-phase progress event, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );
    assert_eq!(events.complete.lock().unwrap().len(), 1, "exactly one write-complete");
}
