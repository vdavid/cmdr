//! The engine's per-op and per-preview events as the page hears them: by the
//! Tauri names in [`super::wire`], with the engine's own payloads, and a failure
//! retained for the queue window first, as the desktop's terminal effects
//! order it (`operations_host/terminal_effects.rs`).

use std::sync::Arc;

use cmdr_operation_log::types::OperationId;
use cmdr_operations::{
    ArmedConflict, ConflictInfo, DryRunResult, OperationEventSink, Operations, ScanPreviewCancelledEvent,
    ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewEventSink, ScanPreviewProgressEvent, ScanProgressEvent,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteLockedSourcesEvent,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent, WriteSourcesGoneEvent,
};

use super::wire::{self, emit};
use crate::events::EventSink;

/// One operation's events.
pub(crate) struct DemoOperationSink {
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) engine: Operations,
}

impl OperationEventSink for DemoOperationSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        emit(&*self.sink, wire::WRITE_PROGRESS, &event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        emit(&*self.sink, wire::WRITE_COMPLETE, &event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        emit(&*self.sink, wire::WRITE_CANCELLED, &event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        // Before the event: a queue window reading on the event finds the
        // retained row already there.
        self.engine.retain_failure(
            &OperationId::from(event.operation_id.as_str()),
            event.operation_type,
            &event.error,
        );
        emit(&*self.sink, wire::WRITE_ERROR, &event);
    }
    fn emit_conflict(&self, event: ArmedConflict) {
        emit(&*self.sink, wire::WRITE_CONFLICT, &event.into_event());
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        emit(&*self.sink, wire::WRITE_CONFLICT_RESOLVED, &event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        emit(&*self.sink, wire::WRITE_SOURCE_ITEM_DONE, &event);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        emit(&*self.sink, wire::WRITE_SOURCES_GONE, &event);
    }
    fn emit_locked_sources(&self, event: WriteLockedSourcesEvent) {
        emit(&*self.sink, wire::WRITE_LOCKED_SOURCES, &event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        emit(&*self.sink, wire::SCAN_PROGRESS, &event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        emit(&*self.sink, wire::SCAN_CONFLICT, &conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        emit(&*self.sink, wire::DRY_RUN_COMPLETE, &result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        emit(&*self.sink, wire::WRITE_SETTLED, &event);
    }
}

/// One scan preview's events.
pub(crate) struct DemoScanPreviewSink {
    pub(crate) sink: Arc<dyn EventSink>,
}

impl ScanPreviewEventSink for DemoScanPreviewSink {
    fn emit_progress(&self, event: ScanPreviewProgressEvent) {
        emit(&*self.sink, wire::SCAN_PREVIEW_PROGRESS, &event);
    }
    fn emit_complete(&self, event: ScanPreviewCompleteEvent) {
        emit(&*self.sink, wire::SCAN_PREVIEW_COMPLETE, &event);
    }
    fn emit_error(&self, event: ScanPreviewErrorEvent) {
        emit(&*self.sink, wire::SCAN_PREVIEW_ERROR, &event);
    }
    fn emit_cancelled(&self, event: ScanPreviewCancelledEvent) {
        emit(&*self.sink, wire::SCAN_PREVIEW_CANCELLED, &event);
    }
}
