//! Starting a copy, move, delete, or trash on the local filesystem: the four
//! local starters and the managed spawn they share (`start_write_operation`).
//! The same-`root` move, delete, and trash reach them through
//! `write_start.rs`, which asks the host for its local disk first. Each is an
//! [`Operations`] method.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::mac_only_files::MacOnlyFiles;
use cmdr_fs::volume::LaneKey;
use cmdr_operation_log::types::{Initiator, OperationId};

use super::delete::already_gone::{self, AlreadyGoneSink};
use super::delete::delete_files_with_progress_inner;
use super::delete::trash::trash_files_with_progress;
use super::manager::{self, OperationDescriptor, OperationSummaryText};
use super::source_binding::{ExpectedSources, retain_bound_sources, retain_bound_sources_with_sizes};
use super::state::{WriteOperationState, WriteSettledGuard};
use super::transfer::copy::copy_files_with_progress_inner;
use super::transfer::move_op::move_files_with_progress_inner;
use super::types::{
    ConflictResolution, RollbackSupport, WriteErrorEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationStartResult, WriteOperationType,
};
use super::validation::{
    ensure_destination_dir, validate_destination_not_inside_source, validate_destination_writable,
    validate_source_names_are_distinct, validate_sources,
};
use super::{OperationEventSink, Operations, journal, path_summary, scan_bridge, transfer_sides};

// ============================================================================
// The starters
// ============================================================================

