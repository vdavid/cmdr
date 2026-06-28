//! Sync serial driver for local-FS copy/move (runs inside `spawn_blocking`).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::file_system::write_operations::state::{WriteOperationState, is_cancelled};
use crate::file_system::write_operations::types::{OperationEventSink, WriteOperationError};

use super::{
    DriverConfig, PostLoopIntent, TransferContext, TransferLoopOutcome, TransferOutcome, emit_progress_and_status,
};

/// Sync serial driver for local-FS operations.
///
/// # Data-safety contract
///
/// The `transfer_one` closure is invoked for a source iff **all** of the
/// following hold:
///
/// - the source is NOT in the pre-known-conflicts bulk-skip set, AND
/// - cancellation has NOT been observed on `state.intent` at the start of this iteration.
///
/// Top-level conflict resolution is the **closure's** responsibility for the
/// sync path (local-FS conflicts happen mid-flight inside `copy_single_item`
/// at parent-directory level, not at the top-level dest). The closure can
/// still return `TransferOutcome::Skipped` to participate in the driver's
/// skip accounting.
///
/// # What the driver does
///
/// 1. Pre-known-conflicts bulk-skip prelude. Sums `bulk_skip_bytes` from the caller, increments
///    counters, and emits one bulk progress event.
/// 2. Per-source loop: a. Check cancellation. If cancelled, return with
///    `PostLoopIntent::Cancelled`. b. Skip pre-known-conflict sources (no closure invocation). c.
///    Call `transfer_one`. On `Ok`, update counters. On `Err` with
///    `WriteOperationError::Cancelled`, return `PostLoopIntent::Cancelled`. On any other `Err`,
///    return `PostLoopIntent::Failed`.
/// 3. Return `PostLoopIntent::Completed` if the loop drained without incident.
///
/// # Why no async / no boxed-future variant
///
/// The local-FS copy/move closures capture `&mut CopyTransaction` and
/// `&mut HashSet<PathBuf>`. Forcing those captures through a boxed-future
/// shape buys nothing — the underlying `std::fs::*` calls are sync.
#[allow(
    clippy::too_many_arguments,
    reason = "Driver wraps the full per-iter loop context; bundling adds ceremony without removing parameters"
)]
pub(in crate::file_system::write_operations::transfer) fn drive_transfer_serial_sync<F>(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    sources: &[PathBuf],
    total_files: usize,
    total_bytes: u64,
    bulk_skip_files: usize,
    bulk_skip_bytes: u64,
    pre_skip_paths: &HashSet<PathBuf>,
    config: &DriverConfig,
    mut transfer_one: F,
) -> TransferLoopOutcome
where
    F: FnMut(TransferContext<'_>) -> Result<TransferOutcome, WriteOperationError>,
{
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut bytes_skipped = 0u64;

    // ---- Pre-known-conflicts bulk-skip prelude. ----
    // The caller has already filtered the source set (sync ops are per-file
    // and have to filter `FileInfo`s, not top-level paths). The driver just
    // emits the one progress event reflecting the bulk skip so the bar jumps
    // in one go, instead of advancing one tick per skipped file interleaved
    // with the real transfers.
    if bulk_skip_files > 0 {
        files_done += bulk_skip_files;
        bytes_done += bulk_skip_bytes;
        files_skipped += bulk_skip_files;
        bytes_skipped += bulk_skip_bytes;
        log::info!(
            "drive_transfer_serial_sync: bulk-skipping {} files ({} bytes) before main iteration",
            bulk_skip_files,
            bulk_skip_bytes
        );
        // Re-anchor the rate estimator BEFORE emitting. The bulk-skip jump
        // is past work credited instantly, not throughput; without this
        // reseed the first real per-file emit's delta is computed against
        // `(0, 0)` and pins `bytes_per_second` at GB/s level. See
        // `eta::EtaEstimator::reseed_baseline` for the full rationale.
        if let Ok(mut est) = state.estimator.lock() {
            est.reseed_baseline(Instant::now(), bytes_done, files_done);
        }
        emit_progress_and_status(
            events,
            state,
            operation_id,
            config.operation_type,
            config.phase,
            None,
            files_done,
            total_files,
            bytes_done,
            total_bytes,
        );
    }

    // ---- Main loop. ----
    for source_path in sources {
        // CRITICAL: cancellation check is BEFORE any destructive call.
        if is_cancelled(&state.intent) {
            log::debug!(
                "drive_transfer_serial_sync: cancellation observed at {} (of {}) for op={}",
                files_done,
                total_files,
                operation_id
            );
            return TransferLoopOutcome {
                files_done,
                bytes_done,
                files_skipped,
                bytes_skipped,
                intent: PostLoopIntent::Cancelled,
            };
        }

        // Pause gate: park here (between files, after the cancel check so the
        // data-safety ordering holds) while the op is paused. Returns
        // immediately if cancelled — the next loop iteration's `is_cancelled`
        // check then bails. This is the BETWEEN-FILES boundary; the cross-volume
        // streaming path also parks BETWEEN CHUNKS (`volume_strategy.rs`
        // `CheckpointStream`). The local-FS sync chunk loop
        // (`chunked_copy.rs`) is the one path that still pauses only between
        // files — it receives just the cancel atom, not the `PauseGate` (see
        // transfer/DETAILS.md § "Pause reaches between chunks").
        state.pause_gate.wait_while_paused_sync(&state.intent);

        // CRITICAL: pre-skip check is BEFORE any closure invocation. The
        // closure must NEVER see a pre-known-conflict source.
        if pre_skip_paths.contains(source_path) {
            continue;
        }

        let ctx = TransferContext {
            events,
            state,
            operation_id,
            operation_type: config.operation_type,
            source_path,
            dest_path: None,
            replace_after_write: None,
            files_done_so_far: files_done,
            bytes_done_so_far: bytes_done,
            total_files,
            total_bytes,
        };

        match transfer_one(ctx) {
            Ok(TransferOutcome::Transferred { bytes }) => {
                // Sync per-file closures (`copy_single_item`) own their own
                // per-file milestone emit and intra-file byte progress, so
                // the driver just updates its counters here. Mirrors the
                // existing conflict-resolution split (sync driver delegates
                // to the closure; async driver dispatches itself).
                files_done += 1;
                bytes_done += bytes;
            }
            Ok(TransferOutcome::Skipped { bytes_accounted }) => {
                files_done += 1;
                bytes_done += bytes_accounted;
                files_skipped += 1;
                bytes_skipped += bytes_accounted;
                // Skip-arm bump emits a throttled progress event so the bar
                // reflects the user's Skip choice immediately.
                emit_progress_and_status(
                    events,
                    state,
                    operation_id,
                    config.operation_type,
                    config.phase,
                    source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                    files_done,
                    total_files,
                    bytes_done,
                    total_bytes,
                );
            }
            Err(WriteOperationError::Cancelled { .. }) => {
                return TransferLoopOutcome {
                    files_done,
                    bytes_done,
                    files_skipped,
                    bytes_skipped,
                    intent: PostLoopIntent::Cancelled,
                };
            }
            Err(e) => {
                return TransferLoopOutcome {
                    files_done,
                    bytes_done,
                    files_skipped,
                    bytes_skipped,
                    intent: PostLoopIntent::Failed(e),
                };
            }
        }
    }

    TransferLoopOutcome {
        files_done,
        bytes_done,
        files_skipped,
        bytes_skipped,
        intent: PostLoopIntent::Completed,
    }
}
