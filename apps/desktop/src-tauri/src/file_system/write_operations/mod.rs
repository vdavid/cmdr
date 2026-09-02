//! Write operations (copy, move, delete) with streaming progress.
//!
//! All operations run in background tasks and emit progress events at configurable intervals.
//! Operations support batch processing (multiple source files) and cancellation.
//!
//! Safety features:
//! - Path canonicalization to prevent ".." and symlink bypass of recursion checks
//! - Destination writability check before starting
//! - Pre-flight disk space validation after scan
//! - Inode identity check to prevent copy-over-self via symlinks/hard links
//! - Path/name length validation (255-byte name, 1024-byte path)
//! - Special file filtering (skips sockets, FIFOs, devices)
//! - macOS copyfile(3) for full metadata preservation (xattrs, ACLs, resource forks)
//! - Symlink preservation (not dereferenced)
//! - Symlink loop detection to prevent infinite recursion
//! - Copy rollback on failure (CopyTransaction)
//! - Atomic cross-filesystem moves using staging directory

mod analytics;
mod archive_edit;
mod archive_remote_edit;
mod cancellable;
mod compress_estimate;
mod conflict;
mod conflict_slot;
mod create;
mod delete;
mod durability;
mod error_classification;
mod eta;
mod event_sinks;
mod human_wait;
mod in_flight_temps;
mod journal;
mod journal_search;
mod ledger;
mod manager;
mod mutation_error;
mod operation_intent;
mod overwrite;
#[cfg(target_os = "macos")]
mod paste_clipboard;
mod rename;
mod reversal;
pub(crate) mod rollback;
mod routing;
mod scan;
mod scan_bridge;
mod scan_cache;
mod scan_preview;
mod scan_watchdog;
mod scratch_dir;
mod source_binding;
mod state;
mod status_cache;
mod transfer;
mod types;
mod unique_name;
mod validation;

// Re-export `trash` at this level so `crate::file_system::write_operations::trash`
// keeps resolving (used by `commands/rename.rs`).
pub(crate) use delete::trash;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::file_system::volume::LaneKey;
use crate::operation_log::types::Initiator;
use delete::delete_files_with_progress_inner;
use manager::OperationDescriptor;
#[cfg(not(test))]
use state::WriteOperationState;
use state::WriteSettledGuard;
use transfer::copy::copy_files_with_progress_inner;
use transfer::move_op::move_files_with_progress_inner;
use trash::trash_files_with_progress;

// The event sink trait + its Tauri-backed implementation. Re-exported so the
// IPC command layer can build `Arc::new(TauriEventSink::new(app))` at the edge
// and inject it into the managed pipeline; the pipeline itself never constructs
// a sink (grep confirms zero `TauriEventSink::new` under `write_operations/`).
/// A sink over the STARTUP-WIRED app handle, for an edge that is generic over the
/// Tauri runtime (`AppHandle<R>`, which the MCP tools are) and so can't build a
/// concrete one from the handle it holds. `None` before wiring (unit tests).
pub(crate) use archive_edit::global_tauri_sink;
pub use event_sinks::{OperationEventSink, TauriEventSink};
#[cfg(not(test))]
use validation::{
    ensure_destination_dir, validate_destination_not_inside_source, validate_destination_writable, validate_sources,
};

