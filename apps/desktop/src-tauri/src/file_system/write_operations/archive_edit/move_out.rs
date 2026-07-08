//! The MOVE whose SOURCE is inside a zip: a compound op that extracts the
//! selected entries through the ordinary cross-volume copy engine, then rewrites
//! the archive with a batch `{ delete }` of exactly the sources that extracted in
//! FULL. The archive-side delete runs ONLY on durably-committed, non-rolled-back
//! extractions, so a crash or cancel can never lose both copies, and a
//! partially-interrupted move CONVERGES on retry instead of restarting.

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
/// destination through the ordinary cross-volume copy engine, then rewrites the
/// archive with a `{ delete }` of exactly the sources that extracted in FULL.
/// This is the MOVE INVARIANT for archives: an entry is deleted ONLY after its
/// destination copy is durably committed (the copy engine fsyncs each file) AND
/// won't be rolled back, so a crash can never lose both copies.
///
/// **Partial-move policy: per-source convergence.** The copy engine reports
/// (via `note_source_landed_clean`) each top-level source that extracted with
/// zero deep skips; the batch `{ delete }` drops exactly those. So:
/// - A source with a deep-merge skip stays in the archive — deleting its subtree
///   would drop the un-landed child (the partial-merge-skip hazard). The DEEP
///   skip is what the copy engine now counts (`CreatedPaths::skipped_file_count`);
///   an uncounted deep skip would let the old all-or-nothing gate delete the whole
///   subtree and lose data.
/// - On a HARD error the durable PREFIX (sources that completed before the
///   failure) is deleted, so a retry moves only the remainder — the move converges
///   instead of restarting from zero.
/// - On CANCEL or ROLLBACK nothing is deleted from the archive: cancel matches the
///   plain cross-volume move (its source-delete never runs on cancel), and a
///   rollback deletes the dest copies so nothing durable remains to move out.
///
/// The delete is still ONE atomic O(archive) rewrite over the converged subset (a
/// directory source deletes by prefix), never n per-entry rewrites.
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
    // The inner (archive-root-relative) path of each top-level source, kept
    // paired with the source so the delete batch can be filtered to exactly the
    // sources that extract in full.
    let mut inner_by_source: Vec<(PathBuf, String)> = Vec::with_capacity(source_paths.len());
    for source in &source_paths {
        let (source_archive, inner) =
            archive::archive_boundary_candidate(source).ok_or_else(|| read_only_error(source))?;
        if source_archive != archive_path {
            return Err(WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "can't move entries out of more than one archive at once".to_string(),
            });
        }
        inner_by_source.push((source.clone(), normalize_inner_path(&inner)));
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
                landed_clean: Mutex::new(Vec::new()),
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

            // Cancel / rollback: leave the archive byte-for-byte intact. This
            // matches the plain cross-volume move, whose source-delete phase never
            // runs on cancel — the least-surprising outcome is "nothing was removed
            // from my archive". The extract already kept (cancel) or rolled back
            // (rollback) its own destination copies. A cancel can land mid-extract
            // (the extract returns `Cancelled`) or in the window after a clean
            // extract (`is_cancelled` true, extract `Ok`); both route here.
            let cancelled = is_cancelled(&state.intent)
                || matches!(&extract, Err(f) if matches!(f.error, WriteOperationError::Cancelled { .. }));
            if cancelled {
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

            // Build the delete batch: the inner paths of exactly the sources that
            // extracted in full. On `Ok` that's every non-skipped source; on a hard
            // error it's the durable prefix that completed before the failure. A
            // source with a deep skip, one that errored, or one never reached stays
            // in the archive — deleting it would drop an un-landed child (data
            // loss). This is the per-source refinement of the old all-or-nothing
            // batch: still ONE O(archive) rewrite, but over the converged subset.
            // `suppress` collected the top-level sources that extracted in FULL
            // (every file durably written, zero deep skips) via its
            // `note_source_landed_clean` override.
            let landed = suppress.landed_clean.lock_ignore_poison().clone();
            let inner_deletes: Vec<String> = inner_by_source
                .iter()
                .filter(|(src, _)| landed.contains(src))
                .map(|(_, inner)| inner.clone())
                .collect();
            let extract_error = match &extract {
                Ok(()) => None,
                Err(f) => Some(f.error.clone()),
            };

            // Nothing extracted cleanly (every source skipped, or the first source
            // errored): leave the archive intact and surface the outcome.
            if inner_deletes.is_empty() {
                match extract_error {
                    None => events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        files_skipped,
                        bytes_processed: bytes_extracted,
                    }),
                    Some(err) => events.emit_error(WriteErrorEvent::new(op_id.clone(), WriteOperationType::Move, err)),
                }
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

            // Phase 2 — rewrite the archive to drop the fully-extracted sources.
            // The MOVE INVARIANT holds: the copy engine fsynced every destination
            // file, and cancel/rollback was handled above, so no entry is deleted
            // whose bytes aren't durably on disk and staying there.
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
                Ok(()) => match extract_error {
                    // Fully clean, or a partial converge (some sources skipped):
                    // the moved entries are gone from the archive.
                    None => events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        files_skipped,
                        bytes_processed: bytes_extracted,
                    }),
                    // The durable prefix moved out, but a later source failed to
                    // extract — surface the failure. A retry moves the rest (it
                    // CONVERGES: the prefix is already gone from the archive).
                    Some(err) => events.emit_error(WriteErrorEvent::new(op_id.clone(), WriteOperationType::Move, err)),
                },
                Err(PlanError::Cancelled) => {
                    // Cancelled during the archive rewrite: the extract already
                    // landed and the archive is intact (temp abandoned) —
                    // effectively a completed copy.
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        rolled_back: false,
                    });
                }
                Err(PlanError::Op(err)) => {
                    // The extract landed but the archive couldn't be rewritten. The
                    // originals are intact and the copies are at the destination (no
                    // data loss), so surface the failure — the move degraded to a copy.
                    events.emit_error(WriteErrorEvent::new(op_id.clone(), WriteOperationType::Move, err));
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
    /// Top-level sources that extracted in FULL (every file durably written,
    /// zero deep skips), collected via `note_source_landed_clean`. The compound
    /// op deletes exactly these from the archive.
    landed_clean: Mutex<Vec<PathBuf>>,
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
    fn note_source_landed_clean(&self, source: &Path) {
        self.landed_clean.lock_ignore_poison().push(source.to_path_buf());
    }
}
