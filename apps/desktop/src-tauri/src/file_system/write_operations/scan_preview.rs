//! Scan preview subsystem for the Copy dialog.
//!
//! Provides background scanning that feeds live stats to the frontend before
//! the actual copy starts. Results are cached so the copy can skip a redundant scan.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::scan::{SubtreeTotals, WalkContext, scan_subtree_with_oracle, sort_files, walk_dir_recursive};
use super::state::{CachedScanResult, FileInfo, SCAN_PREVIEW_RESULTS, SCAN_PREVIEW_STATE, ScanPreviewState};
use super::types::{
    ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent,
    ScanPreviewStartResult,
};
use crate::file_system::listing::caching::try_get_watched_listing;
use crate::file_system::listing::{SortColumn, SortOrder};
use crate::file_system::volume::{BatchScanResult, CopyScanResult, Volume};

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
pub fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    source_volume: Option<Arc<dyn Volume>>,
    source_volume_id: String,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: u64,
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
            run_scan_preview(app, preview_id_clone, sources, sort_column, sort_order, state);
        });
    }

    ScanPreviewStartResult { preview_id }
}

/// Returns true if scan preview results are cached (scan completed successfully).
pub fn is_scan_preview_complete(preview_id: &str) -> bool {
    SCAN_PREVIEW_RESULTS
        .read()
        .is_ok_and(|cache| cache.contains_key(preview_id))
}