// Re-export public types
/// The quit teardown's fence over that same ledger: everything recorded is with
/// the kernel before the process ends, so the next launch's sweep sees it.
pub use in_flight_temps::flush as flush_in_flight_temps;
/// Points the in-flight transfer-partial ledger at the app data dir and clears
/// what an earlier run left behind. Startup only, before any copy can start.
pub use in_flight_temps::init_and_sweep as init_and_sweep_in_flight_temps;
pub use scan_preview::{cancel_scan_preview, get_scan_preview_totals, start_scan_preview};
pub use state::{
    VolumesBusyChanged, busy_volume_ids, cancel_all_write_operations, cancel_write_operation, get_operation_status,
    init_busy_volume_emitter, list_active_operations, pending_write_conflict, resolve_write_conflict,
};
// The hard-abort tier. Exactly one legitimate caller: the quit deadline
// (`crate::quit`), which fires it only after the cooperative cancel has had its
// window. ❌ Never reach for it from anything a person clicked — it skips the
// backend's own partial cleanup. `transfer/DETAILS.md` § "Two tiers of cancel".
pub use state::abort_all_write_operations;
// Operation manager: the single scheduler + registry every write op flows
// through. `OperationsChanged` / `OperationSnapshot` are the thin
// `operations-changed` event payload (the queue window consumes them). The
// `LifecycleStatus` they carry is vocabulary and lives in `types`.
// `init_operation_event_emitter` wires the emitter at startup; the command
// helpers back the new `list_operations` / `cancel_operation(s)` IPC.
pub use manager::{
    OperationSnapshot, OperationSummaryText, OperationsChanged, PauseAllOutcome, PauseOutcome, cancel_operation,
    cancel_operations, dismiss_all_failed_operations, dismiss_failed_operation, init_operation_event_emitter,
    list_operations, pause_all, pause_operation, resume_all, resume_operation,
};
// Managed instant mutations (rename / mkdir / mkfile) + rename validation. The
// thin IPC commands (`commands/rename.rs`, `commands/file_system/write_ops.rs`)
// call these; `RenameValidityResult` rides into `bindings.ts` via the
// `check_rename_validity` command signature.
pub(crate) use create::{create_directory_managed, create_file_managed};
#[cfg(target_os = "macos")]
pub(crate) use paste_clipboard::write_payload_to_dir;
pub(crate) use rename::{
    BulkRenameRow, RenameValidityResult, check_rename_permission_sync, check_rename_validity_impl, rename_managed,
    start_bulk_rename,
};
// The source-identity binding a reviewed batch may supply. `source_binding.rs`.
// Volume + destination resolution and the three routed cross-volume entry points,
// reachable by a backend caller and not only the IPC edge. `routing.rs`.
pub(crate) use routing::{
    resolve_dest_path, resolve_source_volume, start_volume_compress, start_volume_copy, start_volume_move,
    transfer_would_land_on_its_source,
};
#[cfg(not(test))]
use source_binding::retain_bound_sources;
use source_binding::retain_bound_sources_with_sizes;
pub(crate) use source_binding::{ExpectedSources, LocalContent, RemoteContent, SourceFingerprint};
// Test-only reach for the suggestion bridge's suite, which checks that a binding captured at
// preflight refuses a source rewritten afterwards. Production callers get the pre-flight
// through the starters, never directly.
#[cfg(test)]
pub(crate) use source_binding::retain_bound_sources;
// External busy-volume seam for the drag-out fulfillment service (see
// `lifecycle/state.rs` § "External busy-volume seam"). `pub(crate)` so only in-crate
// callers (`native_drag::fulfillment`) reach it. macOS-only: the sole consumer
// (`native_drag`) is `#[cfg(target_os = "macos")]`, so on other targets these
// would be dead code under `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) use state::{register_external_volume_op, release_external_volume_op};
#[allow(unused_imports, reason = "Public API re-exports for consumers of this module")]
pub use types::{
    ConflictId, ConflictInfo, ConflictResolution, ConflictResolutionOutcome, DryRunResult, LifecycleStatus,
    OperationStatus, OperationSummary, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanPreviewStartResult, ScanPreviewTotals, ScanProgressEvent, SortColumn, SortOrder,
    SourceItemOutcome, TransferActivity, TransferWaitReason, WriteCancelledEvent, WriteCompleteEvent,
    WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationPhase, WriteOperationStartResult, WriteOperationType, WriteProgressEvent, WriteSettledEvent,
    WriteSourceItemDoneEvent,
};

