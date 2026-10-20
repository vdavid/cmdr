//! Tests for `transfer_driver.rs`'s concurrent per-file progress callback
//! (`make_concurrent_per_file_progress`).
//!
//! Coverage: the concurrent copy path (`copy_volumes_with_progress`'s
//! window of spawned tasks) has no between-files boundary, so its per-file progress
//! callback is cancel-only — it breaks on cancel but ignores pause. This pins
//! that contract so any future change to gate the concurrent path for pause is a
//! deliberate decision. See transfer/DETAILS.md § "Pause and the concurrent copy
//! path".

use super::super::super::event_sinks::CollectorEventSink;
use super::super::super::types::WriteOperationType;
use super::test_support::make_state;
use crate::testing::TestOperations;
use cmdr_operation_log::types::OperationId;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_per_file_callback_is_cancel_only_not_pause_aware() {
    let engine = TestOperations::new();
    // The concurrent copy path (`copy_volumes_with_progress`, several at once)
    // has no between-files boundary, so v1 does NOT gate it for pause: its
    // per-file progress callback breaks on cancel but ignores pause. Pin that so
    // a future change to gate the concurrent path is a deliberate decision, not
    // an accident. See transfer/DETAILS.md § "Pause and the concurrent copy
    // path".
    use super::LeafProgressLedger;
    use std::sync::atomic::AtomicUsize;

    let op_id = OperationId::from("concurrent-pause-noop");
    let state = make_state(&engine);
    engine.register_state(op_id.as_str(), Arc::clone(&state));
    engine
        .status()
        .register(op_id.as_str(), WriteOperationType::Copy, vec![]);
    let sink: Arc<dyn super::super::super::event_sinks::OperationEventSink> = Arc::new(CollectorEventSink::new());

    let ledger = LeafProgressLedger::new(
        Arc::clone(&sink),
        Arc::clone(&state),
        op_id.to_string(),
        WriteOperationType::Copy,
        Arc::new(AtomicUsize::new(0)),
        1,
        100,
        Duration::from_millis(0),
    );
    let leaf = ledger
        .for_source(
            std::path::Path::new("/src/f"),
            Arc::new(Mutex::new(std::time::Instant::now())),
        )
        .begin_leaf(None);

    // Paused, not cancelled: the chunk callback must still Continue (pause is a
    // no-op on the concurrent per-file path in v1).
    state.pause_gate.pause();
    assert_eq!(
        leaf.on_chunk(10),
        std::ops::ControlFlow::Continue(()),
        "concurrent per-file callback must ignore pause (cancel-only in v1)"
    );

    // Cancelled: it must Break, exactly as before.
    engine.cancel_write_operation(&op_id, false);
    assert_eq!(
        leaf.on_chunk(20),
        std::ops::ControlFlow::Break(()),
        "concurrent per-file callback must still break on cancel"
    );
}
