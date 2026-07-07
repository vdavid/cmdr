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
mod conflict;
mod create;
mod delete;
mod durability;
mod error_classification;
mod eta;
mod event_sinks;
mod manager;
mod operation_intent;
mod overwrite;
#[cfg(target_os = "macos")]
mod paste_clipboard;
mod rename;
mod scan;
mod scan_cache;
mod scan_preview;
mod state;
mod transfer;
mod types;
mod validation;

// Re-export `macos_copy` at this level so existing call sites
// (`crate::file_system::write_operations::macos_copy`) keep compiling.
#[cfg(target_os = "macos")]
pub(crate) use transfer::macos_copy;

// Re-export `trash` at this level so `crate::file_system::write_operations::trash`
// keeps resolving (used by `commands/rename.rs`).
pub(crate) use delete::trash;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::file_system::volume::LaneKey;
use delete::{delete_files_with_progress_inner, delete_volume_files_with_progress_inner};
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
pub use event_sinks::{OperationEventSink, TauriEventSink};
#[cfg(not(test))]
use validation::{
    ensure_destination_dir, validate_destination_not_inside_source, validate_destination_writable,
    validate_not_same_location, validate_sources,
};

// Re-export public types
pub use scan_preview::{cancel_scan_preview, get_scan_preview_totals, start_scan_preview};
pub use state::{
    VolumesBusyChanged, busy_volume_ids, cancel_all_write_operations, cancel_write_operation, get_operation_status,
    init_busy_volume_emitter, list_active_operations, resolve_write_conflict,
};
// Operation manager: the single scheduler + registry every write op flows
// through. `OperationsChanged` / `OperationSnapshot` are the thin
// `operations-changed` event payload (the queue window consumes them; `LifecycleStatus` rides
// along as a snapshot field and is reached via `manager::LifecycleStatus`).
// `init_operation_event_emitter` wires the emitter at startup; the command
// helpers back the new `list_operations` / `cancel_operation(s)` IPC.
pub use manager::{
    OperationSnapshot, OperationSummaryText, OperationsChanged, cancel_operation, cancel_operations,
    init_operation_event_emitter, list_operations, pause_all, pause_operation, resume_all, resume_operation,
};
// Managed instant mutations (rename / mkdir / mkfile) + rename validation. The
// thin IPC commands (`commands/rename.rs`, `commands/file_system/write_ops.rs`)
// call these; `RenameValidityResult` rides into `bindings.ts` via the
// `check_rename_validity` command signature.
pub(crate) use create::{create_directory_managed, create_file_managed};
#[cfg(target_os = "macos")]
pub(crate) use paste_clipboard::write_payload_to_dir;
pub(crate) use rename::{
    RenameValidityResult, check_rename_permission_sync, check_rename_validity_impl, rename_managed,
};
// External busy-volume seam for the drag-out fulfillment service (see
// `state.rs` § "External busy-volume seam"). `pub(crate)` so only in-crate
// callers (`native_drag::fulfillment`) reach it. macOS-only: the sole consumer
// (`native_drag`) is `#[cfg(target_os = "macos")]`, so on other targets these
// would be dead code under `#![deny(unused)]`.
#[cfg(target_os = "macos")]
pub(crate) use state::{register_external_volume_op, release_external_volume_op};
#[allow(unused_imports, reason = "Public API re-exports for consumers of this module")]
pub use types::{
    ConflictInfo, ConflictResolution, DryRunResult, OperationStatus, OperationSummary, ScanPreviewCancelledEvent,
    ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent, ScanPreviewStartResult,
    ScanPreviewTotals, ScanProgressEvent, SortColumn, SortOrder, WriteCancelledEvent, WriteCompleteEvent,
    WriteConflictEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase,
    WriteOperationStartResult, WriteOperationType, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};

// Re-export for tests (these are pub(crate) in validation.rs and state.rs)
#[cfg(test)]
pub(crate) use state::{CopyTransaction, OperationIntent, WriteOperationState, is_cancelled, load_intent};
#[cfg(test)]
#[allow(unused_imports, reason = "Re-exports for test modules in file_system")]
pub(crate) use validation::{
    ensure_destination_dir, is_same_file, is_same_filesystem, validate_destination_not_inside_source,
    validate_destination_writable, validate_disk_space, validate_not_same_location, validate_path_length,
    validate_sources,
};
// Exposed for cross-module integration tests (for example the SMB
// concurrent-copy cross-contamination test in
// `file_system::volume::smb`) that drive `copy_volumes_with_progress`
// directly against a real SMB backend instead of the full Tauri path.
#[cfg(test)]
#[allow(unused_imports, reason = "Used by SMB integration tests in file_system::volume::smb")]
pub(crate) use transfer::volume_move::move_within_same_volume_with_progress;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by SMB integration tests in file_system::volume::smb")]
pub(crate) use types::CollectorEventSink;