// Re-export for tests (these are pub(crate) in validation.rs and state.rs)
#[cfg(test)]
pub(crate) use ledger::CopyTransaction;
#[cfg(test)]
pub(crate) use state::{OperationIntent, WriteOperationState, is_cancelled, load_intent};
#[cfg(test)]
#[allow(unused_imports, reason = "Re-exports for test modules in file_system")]
pub(crate) use validation::{
    ensure_destination_dir, is_same_file, is_same_filesystem, validate_destination_not_inside_source,
    validate_destination_writable, validate_disk_space, validate_path_length, validate_sources,
};
// Exposed for cross-module integration tests (for example the SMB
// concurrent-copy cross-contamination test in
// `file_system::volume::smb`) that drive `copy_volumes_with_progress`
// directly against a real SMB backend instead of the full Tauri path.
#[cfg(test)]
#[allow(unused_imports, reason = "Used by SMB integration tests in file_system::volume::smb")]
pub(crate) use event_sinks::CollectorEventSink;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by the volume-journal capture tests")]
pub(crate) use transfer::volume::move_volumes_with_progress;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by SMB integration tests in file_system::volume::smb")]
pub(crate) use transfer::volume::move_within_same_volume_with_progress;
// The four real-SMB safety cells drive the same axes the in-memory grid does:
// a cache entry that counted files but recorded no per-source result, and a
// volume that stops answering partway through.
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "Used by SMB integration tests in file_system::volume::backends"
)]
pub(crate) use delete::delete_volume_files_for_test;
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "Used by SMB integration tests in file_system::volume::backends"
)]
pub(crate) use scan_cache::seed_incoherent_scan_result_for_test;
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "Used by SMB integration tests in file_system::volume::backends"
)]
pub(crate) use transfer::volume::{FaultyOp, FaultyVolume};

/// Test-only: retain a failure straight in the manager, so a suite outside this
/// module can exercise a CONSUMER of `list_operations` (the MCP `queue` guard)
/// against a failed row without spawning a real operation. Drop it again with
/// the ordinary [`dismiss_failed_operation`]. A function rather than a re-export
/// of `manager::manager`: importing that name shortens `manager::manager()` to
/// `manager()` throughout this module, and clippy's redundant-qualification fix
/// then rewrites those call sites into something that only compiles under
/// `cfg(test)`.
#[cfg(test)]
pub(crate) fn test_retain_failure(operation_id: &str, operation_type: WriteOperationType, error: &WriteOperationError) {
    manager::manager().record_failure(operation_id, operation_type, error);
}

// Re-export volume copy types and functions. `routing.rs` owns which of
// `copy_between_volumes` / `move_between_volumes` / `route_archive_{copy_into,
// move_out}` / `compress_start` a given transfer reaches, so those four are
// reached through `super::` inside this module and are NOT re-exported: one
// routing, one place to keep it right.
pub use mutation_error::MutationError;
pub use transfer::volume::scan_for_volume_copy;
pub use types::{VolumeCopyConfig, VolumeCopyScanResult};
// Test-only: the archive-edit and remote-transfer suites drive these drivers
// directly, under a `CollectorEventSink` and without a routing decision to make.
#[cfg(test)]
pub(crate) use archive_edit::{compress_start, route_archive_copy_into, route_archive_move_out};
// The cross-volume copy body, reused as the extract phase of an out-of-zip MOVE
// (`route_archive_move_out`). Not spawn-managed itself — it runs inside the
// move-out op's deferred under the move op's id/state/sink.
pub(crate) use transfer::volume::copy_volumes_with_progress;
// The remote zip-edit orchestration (pull-local, apply, upload, swap). Exposed at
// crate scope for the live-SMB / MTP integration suites, which drive the real
// mechanism against a real remote volume. The managed driver reaches it directly
// via `super::archive_remote_edit`, so this re-export is test-only.
#[cfg(test)]
pub(crate) use archive_remote_edit::{RemoteEditError, pull_apply_upload_swap};
// A live operation's in-flight table, so a suite that bounds its own wait can put
// the transfer probe's dump in its panic message. Used by the live-SMB
// full-concurrency suite, which sits outside this module.
#[cfg(test)]
pub(crate) use transfer::transfer_probe::render_live_dump as render_live_transfer_dump;

// ============================================================================
// Public API functions
// ============================================================================

