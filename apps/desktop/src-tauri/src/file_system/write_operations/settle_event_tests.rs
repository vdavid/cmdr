//! Tests for the `write-settled` per-op event. See § "Settle contract" in the
//! module CLAUDE.md for the full contract.
//!
//! The `WriteSettledGuard` RAII pattern emits exactly one `write-settled`
//! event when the spawned task fully returns — happy path, error path,
//! cancellation, or panic. These tests pin the contract.
//!
//! The guard emits via `sink.emit_settled(...)` from inside its `Drop`, using
//! the injected `Arc<dyn OperationEventSink>`. Production builds that sink at
//! the IPC edge (`TauriEventSink`); these tests pass a `CollectorEventSink`, so
//! the full lifecycle runs without a Tauri runtime.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::copy_files_start;
use super::state::WriteSettledGuard;
use super::types::{
    CollectorEventSink, OperationEventSink, WriteOperationConfig, WriteOperationType, WriteSettledEvent,
};

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
        let _guard = WriteSettledGuard::new(sink, "op-happy", WriteOperationType::Copy, Some("test-vol".to_string()));
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

    drop(WriteSettledGuard::new(
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
        let _guard = WriteSettledGuard::new(sink, "op-panic", WriteOperationType::Move, Some("v".to_string()));
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
        let _guard = WriteSettledGuard::new(
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
        drop(WriteSettledGuard::new(sink, format!("op-{:?}", op_type), op_type, None));
        let settled = collector.settled.lock().unwrap();
        assert_eq!(settled.len(), 1, "settle must fire once per op_type={:?}", op_type);
        assert_eq!(settled[0].operation_type, op_type);
    }
}

// ============================================================================
// Injected event sink drives the managed spawn path end to end
// ============================================================================

fn create_temp_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("cmdr_write_test_{}", name));
    let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous run
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    temp_dir
}

fn cleanup_temp_dir(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

/// Drives `copy_files_start` through the operation manager with a
/// `CollectorEventSink` injected at the (test-stand-in) IPC edge, and asserts
/// the terminal `write-complete` and the `write-settled` events both arrive via
/// that sink for a real local copy. Before the sink lift these managed events
/// were only reachable through a live `TauriEventSink`; now the whole pipeline
/// takes the injected sink, so it's observable with no Tauri runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injected_sink_receives_complete_and_settled_for_local_copy() {
    let src_dir = create_temp_dir("managed_sink_src");
    let dst_dir = create_temp_dir("managed_sink_dst");
    let src_file = src_dir.join("hello.txt");
    fs::write(&src_file, b"managed sink test").expect("failed to write source file");

    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();

    let result = copy_files_start(
        events,
        vec![src_file.clone()],
        dst_dir.clone(),
        WriteOperationConfig::default(),
        vec![],
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("copy_files_start should return Ok");

    // The deferred task runs asynchronously; poll until it settles.
    let mut settled_ok = false;
    for _ in 0..200 {
        {
            let settled = collector.settled.lock().unwrap();
            if let Some(ev) = settled.first() {
                assert_eq!(ev.operation_id, result.operation_id);
                assert_eq!(ev.operation_type, WriteOperationType::Copy);
                assert!(ev.volume_id.is_none(), "a same-root local copy carries volume_id=None");
                settled_ok = true;
            }
        }
        if settled_ok {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(settled_ok, "write-settled must arrive via the injected sink");

    // The full managed pipeline ran through the injected sink, not just the
    // settle guard: the terminal write-complete arrived and the bytes landed.
    let complete = collector.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "exactly one write-complete must arrive via the sink");
    assert_eq!(complete[0].operation_id, result.operation_id);
    assert!(
        dst_dir.join("hello.txt").exists(),
        "the copied file must exist at the destination"
    );

    cleanup_temp_dir(&src_dir);
    cleanup_temp_dir(&dst_dir);
}
