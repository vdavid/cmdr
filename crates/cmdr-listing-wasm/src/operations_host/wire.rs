//! The write engine's events by their Tauri names, the desktop's
//! `operations_host/wire.rs` table in the page: each name next to the engine
//! type its payload is. The payloads are the engine's own `serde` types, so
//! only a name (or `VolumesBusyChanged`, the one payload the host shapes) can
//! drift; `events_test.rs` pins both against `bindings.ts`.

use serde::Serialize;

use crate::events::EventSink;

/// `WriteProgressEvent`, also a scan's waiting progress.
pub(crate) const WRITE_PROGRESS: &str = "write-progress";
/// `WriteCompleteEvent`.
pub(crate) const WRITE_COMPLETE: &str = "write-complete";
/// `WriteCancelledEvent`.
pub(crate) const WRITE_CANCELLED: &str = "write-cancelled";
/// `WriteErrorEvent`.
pub(crate) const WRITE_ERROR: &str = "write-error";
/// `WriteConflictEvent` (from an `ArmedConflict`).
pub(crate) const WRITE_CONFLICT: &str = "write-conflict";
/// `WriteConflictResolvedEvent`.
pub(crate) const WRITE_CONFLICT_RESOLVED: &str = "write-conflict-resolved";
/// `WriteSourceItemDoneEvent`.
pub(crate) const WRITE_SOURCE_ITEM_DONE: &str = "write-source-item-done";
/// `WriteSourcesGoneEvent`.
pub(crate) const WRITE_SOURCES_GONE: &str = "write-sources-gone";
/// `WriteLockedSourcesEvent`.
pub(crate) const WRITE_LOCKED_SOURCES: &str = "write-locked-sources";
/// `ScanProgressEvent`.
pub(crate) const SCAN_PROGRESS: &str = "scan-progress";
/// `ConflictInfo`.
pub(crate) const SCAN_CONFLICT: &str = "scan-conflict";
/// `DryRunResult`.
pub(crate) const DRY_RUN_COMPLETE: &str = "dry-run-complete";
/// `WriteSettledEvent`.
pub(crate) const WRITE_SETTLED: &str = "write-settled";
/// `MutationSettled`.
pub(crate) const MUTATION_SETTLED: &str = "mutation-settled";
/// `LeftoversKeptEvent`.
pub(crate) const LEFTOVERS_KEPT: &str = "leftovers-kept";
/// `OperationsChanged`.
pub(crate) const OPERATIONS_CHANGED: &str = "operations-changed";
/// [`VolumesBusyChanged`].
pub(crate) const VOLUMES_BUSY_CHANGED: &str = "volumes-busy-changed";
/// `ScanPreviewProgressEvent`.
pub(crate) const SCAN_PREVIEW_PROGRESS: &str = "scan-preview-progress";
/// `ScanPreviewCompleteEvent`.
pub(crate) const SCAN_PREVIEW_COMPLETE: &str = "scan-preview-complete";
/// `ScanPreviewErrorEvent`.
pub(crate) const SCAN_PREVIEW_ERROR: &str = "scan-preview-error";
/// `ScanPreviewCancelledEvent`.
pub(crate) const SCAN_PREVIEW_CANCELLED: &str = "scan-preview-cancelled";

/// `volumes-busy-changed`'s payload (`VolumesBusyChanged` in the app, which
/// lives in the app crate).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VolumesBusyChanged {
    pub(crate) volume_ids: Vec<String>,
}

/// `payload` on the page's event channel as `name`.
pub(crate) fn emit(sink: &dyn EventSink, name: &str, payload: &impl Serialize) {
    match serde_json::to_value(payload) {
        Ok(payload) => sink.emit(name, payload),
        // allowed-log-private: `name` is one of this file's event names, and a serde error names no path
        Err(e) => log::warn!(target: "write_ops", "couldn't serialize `{name}`: {e}"),
    }
}
