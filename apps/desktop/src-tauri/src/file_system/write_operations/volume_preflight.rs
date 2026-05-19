//! Shared preflight scan for volume copy and move.
//!
//! Both `copy_volumes_with_progress` and the two `volume_move` paths need the
//! same thing before the per-source loop runs: total file count, total byte
//! count, and a per-path `is_directory` / size map (the "hints" the loop reads
//! to skip re-probing each source). Previously only copy did this; move
//! shipped `bytes_total = 0` on every emit and built its own per-source
//! directory probe (`collect_known_directory_paths`). That hid the Size bar in
//! the progress dialog and forced extra USB round-trips on MTP.
//!
//! This module centralizes the scan. Callers pass the source paths, the
//! optional `preview_id` from a TransferDialog cache, and an event sink. The
//! helper:
//! - emits one `WriteProgressEvent` for `WriteOperationPhase::Scanning` so the
//!   FE sees the scan stage (even if it's fast),
//! - tries `take_cached_scan_result(preview_id)` first; on hit, the per-path
//!   results seed `source_hints` directly,
//! - on miss, falls through to `volume.scan_for_copy_batch` (so MTP's
//!   group-by-parent and SMB's pipelined-stat optimizations still kick in).
//!
//! The returned `VolumePreflight` carries everything downstream needs:
//! `total_files` / `total_bytes` to feed the driver, `source_hints` so the
//! per-iter loop can read `is_directory` and `size` without an extra trait
//! call, and `known_directory_paths` so `build_pre_skip_set` can exclude
//! top-level directories from the file-only bulk-skip.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use super::scan::take_cached_scan_result;
use super::state::{WriteOperationState, is_cancelled};
use super::types::{
    OperationEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent,
};
use super::volume_copy::WriteFailure;
use crate::file_system::volume::Volume;

/// Per-source hint collected during the scan: whether the top-level path is a
/// directory and, for top-level files, the file size.
///
/// The per-iter copy/move loops reuse these to skip an `is_directory` probe
/// per file. For copy, the `size` value also feeds SMB's compound fast-path
/// (one-RTT CREATE+READ+CLOSE) when the file fits in one READ. For move it
/// feeds the per-source progress emit so the FE's Size bar advances in step
/// with the Files bar.
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct SourceHint {
    pub is_directory: bool,
    /// For top-level files, the file size. For directories this stays `0`
    /// (the recursive total isn't tracked per-source — it lives in
    /// `VolumePreflight::total_bytes`).
    pub size: u64,
}

/// Result of a preflight scan over a set of source paths.
///
/// `source_hints` is keyed by the caller's input path verbatim. Paths missing
/// from the map (for example because a cached local-FS preview didn't carry
/// per-path data) get the `SourceHint::default()` treatment in the per-iter
/// loop: `is_directory = false`, `size = 0`. Downstream call sites handle that
/// case the same way as before this module existed.
#[derive(Debug, Clone)]
pub(super) struct VolumePreflight {
    pub total_files: usize,
    pub total_bytes: u64,
    pub source_hints: HashMap<PathBuf, SourceHint>,
}

impl VolumePreflight {
    /// Returns the set of top-level paths that are directories. Used by
    /// `transfer_driver::build_pre_skip_set` so directories matching a
    /// pre-known conflict name aren't bulk-skipped (a top-level dir match
    /// means only some children collide; dropping the subtree would lose
    /// non-conflicting files).
    pub(super) fn known_directory_paths(&self) -> HashSet<PathBuf> {
        self.source_hints
            .iter()
            .filter(|&(_path, hint)| hint.is_directory)
            .map(|(path, _hint)| path.clone())
            .collect()
    }
}