/// Spawns a write operation in the background with state management and panic handling.
///
/// Creates `WriteOperationState`, registers the operation, spawns `tokio::spawn` +
/// `spawn_blocking`, and handles cleanup and panic recovery. Callers do validation
/// and logging before calling this, then pass a closure for the actual work.
#[allow(
    clippy::too_many_arguments,
    reason = "the managed-spawn entry point threads lane keys + summary + volume ids alongside the handler; bundling them would just shuffle fields into a struct at every call site"
)]
async fn start_write_operation<F>(
    events: Arc<dyn OperationEventSink>,
    operation_type: WriteOperationType,
    initiator: Initiator,
    progress_interval_ms: u64,
    volume_ids: Vec<String>,
    lanes: Vec<LaneKey>,
    summary: OperationSummaryText,
    // The confirming dialog's scan preview, claimed at registration so the op
    // awaits the walk instead of racing a second one down the same tree.
    preview_id: Option<String>,
    // The provisional planned total (the top-level source count) journaled at
    // `open`; finalize refines it to the scanned total. Never 0 for a real op, so
    // the alpha dialog never renders "Copy 0 items" (the header-aggregate rider).
    item_count: u64,
    handler: F,
) -> Result<WriteOperationStartResult, WriteOperationError>
where
    F: FnOnce(Arc<dyn OperationEventSink>, String, Arc<WriteOperationState>) -> Result<(), WriteOperationError>
        + Send
        + 'static,
{
    let operation_id = crate::operation_log::new_operation_id();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type,
        lanes,
        volume_ids,
        summary,
        // A local copy/move writes NEW files it can delete again on rollback
        // (`CopyTransaction` / `MoveTransaction`). A delete or trash has nothing
        // to put back, and everything else that reaches here is neither.
        supports_rollback: matches!(operation_type, WriteOperationType::Copy | WriteOperationType::Move),
        preview_id,
        reverses: None,
    };

    let events_for_op = Arc::clone(&events);
    let operation_id_for_op = operation_id.clone();
    let state_for_op = Arc::clone(&state);

    // Deferred start: the manager spawns this only once the op's lanes are
    // free. It owns the op end-to-end — settle guard, the blocking handler,
    // the terminal-event safety net — and ends by calling `on_settled` (which
    // frees lanes, cleans caches, and admits the next op). The `ManagedTaskGuard`
    // is the panic safety net: if the task unwinds before `on_settled`, its Drop
    // still frees the lanes + caches (but never spawns).
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = operation_id_for_op;
            let state = state_for_op;
            let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
            // RAII guard: emits `write-settled` when this task exits, no matter
            // how (handler success, error, cancel, or panic via JoinError). FE
            // gates the "Cancelling…" dialog close on this event so the user
            // can't dispatch a new op against a still-tearing-down volume.
            let _settled_guard = WriteSettledGuard::new(Arc::clone(&events), op_id.clone(), operation_type, None);

            // Wait out the confirming dialog's scan before any journal row or
            // any I/O: an op that never got past its scan wrote nothing, so
            // there's nothing to journal and its terminal event is already out.
            if scan_bridge::await_claimed_preview(&*events, &op_id, operation_type, &state)
                .await
                .stopped()
            {
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

            // Open the journal row when the op actually starts (not at
            // registration), so a queued op that's canceled before admission
            // never journals. A local copy/move lands its destination on the
            // local FS ("root"); a delete/trash has no destination volume.
            let op_kind = journal::op_kind_of(operation_type);
            let dest_volume_id = match operation_type {
                WriteOperationType::Copy | WriteOperationType::Move => {
                    Some(crate::file_system::volume::DEFAULT_VOLUME_ID)
                }
                _ => None,
            };
            journal::open_local_op(&op_id, op_kind, initiator, item_count, dest_volume_id);

            let op_id_for_blocking = op_id.clone();
            let events_for_handler = Arc::clone(&events);
            let result =
                tokio::task::spawn_blocking(move || handler(events_for_handler, op_id_for_blocking, state)).await;

            // Journal the terminal state BEFORE cache cleanup (finalize computes
            // eligibility + the completeness downgrade from what actually
            // happened). A failed/canceled op stays rollbackable for the items it
            // reached.
            let execution_status = match &result {
                Ok(Ok(())) => crate::operation_log::types::ExecutionStatus::Done,
                Ok(Err(WriteOperationError::Cancelled { .. })) => {
                    crate::operation_log::types::ExecutionStatus::Canceled
                }
                _ => crate::operation_log::types::ExecutionStatus::Failed,
            };
            journal::finalize_op(&op_id, op_kind, execution_status);

            match result {
                Ok(Ok(())) => {} // Handler already emitted write-complete or write-cancelled
                Ok(Err(ref e)) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                    // Handler already emitted write-cancelled
                }
                Ok(Err(e)) => {
                    // Handler error (validation, I/O, etc.): emit write-error as safety net
                    events.emit_error(WriteErrorEvent::new(op_id.clone(), operation_type, e));
                }
                Err(join_error) => {
                    // Panic/abort in spawn_blocking
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        operation_type,
                        WriteOperationError::IoError {
                            path: String::new(),
                            message: format!("Task failed: {}", join_error),
                        },
                    ));
                }
            }

            // Happy-path dequeue: free lanes, clean caches, admit next. Order:
            // terminal event → `on_settled` (cache removal) → `write-settled`
            // via the settle guard's Drop at end of scope. Disarm the panic
            // guard first so its Drop doesn't redo the (now-done) cleanup.
            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type,
    })
}

