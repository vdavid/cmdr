//! The local-FS delete: a `walkdir`-style scan, then `fs::remove_*` per entry.
//! The volume-aware twin is `volume_walker.rs`.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::super::error_classification::IoResultExt;
use super::super::event_sinks::OperationEventSink;
use super::super::scan::scan_sources;
use super::super::scan_source_tracker::{FileVerdict, SourceItemTracker};
use super::super::state::WriteOperationState;
use super::super::types::{
    CancelRollback, DryRunResult, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};

// ============================================================================
// Delete implementation
// ============================================================================

pub(crate) fn delete_files_with_progress_inner(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    // Phase 1: Scan (or reuse cached preview results)
    let scan_result = if let Some(preview_id) = &config.preview_id {
        // Volume scans cache aggregate stats with an empty `files` list; the
        // per-file delete loop needs the file list, so treat an empty-files
        // cache hit the same as a miss and fall through to a fresh local scan.
        let previews = state.scope().stores().previews();
        if let Some(cached) = previews
            .take_cached_delete_scan(preview_id, sources)
            .filter(|c| !c.files.is_empty())
        {
            log::debug!(
                "delete_files_with_progress: reusing cached scan for operation_id={}, preview_id={}, files={}, bytes={}",
                operation_id,
                preview_id,
                cached.file_count,
                cached.total_bytes
            );
            cached
        } else {
            log::warn!(
                "preview_id={} cache miss despite frontend coordination, starting fresh scan for operation_id={}",
                preview_id,
                operation_id
            );
            scan_sources(sources, state, events, operation_id, WriteOperationType::Delete)?
        }
    } else {
        scan_sources(sources, state, events, operation_id, WriteOperationType::Delete)?
    };

    // Handle dry-run mode (delete has no conflicts)
    if config.dry_run {
        events.emit_dry_run_complete(DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: scan_result.file_count,
            // Delete frees the `du`-equivalent source footprint (a hardlinked
            // inode survives until its last link is removed), so report
            // `dedup_bytes`, not the write footprint.
            bytes_total: scan_result.dedup_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        });
        return Ok(());
    }

    // The scan's per-directory mount test may have run in a preview, before a
    // drive was mounted inside the selection; the list would then name the
    // drive's files. Asked again here, right before anything is removed.
    super::super::mount_boundary::refuse_deleting_a_mount(sources)?;

    // Phase 2: Delete files first (deepest first)
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();

    // Emit initial Deleting-phase event with totals. Important when reusing a
    // cached preview: no scanning events were emitted by the BE, so the FE
    // still has scan-phase tallies on screen. This event flips the FE to the
    // active-phase UI with the correct denominator on file/byte progress.
    // The byte denominator is `dedup_bytes`: delete frees each inode once, and
    // the per-file numerator below sums `progress_bytes` (also dedup'd).
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Delete,
            WriteOperationPhase::Deleting,
            None,
            0,
            scan_result.file_count,
            0,
            scan_result.dedup_bytes,
        ),
    );
    state.scope().status().update(
        operation_id,
        WriteOperationPhase::Deleting,
        None,
        0,
        scan_result.file_count,
        0,
        scan_result.dedup_bytes,
    );

    let mut tracker = SourceItemTracker::new(&scan_result.files);

    // Delete files
    for file_info in &scan_result.files {
        // The cooperative boundary, between files in the destructive phase.
        // Only the delete phase is gated; the scan recursion is not, so pausing
        // mid-scan can't freeze a half-counted "Scanning…".
        if state.stop_or_park_sync() {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rollback: CancelRollback::none(), // Delete operations can't be rolled back
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            });
        }

        // Use `progress_bytes` (dedup'd per file) so the numerator stays in
        // lockstep with the `dedup_bytes` denominator: a hardlinked inode is
        // freed once, on its last unlink. See `state.rs::FileInfo` and
        // `ScanResult::dedup_bytes`.
        let progress_bytes = file_info.progress_bytes;

        // Defensive: register with the downloads watcher's ignore set so a
        // future "deleted from Downloads" event source wouldn't surprise us.
        // No-ops outside ~/Downloads.
        state.scope().host().announce(&file_info.path);
        fs::remove_file(&file_info.path).with_path(&file_info.path)?;

        // Journal the deleted leaf so "when did I delete dog.jpg" is searchable.
        // Delete is never rollbackable (the op finalizes `permanent_delete`), so
        // these rows exist purely for search — one per leaf, deliberately.
        super::super::journal::record_local_leaf(
            state.scope(),
            cmdr_operation_log::types::EntryType::File,
            &file_info.path,
            None,
            Some(progress_bytes as i64),
            None,
            false,
            cmdr_operation_log::types::ItemOutcome::Done,
        );

        files_done += 1;
        bytes_done += progress_bytes;

        // Delete has no Skip: reaching here means the entry is unlinked, so
        // every file is a `CarriedOut` and the tracker's verdict is always
        // `Done`.
        if let Some(finished) = tracker.record(file_info, FileVerdict::CarriedOut) {
            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: finished.source_path.display().to_string(),
                // Every file under this top-level item is unlinked. Delete has no
                // Skip, so there is no way for it to still be there.
                source_removed: true,
                outcome: finished.outcome,
            });
        }

        // Emit progress
        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = file_info.path.file_name().map(|n| n.to_string_lossy().to_string());
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Deleting,
                    current_file.clone(),
                    files_done,
                    scan_result.file_count,
                    bytes_done,
                    scan_result.dedup_bytes,
                ),
            );
            state.scope().status().update(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                scan_result.file_count,
                bytes_done,
                scan_result.dedup_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (in reverse order - deepest first)
    for dir in scan_result.dirs.iter().rev() {
        // The cooperative boundary (delete phase, between dirs).
        if state.stop_or_park_sync() {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rollback: CancelRollback::none(), // Delete operations can't be rolled back
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            });
        }

        // Only remove if empty (files should already be deleted)
        state.scope().host().announce(dir);
        let _ = fs::remove_dir(dir);
    }

    // No fsync after a delete: a non-durable delete fails annoyance-class (a
    // deleted file can reappear after a crash → the user re-deletes; never data
    // loss), so targeted fsync isn't worth its cost — and dropping the old
    // whole-machine global sync (`sync(2)`) also removes the stall it caused on
    // unrelated apps (AGENTS.md principle #5). This is a deliberate, slight
    // reduction of an already-weak fire-and-forget guarantee, not an accident.
    // See `CLAUDE.md` § "Durability".

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: files_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
        appeared_during_move: None,
        top_level_skipped: None,
        refused: None,
        metadata_left_behind: None,
        durability_unconfirmed: None,
        locked_kept: None,
    });

    Ok(())
}