/// Scans the source paths up front, reusing a cached preview when one is
/// available.
///
/// Emits one `WriteProgressEvent { phase: Scanning, … }` so the FE sees the
/// scan stage even on the fast cached-hit path; the throttled scan-progress
/// events from `scan_for_copy_batch_with_progress` are not wired through here
/// (the scan-preview pipeline already emits those into the preview's own
/// event channel, not the operation's).
///
/// Cancellation: checked once after the initial emit and before the scan
/// dispatch. The scan itself doesn't internally honor cancellation (the trait
/// pre-dates that need); a long MTP listing in flight will run to completion,
/// then the post-loop cancel check picks it up.
pub(super) async fn scan_volume_sources(
    volume: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    config: &VolumeCopyConfig,
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
) -> Result<VolumePreflight, WriteFailure> {
    // Emit the Scanning-phase progress event up front. The FE keys the dialog
    // stage indicator off `phase`, so this is what flips it from "starting"
    // to "Scanning".
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Scanning,
            None,
            0,
            0,
            0,
            0,
        ),
    );

    if is_cancelled(&state.intent) {
        return Err(cancelled_failure(events, operation_id, operation_type));
    }

    // Cached preview from the TransferDialog: same scan results, no second
    // walk over the device.
    if let Some(cached) = config.preview_id.as_deref().and_then(take_cached_scan_result) {
        log::debug!(
            "scan_volume_sources: reused cached preview for op={}, files={}, bytes={}, per_path={}",
            operation_id,
            cached.file_count,
            cached.total_bytes,
            cached.per_path.len(),
        );
        let mut source_hints = HashMap::with_capacity(cached.per_path.len());
        for (path, scan) in cached.per_path {
            let size = if scan.top_level_is_directory {
                0
            } else {
                scan.total_bytes
            };
            source_hints.insert(
                path,
                SourceHint {
                    is_directory: scan.top_level_is_directory,
                    size,
                },
            );
        }
        return Ok(VolumePreflight {
            total_files: cached.file_count,
            total_bytes: cached.total_bytes,
            source_hints,
        });
    }

    log::debug!(
        "scan_volume_sources: scanning sources for op={} ({} paths)",
        operation_id,
        source_paths.len(),
    );

    // Single pipelined batch scan. The default impl loops per-path for
    // backends where per-path I/O is cheap (local FS, in-memory). MTP groups
    // by parent dir; SMB pipelines stats over one session. Either way, one
    // call per operation here.
    let batch = volume.scan_for_copy_batch(source_paths).await.map_err(|e| {
        let path = source_paths.first().cloned().unwrap_or_default();
        WriteFailure::from_volume(&path, e)
    })?;

    let mut source_hints = HashMap::with_capacity(batch.per_path.len());
    for (source_path, scan) in &batch.per_path {
        let size = if scan.top_level_is_directory {
            0
        } else {
            scan.total_bytes
        };
        source_hints.insert(
            source_path.clone(),
            SourceHint {
                is_directory: scan.top_level_is_directory,
                size,
            },
        );
    }

    log::debug!(
        "scan_volume_sources: scan complete for op={}, files={}, dirs={}, bytes={}",
        operation_id,
        batch.aggregate.file_count,
        batch.aggregate.dir_count,
        batch.aggregate.total_bytes,
    );

    Ok(VolumePreflight {
        total_files: batch.aggregate.file_count,
        total_bytes: batch.aggregate.total_bytes,
        source_hints,
    })
}

/// Emits `write-cancelled` and returns a `WriteFailure::Cancelled`. Used when
/// cancellation is observed before the per-source loop runs (so no driver-side
/// `PostLoopIntent::Cancelled` will fire for it). The outer wrappers of every
/// volume op rely on `write-cancelled` being emitted exactly once for the FE
/// to close the dialog cleanly; without this emit, the FE would only see a
/// silent failure followed by `write-settled`.
fn cancelled_failure(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
) -> WriteFailure {
    events.emit_cancelled(WriteCancelledEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        files_processed: 0,
        rolled_back: false,
    });
    WriteFailure::synthetic(WriteOperationError::Cancelled {
        message: "Operation cancelled by user".to_string(),
    })
}