// Re-export volume copy types and functions
pub use transfer::volume_copy::{copy_between_volumes, scan_for_volume_copy};
pub use transfer::volume_move::move_between_volumes;
pub use types::{VolumeCopyConfig, VolumeCopyScanResult};
// Copy/move INTO a zip: the command layer routes an archive destination here
// (the whole transfer becomes one `{ add }` changeset) instead of the per-file
// cross-volume engine.
pub(crate) use archive_edit::route_archive_copy_into;
// Move OUT of a zip: the command layer routes an archive SOURCE here. It runs a
// compound op — extract via the cross-volume copy engine, then (only on a fully
// clean extract) a batch `{ delete }` archive rewrite (the move invariant). See
// `archive_edit/` and DETAILS § "Archive edits".
pub(crate) use archive_edit::route_archive_move_out;
// The cross-volume copy body, reused as the extract phase of an out-of-zip MOVE
// (`route_archive_move_out`). Not spawn-managed itself — it runs inside the
// move-out op's deferred under the move op's id/state/sink.
pub(crate) use transfer::volume_copy::copy_volumes_with_progress;
// The remote zip-edit orchestration (pull-local, apply, upload, swap). Exposed at
// crate scope for the live-SMB / MTP integration suites, which drive the real
// mechanism against a real remote volume. The managed driver reaches it directly
// via `super::archive_remote_edit`, so this re-export is test-only.
#[cfg(test)]
pub(crate) use archive_remote_edit::{RemoteEditError, pull_apply_upload_swap};

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
    progress_interval_ms: u64,
    volume_ids: Vec<String>,
    lanes: Vec<LaneKey>,
    summary: OperationSummaryText,
    handler: F,
) -> Result<WriteOperationStartResult, WriteOperationError>
where
    F: FnOnce(Arc<dyn OperationEventSink>, String, Arc<WriteOperationState>) -> Result<(), WriteOperationError>
        + Send
        + 'static,
{
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type,
        lanes,
        volume_ids,
        summary,
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

            let op_id_for_blocking = op_id.clone();
            let events_for_handler = Arc::clone(&events);
            let result =
                tokio::task::spawn_blocking(move || handler(events_for_handler, op_id_for_blocking, state)).await;

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
/// substring parsing, so `no-string-matching` holds).
fn local_lanes(volume_ids: &[String]) -> Vec<LaneKey> {
    if volume_ids.is_empty() {
        vec![LaneKey::new(crate::file_system::volume::DEFAULT_VOLUME_ID)]
    } else {
        volume_ids.iter().cloned().map(LaneKey::new).collect()
    }
}

/// Best-effort `source → destination` summary for the queue window: the source
/// items' display names joined, and the destination's. Cheap; no I/O.
fn path_summary(sources: &[PathBuf], destination: Option<&std::path::Path>) -> OperationSummaryText {
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
pub async fn copy_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
    volume_ids: Vec<String>,
    lanes: Option<Vec<LaneKey>>,
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
        config.progress_interval_ms,
        volume_ids,
        lanes,
        summary,
        move |events, op_id, state| {
            validate_sources(&sources)?;
            // Guard against copying a folder into itself BEFORE creating anything:
            // the dest may not exist yet, and the guard resolves it via its nearest
            // existing ancestor.
            validate_destination_not_inside_source(&sources, &destination)?;
            // Create the destination folder (and any missing ancestors) when it
            // doesn't exist, so a copy into a brand-new folder just works.
            ensure_destination_dir(&destination)?;
            validate_destination_writable(&destination)?;
            validate_not_same_location(&sources, &destination)?;
            copy_files_with_progress_inner(&*events, &op_id, &state, &sources, &destination, &config)
        },
    )
    .await
}

