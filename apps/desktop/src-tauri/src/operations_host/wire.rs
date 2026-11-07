//! The write engine's event payloads, as the Tauri events this app emits.
//!
//! The engine's payloads are plain `serde` + `specta` types; the event, with
//! its wire name, is ours. Each line below wraps one payload in a newtype that
//! derives `tauri_specta::Event`, so `ipc.rs` registers the wrapper and an emit
//! site writes `wire::WriteProgress(event).emit(&app)`. A payload emitted
//! unwrapped doesn't compile: it has no `emit`.
//!
//! ❗ The wrapper's `specta::Type` is hand-written to answer with the payload's
//! own definition, so `bindings.ts` names the payload (`makeEvent<WriteProgressEvent>`)
//! and gains no alias. A derived one, even `#[specta(transparent)]`, adds
//! `export type WriteProgress = WriteProgressEvent`. Always referenced as
//! `wire::<Name>`: several wrappers share their payload's name.

use cmdr_operations as engine;

macro_rules! wire_event {
    ($wrapper:ident($inner:ty) = $name:literal) => {
        #[derive(Clone, serde::Serialize, tauri_specta::Event)]
        #[serde(transparent)]
        #[tauri_specta(event_name = $name)]
        pub struct $wrapper(pub $inner);

        impl specta::Type for $wrapper {
            fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
                <$inner as specta::Type>::definition(types)
            }
        }
    };
}

// One operation's events (`sinks.rs`'s `TauriEventSink`).
wire_event!(WriteProgress(engine::WriteProgressEvent) = "write-progress");
wire_event!(WriteComplete(engine::WriteCompleteEvent) = "write-complete");
wire_event!(WriteCancelled(engine::WriteCancelledEvent) = "write-cancelled");
wire_event!(WriteError(engine::WriteErrorEvent) = "write-error");
wire_event!(WriteConflict(engine::WriteConflictEvent) = "write-conflict");
wire_event!(WriteConflictResolved(engine::WriteConflictResolvedEvent) = "write-conflict-resolved");
wire_event!(WriteSourceItemDone(engine::WriteSourceItemDoneEvent) = "write-source-item-done");
wire_event!(WriteSourcesGone(engine::WriteSourcesGoneEvent) = "write-sources-gone");
wire_event!(WriteLockedSources(engine::WriteLockedSourcesEvent) = "write-locked-sources");
wire_event!(ScanProgress(engine::ScanProgressEvent) = "scan-progress");
wire_event!(ScanConflict(engine::ConflictInfo) = "scan-conflict");
wire_event!(DryRunComplete(engine::DryRunResult) = "dry-run-complete");
wire_event!(WriteSettled(engine::WriteSettledEvent) = "write-settled");

// An instant mutation that outlived its reply deadline (`commands/settled.rs`).
wire_event!(MutationSettled(engine::MutationSettled) = "mutation-settled");

// Engine-wide facts (`events.rs`'s `TauriEngineEvents`).
wire_event!(LeftoversKept(engine::LeftoversKeptEvent) = "leftovers-kept");
wire_event!(OperationsChanged(engine::OperationsChanged) = "operations-changed");

// The operation log's tree-summary backfill, one per step (`commands/operation_log.rs`).
wire_event!(OperationLogTreeBackfill(cmdr_operation_log::query::TreeBackfillEvent) = "operation-log-tree-backfill");

// One scan preview's events (`sinks.rs`'s `TauriScanPreviewSink`).
wire_event!(ScanPreviewProgress(engine::ScanPreviewProgressEvent) = "scan-preview-progress");
wire_event!(ScanPreviewComplete(engine::ScanPreviewCompleteEvent) = "scan-preview-complete");
wire_event!(ScanPreviewError(engine::ScanPreviewErrorEvent) = "scan-preview-error");
wire_event!(ScanPreviewCancelled(engine::ScanPreviewCancelledEvent) = "scan-preview-cancelled");

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use cmdr_fs::ignore_poison::IgnorePoison;
    use tauri::Listener;
    use tauri_specta::{Event, collect_events};

    use super::*;

    /// The wrapper emits under the payload's wire name with the payload's exact
    /// JSON: what the frontend's `events.writeSettled.listen` receives.
    #[test]
    fn a_wrapped_payload_reaches_the_wire_unchanged() {
        let app = tauri::test::mock_app();
        let builder = tauri_specta::Builder::<tauri::test::MockRuntime>::new().events(collect_events![WriteSettled]);
        builder.mount_events(app.handle());

        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = Arc::clone(&seen);
        app.listen_any("write-settled", move |event| {
            sink.lock_ignore_poison().push(event.payload().to_string());
        });

        let inner = engine::WriteSettledEvent {
            operation_id: "op-1".into(),
            operation_type: engine::WriteOperationType::Copy,
            volume_id: Some("vol".into()),
        };
        let expected = serde_json::to_string(&inner).expect("serializes");
        WriteSettled(inner).emit(app.handle()).expect("emits");

        assert_eq!(*seen.lock_ignore_poison(), vec![expected.clone()]);
        assert_eq!(
            expected,
            r#"{"operationId":"op-1","operationType":"copy","volumeId":"vol"}"#
        );
    }
}
