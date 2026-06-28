//! Async serial driver for volume copy/move operations.

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::file_system::write_operations::state::{OperationIntent, WriteOperationState, is_cancelled, load_intent};
use crate::file_system::write_operations::types::{OperationEventSink, WriteOperationError};

use super::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, TransferContext, TransferLoopOutcome,
    TransferOutcome, emit_progress_and_status,
};

/// Async serial driver for volume operations.
///
/// # Data-safety contract
///
/// The `transfer_one` closure is invoked for a source iff **all** of the
/// following hold:
///
/// - the source is NOT in the pre-known-conflicts bulk-skip set, AND
/// - cancellation has NOT been observed on `state.intent` at the start of this iteration, AND
/// - the conflict resolver returned `ConflictDecision::Proceed` (not Skip) when a top-level
///   conflict was detected.
///
/// # What the driver does
///
/// 1. Pre-known-conflicts bulk-skip prelude (emit one bulk progress event).
/// 2. Per-source loop: a. Cancellation check. If cancelled, return `PostLoopIntent::Cancelled`. b.
///    Skip pre-known-conflict sources (no closure invocation). c. Conflict detection via
///    `dest_meta_fetcher`. If conflict, invoke `conflict_resolver` and respect its decision. d. If
///    Skip, do skip accounting + emit throttled progress; continue. e. If Proceed, invoke
///    `transfer_one` with the resolved dest path.
/// 3. Return `PostLoopIntent::Completed` if the loop drained without incident.
///
/// # Closure-bound shape: boxed future, not `AsyncFnMut`
///
/// Each closure returns `Pin<Box<dyn Future<...> + Send + 'a>>` rather than
/// being bound as `AsyncFnMut(...) -> T`. The explicit boxed-future shape is
/// load-bearing for production callers: the driver's returned future must be
/// `Send` so callers can `tokio::spawn` it, and `AsyncFnMut`'s HRTB-bound
/// `CallRefFuture<'a>` is not provably `Send` for all `'a` when the closure
/// body captures `&Arc<...>` or similar refs (rust-lang/rust#100013-class).
/// The `+ Send` on the boxed future moves the Send obligation inside the
/// per-call return type, where it's discharged at each call site.
///
/// `transfer_driver_async_tests.rs::driver_future_is_send_across_spawn` pins this:
/// the driver call must compile inside a `tokio::spawn(async move { ... })`,
/// which fails under `AsyncFnMut` and passes under the boxed-future shape.
///
/// The driver loop is still single-threaded (one `.await` per closure call
/// in sequence); only the FUTURE returned by each call needs `Send`. The
/// closure itself doesn't need to be `Send`.
#[allow(
    clippy::too_many_arguments,
    reason = "Driver wraps the full per-iter context; bundling adds ceremony without removing parameters"
)]
pub(in crate::file_system::write_operations::transfer) async fn drive_transfer_serial_async<
    DestMetaFetcher,
    ConflictResolver,
    TransferOne,
