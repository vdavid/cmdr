//! The terminal side-effect matrix: per outcome, which app side effects fire and
//! in what order relative to the emit (`terminal_effects.rs`), which the sink
//! adapter (`sinks.rs`) runs, and the retained failure those effects land on an
//! engine of the test's own. The engine's half of that contract, on its bare
//! manager, is `manager/tests/failures.rs`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::analytics::{CompletionCategory, completion_category};
use super::terminal_effects::{self, AppTerminalEffects, TerminalEffects};
use crate::mcp::terminal_ops::{self, TerminalStatus};
use crate::test_support::wait_until_async;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operations::host::testing::{FakeVolumes, RecordingEngineEvents};
use cmdr_operations::testing::{CancelRollback, QueuedOperationFixture};
use cmdr_operations::{
    LifecycleStatus, OperationSnapshot, Operations, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationError, WriteOperationType, WriteSettledEvent,
};

/// One thing a terminal event did, as the recorder saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fired {
    Analytics(Option<CompletionCategory>),
    Terminal(TerminalStatus),
    Warned,
    Retained,
    SpaceHint,
    Emitted,
}

/// Records every effect in call order; the emit closure appends `Emitted`.
#[derive(Default)]
struct Recorder(Mutex<Vec<Fired>>);

impl Recorder {
    fn push(&self, fired: Fired) {
        self.0.lock_ignore_poison().push(fired);
    }

    fn fired(&self) -> Vec<Fired> {
        self.0.lock_ignore_poison().clone()
    }
}

impl TerminalEffects for Recorder {
    fn completion_analytics(&self, event: &WriteCompleteEvent) {
        self.push(Fired::Analytics(completion_category(event.operation_type)));
    }
    fn record_terminal(&self, _operation_id: &str, _operation_type: WriteOperationType, status: TerminalStatus) {
        self.push(Fired::Terminal(status));
    }
    fn warn_failure(&self, _event: &WriteErrorEvent) {
        self.push(Fired::Warned);
    }
    fn retain_failure(&self, _event: &WriteErrorEvent) {
        self.push(Fired::Retained);
    }
    fn expect_space_change(&self) {
        self.push(Fired::SpaceHint);
    }
}

const PROGRESSED: [WriteOperationType; 6] = [
    WriteOperationType::Copy,
    WriteOperationType::Move,
    WriteOperationType::Delete,
    WriteOperationType::Trash,
    WriteOperationType::ArchiveEdit,
    WriteOperationType::Compress,
];

fn complete_event(operation_id: &str, operation_type: WriteOperationType) -> WriteCompleteEvent {
    WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        files_processed: 3,
        files_skipped: 0,
        bytes_processed: 30,
        appeared_during_move: None,
        top_level_skipped: None,
        refused: None,
        metadata_left_behind: None,
        durability_unconfirmed: None,
        locked_kept: None,
    }
}

fn cancelled_event(operation_id: &str) -> WriteCancelledEvent {
    WriteCancelledEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Copy,
        files_processed: 1,
        rollback: CancelRollback::none(),
    }
}

fn error_event(operation_id: &str) -> WriteErrorEvent {
    WriteErrorEvent::new(
        operation_id.to_string(),
        WriteOperationType::Copy,
        WriteOperationError::IoError {
            path: "/Users/me/a.txt".to_string(),
            message: "disk went away".into(),
        },
    )
}

#[test]
fn complete_fires_its_category_then_the_ring_then_the_emit_for_every_progressed_op() {
    let expected_category = [
        CompletionCategory::FileTransfer,
        CompletionCategory::FileTransfer,
        CompletionCategory::Delete,
        CompletionCategory::Delete,
        CompletionCategory::ArchiveEdit,
        CompletionCategory::ArchiveEdit,
    ];
    for (operation_type, category) in PROGRESSED.into_iter().zip(expected_category) {
        let fx = Recorder::default();
        terminal_effects::complete(&fx, complete_event("op-complete", operation_type), |_| {
            fx.push(Fired::Emitted);
        });
        assert_eq!(
            fx.fired(),
            vec![
                Fired::Analytics(Some(category)),
                Fired::Terminal(TerminalStatus::Completed),
                Fired::Emitted
            ],
            "write-complete for {operation_type:?}"
        );
    }
}

#[test]
fn instant_ops_have_no_completion_category() {
    for operation_type in [
        WriteOperationType::Rename,
        WriteOperationType::CreateFolder,
        WriteOperationType::CreateFile,
    ] {
        assert_eq!(completion_category(operation_type), None, "{operation_type:?}");
    }
}

#[test]
fn cancel_fires_the_ring_then_the_emit_and_nothing_else() {
    let fx = Recorder::default();
    terminal_effects::cancelled(&fx, cancelled_event("op-cancel"), |_| fx.push(Fired::Emitted));
    assert_eq!(
        fx.fired(),
        vec![Fired::Terminal(TerminalStatus::Cancelled), Fired::Emitted]
    );
}

