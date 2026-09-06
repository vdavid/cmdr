//! What a folder→file Overwrite owes the file it displaced, until the whole
//! operation commits.
//!
//! **Data-safety contract pinned here.** Replacing a file with a folder is not
//! over when the folder appears: `copy_single_item` creates the directory the
//! moment the first child needs a parent, and the subtree lands leaf by leaf
//! over the rest of the operation. So the user's file has to stay recoverable
//! for that whole stretch, and the directory that took its place has to be in
//! the ledger — otherwise a cancel-with-rollback (or a failure) leaves an empty
//! `X/` where an `X` file used to be, with nothing to put back and nothing to
//! remove.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::super::state::{ConflictResolutionResponse, OperationIntent, WriteOperationState};
use super::super::types::{
    ConflictInfo, ConflictResolution, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent,
    WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationPhase,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use super::copy::copy_files_with_progress_inner;
use crate::test_support::TestDir;

/// Answers the folder→file clash with Overwrite (the explicit consent a blanket
/// policy doesn't have), then asks for a rollback as soon as the first leaf has
/// landed — the shape of a person clicking Cancel and choosing "put it back"
/// one file into a folder that's replacing their file.
struct AnswerThenRollBack {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    asked: AtomicBool,
}

impl OperationEventSink for AnswerThenRollBack {
    fn emit_conflict(&self, e: WriteConflictEvent) {
        let clash = e.conflict_id;
        self.inner.emit_conflict(e);
        let _ = self.state.conflict_slot.answer(
            clash,
            ConflictResolutionResponse {
                resolution: ConflictResolution::Overwrite,
                apply_to_all: false,
            },
        );
    }
    fn emit_progress(&self, e: WriteProgressEvent) {
        // One COPIED file in, and only once. The phase matters: the scan emits
        // its own progress with a rising `files_done`, and flipping there would
        // cancel the operation before it ever reached the clash.
        if e.phase == WriteOperationPhase::Copying && e.files_done >= 1 && !self.asked.swap(true, Ordering::SeqCst) {
            self.state
                .intent
                .store(OperationIntent::RollingBack as u8, Ordering::SeqCst);
        }
        self.inner.emit_progress(e);
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_settled(&self, e: WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: DryRunResult) {}
}

#[test]
fn a_rollback_mid_folder_over_file_overwrite_puts_the_file_back() {
    let temp_dir = TestDir::new(&format!("folder_over_file_rollback_{}", uuid::Uuid::new_v4()));
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    // Two children, so the rollback is asked for with the subtree half-landed.
    fs::write(src_root.join("thing/a.txt"), "source-a").unwrap();
    fs::write(src_root.join("thing/b.txt"), "source-b").unwrap();
    fs::write(dst_root.join("thing"), "the user's only copy").unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = AnswerThenRollBack {
        inner: CollectorEventSink::new(),
        state: Arc::clone(&state),
        asked: AtomicBool::new(false),
    };
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Stop,
        ..Default::default()
    };

    let result = copy_files_with_progress_inner(
        &events,
        "op-folder-over-file-rollback",
        &state,
        &[src_root.join("thing")],
        &dst_root,
        &config,
    );
    assert!(
        matches!(
            result,
            Err(crate::file_system::write_operations::types::WriteOperationError::Cancelled { .. })
        ),
        "the rollback request should surface as Cancelled, got {result:?}"
    );

    // The whole point: the file is back, byte for byte.
    let dest = dst_root.join("thing");
    let meta = fs::symlink_metadata(&dest).expect("the displaced file must be back at its own name");
    assert!(
        meta.is_file(),
        "a rolled-back folder→file Overwrite must leave a FILE at the name, not the empty folder that replaced it"
    );
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        "the user's only copy",
        "the displaced file must come back with its own bytes"
    );

    // And nothing of the reversed copy is left lying around.
    let mut names: Vec<String> = fs::read_dir(&dst_root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["thing".to_string()],
        "no aside or half-landed subtree may survive"
    );
}

/// The commit side of the same contract: when the copy runs to the end, the
/// displaced file is gone and the folder that replaced it holds its whole
/// subtree. Without this the fix above would just be "never overwrite".
#[test]
fn a_completed_folder_over_file_overwrite_replaces_the_file_and_drops_the_aside() {
    let temp_dir = TestDir::new(&format!("folder_over_file_commit_{}", uuid::Uuid::new_v4()));
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(src_root.join("thing/nested")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    fs::write(src_root.join("thing/a.txt"), "source-a").unwrap();
    fs::write(src_root.join("thing/nested/deep.txt"), "source-deep").unwrap();
    fs::write(dst_root.join("thing"), "the file being replaced").unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = super::conflict_responder_test_support::ConflictResponderSink::new(
        &state,
        ConflictResolution::Overwrite,
        false,
    );
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Stop,
        ..Default::default()
    };

    copy_files_with_progress_inner(
        &events,
        "op-folder-over-file-commit",
        &state,
        &[src_root.join("thing")],
        &dst_root,
        &config,
    )
    .expect("the copy should succeed");

    let dest = dst_root.join("thing");
    assert!(
        fs::symlink_metadata(&dest).unwrap().is_dir(),
        "the folder replaced the file"
    );
    assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "source-a");
    assert_eq!(fs::read_to_string(dest.join("nested/deep.txt")).unwrap(), "source-deep");

    let mut names: Vec<String> = fs::read_dir(&dst_root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["thing".to_string()],
        "the aside must be gone once the operation commits"
    );
}
