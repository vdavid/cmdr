//! Scan preview subsystem for the Copy dialog.
//!
//! Provides background scanning that feeds live stats to the frontend before
//! the actual copy starts. Results are cached so the copy can skip a redundant scan.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::scan::{SubtreeTotals, WalkContext, scan_subtree_with_oracle, sort_files, walk_dir_recursive};
use super::state::{
    CachedScanResult, FileInfo, SCAN_PREVIEW_RESULTS, SCAN_PREVIEW_STATE, ScanPreviewState, insert_scan_result,
    release_scan_result,
};
use super::types::{
    ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent,
    ScanPreviewStartResult,
};
use crate::file_system::listing::caching::try_get_watched_listing;
use crate::file_system::listing::{SortColumn, SortOrder};
use crate::file_system::volume::{BatchScanResult, CopyScanResult, Volume};
use crate::indexing::read::expected_totals;

/// Starts a scan preview for the Copy dialog.
/// Returns a preview_id that can be used to cancel or to pass to copy_files.
///
/// When `source_volume` is provided, uses `Volume::scan_for_copy()` instead of `std::fs`,
/// enabling MTP and other non-local volumes to produce scan previews.
///
/// `source_volume_id` identifies the volume the sources live on. It's used by the
/// fresh-listing oracle (`try_get_watched_listing`) to short-circuit re-reading
/// directories that an open pane is already keeping in sync. Pass `"root"` for
/// local-FS scans.
/// `sample_for_estimate` turns on the compressed-size sampler for the LOCAL
/// walk only (compress-mode scans). It's ignored for volume/remote scans, which
/// never sample (the estimate is suppressed there). See `compress_estimate`.
#[allow(
    clippy::too_many_arguments,
    reason = "IPC pass-through mirroring the command's parameter list"
)]
pub fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    source_volume: Option<Arc<dyn Volume>>,
    source_volume_id: String,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: u64,
    sample_for_estimate: bool,
) -> ScanPreviewStartResult {
    let preview_id = Uuid::new_v4().to_string();
    let preview_id_clone = preview_id.clone();

    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(progress_interval_ms),
    });

    // Register state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.insert(preview_id.clone(), Arc::clone(&state));
    }

    // Spawn background task.
    // Volume scans need a Tokio runtime context (MtpVolume uses Handle::block_on),
    // so we capture the runtime handle and enter it on the spawned thread.
    // Local scans use std::thread directly (no runtime needed).
    if let Some(volume) = source_volume {
        tokio::spawn(async move {
            run_volume_scan_preview(app, preview_id_clone, sources, volume, source_volume_id, state).await;
        });
    } else {
        std::thread::spawn(move || {
            run_scan_preview(
                app,
                preview_id_clone,
                sources,
                sort_column,
                sort_order,
                state,
                sample_for_estimate,
            );
        });
    }

    ScanPreviewStartResult { preview_id }
}

/// Returns the cached totals from a completed scan preview, or `None` if the
/// scan is still running, was cancelled, or errored. The FE uses this both to
/// know whether the scan is done AND to recover its display state when the
/// scan-preview events fired before listeners were attached (a real race
/// surfaced by M2a's watcher-backed oracle, which can complete a scan in
/// ~5 ms — faster than the FE's `await startScanPreview()` IPC round-trip).
pub fn get_scan_preview_totals(preview_id: &str) -> Option<super::types::ScanPreviewTotals> {
    let cache = SCAN_PREVIEW_RESULTS.read().ok()?;
    let cached = cache.get(preview_id)?;
    Some(super::types::ScanPreviewTotals {
        files_total: cached.file_count,
        dirs_total: cached.dirs.len(),
        bytes_total: cached.total_bytes,
        dedup_bytes_total: cached.dedup_bytes,
        estimated_compressed_bytes: cached.estimated_compressed_bytes.clone(),
    })
}