/// Lane keys for a local-FS op when the caller didn't supply explicit ones.
/// A pure same-`root` op gets the single `root` lane; a local→removable copy
/// (which carries the ejectable volume's id in `volume_ids`) gets a lane per
/// distinct id, so two transfers to the same local disk serialize. This is a
/// proxy for `Volume::lane_key()` on the local-only path where no `Volume`
/// handle is threaded through; it uses each id as an opaque whole (no
/// substring parsing).
fn local_lanes(volume_ids: &[String]) -> Vec<LaneKey> {
    if volume_ids.is_empty() {
        vec![LaneKey::new(crate::file_system::volume::DEFAULT_VOLUME_ID)]
    } else {
        volume_ids.iter().cloned().map(LaneKey::new).collect()
    }
}

/// Best-effort `source → destination` summary for the queue window: the source
/// items' display names joined, and the destination's. Cheap; no I/O.
pub(super) fn path_summary(sources: &[PathBuf], destination: Option<&std::path::Path>) -> OperationSummaryText {
    fn name(p: &std::path::Path) -> String {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned())
    }
    let source = match sources {
        [] => None,
        [one] => Some(name(one)),
        many => Some(format!("{} ({} items)", name(&many[0]), many.len())),
    };
    OperationSummaryText {
        source,
        destination: destination.map(name),
    }
}

/// Starts a copy operation in the background.
///
/// `volume_ids` lists the volumes this copy touches (source + destination), so
/// an ejectable USB / DMG / SMB volume is marked busy while the copy runs. Pass
/// an empty `Vec` for a same-`root` local copy (root is never ejectable).
///
/// `lanes` are the operation-manager lanes this op occupies. Pass `None` to
/// derive them from `volume_ids` (the plain local-copy command path); the
/// both-local branch of `copy_between_volumes` passes the real
/// `Volume::lane_key()`s of the two volumes.
///
/// `expected_sources` is the caller's source binding, or `None` for the ordinary
/// user-started case. `source_binding.rs`.
#[allow(
    clippy::too_many_arguments,
    reason = "the manager's context alongside the op's own inputs"
)]
pub async fn copy_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
    volume_ids: Vec<String>,
    lanes: Option<Vec<LaneKey>>,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!(
        "copy_files_start: sources={:?}, destination={:?}, dry_run={}",
        sources,
        destination,
        config.dry_run
    );

    let lanes = lanes.unwrap_or_else(|| local_lanes(&volume_ids));
    let summary = path_summary(&sources, Some(&destination));
    start_write_operation(
        events,
        WriteOperationType::Copy,
        initiator,
        config.progress_interval_ms,
        volume_ids,
        lanes,
        summary,
        config.preview_id.clone(),
        sources.len() as u64,
        move |events, op_id, state| {
            let Some(sources) = retain_bound_sources(
                &*events,
                &op_id,
                WriteOperationType::Copy,
                expected_sources.as_ref(),
                sources,
            ) else {
                return Ok(());
            };
            validate_sources(&sources)?;
            // Guard against copying a folder into itself BEFORE creating anything:
            // the dest may not exist yet, and the guard resolves it via its nearest
            // existing ancestor.
            validate_destination_not_inside_source(&sources, &destination)?;
            // Create the destination folder (and any missing ancestors) when it
            // doesn't exist, so a copy into a brand-new folder just works.
            ensure_destination_dir(&destination)?;
            validate_destination_writable(&destination)?;
            copy_files_with_progress_inner(&*events, &op_id, &state, &sources, &destination, &config)
        },
    )
    .await
}

