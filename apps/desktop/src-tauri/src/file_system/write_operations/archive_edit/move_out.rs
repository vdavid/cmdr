//! The MOVE whose SOURCE is inside a zip: a compound op that extracts the
//! selected entries through the ordinary cross-volume copy engine, then — only on
//! a fully clean extract — rewrites the archive with a batch `{ delete }`. The
//! archive-side delete runs ONLY after every destination file is durably
//! committed, so a crash or cancel can never lose both copies (all-or-nothing).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use uuid::Uuid;

use super::super::OperationEventSink;
use super::super::manager::{self, ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::super::operation_intent::is_cancelled;
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::types::{
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteErrorEvent, WriteOperationError,
    WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
};
use super::engine::{MutatorHooks, PlanError, run_managed_edit, to_write_error};
use super::routing::{ensure_zip_writable, normalize_inner_path, read_only_error};
use crate::file_system::volume::Volume;
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::mutator::{self, Changeset, MutationError};
use crate::ignore_poison::IgnorePoison;

/// Routes a MOVE whose SOURCE is inside a zip (extract-out + delete-inside) to a
/// single managed compound op. The op extracts the selected entries to the
/// destination through the ordinary cross-volume copy engine, then — only if the
/// whole extract landed cleanly — rewrites the archive with a `{ delete }` of
/// those entries. This is the MOVE INVARIANT for archives: the archive-side
/// delete runs ONLY after every destination file is durably committed (the copy
/// engine's `write_from_stream` fsyncs each file), so a crash or cancel can never
/// lose both copies.
///
/// **Partial-move policy: batch all-or-nothing (skip / error / cancel aware).**
/// The archive entries are deleted ONLY when the extract completed with zero
/// skips, zero errors, and no cancel. Any skip (a destination collision the
/// policy resolved to Skip), any failure, or a cancel leaves the archive
/// byte-for-byte intact and deletes nothing — the landed copies stay at the
/// destination and the move degrades to a copy for that run. The archive delete
/// is itself one atomic O(archive) rewrite, so batch granularity is the natural
/// unit and this sidesteps the partial-merge-skip hazard (a directory source with
/// an inner skipped child must not have its whole subtree deleted). Per-entry
/// deletion is a future refinement.
///
/// `source_volume` is the read-only `ArchiveVolume`; `source_volume_id` is the
/// parent drive's id (the FE tab volume) used for the lane + busy set. All
/// `source_paths` must live in ONE archive (a multi-select can't straddle zips in
/// one pane).
#[allow(
    clippy::too_many_arguments,
    reason = "the archive-source→dest move threads both volumes with their ids plus the resolved dest path and config; a struct would just shuffle the same fields"
)]
pub(crate) async fn route_archive_move_out(
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: crate::file_system::VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Resolve the one archive every source belongs to, and the inner path of each
    // (the `{ delete }` changeset needs archive-root-relative names).
    let first = source_paths.first().ok_or_else(|| WriteOperationError::IoError {
        path: String::new(),
        message: "no entries to move".to_string(),
    })?;
    let (archive_path, _) = archive::archive_boundary_candidate(first).ok_or_else(|| read_only_error(first))?;
    // Moving OUT of a read-only archive would need to DELETE the source entries;
    // tar/7z can't be edited, so refuse (copy-out still works via the read path).
    ensure_zip_writable(&archive_path)?;
    let mut inner_deletes = Vec::with_capacity(source_paths.len());
    for source in &source_paths {
        let (source_archive, inner) =
            archive::archive_boundary_candidate(source).ok_or_else(|| read_only_error(source))?;
        if source_archive != archive_path {
            return Err(WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "can't move entries out of more than one archive at once".to_string(),
            });
        }
        inner_deletes.push(normalize_inner_path(&inner));
    }

    let operation_id = Uuid::new_v4().to_string();
    let progress_interval_ms = config.progress_interval_ms;
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    // The op holds BOTH lanes (the archive's parent drive AND the destination),
    // so it serializes against other work on either device. The archive delete
    // runs on the parent drive's lane, which this op already owns.
    let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
    let summary = OperationSummaryText {
        source: Some(source_volume.name().to_string()),
        destination: Some(dest_volume.name().to_string()),
    };
    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Move,
        lanes,
        volume_ids: vec![source_volume_id.clone(), dest_volume_id],
        summary,
    };

    let events_for_op = Arc::clone(&events);
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let progress_interval = Duration::from_millis(progress_interval_ms);

    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = ManagedTaskGuard::new(op_id.clone());
            let settle_volume = (source_volume_id != "root").then(|| source_volume_id.clone());
            let _settled = WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Move,
                settle_volume,
            );

            // Phase 1 — extract. The suppressing sink forwards progress/conflict
            // to the FE but withholds the copy's terminal events (the compound op
            // owns the single Move terminal); it captures the completion so we can
            // read `files_skipped`.
            let suppress = Arc::new(SuppressTerminalsSink {
                inner: Arc::clone(&events),
                captured_complete: Mutex::new(None),
            });
            let extract = crate::file_system::write_operations::copy_volumes_with_progress(
                Arc::clone(&suppress) as Arc<dyn OperationEventSink>,
                &op_id,
                &state,
                Arc::clone(&source_volume),
                &source_paths,
                Arc::clone(&dest_volume),
                &dest_path,
                &config,
            )
            .await;

            let captured = suppress.captured_complete.lock_ignore_poison().take();
            let files_extracted = captured.as_ref().map(|c| c.files_processed).unwrap_or(0);
            let bytes_extracted = captured.as_ref().map(|c| c.bytes_processed).unwrap_or(0);
            let files_skipped = captured.as_ref().map(|c| c.files_skipped).unwrap_or(0);

            // A cancel that landed in the window between a clean extract and the
            // delete phase must still leave the archive untouched.
            if is_cancelled(&state.intent) {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: op_id.clone(),
                    operation_type: WriteOperationType::Move,
                    files_processed: files_extracted,
                    rolled_back: false,
                });
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

            match extract {
                Ok(()) if files_skipped == 0 => {
                    // Phase 2 — durable extract confirmed; rewrite the archive to
                    // drop the moved entries (the move invariant is satisfied: the
                    // copy engine fsynced every destination file before returning).
                    let changeset = Changeset {
                        deletes: inner_deletes,
                        ..Default::default()
                    };
                    let hooks = Arc::new(MutatorHooks::new(
                        Arc::clone(&state),
                        Arc::clone(&events),
                        op_id.clone(),
                        WriteOperationType::Move,
                        progress_interval,
                    ));
                    let hooks_for_blocking = Arc::clone(&hooks);
                    let delete_result = run_managed_edit(
                        &source_volume_id,
                        archive_path.clone(),
                        Arc::clone(&state),
                        move |working: &Path| {
                            mutator::apply(working, &changeset, &*hooks_for_blocking).map_err(|e| match e {
                                MutationError::Cancelled => PlanError::Cancelled,
                                other => PlanError::Op(to_write_error(working, other)),
                            })
                        },
                    )
                    .await;
                    match delete_result {
                        Ok(()) => {
                            events.emit_complete(WriteCompleteEvent {
                                operation_id: op_id.clone(),
                                operation_type: WriteOperationType::Move,
                                files_processed: files_extracted,
                                files_skipped: 0,
                                bytes_processed: bytes_extracted,
                            });
                        }
                        Err(PlanError::Cancelled) => {
                            // Cancelled during the archive rewrite: the extract
                            // already landed, the archive is intact (temp
                            // abandoned) — effectively a completed copy.
                            events.emit_cancelled(WriteCancelledEvent {
                                operation_id: op_id.clone(),
                                operation_type: WriteOperationType::Move,
                                files_processed: files_extracted,
                                rolled_back: false,
                            });
                        }
                        Err(PlanError::Op(err)) => {
                            // The extract landed but the archive couldn't be
                            // rewritten. The originals are intact and the copies
                            // are at the destination (no data loss), so surface
                            // the failure — the move degraded to a copy.
                            events.emit_error(WriteErrorEvent::new(op_id.clone(), WriteOperationType::Move, err));
                        }
                    }
                }
                Ok(()) => {
                    // Extract completed but something was skipped (a destination
                    // collision the policy resolved to Skip). All-or-nothing:
                    // delete nothing, leave the archive intact. The landed copies
                    // stay at the destination.
                    events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        files_skipped,
                        bytes_processed: bytes_extracted,
                    });
                }
                Err(failure) if matches!(failure.error, WriteOperationError::Cancelled { .. }) => {
                    // Cancel mid-extract: nothing was deleted, the archive is
                    // untouched. The copy engine already emitted nothing terminal
                    // (suppressed), so emit the Move cancel here.
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        rolled_back: false,
                    });
                }
                Err(failure) => {
                    // Extract failed: nothing deleted, archive untouched.
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        WriteOperationType::Move,
                        failure.error,
                    ));
                }
            }

            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}

/// Wraps the real event sink for the extract phase of an out-of-zip move: it
/// forwards every non-terminal event (progress, conflict prompts, scan) to the FE
/// but WITHHOLDS the copy's terminal events (`write-complete` / `write-cancelled`
/// / `write-error`) — the compound move op emits the single Move terminal itself.
/// The completion is captured so the driver can read `files_skipped`.
struct SuppressTerminalsSink {
    inner: Arc<dyn OperationEventSink>,
    captured_complete: Mutex<Option<WriteCompleteEvent>>,
}

impl OperationEventSink for SuppressTerminalsSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        *self.captured_complete.lock_ignore_poison() = Some(event);
    }
    fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
    fn emit_error(&self, _event: WriteErrorEvent) {}
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.inner.emit_conflict(event);
    }
    fn emit_source_item_done(&self, event: super::super::types::WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
    }
    fn emit_scan_progress(&self, event: super::super::types::ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: super::super::types::ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: super::super::types::DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn emit_settled(&self, event: super::super::types::WriteSettledEvent) {
        self.inner.emit_settled(event);
    }
}
