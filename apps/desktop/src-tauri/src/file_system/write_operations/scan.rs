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
use std::time::{Duration, Instant};

use super::cancellable::run_cancellable_scoped;
use super::event_sinks::OperationEventSink;
use super::scan_source_tracker::sort_files;
use super::scan_walker::{WalkContext, walk_dir_recursive};
use super::state::{ScanResult, WriteOperationState, update_operation_status};
use super::types::{WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use crate::file_system::listing::caching::try_get_authoritative_listing;
use crate::file_system::listing::{FileEntry, SortColumn, SortOrder};
use crate::file_system::volume::{CopyScanResult, Volume, VolumeError};

/// Totals returned by `scan_subtree_with_oracle`.
///
/// `per_path` carries one entry per direct child of the scanned `path`, sized
/// to feed into a parent `BatchScanResult` upstream. The vec is empty when
/// `path` itself is a file (the caller knows it's a file in that case).
// DEFAULT-OK: zeroed totals are what a walk that hasn't counted anything has counted.
// This is an accumulator seed, not a finding: it only ever grows from here.
#[derive(Debug, Clone, Default)]
pub(super) struct SubtreeTotals {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
    /// Source on-disk footprint, hardlinks counted once. See
    /// `CopyScanResult::dedup_bytes`. Equal to `total_bytes` when the cached
    /// `FileEntry`s carry no inode (non-local backends).
    pub dedup_bytes: u64,
    /// Per-direct-child results so the scan-preview can populate the
    /// `BatchScanResult::per_path` slot the copy engine reads later.
    pub per_path: Vec<(PathBuf, CopyScanResult)>,
}

/// Scans a subtree using the fresh-listing oracle at every recursion level,
/// falling back to `volume.list_directory` on cache miss.
///
/// This is the oracle-aware analogue of the per-volume `scan_for_copy`. It's
/// designed for `run_volume_scan_preview` to call when the parent directory of
/// the selected sources is watcher-backed: top-level files come from the
/// cached listing directly, top-level directories recurse here.
///
/// Cancellation: the future polls `is_cancelled` between entries. Symlinks
/// (cached `is_symlink == true`) are counted as one entry and not recursed,
/// matching the local-FS walker's policy.
///
/// `pause` parks the walk at those same boundaries when the owning operation is
/// paused (`super::scan_bridge::ScanPause`), so pause is exactly as responsive
/// as cancel here.
pub(super) async fn scan_subtree_with_oracle(
    volume: &dyn Volume,
    volume_id: &str,
    path: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    pause: Option<&super::scan_bridge::ScanPause>,
    on_progress: Option<&(dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    seen_inodes: &mut HashSet<u64>,
) -> Result<SubtreeTotals, VolumeError> {
    use crate::file_system::volume::ListingProgress;

    if is_cancelled() {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
    }
    if let Some(pause) = pause {
        pause.park_while_paused_async().await;
    }

    // Load entries from oracle or the volume itself.
    let entries: Vec<FileEntry> = match try_get_authoritative_listing(volume_id, path) {
        Some(e) => e,
        None => volume.list_directory(path, on_progress).await?,
    };

    let mut totals = SubtreeTotals::default();
    // Running tally for the on_progress callback so dirs/bytes climb alongside
    // file count as we walk this subtree. `dir_count` on the SubtreeTotals
    // counts descendant dirs only; this `tally.dirs` mirrors that semantic,
    // but the callback's running count tells the FE "we've seen N dirs so far
    // in this subtree" which is the intuitive display.
    let mut tally = ListingProgress::default();

    for entry in entries {
        if is_cancelled() {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
        }
        // After the cancel check, never before: cancel outranks pause.
        if let Some(pause) = pause {
            pause.park_while_paused_async().await;
        }
        let child_path = PathBuf::from(&entry.path);
        if entry.is_directory && !entry.is_symlink {
            // Recurse — oracle re-applies inside this call. The recursive
            // emit reports counts local to the child subtree (starting fresh),
            // so wrap `on_progress` with a baseline of the current `tally` so
            // the FE display stays cumulative across sibling dirs.
            let baseline = tally;
            let child_totals = match on_progress {
                Some(cb) => {
                    let shifted = move |p: ListingProgress| {
                        cb(ListingProgress {
                            files: baseline.files + p.files,
                            dirs: baseline.dirs + p.dirs,
                            bytes: baseline.bytes + p.bytes,
                        })
                    };
                    Box::pin(scan_subtree_with_oracle(
                        volume,
                        volume_id,
                        &child_path,
                        is_cancelled,
                        pause,
                        Some(&shifted),
                        seen_inodes,
                    ))
                    .await?
                }
                None => {
                    Box::pin(scan_subtree_with_oracle(
                        volume,
                        volume_id,
                        &child_path,
                        is_cancelled,
                        pause,
                        None,
                        seen_inodes,
                    ))
                    .await?
                }
            };
            totals.file_count += child_totals.file_count;
            // The directory itself plus all its descendant dirs.
            totals.dir_count += 1 + child_totals.dir_count;
            totals.total_bytes += child_totals.total_bytes;
            totals.dedup_bytes += child_totals.dedup_bytes;
            tally.files += child_totals.file_count;
            tally.dirs += 1 + child_totals.dir_count;
            // The scan-phase climbing display is dedup'd so it converges with
            // the inode-dedup'd index estimate (the copy headline shows the
            // write footprint separately).
            tally.bytes += child_totals.dedup_bytes;
            totals.per_path.push((
                child_path,
                CopyScanResult {
                    file_count: child_totals.file_count,
                    dir_count: child_totals.dir_count,
                    total_bytes: child_totals.total_bytes,
                    dedup_bytes: child_totals.dedup_bytes,
                    top_level_is_directory: true,
                },
            ));
            if let Some(cb) = on_progress {
                cb(tally);
            }
        } else {
            let size = entry.size.unwrap_or(0);
            // Hardlink dedup for the source-footprint number. `FileEntry.inode`
            // is `Some` only for `LocalPosixVolume` files with `nlink > 1`;
            // non-local backends leave it `None` so every file counts as
            // unique. Mirrors `LocalPosixVolume::scan_for_copy`.
            let dedup_contribution = match entry.inode {
                Some(ino) if !seen_inodes.insert(ino) => 0,
                _ => size,
            };
            totals.file_count += 1;
            totals.total_bytes += size;
            totals.dedup_bytes += dedup_contribution;
            tally.files += 1;
            // Dedup'd climbing display — see the dir branch above.
            tally.bytes += dedup_contribution;
            totals.per_path.push((
                child_path,
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: size,
                    dedup_bytes: dedup_contribution,
                    top_level_is_directory: false,
                },
            ));
            if let Some(cb) = on_progress {
                cb(tally);
            }
        }
    }

    Ok(totals)
}

/// Scans source paths recursively, returns file list and totals.
/// Files are sorted according to the specified column and order.
///
/// Uses polling-based cancellation to remain responsive even when filesystem
/// operations block (for example, on stuck network drives).
pub(super) fn scan_sources(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
) -> Result<ScanResult, WriteOperationError> {
    let progress_interval = state.progress_interval;

    run_cancellable_scoped(
        || {
            scan_sources_internal(
                sources,
                state,
                events,
                operation_id,
                operation_type,
                sort_column,
                sort_order,
                progress_interval,
            )
        },
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
#[allow(
    clippy::too_many_arguments,
    reason = "Internal helper passes through all required context"
)]
fn scan_sources_internal(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval: Duration,
) -> Result<ScanResult, WriteOperationError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    // Write footprint (every file at full size) and `du`-equivalent source
    // footprint (each inode once). See `walk_dir_recursive`.
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();
    // Shared across all sources in this scan so a file hardlinked between
    // separate source roots still only contributes its bytes once, matching
    // what the indexer does for dir_stats aggregation.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Index-derived expected totals: the denominator the FE renders the
    // scan-phase progress bar against while the foolproof scan runs. `None`
    // when any source isn't in the index; the FE falls back to tallies only.
    let expected = crate::index_host::index().expected_totals(sources);
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
    let ctx = WalkContext {
        progress_interval,
        is_cancelled: &|| super::state::is_cancelled(&state.intent),
        park_while_paused: &|| pause.park_while_paused(),
        on_io_error: &|path, e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: e.to_string(),
        },
        on_cancelled: &|| WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        },
        on_symlink_loop: &|path| WriteOperationError::SymlinkLoop {
            path: path.display().to_string(),
        },
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
            update_operation_status(
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
    };

    // Local FS scan goes through `LocalPosixVolume`, which is always registered as
    // the `"root"` volume. Passing it threads the oracle through: when the source
    // (or any subdirectory we recurse into) is open in a pane with a live FSEvents
    // watcher, the walker skips the disk read for that level.
    let volume_id = Some(crate::file_system::volume::DEFAULT_VOLUME_ID);

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

    // Sort files according to configuration
    sort_files(&mut files, sort_column, sort_order);

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
    })
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
