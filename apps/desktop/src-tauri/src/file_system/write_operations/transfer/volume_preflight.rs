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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use super::super::scan::take_cached_scan_result;
use super::super::state::{WriteOperationState, is_cancelled};
use super::super::types::{
    OperationEventSink, VolumeCopyConfig, WriteCancelledEvent, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent,
};
use super::volume_copy::WriteFailure;
use crate::file_system::volume::{ListingProgress, Volume};
use crate::ignore_poison::IgnorePoison;

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

    // Throttled scan-tally emit: every per-listing callback from the volume
    // turns into a `Scanning`-phase `WriteProgressEvent` carrying running
    // tallies. Without this, the FE shows "Scanning... 0 / 0 / 0" for the
    // entire scan duration (which can be seconds on cold MTP / large SMB
    // trees) when no `preview_id` is available (programmatic / MCP-triggered
    // moves, copies started outside the dialog flow). The scan-preview
    // pipeline emits its own climbing tallies into the preview's event
    // channel, not the operation's, so the operation needs its own.
    //
    // The closure borrows `events` and `state` for the duration of the
    // trait-method call below — same scope as the function body, so no
    // `Arc`-ing or pointer gymnastics needed. Throttle interval comes from
    // the op state (default 200 ms in prod, 0 ms in tests).
    let last_scan_emit: Mutex<Instant> = Mutex::new(Instant::now());
    let progress_interval = state.progress_interval;
    let scan_progress = |progress: ListingProgress| {
        let mut last = last_scan_emit.lock_ignore_poison();
        if last.elapsed() < progress_interval {
            return;
        }
        *last = Instant::now();
        drop(last);
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                operation_type,
                WriteOperationPhase::Scanning,
                None,
                progress.files,
                0,
                progress.bytes,
                0,
            ),
        );
    };

    // Single pipelined batch scan. The default impl loops per-path for
    // backends where per-path I/O is cheap (local FS, in-memory). MTP groups
    // by parent dir; SMB pipelines stats over one session. Either way, one
    // call per operation here. `_with_progress` threads the throttled scan
    // callback through so the FE sees tallies climb during the walk.
    let batch = volume
        .scan_for_copy_batch_with_progress(source_paths, Some(&scan_progress))
        .await
        .map_err(|e| {
            let path = source_paths.first().cloned().unwrap_or_default();
            WriteFailure::from_volume(&path, e)
        })?;

    // Final Scanning event with the aggregate totals. Bypasses the throttle
    // so the FE's last-known scan-phase state lands on the real total before
    // the phase flips to Copying — without this, a fast scan whose per-
    // listing emits all got throttled would still flash the right number.
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Scanning,
            None,
            batch.aggregate.file_count,
            0,
            batch.aggregate.total_bytes,
            0,
        ),
    );

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

/// Top-level-only source hints for the same-volume move fast path.
///
/// Unlike [`VolumePreflight`], this carries no recursive byte total — a
/// same-volume move is a rename and transfers zero bytes, so there's nothing to
/// drive a Size bar. We collect ONLY the per-top-level-item `is_directory` /
/// size hints (for the conflict resolver and `known_directory_paths`), at the
/// cost of one pipelined batch stat of the top-level items — O(top-level
/// items), never a subtree walk.
pub(super) struct TopLevelMoveHints {
    pub source_hints: HashMap<PathBuf, SourceHint>,
}

impl TopLevelMoveHints {
    /// Top-level paths that are directories — same role as
    /// [`VolumePreflight::known_directory_paths`] (keeps bulk-skip file-only).
    pub(super) fn known_directory_paths(&self) -> HashSet<PathBuf> {
        self.source_hints
            .iter()
            .filter(|&(_path, hint)| hint.is_directory)
            .map(|(path, _hint)| path.clone())
            .collect()
    }
}

/// Collects top-level `is_directory` / size hints for a same-volume move WITHOUT
/// walking any subtree — the move is a rename, so a deep walk (which
/// `scan_for_copy` / `scan_for_copy_batch` do to count bytes) would be pure
/// waste, and it's exactly the 30–40 s "Verifying before move…" the fast path
/// exists to kill.
///
/// Consumes a cached TransferDialog preview when present (free — the dialog
/// already scanned); we read ONLY each top-level item's `top_level_is_directory`
/// / size from it, never re-walking. Otherwise, groups the top-level sources by
/// PARENT and lists each distinct parent ONCE (`list_directory` is one
/// round-trip per parent: a single pipelined op on SMB, one parent listing on
/// MTP — the same shape `MtpVolume`'s `scan_for_copy_batch_with_progress` uses,
/// minus the recursion). Cost is O(distinct parents), never O(subtree).
pub(super) async fn top_level_move_hints(
    volume: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    config: &VolumeCopyConfig,
) -> Result<TopLevelMoveHints, WriteOperationError> {
    // Cached preview: read only the per-top-level-path type + (file) size.
    if let Some(cached) = config.preview_id.as_deref().and_then(take_cached_scan_result) {
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
        return Ok(TopLevelMoveHints { source_hints });
    }

    // No cached preview: group sources by parent and list each parent once,
    // indexing entries by name for an O(1) per-source lookup. One listing per
    // distinct parent — never a subtree walk.
    let mut by_parent: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for path in source_paths {
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        by_parent.entry(parent).or_default().push(path.clone());
    }

    let mut source_hints = HashMap::with_capacity(source_paths.len());
    for (parent, paths) in by_parent {
        let entries = volume
            .list_directory(&parent, None)
            .await
            .map_err(|e| WriteOperationError::IoError {
                path: parent.display().to_string(),
                message: format!("listing move-source parent failed: {e}"),
            })?;
        let by_name: HashMap<&str, &crate::file_system::listing::FileEntry> =
            entries.iter().map(|e| (e.name.as_str(), e)).collect();
        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if let Some(entry) = by_name.get(name) {
                let size = if entry.is_directory { 0 } else { entry.size.unwrap_or(0) };
                source_hints.insert(
                    path,
                    SourceHint {
                        is_directory: entry.is_directory,
                        size,
                    },
                );
            }
            // A source missing from its parent listing surfaces later as a
            // per-source rename error; leave it out of the hint map (the loop
            // falls back to a trait probe for it).
        }
    }
    Ok(TopLevelMoveHints { source_hints })
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