/// Cancels a running scan preview.
pub fn cancel_scan_preview(preview_id: &str) {
    if let Ok(cache) = SCAN_PREVIEW_STATE.read()
        && let Some(state) = cache.get(preview_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Internal function that runs the scan preview in a background thread.
fn run_scan_preview(
    app: tauri::AppHandle,
    preview_id: String,
    sources: Vec<PathBuf>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    state: Arc<ScanPreviewState>,
) {
    use tauri::Emitter;

    let mut files: Vec<FileInfo> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut total_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();
    // Shared across sources so hardlinks crossing source roots count once,
    // matching `indexing/scanner.rs`'s dir_stats aggregation policy.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Index-derived expected totals: lets the UI render a real progress bar
    // from the first scan event instead of an indeterminate spinner. `None`
    // when any source isn't covered by the index.
    let expected = crate::indexing::expected_totals::expected_totals_for_sources(&sources);

    let result: Result<(), String> = (|| {
        let ctx = WalkContext {
            progress_interval: state.progress_interval,
            is_cancelled: &|| state.cancelled.load(Ordering::Relaxed),
            on_io_error: &|_, e| e.to_string(),
            on_cancelled: &|| "Cancelled".to_string(),
            on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
            on_progress: &|files_found, dirs_found, bytes_found, current_path, current_dir| {
                let _ = app.emit(
                    "scan-preview-progress",
                    ScanPreviewProgressEvent {
                        preview_id: preview_id.to_string(),
                        files_found,
                        dirs_found,
                        bytes_found,
                        current_path,
                        current_dir,
                        expected_files_total: expected.map(|e| e.files),
                        expected_bytes_total: expected.map(|e| e.bytes),
                    },
                );
            },
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
                &mut last_progress_time,
                &mut visited,
                &mut seen_inodes,
                volume_id,
                &ctx,
            )?;
        }
        Ok(())
    })();

    // Clean up state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.remove(&preview_id);
    }

    match result {
        Ok(()) => {
            if state.cancelled.load(Ordering::Relaxed) {
                // Cancelled
                let _ = app.emit(
                    "scan-preview-cancelled",
                    ScanPreviewCancelledEvent {
                        preview_id: preview_id.clone(),
                    },
                );
            } else {
                // Sort files
                sort_files(&mut files, sort_column, sort_order);

                // Cache the results
                let file_count = files.len();
                let dirs_count = dirs.len();
                if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
                    cache.insert(
                        preview_id.clone(),
                        CachedScanResult {
                            files,
                            dirs,
                            file_count,
                            total_bytes,
                            per_path: Vec::new(),
                        },
                    );
                }

                // Emit completion
                let _ = app.emit(
                    "scan-preview-complete",
                    ScanPreviewCompleteEvent {
                        preview_id,
                        files_total: file_count,
                        dirs_total: dirs_count,
                        bytes_total: total_bytes,
                    },
                );
            }
        }
        Err(message) => {
            let _ = app.emit("scan-preview-error", ScanPreviewErrorEvent { preview_id, message });
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
    use tauri::Emitter;

    // Throttled progress emitter: the underlying MTP listing fires the callback
    // per entry (~60/s for 1047 files at ~17 ms each). We collapse those down to
    // ~5 events/s for the FE so the dialog's file count climbs smoothly without
    // flooding the IPC layer. Throttling lives in the closure rather than inside
    // each Volume impl so different backends share the same rate-limit policy.
    let progress_state = Arc::new(std::sync::Mutex::new(Instant::now()));
    let state_for_cb = Arc::clone(&state);
    let app_for_cb = app.clone();
    let preview_id_for_cb = preview_id.clone();
    let on_progress = move |files_found: usize| {
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
        let _ = app_for_cb.emit(
            "scan-preview-progress",
            ScanPreviewProgressEvent {
                preview_id: preview_id_for_cb.clone(),
                files_found,
                dirs_found: 0,
                // Bytes aren't surfaced by `list_directory_with_progress` mid-stream
                // (it only knows the count). The FE will show "0 bytes" climbing in
                // count alongside, then the final size lands on scan-preview-complete.
                bytes_found: 0,
                current_path: None,
                current_dir: None,
                expected_files_total: None,
                expected_bytes_total: None,
            },
        );
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
    let (total_files, total_dirs, total_bytes) = match &result {
        Ok(batch) => (
            batch.aggregate.file_count,
            batch.aggregate.dir_count,
            batch.aggregate.total_bytes,
        ),
        Err(_) => (0, 0, 0),
    };

    // Clean up state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.remove(&preview_id);
    }

    match result {
        Ok(batch) => {
            if state.cancelled.load(Ordering::Relaxed) {
                let _ = app.emit(
                    "scan-preview-cancelled",
                    ScanPreviewCancelledEvent {
                        preview_id: preview_id.clone(),
                    },
                );
            } else {
                // Cache results: volume scans don't produce per-file FileInfo, but
                // the cache stores aggregate stats AND per-path scan results so
                // copy_between_volumes can reuse both without re-statting.
                if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
                    cache.insert(
                        preview_id.clone(),
                        CachedScanResult {
                            files: Vec::new(),
                            dirs: Vec::new(),
                            file_count: total_files,
                            total_bytes,
                            per_path: batch.per_path,
                        },
                    );
                }

                let _ = app.emit(
                    "scan-preview-complete",
                    ScanPreviewCompleteEvent {
                        preview_id,
                        files_total: total_files,
                        dirs_total: total_dirs,
                        bytes_total: total_bytes,
                    },
                );
            }
        }
        Err(message) => {
            let _ = app.emit("scan-preview-error", ScanPreviewErrorEvent { preview_id, message });
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
    on_progress: &(dyn Fn(usize) + Sync),
) -> Result<BatchScanResult, crate::file_system::volume::VolumeError> {
    use crate::file_system::listing::FileEntry;
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
        // Aggregate across multiple paths — meaningless, per the BatchScanResult contract.
        top_level_is_directory: false,
    };
    let mut per_path_unordered: HashMap<PathBuf, CopyScanResult> = HashMap::new();

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
                    per_path_unordered.insert(source.clone(), scan);
                    on_progress(aggregate.file_count);
                    continue;
                };

                if entry.is_directory && !entry.is_symlink {
                    let subtree: SubtreeTotals =
                        scan_subtree_with_oracle(volume, volume_id, source, is_cancelled, Some(on_progress)).await?;
                    aggregate.file_count += subtree.file_count;
                    // `scan_for_copy_batch`'s aggregate.dir_count counts descendants
                    // only, not the top-level path itself. Match that convention
                    // so the FE's "X dirs" number is consistent across paths.
                    aggregate.dir_count += subtree.dir_count;
                    aggregate.total_bytes += subtree.total_bytes;
                    per_path_unordered.insert(
                        source.clone(),
                        CopyScanResult {
                            file_count: subtree.file_count,
                            dir_count: subtree.dir_count,
                            total_bytes: subtree.total_bytes,
                            top_level_is_directory: true,
                        },
                    );
                } else {
                    let size = entry.size.unwrap_or(0);
                    aggregate.file_count += 1;
                    aggregate.total_bytes += size;
                    per_path_unordered.insert(
                        source.clone(),
                        CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: size,
                            top_level_is_directory: false,
                        },
                    );
                    on_progress(aggregate.file_count);
                }
            }
        } else {
            // Cold cache for this parent. Delegate to the volume's own batch
            // scan: it preserves the MTP parent-grouping and SMB pipelined-stat
            // optimizations for cold paths.
            let group_result = volume
                .scan_for_copy_batch_with_progress(paths_in_group, Some(on_progress))
                .await?;
            aggregate.file_count += group_result.aggregate.file_count;
            aggregate.dir_count += group_result.aggregate.dir_count;
            aggregate.total_bytes += group_result.aggregate.total_bytes;
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