/// Cancels a running scan preview AND frees any cached result.
///
/// Cancelling sets the in-flight cancel flag (a still-running scan exits
/// promptly). Freeing the cached result covers the dialog-dismissed-after-scan-
/// completed case: the FE calls this on every dialog teardown, regardless of
/// whether the scan was still running, so a completed-but-unconsumed
/// `CachedScanResult` (tens of thousands of `FileInfo`) doesn't linger until
/// quit. Consuming the result for a started op goes through
/// `take_cached_scan_result` instead, which already removes it.
pub fn cancel_scan_preview(preview_id: &str) {
    if let Ok(cache) = SCAN_PREVIEW_STATE.read()
        && let Some(state) = cache.get(preview_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
    }
    release_scan_result(preview_id);
}

/// Internal function that runs the scan preview in a background thread.
///
/// When `sample_for_estimate` is set (compress-mode scans), a budget-capped worker
/// thread computes a compressed-size estimate off the walk thread: the walk
/// pushes `(path, size)` per regular file into a channel, the worker samples a
/// head window under a byte budget (see `compress_estimate`), and the estimate
/// rides the complete event. The worker is joined after the walk (usually
/// already done, since it ran concurrently) and cancels with the scan; a
/// sampling failure degrades to "no estimate" and never affects the scan.
fn run_scan_preview(
    app: tauri::AppHandle,
    preview_id: String,
    sources: Vec<PathBuf>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    state: Arc<ScanPreviewState>,
    sample_for_estimate: bool,
) {
    use tauri_specta::Event;

    use super::compress_estimate::CompressEstimator;

    let mut files: Vec<FileInfo> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    // Write footprint (un-dedup'd) and `du`-equivalent source footprint. The
    // dialog shows the first as the headline transfer size, the second as
    // hardlink context. See `walk_dir_recursive`.
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();
    // Shared across sources so hardlinks crossing source roots count once,
    // matching `indexing/scanner.rs`'s dir_stats aggregation policy.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Index-derived expected totals: lets the UI render a real progress bar
    // from the first scan event instead of an indeterminate spinner. `None`
    // when any source isn't covered by the index.
    let expected = expected_totals::expected_totals_for_sources(&sources);

    // Compress-size estimator: a budget-capped worker samples file heads OFF the walk
    // thread so the sampling CPU never lands on the scan's critical path. The
    // per-file hook below pushes `(path, size)` into the channel; the worker
    // deflates a head window under a byte budget and accumulates the estimate.
    // `None` for non-compress scans (the estimate is suppressed). Cancels with
    // the scan via the shared `cancelled` flag.
    let (estimate_tx, estimate_worker) = if sample_for_estimate {
        let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, u64)>();
        let cancel = Arc::clone(&state);
        let handle = std::thread::spawn(move || {
            let mut estimator = CompressEstimator::new();
            while let Ok((path, size)) = rx.recv() {
                if cancel.cancelled.load(Ordering::Relaxed) {
                    break;
                }
                estimator.observe(&path, size);
            }
            estimator.finish()
        });
        (Some(tx), Some(handle))
    } else {
        (None, None)
    };

    let result: Result<(), String> = (|| {
        // Cheap per-file hook: a channel push, so it never delays the walk. A
        // dropped receiver (worker gone) just drops the sample (best-effort). The
        // channel is deliberately UNBOUNDED: a sync_channel would block the walk
        // when the sampler falls behind (e.g. the oracle-cached fast-walk case),
        // which violates the never-touch-the-critical-path contract. The queue is
        // small in practice — post-budget the worker drains via hashmap lookups.
        let send_sample = |path: &Path, size: u64| {
            if let Some(tx) = &estimate_tx {
                let _ = tx.send((path.to_path_buf(), size));
            }
        };
        let ctx = WalkContext {
            progress_interval: state.progress_interval,
            is_cancelled: &|| state.cancelled.load(Ordering::Relaxed),
            on_io_error: &|_, e| e.to_string(),
            on_cancelled: &|| "Cancelled".to_string(),
            on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
            on_progress: &|files_found, dirs_found, bytes_found, current_path, current_dir| {
                let _ = ScanPreviewProgressEvent {
                    preview_id: preview_id.to_string(),
                    files_found,
                    dirs_found,
                    bytes_found,
                    current_path,
                    current_dir,
                    expected_files_total: expected.map(|e| e.files),
                    expected_bytes_total: expected.map(|e| e.bytes),
                }
                .emit(&app);
            },
            on_file: sample_for_estimate.then_some(&send_sample as &dyn Fn(&Path, u64)),
        };
        // Local FS scan preview uses the "root" volume ID. The oracle short-circuits
        // any subtree currently open in a pane with a live FSEvents watcher.
        let volume_id = Some(crate::file_system::volume::DEFAULT_VOLUME_ID);
        for source in &sources {
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
        Ok(())
    })();

    // Close the channel (drop the only sender) so the worker drains and returns,
    // then collect the estimate. The worker ran concurrently with the walk, so
    // this join is usually already done; a sampling panic degrades to `None`.
    drop(estimate_tx);
    let estimate = estimate_worker.and_then(|handle| handle.join().ok());

    // Clean up state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.remove(&preview_id);
    }

    match result {
        Ok(()) => {
            if state.cancelled.load(Ordering::Relaxed) {
                // Cancelled
                let _ = ScanPreviewCancelledEvent {
                    preview_id: preview_id.clone(),
                }
                .emit(&app);
            } else {
                // Sort files
                sort_files(&mut files, sort_column, sort_order);

                // Cache the results
                let file_count = files.len();
                let dirs_count = dirs.len();
                insert_scan_result(
                    preview_id.clone(),
                    CachedScanResult {
                        files,
                        dirs,
                        file_count,
                        total_bytes,
                        dedup_bytes,
                        per_path: Vec::new(),
                        estimated_compressed_bytes: estimate.clone(),
                        inserted_at: Instant::now(),
                    },
                );

                // Emit completion
                let _ = ScanPreviewCompleteEvent {
                    preview_id,
                    files_total: file_count,
                    dirs_total: dirs_count,
                    bytes_total: total_bytes,
                    dedup_bytes_total: dedup_bytes,
                    estimated_compressed_bytes: estimate,
                }
                .emit(&app);
            }
        }
        Err(message) => {
            let _ = ScanPreviewErrorEvent { preview_id, message }.emit(&app);
        }
    }
}