impl Operations {
    /// Spawns a write operation in the background with state management and panic handling.
    ///
    /// Creates `WriteOperationState`, registers the operation, spawns `tokio::spawn` +
    /// `spawn_blocking`, and handles cleanup and panic recovery. Callers do validation
    /// and logging before calling this, then pass a closure for the actual work.
    #[allow(
        clippy::too_many_arguments,
        reason = "the managed-spawn entry point threads lane keys + summary + volume ids alongside the handler; bundling them would just shuffle fields into a struct at every call site"
    )]
    pub(super) async fn start_write_operation<F>(
        &self,
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
        // The transfer's two volumes, captured here because a drive that vanishes
        // can't be named afterwards (`transfer_sides.rs`). `None` for an operation
        // with only one side (a delete, a trash).
        sides: Option<transfer_sides::TransferSides>,
        // What a copy or move does with a name already taken at the destination,
        // as the snapshot reports it (`conflict_policy_cell.rs`). `None` for an
        // operation with no destination to clash at.
        conflict_policy: Option<ConflictResolution>,
        // What a copy or move does with Mac-only files (`WriteOperationState::mac_only`).
        mac_only: MacOnlyFiles,
        handler: F,
    ) -> Result<WriteOperationStartResult, WriteOperationError>
    where
        F: FnOnce(Arc<dyn OperationEventSink>, String, Arc<WriteOperationState>) -> Result<(), WriteOperationError>
            + Send
            + 'static,
    {
        let operation_id = OperationId::from(cmdr_operation_log::new_operation_id());
        let mut state = self
            .new_operation_state(Duration::from_millis(progress_interval_ms))
            .with_sides(sides)
            .leaving_out(mac_only);
        if let Some(policy) = conflict_policy {
            state = state.with_conflict_policy(policy);
        }
        let state = Arc::new(state);

        let descriptor = OperationDescriptor {
            operation_id: operation_id.clone(),
            operation_type,
            lanes,
            volume_ids,
            summary,
            rollback_support: local_rollback_support(operation_type),
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
        let deferred = move |ops: Operations| -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                let events = events_for_op;
                let op_id = operation_id_for_op;
                let state = state_for_op;
                let task_guard = manager::ManagedTaskGuard::new(ops.manager(), op_id.clone());
                // RAII guard: emits `write-settled` when this task exits, no matter
                // how (handler success, error, cancel, or panic via JoinError). FE
                // gates the "Cancelling…" dialog close on this event so the user
                // can't dispatch a new op against a still-tearing-down volume.
                let _settled_guard =
                    WriteSettledGuard::new(Arc::clone(&events), op_id.to_string(), operation_type, None);

                // Wait out the confirming dialog's scan before any journal row or
                // any I/O: an op that never got past its scan wrote nothing, so
                // there's nothing to journal and its terminal event is already out.
                if scan_bridge::await_claimed_preview(&ops, &*events, &op_id, operation_type, &state)
                    .await
                    .stopped()
                {
                    task_guard.disarm();
                    ops.manager().on_settled(&op_id);
                    return;
                }

                // Open the journal row when the op actually starts (not at
                // registration), so a queued op that's canceled before admission
                // never journals. A local copy/move lands its destination on the
                // local FS ("root"); a delete/trash has no destination volume.
                let op_kind = journal::op_kind_of(operation_type);
                let dest_volume_id = match operation_type {
                    WriteOperationType::Copy | WriteOperationType::Move => Some(cmdr_fs::volume::DEFAULT_VOLUME_ID),
                    _ => None,
                };
                journal::open_local_op(
                    state.scope(),
                    op_id.as_str(),
                    op_kind,
                    initiator,
                    item_count,
                    dest_volume_id,
                );

                let op_id_for_blocking = op_id.to_string();
                let events_for_handler = Arc::clone(&events);
                // Kept out of the handler's move so the safety net below can still
                // read the operation's captured sides: a transfer that returned an
                // error without emitting one itself gets the vanished drive named
                // here, the same way the engines do.
                let state_for_net = Arc::clone(&state);
                let result = ops
                    .host()
                    .runtime()
                    .spawn_blocking(move || handler(events_for_handler, op_id_for_blocking, state))
                    .await;

                // Journal the terminal state BEFORE cache cleanup (finalize computes
                // eligibility + the completeness downgrade from what actually
                // happened). A failed/canceled op stays rollbackable for the items it
                // reached.
                let execution_status = match &result {
                    Ok(Ok(())) => cmdr_operation_log::types::ExecutionStatus::Done,
                    Ok(Err(WriteOperationError::Cancelled { .. })) => {
                        cmdr_operation_log::types::ExecutionStatus::Canceled
                    }
                    _ => cmdr_operation_log::types::ExecutionStatus::Failed,
                };
                journal::finalize_op(state_for_net.scope(), op_kind, execution_status);

                match result {
                    Ok(Ok(())) => {} // Handler already emitted write-complete or write-cancelled
                    Ok(Err(ref e)) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                        // Handler already emitted write-cancelled
                    }
                    Ok(Err(e)) => {
                        // Handler error (validation, I/O, etc.): emit write-error as safety net
                        events.emit_error(transfer_sides::transfer_stop_event(
                            op_id.as_str(),
                            operation_type,
                            &state_for_net,
                            e,
                            None,
                        ));
                    }
                    Err(join_error) => {
                        // Panic/abort in spawn_blocking
                        events.emit_error(WriteErrorEvent::new(
                            op_id.to_string(),
                            operation_type,
                            WriteOperationError::IoError {
                                path: String::new(),
                                message: format!("Task failed: {}", join_error).into(),
                            },
                        ));
                    }
                }

                // Happy-path dequeue: free lanes, clean caches, admit next. Order:
                // terminal event → `on_settled` (cache removal) → `write-settled`
                // via the settle guard's Drop at end of scope. Disarm the panic
                // guard first so its Drop doesn't redo the (now-done) cleanup.
                task_guard.disarm();
                ops.manager().on_settled(&op_id);
            })
        };

        self.manager().spawn_managed(descriptor, state, Box::new(deferred));

        Ok(WriteOperationStartResult {
            operation_id: operation_id.to_string(),
            operation_type,
        })
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
    pub(crate) async fn copy_files_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        destination: PathBuf,
        config: WriteOperationConfig,
        volume_ids: Vec<String>,
        lanes: Option<Vec<LaneKey>>,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
        sides: Option<transfer_sides::TransferSides>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        log::info!(
            "copy_files_start: sources={:?}, destination={:?}, dry_run={}",
            sources.iter().map(LogPrivate::path).collect::<Vec<_>>(),
            LogPrivate::dir_path(&destination),
            config.dry_run
        );

        let lanes = lanes.unwrap_or_else(|| local_lanes(&volume_ids));
        let summary = path_summary(&sources, Some(&destination), config.source_kinds);
        self.start_write_operation(
            events,
            WriteOperationType::Copy,
            initiator,
            config.progress_interval_ms,
            volume_ids,
            lanes,
            summary,
            config.preview_id.clone(),
            sources.len() as u64,
            sides,
            Some(config.conflict_resolution),
            config.mac_only_files,
            move |events, op_id, state| {
                let Some(sources) = retain_bound_sources(
                    &*events,
                    &op_id,
                    WriteOperationType::Copy,
                    &state,
                    expected_sources.as_ref(),
                    sources,
                ) else {
                    return Ok(());
                };
                validate_sources(&sources)?;
                validate_source_names_are_distinct(&sources)?;
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

    /// Starts a local move in the background, its leaves already validated by
    /// the start that reached it ([`Self::move_files_start`], or the routed
    /// start through the both-local facade).
    ///
    /// Uses instant rename() for same-filesystem moves.
    /// Uses atomic staging pattern for cross-filesystem moves.
    #[allow(
        clippy::too_many_arguments,
        reason = "the manager's context alongside the op's own inputs"
    )]
    pub(crate) async fn local_move_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        destination: PathBuf,
        config: WriteOperationConfig,
        volume_ids: Vec<String>,
        lanes: Option<Vec<LaneKey>>,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
        sides: Option<transfer_sides::TransferSides>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        log::info!(
            "move_files_start: sources={:?}, destination={:?}, dry_run={}",
            sources.iter().map(LogPrivate::path).collect::<Vec<_>>(),
            LogPrivate::dir_path(&destination),
            config.dry_run
        );

        let lanes = lanes.unwrap_or_else(|| local_lanes(&volume_ids));
        let summary = path_summary(&sources, Some(&destination), config.source_kinds);
        self.start_write_operation(
            events,
            WriteOperationType::Move,
            initiator,
            config.progress_interval_ms,
            volume_ids,
            lanes,
            summary,
            config.preview_id.clone(),
            sources.len() as u64,
            sides,
            Some(config.conflict_resolution),
            config.mac_only_files,
            move |events, op_id, state| {
                let Some(sources) = retain_bound_sources(
                    &*events,
                    &op_id,
                    WriteOperationType::Move,
                    &state,
                    expected_sources.as_ref(),
                    sources,
                ) else {
                    return Ok(());
                };
                validate_sources(&sources)?;
                validate_source_names_are_distinct(&sources)?;
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

    /// The local half of [`Self::delete_files_start`]: a delete on the local
    /// disk, after the start's archive and routed-path checks.
    pub(super) async fn local_delete_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        config: WriteOperationConfig,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        {
            // Local same-`root` delete: no ejectable volume involved.
            let summary = path_summary(&sources, None, config.source_kinds);
            self.start_write_operation(
                events,
                WriteOperationType::Delete,
                initiator,
                config.progress_interval_ms,
                vec![],
                vec![LaneKey::new(cmdr_fs::volume::DEFAULT_VOLUME_ID)],
                summary,
                config.preview_id.clone(),
                sources.len() as u64,
                // One side only: a delete has no destination volume.
                None,
                None,
                MacOnlyFiles::Include,
                move |events, op_id, state| {
                    // An item that's already gone is done (`delete/already_gone.rs`).
                    let split = already_gone::split_local(&*events, &op_id, &sources);
                    if split.kept.is_empty()
                        && let Some(gone) = split.already_gone
                    {
                        already_gone::complete_all_gone(&*events, &op_id, WriteOperationType::Delete, gone);
                        return Ok(());
                    }
                    let sources = split.select(&sources);
                    let events = AlreadyGoneSink::new(&*events, split.already_gone);
                    let Some(sources) = retain_bound_sources(
                        &events,
                        &op_id,
                        WriteOperationType::Delete,
                        &state,
                        expected_sources.as_ref(),
                        sources,
                    ) else {
                        return Ok(());
                    };
                    validate_sources(&sources)?;
                    delete_files_with_progress_inner(&events, &op_id, &state, &sources, &config)
                },
            )
            .await
        }
    }

    /// The local half of [`Self::trash_files_start`]: the OS trash, after the
    /// start freed its preview and checked for routed paths.
    pub(super) async fn local_trash_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        item_sizes: Option<Vec<u64>>,
        config: WriteOperationConfig,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        // Trash always targets the local macOS Trash; no ejectable volume involved.
        let summary = path_summary(&sources, None, config.source_kinds);
        self.start_write_operation(
            events,
            WriteOperationType::Trash,
            initiator,
            config.progress_interval_ms,
            vec![],
            vec![LaneKey::new(cmdr_fs::volume::DEFAULT_VOLUME_ID)],
            summary,
            None,
            sources.len() as u64,
            // One side only: trash always lands in the local macOS Trash.
            None,
            None,
            MacOnlyFiles::Include,
            move |events, op_id, state| {
                // An item that's already gone is done (`delete/already_gone.rs`).
                let split = already_gone::split_local(&*events, &op_id, &sources);
                if split.kept.is_empty()
                    && let Some(gone) = split.already_gone
                {
                    already_gone::complete_all_gone(&*events, &op_id, WriteOperationType::Trash, gone);
                    return Ok(());
                }
                let sources = split.select(&sources);
                let item_sizes = item_sizes.map(|sizes| split.select(&sizes));
                let events = AlreadyGoneSink::new(&*events, split.already_gone);
                let Some((sources, item_sizes)) = retain_bound_sources_with_sizes(
                    &events,
                    &op_id,
                    WriteOperationType::Trash,
                    &state,
                    expected_sources.as_ref(),
                    sources,
                    item_sizes,
                ) else {
                    return Ok(());
                };
                validate_sources(&sources)?;
                trash_files_with_progress(&events, &op_id, &state, &sources, item_sizes.as_deref())
            },
        )
        .await
    }
}

/// A local copy/move writes NEW files it can delete again on rollback
/// (`CopyTransaction` / `MoveTransaction`); a delete or trash has nothing to put
/// back. Only those four reach `start_write_operation`, but the match stays
/// exhaustive so a new operation type has to decide.
fn local_rollback_support(operation_type: WriteOperationType) -> RollbackSupport {
    match operation_type {
        WriteOperationType::Copy | WriteOperationType::Move => RollbackSupport::Supported,
        WriteOperationType::Delete | WriteOperationType::Trash => RollbackSupport::NothingToPutBack,
        WriteOperationType::Rename
        | WriteOperationType::CreateFolder
        | WriteOperationType::CreateFile
        | WriteOperationType::Tag => RollbackSupport::InstantChange,
        WriteOperationType::ArchiveEdit | WriteOperationType::Compress => RollbackSupport::ArchiveRewrite,
    }
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
        vec![LaneKey::new(cmdr_fs::volume::DEFAULT_VOLUME_ID)]
    } else {
        volume_ids.iter().cloned().map(LaneKey::new).collect()
    }
}
