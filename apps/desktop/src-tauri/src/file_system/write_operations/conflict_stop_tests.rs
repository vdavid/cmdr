//! Part of `conflict.rs`, split out as a `#[path]` child so the module itself
//! stays readable. `super::` here is `conflict`, exactly as when these lived
//! inline.
//!
//! What the local-FS Stop branch of `resolve_conflict` must do when it parks
//! on a person: arm the slot before the prompt goes out, and say out loud
//! that it has parked.
//!
//! The ordering pin: the oneshot sender must be armed in
//! `state.conflict_slot` BEFORE the `write-conflict` event is
//! emitted, so a responder that observes the event and answers it
//! synchronously (the FE's `resolve_write_conflict`, modeled here by a sink
//! that answers inside `emit_conflict`) finds the sender already present.

use super::*;
use crate::file_system::write_operations::state::{ConflictResolutionResponse, WriteOperationState};
use crate::file_system::write_operations::types::{
    CollectorEventSink, ConflictInfo, ConflictResolution, DryRunResult, OperationEventSink, ScanProgressEvent,
    TransferWaitReason, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteErrorEvent,
    WriteOperationConfig, WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use crate::ignore_poison::IgnorePoison;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// A sink that answers a Stop-mode `write-conflict` synchronously the moment
/// it observes it, through the same conflict slot the FE's
/// `resolve_write_conflict` answers through, but driven from inside
/// `emit_conflict`. Forwards every event to an inner `CollectorEventSink`
/// for inspection.
///
/// This only resolves the conflict if the slot is ALREADY armed when the
/// event arrives. If the production code emitted before arming, the answer
/// would land on nothing, and `resolve_conflict`'s
/// `rx.blocking_recv()` would deadlock — turning the ordering bug into a hang
/// instead of a wrong value. The store-before-emit fix is what keeps this
/// test from hanging.
struct AnswerOnConflictSink {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    resolution: ConflictResolution,
}

impl OperationEventSink for AnswerOnConflictSink {
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_conflict(&self, e: WriteConflictEvent) {
        // Answer the clash this event names, exactly as a surface would.
        let clash = e.conflict_id;
        self.inner.emit_conflict(e);
        let _ = self.state.conflict_slot.answer(
            clash,
            ConflictResolutionResponse {
                resolution: self.resolution,
                apply_to_all: false,
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

/// THE PIN: with the sender stored before the emit, a responder answering
/// synchronously inside `emit_conflict` resolves the local-FS Stop clash and
/// `resolve_conflict` returns the scripted resolution. Run WITHOUT a Tokio
/// runtime so the function's `rx.blocking_recv()` is legal; the answer is
/// already buffered in the oneshot by the time `blocking_recv` runs, so it
/// returns immediately. Against the pre-fix emit-then-store ordering the take
/// inside `emit_conflict` would miss and this test would deadlock.
#[test]
fn stop_clash_answered_from_within_emit_resolves_without_hanging() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    fs::write(&src, b"SRC").unwrap();
    fs::write(&dst, b"DEST").unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = AnswerOnConflictSink {
        inner: CollectorEventSink::new(),
        state: Arc::clone(&state),
        resolution: ConflictResolution::Skip,
    };
    let config = WriteOperationConfig::default(); // Stop, overwrite=false
    let mut latch = ApplyToAll::default();

    let result = resolve_conflict(&src, &dst, &config, &events, "op-local-stop-pin", &state, &mut latch);

    // Skip resolves to "skip this file" (None), proving the responder's
    // answer reached the op — which is only possible if the sender was
    // stored before the event was emitted.
    assert!(
        matches!(result, Ok(None)),
        "Skip answer should resolve the clash to None, got {result:?}"
    );
    // Exactly one conflict event was recorded, and it's a file-vs-file clash.
    let conflicts = events.inner.conflicts.lock_ignore_poison();
    assert_eq!(conflicts.len(), 1, "exactly one Stop prompt for the file clash");
    assert!(
        !conflicts[0].source_is_directory && !conflicts[0].destination_is_directory,
        "the clash is file-vs-file"
    );
}

/// A clash that gets answered is announced as over, naming itself.
///
/// The prompt went out to every webview, and only the surface whose own call
/// returned learns what became of it. Without this, the queue window's copy
/// of the prompt — or the main window's, after an AGENT answered over MCP —
/// keeps asking a question with no answer left to give, and blocks anything
/// new from starting behind it.
#[test]
fn an_answered_clash_is_announced_as_over_by_id() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src.txt");
    let dst = dir.path().join("dst.txt");
    fs::write(&src, b"SRC").unwrap();
    fs::write(&dst, b"DEST").unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = AnswerOnConflictSink {
        inner: CollectorEventSink::new(),
        state: Arc::clone(&state),
        resolution: ConflictResolution::Skip,
    };
    let config = WriteOperationConfig::default();
    let mut latch = ApplyToAll::default();

    let _ = resolve_conflict(&src, &dst, &config, &events, "op-local-resolved", &state, &mut latch);

    let raised = events.inner.conflicts.lock_ignore_poison();
    let resolved = events.inner.conflicts_resolved.lock_ignore_poison();
    assert_eq!(resolved.len(), 1, "the answered clash is announced exactly once");
    assert_eq!(
        resolved[0].conflict_id, raised[0].conflict_id,
        "it names the clash that was answered, ❌ never 'whatever is on screen'"
    );
    assert_eq!(resolved[0].operation_id, "op-local-resolved");
}

/// A local copy keeps no in-flight table, so nothing speaks for it while it
/// stands still: it emits no progress at all with a prompt up, and the tick
/// it emitted just before the clash says it was moving at whatever it was
/// doing then. Every window would keep that speed on screen for the whole
/// answer. So the park announces itself, and so does its end.
#[test]
fn a_local_clash_announces_the_wait_and_its_end() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("f001");
    let dst = dir.path().join("dst-f001");
    fs::write(&src, b"SRC").unwrap();
    fs::write(&dst, b"DEST").unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    let events = AnswerOnConflictSink {
        inner: CollectorEventSink::new(),
        state: Arc::clone(&state),
        resolution: ConflictResolution::Skip,
    };

    // The copy was moving, and said so, right up to the clash.
    state.emit_progress_via_sink(
        &events,
        WriteProgressEvent::new(
            "op-local-park".to_owned(),
            WriteOperationType::Copy,
            WriteOperationPhase::Copying,
            Some("f000".to_owned()),
            3,
            40,
            300,
            4_000,
        ),
    );

    let config = WriteOperationConfig::default(); // Stop, overwrite=false
    let mut latch = ApplyToAll::default();
    let result = resolve_conflict(&src, &dst, &config, &events, "op-local-park", &state, &mut latch);
    assert!(matches!(result, Ok(None)), "the scripted Skip resolves the clash");

    let progress = events.inner.progress.lock_ignore_poison();
    assert_eq!(progress.len(), 3, "the copy's own tick, then one per edge of the wait");
    assert_eq!(progress[0].activity, None, "nothing was waiting on anybody yet");

    let parked = progress[1].activity.expect("the park has to reach the windows");
    assert_eq!(parked.waiting_on, TransferWaitReason::Conflict);
    assert_eq!(
        (progress[1].files_done, progress[1].bytes_done),
        (3, 300),
        "the counters are the ones from before the clash: nothing moved, and saying otherwise would be a lie",
    );

    assert_eq!(
        progress[2].activity, None,
        "the answer is in, so the copy is nobody's to wait on any more",
    );
}