/// Starts a move operation in the background.
///
/// Uses instant rename() for same-filesystem moves.
/// Uses atomic staging pattern for cross-filesystem moves.
///
/// `expected_sources` is the caller's source binding, or `None` for the ordinary
/// user-started case. `source_binding.rs`.
#[allow(
    clippy::too_many_arguments,
    reason = "the manager's context alongside the op's own inputs"
)]
pub async fn move_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
    volume_ids: Vec<String>,
    lanes: Option<Vec<LaneKey>>,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!(
        "move_files_start: sources={:?}, destination={:?}, dry_run={}",
        sources,
        destination,
        config.dry_run
    );

    let lanes = lanes.unwrap_or_else(|| local_lanes(&volume_ids));
    let summary = path_summary(&sources, Some(&destination));
    start_write_operation(
        events,
        WriteOperationType::Move,
        initiator,
        config.progress_interval_ms,
        volume_ids,
        lanes,
        summary,
        config.preview_id.clone(),
        sources.len() as u64,
        move |events, op_id, state| {
            let Some(sources) = retain_bound_sources(
                &*events,
                &op_id,
                WriteOperationType::Move,
                expected_sources.as_ref(),
                sources,
            ) else {
                return Ok(());
            };
            validate_sources(&sources)?;
            // Guard against moving a folder into itself BEFORE creating anything:
            // the dest may not exist yet, and the guard resolves it via its nearest
            // existing ancestor.
            validate_destination_not_inside_source(&sources, &destination)?;
            // Create the destination folder (and any missing ancestors) when it
            // doesn't exist, so a move into a brand-new folder just works.
            ensure_destination_dir(&destination)?;
            validate_destination_writable(&destination)?;
            move_files_with_progress_inner(&*events, &op_id, &state, &sources, &destination, &config)
        },
    )
    .await
}

/// Starts a delete operation in the background.
///
/// Recursively deletes files and directories. When `volume_id` is provided and
/// is not the default volume, routes through `delete_volume_files_with_progress`
/// which uses the Volume trait (needed for MTP and other non-local volumes).
/// `expected_sources` is the caller's source binding, or `None` for the ordinary
/// user-started case. `source_binding.rs`.
pub async fn delete_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    config: WriteOperationConfig,
    volume_id: Option<String>,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let volume_id_str = volume_id.unwrap_or_else(|| "root".to_string());

    log::info!(
        "delete_files_start: sources={:?}, volume={}, dry_run={}",
        sources,
        volume_id_str,
        config.dry_run
    );

    // Deleting entries INSIDE a zip is a mutation: route to the managed archive-edit
    // driver as a single `{ delete }` changeset (a rewrite, not per-entry). The
    // `.zip` file itself is a regular file — deleting it stays on the normal path.
    // Parent-aware detection (not the `std::fs`-only sync predicate) so a delete
    // inside a REMOTE zip (direct SMB / MTP) also reaches the driver instead of
    // falling through to a confusing parent-volume delete.
    let first_is_archive_inner = match sources.first() {
        Some(s) => {
            crate::file_system::volume::manager::get_volume_manager()
                .path_is_inside_archive(&volume_id_str, s)
                .await
        }
        None => false,
    };
    if first_is_archive_inner {
        return archive_edit::route_archive_delete(
            events,
            &sources,
            &volume_id_str,
            config.progress_interval_ms,
            config.preview_id.clone(),
        )
        .await;
    }

    if volume_id_str != "root" {
        // Volume-aware delete: its body is async (the `Volume` trait's I/O is), so
        // it owns its own deferred start rather than riding `start_write_operation`.
        // `delete/volume_start.rs`.
        return Ok(delete::start_volume_delete(
            events,
            sources,
            config,
            volume_id_str,
            initiator,
            expected_sources,
        ));
    }
    {
        // Local same-`root` delete: no ejectable volume involved.
        let summary = path_summary(&sources, None);
        start_write_operation(
            events,
            WriteOperationType::Delete,
            initiator,
            config.progress_interval_ms,
            vec![],
            vec![LaneKey::new(crate::file_system::volume::DEFAULT_VOLUME_ID)],
            summary,
            config.preview_id.clone(),
            sources.len() as u64,
            move |events, op_id, state| {
                let Some(sources) = retain_bound_sources(
                    &*events,
                    &op_id,
                    WriteOperationType::Delete,
                    expected_sources.as_ref(),
                    sources,
                ) else {
                    return Ok(());
                };
                validate_sources(&sources)?;
                delete_files_with_progress_inner(&*events, &op_id, &state, &sources, &config)
            },
        )
        .await
    }
}

