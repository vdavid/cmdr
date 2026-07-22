//! Event sinks: the `OperationEventSink` trait and its production (`TauriEventSink`)
//! and test (`CollectorEventSink`) implementations, plus the event-builder `impl`
//! blocks for `WriteProgressEvent` and `WriteErrorEvent`.
//!
//! Decouples the copy/move/delete pipeline from `tauri::AppHandle`: the Tauri layer
//! provides `TauriEventSink` (calls `app.emit`), tests use `CollectorEventSink`
//! (stores events in a `Vec` for assertions).

use tauri_specta::Event;

#[cfg(test)]
use crate::ignore_poison::IgnorePoison;

use super::analytics::emit_completion_analytics;
use super::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteErrorEvent, WriteOperationError, WriteOperationType, WriteProgressEvent, WriteSettledEvent,
    WriteSourceItemDoneEvent,
};
use crate::indexing::read::expected_totals;

impl WriteProgressEvent {
    /// Construct an event with the 8 core counter fields. Rate/ETA fields are
    /// filled in by `WriteOperationState::enrich_progress` right before the
    /// event is emitted. The scanning-only metadata (`current_dir`,
    /// `expected_files_total`, `expected_bytes_total`) defaults to `None` and
    /// is populated by the scan emit sites via `with_scan_meta`. Always go
    /// through this constructor at emit sites so the extra fields stay out of
    /// call sites as visual noise.
    #[allow(
        clippy::too_many_arguments,
        reason = "These are the natural fields of a progress event. Bundling into a struct adds ceremony without cleaning anything up."
    )]
    pub fn new(
        operation_id: String,
        operation_type: WriteOperationType,
        phase: super::types::WriteOperationPhase,
        current_file: Option<String>,
        files_done: usize,
        files_total: usize,
        bytes_done: u64,
        bytes_total: u64,
    ) -> Self {
        Self {
            operation_id,
            operation_type,
            phase,
            current_file,
            current_dir: None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            dirs_done: 0,
            bytes_per_second: None,
            files_per_second: None,
            eta_seconds: None,
            expected_files_total: None,
            expected_bytes_total: None,
        }
    }

    /// Attach scanning-phase metadata (current directory, running dirs count,
    /// and index-derived expected totals) to an event. Only emit sites in the
    /// scanning phase call this; everywhere else leaves the fields at their
    /// defaults (`None` / `0`).
    #[must_use]
    pub fn with_scan_meta(
        mut self,
        current_dir: Option<String>,
        dirs_done: usize,
        expected: Option<expected_totals::ExpectedTotals>,
    ) -> Self {
        self.current_dir = current_dir;
        self.dirs_done = dirs_done;
        if let Some(e) = expected {
            self.expected_files_total = Some(e.files);
            self.expected_bytes_total = Some(e.bytes);
        }
        self
    }
}

impl WriteErrorEvent {
    /// Construct a `WriteErrorEvent` from the typed `error`. The FE renders all
    /// user-facing copy and the category/retry classification from the typed
    /// variant; no rendered prose crosses IPC.
    pub fn new(operation_id: String, operation_type: WriteOperationType, error: WriteOperationError) -> Self {
        Self {
            operation_id,
            operation_type,
            error,
        }
    }
}

/// Abstraction for emitting write operation events.
///
/// Decouples the copy/move/delete pipeline from `tauri::AppHandle`. The Tauri
/// layer provides `TauriEventSink` (calls `app.emit`). Tests use
/// `CollectorEventSink` (stores events in a `Vec` for assertions).
pub trait OperationEventSink: Send + Sync {
    fn emit_progress(&self, event: WriteProgressEvent);
    fn emit_complete(&self, event: WriteCompleteEvent);
    fn emit_cancelled(&self, event: WriteCancelledEvent);
    fn emit_error(&self, event: WriteErrorEvent);
    fn emit_conflict(&self, event: WriteConflictEvent);
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent);
    /// Per-iteration progress during dry-run scanning (separate from `write-progress`).
    fn emit_scan_progress(&self, event: ScanProgressEvent);
    /// One `ConflictInfo` per conflicting file during dry-run scanning.
    fn emit_scan_conflict(&self, conflict: ConflictInfo);
    /// Final dry-run result with conflict sample.
    fn emit_dry_run_complete(&self, result: DryRunResult);
    /// Emitted exactly once per op, after the spawned task fully returns
    /// (success, error, cancel, or panic). `WriteSettledGuard` calls this from
    /// its `Drop`, so every op settles through the injected sink. See
    /// `WriteSettledEvent` for the ordering contract.
    fn emit_settled(&self, event: WriteSettledEvent);

    /// Notes that a top-level source extracted in FULL (every file durably
    /// written, zero deep skips). Only the out-of-zip move op cares: it collects
    /// these to delete exactly the fully-extracted sources from the archive, so a
    /// partial move converges on retry. Default no-op for every other sink.
    fn note_source_landed_clean(&self, _source: &std::path::Path) {}
}

