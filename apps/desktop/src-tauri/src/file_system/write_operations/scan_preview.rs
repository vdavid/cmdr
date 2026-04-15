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

use super::scan::{WalkContext, sort_files, walk_dir_recursive};
use super::state::{CachedScanResult, FileInfo, SCAN_PREVIEW_RESULTS, SCAN_PREVIEW_STATE, ScanPreviewState};
use super::types::{
    ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent,
    ScanPreviewStartResult,
};
use crate::file_system::listing::{SortColumn, SortOrder};
use crate::file_system::volume::CopyScanResult;
use crate::file_system::volume::Volume;

/// Starts a scan preview for the Copy dialog.
/// Returns a preview_id that can be used to cancel or to pass to copy_files.
///
/// When `source_volume` is provided, uses `Volume::scan_for_copy()` instead of `std::fs`,
/// enabling MTP and other non-local volumes to produce scan previews.
pub fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    source_volume: Option<Arc<dyn Volume>>,
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
            run_volume_scan_preview(app, preview_id_clone, sources, volume, state).await;
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

    let result: Result<(), String> = (|| {
        let ctx = WalkContext {
            progress_interval: state.progress_interval,
            is_cancelled: &|| state.cancelled.load(Ordering::Relaxed),
            on_io_error: &|_, e| e.to_string(),
            on_cancelled: &|| "Cancelled".to_string(),
            on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
            on_progress: &|files_found, dirs_found, bytes_found, current_path| {
                let _ = app.emit(
                    "scan-preview-progress",
                    ScanPreviewProgressEvent {
                        preview_id: preview_id.to_string(),
                        files_found,
                        dirs_found,
                        bytes_found,
                        current_path,
                    },
                );
            },
        };
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
/// Uses `Volume::scan_for_copy_batch()` to scan all sources in one call, allowing
/// volume implementations to batch I/O (for example, MTP groups by parent directory).
/// Emits the same events as `run_scan_preview` so the frontend can't tell the difference.
async fn run_volume_scan_preview(
    app: tauri::AppHandle,
    preview_id: String,
    sources: Vec<PathBuf>,
    volume: Arc<dyn Volume>,
    state: Arc<ScanPreviewState>,
) {
    use tauri::Emitter;

    let result: Result<CopyScanResult, String> = async {
        if state.cancelled.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }

        volume
            .scan_for_copy_batch(&sources)
            .await
            .map_err(|e| format!("Scan failed: {}", e))
    }
    .await;

    // Extract stats from the result for the completion event
    let (total_files, total_dirs, total_bytes) = match &result {
        Ok(scan) => (scan.file_count, scan.dir_count, scan.total_bytes),
        Err(_) => (0, 0, 0),
    };
    let result = result.map(|_| ());

    // Clean up state
    if let Ok(mut cache) = SCAN_PREVIEW_STATE.write() {
        cache.remove(&preview_id);
    }

    match result {
        Ok(()) => {
            if state.cancelled.load(Ordering::Relaxed) {
                let _ = app.emit(
                    "scan-preview-cancelled",
                    ScanPreviewCancelledEvent {
                        preview_id: preview_id.clone(),
                    },
                );
            } else {
                // Cache results — volume scans don't produce per-file FileInfo,
                // but the cache stores aggregate stats that copy_between_volumes can reuse.
                if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
                    cache.insert(
                        preview_id.clone(),
                        CachedScanResult {
                            files: Vec::new(),
                            dirs: Vec::new(),
                            file_count: total_files,
                            total_bytes,
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
