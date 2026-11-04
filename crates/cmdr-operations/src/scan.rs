//! Turning a walk into a `ScanResult`: what a copy, move, or delete is about to
//! act on, and how big it is.
//!
//! The walk itself is `scan_walker.rs`, the per-source bookkeeping over the
//! result is `scan_source_tracker.rs`, and the write-nothing preview is
//! `scan_dry_run.rs`. What stays here is the two entry points that produce
//! totals: `scan_sources` (local, over a list of top-level sources) and
//! `scan_subtree_with_oracle` (one subtree, cache-fed where a watcher can
//! vouch for it).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::cancellable::run_cancellable_scoped;
use super::event_sinks::OperationEventSink;
use super::mount_boundary::refused_at;
use super::scan_walker::{WalkContext, walk_dir_recursive};
use super::state::{ScanResult, WriteOperationState};
use super::types::{
    MountedVolumeAction, WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};

/// Scans source paths recursively, returns file list and totals.
/// Files come back in walk order: each source depth first, in the order given.
/// A transfer reorders them into its tree's order
/// (`ScanResult::order_by_transfer_tree`); a delete keeps walk order.
///
/// Uses polling-based cancellation to remain responsive even when filesystem
/// operations block (for example, on stuck network drives).
pub(super) fn scan_sources(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
) -> Result<ScanResult, WriteOperationError> {
    let progress_interval = state.progress_interval;

    run_cancellable_scoped(
        || scan_sources_internal(sources, state, events, operation_id, operation_type, progress_interval),
        state,
        "scan",
        operation_id,
    )
}

/// Internal scan implementation (runs in background thread).
///
/// Its `ScanResult::per_path` is empty BY CONTRACT, and that's safe here for one
/// reason: this result is returned straight to its caller and never inserted
/// into the preview cache (`insert_scan_result` has exactly two production call
/// sites, both in `scan_preview.rs`), so no consumer can read the empty map and
/// mistake it for "these sources are files". ❌ Don't fill it to match
/// `run_scan_preview`'s shape: that costs a per-source counter bracket on the
/// local copy/move/delete hot path for a field nobody reads. If this result ever
/// starts crossing the cache, it has to collect `per_path` first.
fn scan_sources_internal(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
) -> Result<ScanResult, WriteOperationError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    // Write footprint (every file at full size) and `du`-equivalent source
    // footprint (each inode once). See `walk_dir_recursive`.
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress_time = crate::rt::now();
    let mut visited = HashSet::new();
    // Shared across all sources in this scan so a file hardlinked between
    // separate source roots still only contributes its bytes once, matching
    // what the indexer does for dir_stats aggregation.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Index-derived expected totals: the denominator the FE renders the
    // scan-phase progress bar against while the foolproof scan runs. `None`
    // when any source isn't in the index; the FE falls back to tallies only.
    let expected = state.scope().host().index().expected_totals(sources);
    log::debug!(
        "scan: op={} index expected={}",
        operation_id,
        expected
            .map(|e| format!("{} files / {} bytes", e.files, e.bytes))
            .unwrap_or_else(|| "(not available)".to_string())
    );

    // The operation owns this walk, so it knows its own gate: no claim to
    // resolve and no watchdog (that bounds a PREVIEW, not an operation's scan).
    let pause = super::scan_bridge::ScanPause::for_operation(Arc::clone(state));
    let sidecars = cmdr_fs::apple_double::SidecarRule::new();
    let refuse_mounted_volume = |path: &Path| refused_at(path, MountedVolumeAction::Delete);
    let ctx = WalkContext {
        sidecars: &sidecars,
        progress_interval,
        is_cancelled: &|| state.intent().is_cancelled(),
        park_while_paused: &|| pause.park_while_paused(),
        on_io_error: &|path, e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: cmdr_fs::error_detail::ErrorDetail::from_io(&e),
        },
        on_cancelled: &|| WriteOperationError::Cancelled {
            message: "Operation cancelled by user".into(),
        },
        on_symlink_loop: &|path| WriteOperationError::SymlinkLoop {
            path: path.display().to_string(),
        },
        on_mount_boundary: (operation_type == WriteOperationType::Delete).then_some(&refuse_mounted_volume),
        on_progress: &|files_done, dirs_done, bytes_done, current_file, current_dir| {
            log::debug!(
                "scan: emitting write-progress op={} phase=scanning files_found={} dirs_found={} bytes_found={}",
                operation_id,
                files_done,
                dirs_done,
                bytes_done
            );
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    operation_type,
                    WriteOperationPhase::Scanning,
                    current_file.clone(),
                    files_done,
                    0,
                    bytes_done,
                    0,
                )
                .with_scan_meta(current_dir, dirs_done, expected),
            );
            state.scope().status().update(
                operation_id,
                WriteOperationPhase::Scanning,
                current_file,
                files_done,
                0,
                bytes_done,
                0,
            );
        },
        // The real copy/move/delete scan never samples for a compress estimate.
        on_file: None,
        // The online-only tally belongs to the confirmation dialog's PREVIEW: by
        // the time this walk runs the routing is already decided.
        online_only: None,
        listings: state.scope().host().listings(),
    };

    // Local FS scan goes through `LocalPosixVolume`, which is always registered as
    // the `"root"` volume. Passing it threads the oracle through: when the source
    // (or any subdirectory we recurse into) is open in a pane with a live FSEvents
    // watcher, the walker skips the disk read for that level.
    let volume_id = Some(cmdr_fs::volume::DEFAULT_VOLUME_ID);

    for source in sources {
        let source_root = source.parent().unwrap_or(source);
        walk_dir_recursive(
            source,
            source_root,
            &mut files,
            &mut dirs,
            &mut total_bytes,
            &mut dedup_bytes,
            &mut last_progress_time,
            &mut visited,
            &mut seen_inodes,
            volume_id,
            &ctx,
        )?;
    }

    // Emit final scanning progress. The scan-phase bar reports the dedup'd
    // running total (matches the inode-dedup'd index estimate); the final
    // snapshot does the same so it lands exactly on the estimate.
    log::debug!(
        "scan: emitting final write-progress op={} phase=scanning files={} write_bytes={} dedup_bytes={}",
        operation_id,
        files.len(),
        total_bytes,
        dedup_bytes
    );
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Scanning,
            None,
            files.len(),
            files.len(),
            dedup_bytes,
            dedup_bytes,
        )
        .with_scan_meta(None, dirs.len(), expected),
    );

    Ok(ScanResult {
        file_count: files.len(),
        files,
        dirs,
        total_bytes,
        dedup_bytes,
        per_path: Vec::new(),
        tree: None,
    })
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scan_sidecar_tests.rs"]
mod sidecar_tests;
