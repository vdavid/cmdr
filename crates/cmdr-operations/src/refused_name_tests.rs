//! `refused_name.rs`: the prompt for a name the destination refuses rides the
//! clash machinery, and its answers mean exactly Skip, Skip all, or Stop.

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use cmdr_fs::ignore_poison::IgnorePoison;
use std::path::PathBuf;
use std::time::Duration;

/// Answers every prompt from inside its emit, the way a surface may: the slot
/// is armed before the prompt goes out.
struct AnsweringSink {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    resolution: ConflictResolution,
    apply_to_all: bool,
}

impl OperationEventSink for AnsweringSink {
    fn emit_settled(&self, e: WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_conflict(&self, e: crate::ArmedConflict) {
        let asked = e.conflict_id;
        self.inner.emit_conflict(e);
        let _ = self.state.conflict_slot.answer(
            asked,
            ConflictResolutionResponse {
                resolution: self.resolution,
                apply_to_all: self.apply_to_all,
            },
        );
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_progress(&self, e: WriteProgressEvent) {
        self.inner.emit_progress(e);
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
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: DryRunResult) {}
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(TestOperations::new().new_operation_state(Duration::from_millis(0)))
}

fn sink(state: &Arc<WriteOperationState>, resolution: ConflictResolution, apply_to_all: bool) -> AnsweringSink {
    AnsweringSink {
        inner: CollectorEventSink::new(),
        state: Arc::clone(state),
        resolution,
        apply_to_all,
    }
}

struct Paths {
    source: PathBuf,
    dest: PathBuf,
}

fn paths(name: &str) -> Paths {
    Paths {
        source: PathBuf::from(format!("/Users/me/Music/{name}")),
        dest: PathBuf::from(format!("/Volumes/testshare/music/{name}")),
    }
}

fn item<'a>(paths: &'a Paths) -> RefusedItem<'a> {
    RefusedItem {
        operation_id: "op-refused",
        source_path: &paths.source,
        dest_path: &paths.dest,
        source_size: Some(4_096),
        source_is_directory: false,
        reason: RefusedNameReason::TooLong,
    }
}

#[test]
fn only_a_refused_name_is_asked_about() {
    let too_long = WriteOperationError::NameTooLong { path: "/x".into() };
    let not_allowed = WriteOperationError::InvalidName {
        path: "/x".into(),
        message: "CON".into(),
    };
    let full = WriteOperationError::DestinationFull { path: "/x".into() };

    assert_eq!(refusal_reason(&too_long), Some(RefusedNameReason::TooLong));
    assert_eq!(refusal_reason(&not_allowed), Some(RefusedNameReason::NotAllowed));
    assert_eq!(refusal_reason(&full), None);
}

/// The prompt goes out as a `write-conflict` that says what was refused, and
/// an answered one is announced as over, like a clash.
#[test]
fn a_skip_answers_one_prompt_that_names_the_refusal() {
    let state = state();
    let events = sink(&state, ConflictResolution::Skip, false);
    let paths = paths("long.flac");

    assert_eq!(ask_blocking(&state, &events, &item(&paths)), RefusedNameAnswer::Skip);

    let prompts = events.inner.conflicts.lock_ignore_poison();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].refused_name, Some(RefusedNameReason::TooLong));
    assert_eq!(prompts[0].destination_path, paths.dest.display().to_string());
    assert_eq!(prompts[0].destination_size, None, "nothing is at the destination");
    let resolved = events.inner.conflicts_resolved.lock_ignore_poison();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].conflict_id, prompts[0].conflict_id);
}

/// "Skip all" latches for the rest of the operation: the next refused name
/// skips without a second prompt.
#[test]
fn skip_all_stops_asking() {
    let state = state();
    let events = sink(&state, ConflictResolution::Skip, true);
    let first = paths("first.flac");
    let second = paths("second.flac");

    assert_eq!(ask_blocking(&state, &events, &item(&first)), RefusedNameAnswer::Skip);
    assert_eq!(ask_blocking(&state, &events, &item(&second)), RefusedNameAnswer::Skip);

    assert_eq!(events.inner.conflicts.lock_ignore_poison().len(), 1);
}

/// A plain Skip latches nothing: the next refused name asks again.
#[test]
fn a_plain_skip_asks_again_next_time() {
    let state = state();
    let events = sink(&state, ConflictResolution::Skip, false);
    let first = paths("first.flac");
    let second = paths("second.flac");

    ask_blocking(&state, &events, &item(&first));
    ask_blocking(&state, &events, &item(&second));

    assert_eq!(events.inner.conflicts.lock_ignore_poison().len(), 2);
}

/// No other answer can store the name, so it stops the operation rather than
/// reading consent into it; even "Overwrite all" latches nothing here.
#[test]
fn any_other_answer_stops() {
    let state = state();
    let events = sink(&state, ConflictResolution::Overwrite, true);
    let paths = paths("long.flac");

    assert_eq!(ask_blocking(&state, &events, &item(&paths)), RefusedNameAnswer::Stop);
    assert!(!state.refused_names.skips_all());
}

/// A stopped operation asks nothing: a prompt emitted then would never be
/// answered.
#[test]
fn a_stopped_operation_asks_nothing() {
    let state = state();
    let events = sink(&state, ConflictResolution::Skip, false);
    state.stop_work_after_failure();
    let paths = paths("long.flac");

    assert_eq!(
        ask_blocking(&state, &events, &item(&paths)),
        RefusedNameAnswer::Cancelled
    );
    assert!(events.inner.conflicts.lock_ignore_poison().is_empty());
}

/// A cancel while the prompt is up abandons the slot, which the asker reads as
/// the cancel it is.
#[tokio::test]
async fn a_cancel_while_asking_is_a_cancel() {
    let state = state();
    let events = Arc::new(CollectorEventSink::new());
    let task = {
        let state = Arc::clone(&state);
        let events = Arc::clone(&events);
        tokio::spawn(async move {
            let paths = paths("long.flac");
            ask_async(&state, &*events, &item(&paths)).await
        })
    };
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the refused-name prompt", || {
        !events.conflicts.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(state.conflict_slot.is_awaiting());

    state.conflict_slot.abandon();

    assert_eq!(task.await.unwrap(), RefusedNameAnswer::Cancelled);
}