/// Starts a move operation in the background.
///
/// Uses instant rename() for same-filesystem moves.
/// Uses atomic staging pattern for cross-filesystem moves.
pub async fn move_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
    volume_ids: Vec<String>,
    lanes: Option<Vec<LaneKey>>,
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
        config.progress_interval_ms,
        volume_ids,
        lanes,
        summary,
        move |events, op_id, state| {
            validate_sources(&sources)?;
            // Guard against moving a folder into itself BEFORE creating anything:
            // the dest may not exist yet, and the guard resolves it via its nearest
            // existing ancestor.
            validate_destination_not_inside_source(&sources, &destination)?;
            // Create the destination folder (and any missing ancestors) when it
            // doesn't exist, so a move into a brand-new folder just works.
            ensure_destination_dir(&destination)?;
            validate_destination_writable(&destination)?;
            validate_not_same_location(&sources, &destination)?;
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
pub async fn delete_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    config: WriteOperationConfig,
    volume_id: Option<String>,
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
            crate::file_system::get_volume_manager()
                .path_is_inside_archive(&volume_id_str, s)
                .await
        }
        None => false,
    };
    if first_is_archive_inner {
        return archive_edit::route_archive_delete(events, &sources, &volume_id_str, config.progress_interval_ms).await;
    }

    if volume_id_str != "root" {
        // Volume-aware delete (async handler): route through the manager via a
        // deferred async start. The lane is the volume's own lane (resolved
        // from its `Volume::lane_key()`); falls back to the volume id if the
        // volume isn't registered yet (it'll surface the not-found error on
        // admission). The manager owns lifecycle, cache cleanup, and the busy
        // registration; this closure owns the op body + terminal emit + settle.
        let operation_id = Uuid::new_v4().to_string();
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(
            config.progress_interval_ms,
        )));

        let lane = crate::file_system::get_volume_manager()
            .get(&volume_id_str)
            .map(|v| v.lane_key())
            .unwrap_or_else(|| LaneKey::new(volume_id_str.clone()));

        let descriptor = OperationDescriptor {
            operation_id: operation_id.clone(),
            operation_type: WriteOperationType::Delete,
            lanes: vec![lane],
            volume_ids: vec![volume_id_str.clone()],
            summary: path_summary(&sources, None),
        };

        let events_for_op = Arc::clone(&events);
        let op_id_outer = operation_id.clone();
        let state_for_op = Arc::clone(&state);
        let volume_id_for_op = volume_id_str.clone();
        let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                let events = events_for_op;
                let op_id = op_id_outer;
                let state = state_for_op;
                let volume_id_str = volume_id_for_op;
                let task_guard = manager::ManagedTaskGuard::new(op_id.clone());
                // Settle guard: fires `write-settled` at end of scope, AFTER the
                // terminal event and AFTER `on_settled`'s cache cleanup (the
                // settle guard drops last). Matches the FE ordering contract.
                let _settled_guard = WriteSettledGuard::new(
                    Arc::clone(&events),
                    op_id.clone(),
                    WriteOperationType::Delete,
                    Some(volume_id_str.clone()),
                );

                match crate::file_system::get_volume_manager().get(&volume_id_str) {
                    None => {
                        events.emit_error(WriteErrorEvent::new(
                            op_id.clone(),
                            WriteOperationType::Delete,
                            WriteOperationError::IoError {
                                path: volume_id_str.clone(),
                                message: format!("Volume '{}' not found", volume_id_str),
                            },
                        ));
                    }
                    Some(volume) => {
                        let result = delete_volume_files_with_progress_inner(
                            volume,
                            &volume_id_str,
                            &*events,
                            &op_id,
                            &state,
                            &sources,
                            &config,
                        )
                        .await;
                        match result {
                            Ok(()) => {}
                            Err(ref e) if matches!(e, WriteOperationError::Cancelled { .. }) => {}
                            Err(e) => {
                                events.emit_error(WriteErrorEvent::new(op_id.clone(), WriteOperationType::Delete, e));
                            }
                        }
                    }
                }

                task_guard.disarm();
                manager::manager().on_settled(&op_id);
            })
        };

        manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

        Ok(WriteOperationStartResult {
            operation_id,
            operation_type: WriteOperationType::Delete,
        })
    } else {
        // Local same-`root` delete: no ejectable volume involved.
        let summary = path_summary(&sources, None);
        start_write_operation(
            events,
            WriteOperationType::Delete,
            config.progress_interval_ms,
            vec![],
            vec![LaneKey::new(crate::file_system::volume::DEFAULT_VOLUME_ID)],
            summary,
            move |events, op_id, state| {
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
pub async fn trash_files_start(
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    item_sizes: Option<Vec<u64>>,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!("trash_files_start: sources={:?}", sources);

    // Trash always targets the local macOS Trash; no ejectable volume involved.
    let summary = path_summary(&sources, None);
    start_write_operation(
        events,
        WriteOperationType::Trash,
        config.progress_interval_ms,
        vec![],
        vec![LaneKey::new(crate::file_system::volume::DEFAULT_VOLUME_ID)],
        summary,
        move |events, op_id, state| {
            validate_sources(&sources)?;
            trash_files_with_progress(&*events, &op_id, &state, &sources, item_sizes.as_deref())
        },
    )
    .await
}

#[cfg(test)]
mod scan_preview_listing_progress_tests;
#[cfg(test)]
mod scan_preview_oracle_tests;
#[cfg(test)]
mod settle_event_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_integration_test;