#[test]
fn error_fires_the_ring_the_warning_and_the_retained_failure_before_the_emit() {
    let fx = Recorder::default();
    terminal_effects::error(&fx, error_event("op-error"), |_| fx.push(Fired::Emitted));
    assert_eq!(
        fx.fired(),
        vec![
            Fired::Terminal(TerminalStatus::Failed),
            Fired::Warned,
            Fired::Retained,
            Fired::Emitted
        ]
    );
}

#[test]
fn settle_fires_the_space_hint_then_the_emit() {
    let fx = Recorder::default();
    let event = WriteSettledEvent {
        operation_id: "op-settle".to_string(),
        operation_type: WriteOperationType::Delete,
        volume_id: None,
    };
    terminal_effects::settled(&fx, event, |_| fx.push(Fired::Emitted));
    assert_eq!(fx.fired(), vec![Fired::SpaceHint, Fired::Emitted]);
}

/// The production effects land the MCP ring entry before the frontend hears,
/// which is what lets `await operation_complete` answer the instant the event
/// arrives. Unique ids: the ring is process-global.
#[test]
fn the_production_ring_already_holds_the_outcome_when_the_emit_runs() {
    let fx = AppTerminalEffects {
        operations: Some(crate::operations_host::app_engine()),
    };
    let complete_id = format!("terminal-ring-complete-{}", uuid::Uuid::new_v4());
    let mut seen = None;
    terminal_effects::complete(&fx, complete_event(&complete_id, WriteOperationType::Copy), |event| {
        seen = terminal_ops::lookup(&event.operation_id).map(|op| op.status)
    });
    assert_eq!(seen, Some(TerminalStatus::Completed));

    let cancel_id = format!("terminal-ring-cancel-{}", uuid::Uuid::new_v4());
    let mut seen = None;
    terminal_effects::cancelled(&fx, cancelled_event(&cancel_id), |event| {
        seen = terminal_ops::lookup(&event.operation_id).map(|op| op.status);
    });
    assert_eq!(seen, Some(TerminalStatus::Cancelled));
}

/// The rows of `engine`'s snapshot carrying `operation_id`.
fn rows_for(engine: &Operations, operation_id: &str) -> Vec<OperationSnapshot> {
    engine
        .list_operations()
        .into_iter()
        .filter(|row| row.operation_id == operation_id)
        .collect()
}

/// A failure reached the way production reaches it, through the sink's
/// `write-error` effects fired from inside the live op: retained, hidden behind
/// the live record without a broadcast, and surfacing once the record settles.
///
/// On an engine of the test's own: the broadcast assertion is an equality on
/// every `operations-changed` that engine sent, so each one is this test's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_through_the_production_sink_effects_is_retained_and_surfaces_after_its_record() {
    let events = Arc::new(RecordingEngineEvents::default());
    let engine = Operations::builder(Arc::new(FakeVolumes::default()), events.clone()).build();
    let parked = QueuedOperationFixture::park_on(&engine, "fail-through-sink");
    let op = parked.holder_id().to_string();
    let broadcasts = || events.operations_changed.lock_ignore_poison().len();

    let broadcasts_before = broadcasts();
    let mut rows_at_emit = Vec::new();
    terminal_effects::error(
        &AppTerminalEffects {
            operations: Some(&engine),
        },
        WriteErrorEvent::new(
            op.clone(),
            WriteOperationType::Copy,
            WriteOperationError::IoError {
                path: "/Users/me/a.raw".to_string(),
                message: "disk went away".into(),
            },
        ),
        |_| rows_at_emit = rows_for(&engine, &op),
    );
    assert_eq!(rows_at_emit.len(), 1, "one row while the record is live");
    assert_eq!(rows_at_emit[0].status, LifecycleStatus::Running);
    assert!(
        rows_at_emit[0].error.is_none(),
        "the failure stays hidden behind the live record"
    );
    assert_eq!(
        broadcasts(),
        broadcasts_before,
        "nothing broadcasts while the record is live"
    );

    // Releasing the holder settles it, which removes its record.
    drop(parked);
    wait_until_async(
        Duration::from_secs(5),
        "the failed op to settle and surface its retained row",
        || {
            rows_for(&engine, &op)
                .iter()
                .any(|row| row.status == LifecycleStatus::Failed)
        },
    )
    .await;
    // A live record would still hide the failure behind a `Running` row, so one
    // `Failed` row is the record gone and the retained failure in its place.
    let retained = rows_for(&engine, &op);
    assert_eq!(retained.len(), 1, "the record is gone");
    assert!(matches!(retained[0].error, Some(WriteOperationError::IoError { .. })));
}