/// Starts a trash operation in the background.
///
/// Moves top-level items to the macOS Trash via `NSFileManager.trashItemAtURL`.
/// Supports cancellation between items and partial failure (some items may fail
/// while others succeed).
/// `expected_sources` is the caller's source binding, or `None` for the ordinary
/// user-started case. `source_binding.rs`.
///
/// ⚠️ `item_sizes` is positional, so the binding filters both halves together.
pub async fn trash_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    item_sizes: Option<Vec<u64>>,
    config: WriteOperationConfig,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!("trash_files_start: sources={:?}", sources);

    // ❌ Trash does NOT wait for the confirming dialog's scan preview, and this
    // is the one operation that doesn't. `trashItemAtURL` is atomic per
    // top-level item: trash walks nothing, so there is no second walk to
    // serialize against and no cached result to consume — waiting would be pure
    // delay, and on a big tree a long one. Nothing downstream will consume the
    // preview either, so free it here rather than leaving an ownerless walk to
    // finish for nobody and its result to sit until a TTL sweep. (The dialog
    // can't free it: it sets `confirmed` and deliberately skips its own
    // cleanup, because on the DELETE path the operation does consume it.)
    if let Some(preview_id) = &config.preview_id {
        cancel_scan_preview(preview_id);
    }

    // Trash always targets the local macOS Trash; no ejectable volume involved.
    let summary = path_summary(&sources, None);
    start_write_operation(
        events,
        WriteOperationType::Trash,
        initiator,
        config.progress_interval_ms,
        vec![],
        vec![LaneKey::new(crate::file_system::volume::DEFAULT_VOLUME_ID)],
        summary,
        None,
        sources.len() as u64,
        move |events, op_id, state| {
            let Some((sources, item_sizes)) = retain_bound_sources_with_sizes(
                &*events,
                &op_id,
                WriteOperationType::Trash,
                expected_sources.as_ref(),
                sources,
                item_sizes,
            ) else {
                return Ok(());
            };
            validate_sources(&sources)?;
            trash_files_with_progress(&*events, &op_id, &state, &sources, item_sizes.as_deref())
        },
    )
    .await
}

#[cfg(test)]
mod approved_op_parity_tests;
#[cfg(test)]
mod journal_capture_tests;
#[cfg(test)]
mod journal_capture_volume_tests;
// The source the cancel scenario holds still at a chunk boundary.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod network_gated_source_test_support;
// The transfer scenarios the WebDAV and SFTP suites below share, written once
// and driven against both live servers.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod network_transfer_test_support;
#[cfg(test)]
mod scan_bridge_tests;
#[cfg(test)]
mod scan_preview_listing_progress_tests;
#[cfg(test)]
mod scan_preview_oracle_tests;
#[cfg(test)]
mod scan_watchdog_tests;
#[cfg(test)]
mod settle_event_tests;
// Real copies in BOTH directions against a live SFTP server, through
// `copy_between_volumes`. Gated on the Docker fixture, and named for the
// `sftp_integration_` lane the check runner selects on.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod sftp_transfer_integration_test;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_integration_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod webdav_transfer_integration_test;