/// Tauri-backed event sink: calls `app.emit()` for each event.
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl OperationEventSink for TauriEventSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        // PII-free analytics: a transfer or delete completed. Categorical only (op, a count
        // bucket, a bool); never names or paths. Copy/Move map to `file_transfer_completed`,
        // Delete/Trash to `delete_used`. Fires before the emit so it can read the moved event.
        emit_completion_analytics(&event);
        // Record the terminal outcome for MCP's `await operation_complete` before the FE
        // event fires (the emit-site pattern `listing_errors` uses), since the manager
        // removes the op before `operations-changed` could carry a terminal status.
        crate::mcp::terminal_ops::record(
            &event.operation_id,
            event.operation_type,
            crate::mcp::terminal_ops::TerminalStatus::Completed,
        );
        let _ = event.emit(&self.app);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        crate::mcp::terminal_ops::record(
            &event.operation_id,
            event.operation_type,
            crate::mcp::terminal_ops::TerminalStatus::Cancelled,
        );
        let _ = event.emit(&self.app);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        crate::mcp::terminal_ops::record(
            &event.operation_id,
            event.operation_type,
            crate::mcp::terminal_ops::TerminalStatus::Failed,
        );
        let _ = event.emit(&self.app);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        let _ = event.emit(&self.app);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        let _ = conflict.emit(&self.app);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        let _ = result.emit(&self.app);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        let _ = event.emit(&self.app);
    }
}

/// Test event sink: stores events for inspection.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "Fields are populated by emit_* methods; read in test assertions as needed"
)]
pub(crate) struct CollectorEventSink {
    pub progress: std::sync::Mutex<Vec<WriteProgressEvent>>,
    pub complete: std::sync::Mutex<Vec<WriteCompleteEvent>>,
    pub cancelled: std::sync::Mutex<Vec<WriteCancelledEvent>>,
    pub errors: std::sync::Mutex<Vec<WriteErrorEvent>>,
    pub conflicts: std::sync::Mutex<Vec<WriteConflictEvent>>,
    pub scan_progress: std::sync::Mutex<Vec<ScanProgressEvent>>,
    pub scan_conflicts: std::sync::Mutex<Vec<ConflictInfo>>,
    pub dry_run: std::sync::Mutex<Vec<DryRunResult>>,
    pub settled: std::sync::Mutex<Vec<WriteSettledEvent>>,
}

#[cfg(test)]
impl CollectorEventSink {
    pub fn new() -> Self {
        Self {
            progress: std::sync::Mutex::new(Vec::new()),
            complete: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::Mutex::new(Vec::new()),
            errors: std::sync::Mutex::new(Vec::new()),
            conflicts: std::sync::Mutex::new(Vec::new()),
            scan_progress: std::sync::Mutex::new(Vec::new()),
            scan_conflicts: std::sync::Mutex::new(Vec::new()),
            dry_run: std::sync::Mutex::new(Vec::new()),
            settled: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl OperationEventSink for CollectorEventSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.progress.lock_ignore_poison().push(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.complete.lock_ignore_poison().push(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.cancelled.lock_ignore_poison().push(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.errors.lock_ignore_poison().push(event);
    }
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.conflicts.lock_ignore_poison().push(event);
    }
    fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.scan_progress.lock_ignore_poison().push(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.scan_conflicts.lock_ignore_poison().push(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.dry_run.lock_ignore_poison().push(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.settled.lock_ignore_poison().push(event);
    }
}
