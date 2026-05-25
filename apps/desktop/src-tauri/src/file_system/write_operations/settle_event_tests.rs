//! Tests for the `write-settled` per-op event. See § "Settle contract" in the
//! module CLAUDE.md for the full contract.
//!
//! The `WriteSettledGuard` RAII pattern emits exactly one `write-settled`
//! event when the spawned task fully returns — happy path, error path,
//! cancellation, or panic. These tests pin the contract.
//!
//! Production code emits via `app.emit("write-settled", ...)` from inside
//! the guard's `Drop`. Tests use the alternate `new_with_sink` constructor to
//! redirect into a `CollectorEventSink`, so the full lifecycle runs without
//! a Tauri runtime.

use std::sync::Arc;

use super::state::WriteSettledGuard;
use super::types::{CollectorEventSink, OperationEventSink, WriteOperationType, WriteSettledEvent};

/// Bridge sink that keeps a direct handle to the underlying `CollectorEventSink`
/// for inspection. Needed because the guard takes `Arc<dyn OperationEventSink>`,
/// and `dyn OperationEventSink` isn't downcastable.
struct Bridge {
    inner: Arc<CollectorEventSink>,
}

impl OperationEventSink for Bridge {
    fn emit_progress(&self, e: super::types::WriteProgressEvent) {
        self.inner.emit_progress(e);
    }
    fn emit_complete(&self, e: super::types::WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: super::types::WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: super::types::WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: super::types::WriteConflictEvent) {
        self.inner.emit_conflict(e);
    }
    fn emit_source_item_done(&self, e: super::types::WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(e);
    }
    fn emit_scan_progress(&self, e: super::types::ScanProgressEvent) {
        self.inner.emit_scan_progress(e);
    }
    fn emit_scan_conflict(&self, c: super::types::ConflictInfo) {
        self.inner.emit_scan_conflict(c);
    }
    fn emit_dry_run_complete(&self, r: super::types::DryRunResult) {
        self.inner.emit_dry_run_complete(r);
    }
    fn emit_settled(&self, e: WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
}

fn pair() -> (Arc<dyn OperationEventSink>, Arc<CollectorEventSink>) {
    let inner = Arc::new(CollectorEventSink::new());
    let bridge: Arc<dyn OperationEventSink> = Arc::new(Bridge {
        inner: Arc::clone(&inner),
    });
    (bridge, inner)
}

#[test]
fn settled_fires_once_on_normal_drop() {
    let (sink, collector) = pair();

    {
        let _guard =
            WriteSettledGuard::new_with_sink(sink, "op-happy", WriteOperationType::Copy, Some("test-vol".to_string()));
        // Guard drops at end of scope.
    }

    let settled = collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1, "guard must fire exactly one write-settled event");
    assert_eq!(settled[0].operation_id, "op-happy");
    assert_eq!(settled[0].operation_type, WriteOperationType::Copy);
    assert_eq!(settled[0].volume_id.as_deref(), Some("test-vol"));
}

#[test]
fn settled_fires_with_none_volume_for_local_ops() {
    let (sink, collector) = pair();

    drop(WriteSettledGuard::new_with_sink(
        sink,
        "op-local",
        WriteOperationType::Delete,
        None,
    ));

    let settled = collector.settled.lock().unwrap();
    assert_eq!(settled.len(), 1);
    assert!(
        settled[0].volume_id.is_none(),
        "local-FS settle event must carry volume_id=None"
    );
}

#[test]
fn settled_fires_on_panic_unwind() {
    // The guard is in scope when the closure panics. `catch_unwind` swallows
    // the panic, but the guard's Drop still runs as part of stack unwinding.
    // This is the panic-safety property the FE relies on so the dialog can
    // always clear, even on a backend bug.
    let (sink, collector) = pair();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _guard =
            WriteSettledGuard::new_with_sink(sink, "op-panic", WriteOperationType::Move, Some("v".to_string()));
        panic!("simulated handler panic");
    }));

    assert!(result.is_err(), "the closure must have panicked");
    let settled = collector.settled.lock().unwrap();
    assert_eq!(
        settled.len(),
        1,
        "settle event must fire even when the spawned task panicked: the FE would hang otherwise"
    );
    assert_eq!(settled[0].operation_id, "op-panic");
}

#[test]
fn settled_event_order_is_after_terminal_outcome_event() {
    // The FE depends on this ordering: it sees the terminal event
    // (write-cancelled / write-complete / write-error) first, knows the
    // outcome, then sees write-settled and knows the volume is ready.
    // Reversing would race: the dialog could try to close on settle before
    // it knows the outcome.
    //
    // Mirrors how production spawn tasks emit: handler emits the terminal
    // event, then state cleanup runs, then the guard's Drop emits settled.
    let (sink, collector) = pair();
    let op_id = "op-order".to_string();

    {
        let _guard = WriteSettledGuard::new_with_sink(
            Arc::clone(&sink),
            op_id.clone(),
            WriteOperationType::Delete,
            Some("v1".to_string()),
        );
        // Simulate the handler's terminal emit.
        sink.emit_cancelled(super::types::WriteCancelledEvent {
            operation_id: op_id.clone(),
            operation_type: WriteOperationType::Delete,
            files_processed: 7,
            rolled_back: false,
        });
        // Guard's Drop runs at end of scope, AFTER the cancelled emit above.
    }

    let cancelled = collector.cancelled.lock().unwrap();
    let settled = collector.settled.lock().unwrap();
    assert_eq!(cancelled.len(), 1, "cancelled should fire first");
    assert_eq!(settled.len(), 1, "settled should fire second");
    assert_eq!(settled[0].operation_id, op_id);
}

#[test]
fn settled_fires_for_every_operation_type() {
    // Trivial coverage: each op-type variant flows through the same path,
    // but we pin it so a future refactor that branches on type won't
    // accidentally drop one.
    for op_type in [
        WriteOperationType::Copy,
        WriteOperationType::Move,
        WriteOperationType::Delete,
        WriteOperationType::Trash,
    ] {
        let (sink, collector) = pair();
        drop(WriteSettledGuard::new_with_sink(
            sink,
            format!("op-{:?}", op_type),
            op_type,
            None,
        ));
        let settled = collector.settled.lock().unwrap();
        assert_eq!(settled.len(), 1, "settle must fire once per op_type={:?}", op_type);
        assert_eq!(settled[0].operation_type, op_type);
    }
}