/// Runs a volume-based scan preview (for MTP and other non-local volumes).
///
/// Decision flow per parent group (sources sharing a parent directory):
/// - Fresh-listing oracle hit: cached entries supply size + `is_directory` for each selected child,
///   so the per-group `BatchScanResult` slice is built without any volume I/O for top-level files.
///   Top-level directories among the inputs recurse via `scan_subtree_with_oracle`, which
///   re-applies the oracle at every level (so a subfolder open in another pane is also
///   short-circuited).
/// - Oracle miss: falls through to `volume.scan_for_copy_batch_with_progress`, preserving the
///   cold-cache parent-grouping optimizations on MTP and the pipelined stat optimization on SMB.
///
/// Emits the same `scan-preview-progress` / `scan-preview-complete` events as
/// the pre-oracle code, so the FE dialog behavior is unchanged.
async fn run_volume_scan_preview(
    app: tauri::AppHandle,
    preview_id: String,
    sources: Vec<PathBuf>,
    volume: Arc<dyn Volume>,
    source_volume_id: String,
    state: Arc<ScanPreviewState>,
) {
    use tauri_specta::Event;

    // Throttled progress emitter: the underlying MTP listing fires the callback
    // per entry (~60/s for 1047 files at ~17 ms each). We collapse those down to
    // ~5 events/s for the FE so the dialog's file count climbs smoothly without
    // flooding the IPC layer. Throttling lives in the closure rather than inside
    // each Volume impl so different backends share the same rate-limit policy.
    let progress_state = Arc::new(std::sync::Mutex::new(Instant::now()));
    let state_for_cb = Arc::clone(&state);
    let app_for_cb = app.clone();
    let preview_id_for_cb = preview_id.clone();
    let on_progress = move |p: crate::file_system::volume::ListingProgress| {
        if state_for_cb.cancelled.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut last) = progress_state.lock() else {
            return;
        };
        if last.elapsed() < Duration::from_millis(200) {
            return;
        }
        *last = Instant::now();
        drop(last);
        let _ = ScanPreviewProgressEvent {
            preview_id: preview_id_for_cb.clone(),
            files_found: p.files,
            dirs_found: p.dirs,
            bytes_found: p.bytes,
            current_path: None,
            current_dir: None,
            expected_files_total: None,
            expected_bytes_total: None,
        }
        .emit(&app_for_cb);
    };

    // Cancellation predicate, captured by reference inside the async helpers.
    let state_for_cancel = Arc::clone(&state);
    let is_cancelled = move || state_for_cancel.cancelled.load(Ordering::Relaxed);

    let result: Result<BatchScanResult, String> = async {
        if state.cancelled.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }

        run_oracle_aware_batch_scan(
            volume.as_ref(),
            &source_volume_id,
            &sources,
            &is_cancelled,
            &on_progress,
        )
        .await
        .map_err(|e| format!("Scan failed: {}", e))
    }
    .await;

    // Extract stats from the result for the completion event
    let (total_files, total_dirs, total_bytes, dedup_bytes) = match &result {
        Ok(batch) => (
            batch.aggregate.file_count,
            batch.aggregate.dir_count,
            batch.aggregate.total_bytes,
            batch.aggregate.dedup_bytes,
        ),
        Err(_) => (0, 0, 0, 0),
    };

    // Clean up state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.remove(&preview_id);
    }

    match result {
        Ok(batch) => {
            if state.cancelled.load(Ordering::Relaxed) {
                let _ = ScanPreviewCancelledEvent {
                    preview_id: preview_id.clone(),
                }
                .emit(&app);
            } else {
                // Cache results: volume scans don't produce per-file FileInfo, but
                // the cache stores aggregate stats AND per-path scan results so
                // copy_between_volumes can reuse both without re-statting.
                insert_scan_result(
                    preview_id.clone(),
                    CachedScanResult {
                        files: Vec::new(),
                        dirs: Vec::new(),
                        file_count: total_files,
                        total_bytes,
                        dedup_bytes,
                        per_path: batch.per_path,
                        // Remote sources never sample: the estimate is suppressed.
                        estimated_compressed_bytes: None,
                        inserted_at: Instant::now(),
                    },
                );

                let _ = ScanPreviewCompleteEvent {
                    preview_id,
                    files_total: total_files,
                    dirs_total: total_dirs,
                    bytes_total: total_bytes,
                    dedup_bytes_total: dedup_bytes,
                    // Remote sources never sample: the estimate is suppressed.
                    estimated_compressed_bytes: None,
                }
                .emit(&app);
            }
        }
        Err(message) => {
            let _ = ScanPreviewErrorEvent { preview_id, message }.emit(&app);
        }
    }
}