>(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    sources: &[PathBuf],
    dest_root: &Path,
    total_files: usize,
    total_bytes: u64,
    bulk_skip_files: usize,
    bulk_skip_bytes: u64,
    pre_skip_paths: &HashSet<PathBuf>,
    config: &DriverConfig,
    mut dest_meta_fetcher: DestMetaFetcher,
    mut conflict_resolver: ConflictResolver,
    mut transfer_one: TransferOne,
) -> TransferLoopOutcome
where
    DestMetaFetcher: for<'a> FnMut(&'a Path) -> Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>,
    ConflictResolver: for<'a> FnMut(
        ConflictDecisionInput<'a>,
    ) -> Pin<
        Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>,
    >,
    TransferOne: for<'a> FnMut(
        TransferContext<'a>,
    ) -> Pin<
        Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>,
    >,
{
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut bytes_skipped = 0u64;
    let mut last_progress_time = Instant::now();

    // ---- Pre-known-conflicts bulk-skip prelude. ----
    if bulk_skip_files > 0 {
        files_done += bulk_skip_files;
        bytes_done += bulk_skip_bytes;
        files_skipped += bulk_skip_files;
        bytes_skipped += bulk_skip_bytes;
        log::info!(
            "drive_transfer_serial_async: bulk-skipping {} files ({} bytes) before main iteration",
            bulk_skip_files,
            bulk_skip_bytes
        );
        // Re-anchor the rate estimator: bulk-skip credit is past work, not
        // throughput. See `drive_transfer_serial_sync` for the rationale.
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

    let progress_interval = state.progress_interval;

    // ---- Main loop. ----
    for source_path in sources {
        // CRITICAL: cancellation check is BEFORE pre-skip, which is BEFORE
        // any closure / conflict-resolver / destructive call.
        if is_cancelled(&state.intent) {
            log::debug!(
                "drive_transfer_serial_async: cancellation observed at {} (of {}) for op={}",
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

        // Pause gate: park here (between files, after the cancel check) while
        // the op is paused, without blocking an executor thread. Returns
        // immediately if cancelled — the next iteration's `is_cancelled` bails.
        // The cross-volume streaming path ALSO parks between chunks
        // (`volume_strategy.rs` `CheckpointStream`), so a paused single large
        // file stops mid-stream rather than streaming to completion. The
        // concurrent `copy_volumes_with_progress` `FuturesUnordered` path has no
        // between-files boundary and does NOT honor mid-batch pause (see
        // transfer/DETAILS.md § "Pause and the concurrent copy path").
        state.pause_gate.wait_while_paused_async(&state.intent).await;

        if pre_skip_paths.contains(source_path) {
            continue;
        }

        let initial_dest_path = if let Some(name) = source_path.file_name() {
            dest_root.join(name)
        } else {
            dest_root.to_path_buf()
        };

        // Conflict detection via caller-supplied dest meta fetcher.
        // `Some(size)` => conflict; `None` => no conflict (or stat failed,
        // treated identically to no-conflict at the top-level — same shape as
        // today's `dest_volume.get_metadata(...).await.ok()` check in
        // `copy_volumes_with_progress`).
        let dest_size_hint = dest_meta_fetcher(&initial_dest_path).await;

        let (resolved_dest, replace_after_write) = if dest_size_hint.is_some() {
            log::debug!(
                "drive_transfer_serial_async: conflict detected at {}",
                initial_dest_path.display()
            );
            let decision = conflict_resolver(ConflictDecisionInput {
                source_path,
                initial_dest_path: &initial_dest_path,
                dest_size_hint,
                source_is_directory_hint: None,
                source_size_hint: None,
            })
            .await;

            match decision {
                Ok(ConflictDecision::Skip { bytes_accounted }) => {
                    // Per-iter skip accounting: bump counters and emit
                    // throttled progress so the bar reflects the skip
                    // immediately.
                    files_done += 1;
                    bytes_done += bytes_accounted;
                    files_skipped += 1;
                    bytes_skipped += bytes_accounted;
                    if last_progress_time.elapsed() >= progress_interval {
                        last_progress_time = Instant::now();
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
                    continue;
                }
                Ok(ConflictDecision::Proceed {
                    dest_path,
                    replace_after_write,
                }) => (dest_path, replace_after_write),
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
        } else {
            (initial_dest_path, None)
        };

        let ctx = TransferContext {
            events,
            state,
            operation_id,
            operation_type: config.operation_type,
            source_path,
            dest_path: Some(&resolved_dest),
            replace_after_write: replace_after_write.as_deref(),
            files_done_so_far: files_done,
            bytes_done_so_far: bytes_done,
            total_files,
            total_bytes,
        };

        match transfer_one(ctx).await {
            Ok(TransferOutcome::Transferred { bytes }) => {
                files_done += 1;
                bytes_done += bytes;
                // Per-source milestone, gated on `emit_per_source_milestone`.
                // The streaming paths set it `false`: their closures emit
                // LEAF-granular milestones via `SerialLeafProgress`, and a
                // top-level-granular emit here would regress the File bar at the
                // end of a directory source (9/9 → 1/9). The same-volume
                // rename-merge sets it `true`: its `transfer_one` does a bare
                // `rename` with no streaming, so this is its ONLY Copying event.
                if config.emit_per_source_milestone {
                    last_progress_time = Instant::now();
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
            }
            Ok(TransferOutcome::Skipped { bytes_accounted }) => {
                files_done += 1;
                bytes_done += bytes_accounted;
                files_skipped += 1;
                bytes_skipped += bytes_accounted;
                if last_progress_time.elapsed() >= progress_interval {
                    last_progress_time = Instant::now();
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

    // One final post-loop check: a `RollingBack` or `Stopped` transition could
    // land after the last iteration's check and before we return Completed.
    // Today's `copy_files_with_progress_inner` post-loop intent check exists
    // exactly for this race (commit `1de4255d`). We mirror it here so a
    // future caller doesn't have to remember.
    if load_intent(&state.intent) != OperationIntent::Running {
        return TransferLoopOutcome {
            files_done,
            bytes_done,
            files_skipped,
            bytes_skipped,
            intent: PostLoopIntent::Cancelled,
        };
    }

    TransferLoopOutcome {
        files_done,
        bytes_done,
        files_skipped,
        bytes_skipped,
        intent: PostLoopIntent::Completed,
    }
}