/// Oracle-aware batch scan: short-circuits parent directories that an open pane
/// is keeping watcher-fresh; falls through to the volume's own batch scan for
/// cold-cache parents. Builds a single merged `BatchScanResult` keyed back to
/// the caller's original `sources` slice (order matches input).
pub(super) async fn run_oracle_aware_batch_scan(
    volume: &dyn Volume,
    volume_id: &str,
    sources: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_progress: &(dyn Fn(crate::file_system::volume::ListingProgress) + Sync),
) -> Result<BatchScanResult, crate::file_system::volume::VolumeError> {
    use crate::file_system::listing::FileEntry;
    use crate::file_system::volume::ListingProgress;
    use std::collections::HashMap;

    // Group sources by parent dir, preserving the input order of paths within
    // each group. The merged result puts the per-path entries back in original
    // input order (callers downstream don't currently depend on order, but it
    // matches `BatchScanResult::per_path`'s documented contract).
    let mut groups: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    let mut group_order: Vec<PathBuf> = Vec::new();
    for source in sources {
        let parent = source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        if !groups.contains_key(&parent) {
            group_order.push(parent.clone());
        }
        groups.entry(parent).or_default().push(source.clone());
    }

    let mut aggregate = CopyScanResult {
        file_count: 0,
        dir_count: 0,
        total_bytes: 0,
        dedup_bytes: 0,
        // Aggregate across multiple paths — meaningless, per the BatchScanResult contract.
        top_level_is_directory: false,
    };
    let mut per_path_unordered: HashMap<PathBuf, CopyScanResult> = HashMap::new();
    // Batch-scoped hardlink dedup for the source-footprint number. Shared
    // across all groups so a hardlink spanning two selected sources counts
    // once. Only `LocalPosixVolume` cached entries carry inodes; other
    // backends leave them `None` (no dedup, source == write footprint).
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    for parent in &group_order {
        if is_cancelled() {
            return Err(crate::file_system::volume::VolumeError::Cancelled(
                "Operation cancelled by user".to_string(),
            ));
        }
        let paths_in_group = groups
            .get(parent)
            .expect("group_order tracks every parent inserted into groups");

        if let Some(cached_entries) = try_get_watched_listing(volume_id, parent) {
            log::debug!(
                "scan-preview: oracle hit for parent {} ({} cached entries, {} selected children)",
                parent.display(),
                cached_entries.len(),
                paths_in_group.len()
            );
            // Index cached entries by their last path component so we can resolve
            // selected paths without a per-path linear search. `path` on a cached
            // FileEntry is the absolute path string, so the file name component
            // is the disambiguator within this parent.
            let by_name: HashMap<String, &FileEntry> = cached_entries
                .iter()
                .filter_map(|e| {
                    PathBuf::from(&e.path)
                        .file_name()
                        .map(|n| (n.to_string_lossy().to_string(), e))
                })
                .collect();

            for source in paths_in_group {
                let name = source
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let Some(entry) = by_name.get(&name) else {
                    // Cache doesn't know this child. Could be a stale selection
                    // (entry deleted out-of-band) or a name encoding mismatch.
                    // Either way, fall through to a real stat for safety.
                    let scan = volume.scan_for_copy(source).await?;
                    aggregate.file_count += scan.file_count;
                    aggregate.dir_count += scan.dir_count;
                    aggregate.total_bytes += scan.total_bytes;
                    aggregate.dedup_bytes += scan.dedup_bytes;
                    per_path_unordered.insert(source.clone(), scan);
                    on_progress(ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.dedup_bytes,
                    });
                    continue;
                };

                if entry.is_directory && !entry.is_symlink {
                    // `scan_subtree_with_oracle` emits counts local to this
                    // subtree (starting at 1). Shift by the current aggregate
                    // so the FE display stays cumulative across multiple
                    // top-level dirs in this call — files, dirs, AND bytes.
                    // Scan-phase byte baseline is dedup'd (converges with the
                    // index estimate); the headline write footprint is tracked
                    // separately in `aggregate.total_bytes`.
                    let baseline = ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.dedup_bytes,
                    };
                    let shifted = |p: ListingProgress| {
                        on_progress(ListingProgress {
                            files: baseline.files + p.files,
                            dirs: baseline.dirs + p.dirs,
                            bytes: baseline.bytes + p.bytes,
                        })
                    };
                    let subtree: SubtreeTotals = scan_subtree_with_oracle(
                        volume,
                        volume_id,
                        source,
                        is_cancelled,
                        Some(&shifted),
                        &mut seen_inodes,
                    )
                    .await?;
                    aggregate.file_count += subtree.file_count;
                    // `scan_for_copy_batch`'s aggregate.dir_count counts descendants
                    // only, not the top-level path itself. Match that convention
                    // so the FE's "X dirs" number is consistent across paths.
                    aggregate.dir_count += subtree.dir_count;
                    aggregate.total_bytes += subtree.total_bytes;
                    aggregate.dedup_bytes += subtree.dedup_bytes;
                    per_path_unordered.insert(
                        source.clone(),
                        CopyScanResult {
                            file_count: subtree.file_count,
                            dir_count: subtree.dir_count,
                            total_bytes: subtree.total_bytes,
                            dedup_bytes: subtree.dedup_bytes,
                            top_level_is_directory: true,
                        },
                    );
                } else {
                    let size = entry.size.unwrap_or(0);
                    // Top-level cached file: dedupe by inode for the source
                    // footprint. Single top-level files rarely collide, but a
                    // hardlink also selected inside a sibling dir would.
                    let dedup_contribution = match entry.inode {
                        Some(ino) if !seen_inodes.insert(ino) => 0,
                        _ => size,
                    };
                    aggregate.file_count += 1;
                    aggregate.total_bytes += size;
                    aggregate.dedup_bytes += dedup_contribution;
                    per_path_unordered.insert(
                        source.clone(),
                        CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: size,
                            dedup_bytes: dedup_contribution,
                            top_level_is_directory: false,
                        },
                    );
                    on_progress(ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.dedup_bytes,
                    });
                }
            }
        } else {
            // Cold cache for this parent. Delegate to the volume's own batch
            // scan: it preserves the MTP parent-grouping and SMB pipelined-stat
            // optimizations for cold paths.
            //
            // The volume's callback reports counts LOCAL to its current
            // `list_directory` call (starts at 1 for files, 0 for dirs/bytes
            // until entries are enumerated). Shift by the current aggregate
            // before forwarding so the FE display stays cumulative as we walk
            // multiple parent groups — without this, every new group's first
            // entry drops the visible counts back to local values, then climbs
            // to the group's local totals before the next group restarts.
            // Cold-cache backends (MTP, SMB) report dedup_bytes == total_bytes
            // (no hardlinks), so the dedup'd baseline matches their stream.
            let baseline = ListingProgress {
                files: aggregate.file_count,
                dirs: aggregate.dir_count,
                bytes: aggregate.dedup_bytes,
            };
            let shifted = |p: ListingProgress| {
                on_progress(ListingProgress {
                    files: baseline.files + p.files,
                    dirs: baseline.dirs + p.dirs,
                    bytes: baseline.bytes + p.bytes,
                })
            };
            let group_result = volume
                .scan_for_copy_batch_with_progress(paths_in_group, Some(&shifted))
                .await?;
            aggregate.file_count += group_result.aggregate.file_count;
            aggregate.dir_count += group_result.aggregate.dir_count;
            aggregate.total_bytes += group_result.aggregate.total_bytes;
            aggregate.dedup_bytes += group_result.aggregate.dedup_bytes;
            for (path, scan) in group_result.per_path {
                per_path_unordered.insert(path, scan);
            }
        }
    }

    // Rebuild per_path in caller's original source order. Missing entries
    // (shouldn't happen, but be defensive) are skipped silently.
    let per_path: Vec<(PathBuf, CopyScanResult)> = sources
        .iter()
        .filter_map(|src| per_path_unordered.remove(src).map(|scan| (src.clone(), scan)))
        .collect();

    Ok(BatchScanResult { aggregate, per_path })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `get_scan_preview_totals` must return the cached counters when a scan
    /// has completed. Pins the contract the FE relies on to recover its
    /// display state when scan events fire before listeners attach (the
    /// regression that flaked `mtp-copy-preflight-uses-cache.spec.ts` after
    /// M2a's watcher-backed oracle made scans nearly instant).
    #[test]
    fn get_scan_preview_totals_returns_cached_counts_after_complete() {
        let preview_id = format!("test-{}", Uuid::new_v4());
        SCAN_PREVIEW_RESULTS.write().unwrap().insert(
            preview_id.clone(),
            CachedScanResult {
                files: Vec::new(),
                dirs: vec![PathBuf::from("/d1"), PathBuf::from("/d2")],
                file_count: 7,
                total_bytes: 12_345,
                dedup_bytes: 12_345,
                per_path: Vec::new(),
                estimated_compressed_bytes: None,
                inserted_at: Instant::now(),
            },
        );

        let totals = get_scan_preview_totals(&preview_id).expect("totals should be present");
        assert_eq!(totals.files_total, 7);
        assert_eq!(totals.dirs_total, 2);
        assert_eq!(totals.bytes_total, 12_345);

        SCAN_PREVIEW_RESULTS.write().unwrap().remove(&preview_id);
    }

    /// `get_scan_preview_totals` returns `None` while the scan is still
    /// running (the cache is keyed by preview_id; absence == not complete).
    #[test]
    fn get_scan_preview_totals_returns_none_for_unknown_preview() {
        let unknown = format!("nonexistent-{}", Uuid::new_v4());
        assert!(get_scan_preview_totals(&unknown).is_none());
    }
}
