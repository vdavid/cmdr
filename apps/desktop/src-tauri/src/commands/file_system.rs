//! Tauri commands for file system operations.

use crate::file_system::get_files_at_indices as ops_get_files_at_indices;
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
use crate::file_system::write_operations::{
    ConflictResolution, ScanPreviewStartResult, cancel_scan_preview as ops_cancel_scan_preview,
    is_scan_preview_complete as ops_is_scan_preview_complete, resolve_write_conflict as ops_resolve_write_conflict,
    start_scan_preview as ops_start_scan_preview,
};
use crate::file_system::{
    DirectorySortMode, FileEntry, ListingStartResult, ListingStats, OperationStatus, OperationSummary, ResortResult,
    ScanConflict, SortColumn, SortOrder, StreamingListingStartResult, VolumeCopyConfig, VolumeCopyScanResult,
    WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    cancel_all_write_operations as ops_cancel_all_write_operations, cancel_listing as ops_cancel_listing,
    cancel_write_operation as ops_cancel_write_operation, copy_between_volumes as ops_copy_between_volumes,
    copy_files_start as ops_copy_files_start, delete_files_start as ops_delete_files_start,
    find_file_index as ops_find_file_index, find_file_indices as ops_find_file_indices, get_file_at as ops_get_file_at,
    get_file_range as ops_get_file_range, get_listing_stats as ops_get_listing_stats,
    get_max_filename_width as ops_get_max_filename_width, get_operation_status as ops_get_operation_status,
    get_total_count as ops_get_total_count, get_volume_manager, list_active_operations as ops_list_active_operations,
    list_directory_end as ops_list_directory_end, list_directory_start_streaming as ops_list_directory_start_streaming,
    list_directory_start_with_volume as ops_list_directory_start_with_volume,
    move_between_volumes as ops_move_between_volumes, move_files_start as ops_move_files_start,
    refresh_listing_index_sizes as ops_refresh_listing_index_sizes, resort_listing as ops_resort_listing,
    scan_for_volume_copy as ops_scan_for_volume_copy, trash_files_start as ops_trash_files_start,
};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::mpsc::channel;
#[cfg(target_os = "macos")]
use tauri::Manager;
use tokio::time::Duration;

use super::util::{
    IpcError, TimedOut, blocking_result_with_timeout, blocking_with_timeout, blocking_with_timeout_flag,
};
use crate::file_system::validation::{MAX_NAME_BYTES, MAX_PATH_BYTES};

const PATH_EXISTS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathLimits {
    pub max_name_bytes: usize,
    pub max_path_bytes: usize,
}

#[tauri::command]
pub fn get_path_limits() -> PathLimits {
    PathLimits {
        max_name_bytes: MAX_NAME_BYTES,
        max_path_bytes: MAX_PATH_BYTES,
    }
}

#[tauri::command]
pub async fn path_exists(volume_id: Option<String>, path: String) -> bool {
    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let path_for_check = expanded_path.clone();
        return blocking_with_timeout(PATH_EXISTS_TIMEOUT, false, move || {
            volume.exists(Path::new(&path_for_check))
        })
        .await;
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let path_buf = PathBuf::from(expanded_path);
    blocking_with_timeout(PATH_EXISTS_TIMEOUT, false, move || path_buf.exists()).await
}

#[tauri::command]
pub async fn create_directory(
    app: tauri::AppHandle,
    volume_id: Option<String>,
    parent_path: String,
    name: String,
) -> Result<String, IpcError> {
    let (new_path, expanded_path) = create_directory_core(volume_id.clone(), &parent_path, &name).await?;

    // Synthetic diff only works for volumes backed by the local filesystem.
    // Protocol-only volumes (MTP) handle UI updates through their own event systems.
    if should_emit_synthetic_diff(volume_id.as_deref()) {
        emit_synthetic_entry_diff(&app, &new_path, &PathBuf::from(&expanded_path));
    }
    Ok(new_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn create_file(
    app: tauri::AppHandle,
    volume_id: Option<String>,
    parent_path: String,
    name: String,
) -> Result<String, IpcError> {
    let (new_path, expanded_path) = create_file_core(volume_id.clone(), &parent_path, &name).await?;

    if should_emit_synthetic_diff(volume_id.as_deref()) {
        emit_synthetic_entry_diff(&app, &new_path, &PathBuf::from(&expanded_path));
    }
    Ok(new_path.to_string_lossy().to_string())
}

/// Core mkdir logic, separated from the Tauri command so it can be tested without `AppHandle`.
async fn create_directory_core(
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), IpcError> {
    if name.is_empty() {
        return Err(IpcError::from_err("Folder name cannot be empty"));
    }
    if name.contains('/') || name.contains('\0') {
        return Err(IpcError::from_err("Folder name contains invalid characters"));
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" {
        expand_tilde(parent_path)
    } else {
        parent_path.to_string()
    };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(name);
        let new_path_clone = new_path.clone();
        let parent_path_owned = parent_path.to_string();
        let name_owned = name.to_string();

        // Use spawn_blocking to run the Volume operation in a context where
        // tokio::runtime::Handle::current() is available (needed for MtpVolume)
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                volume.create_directory(&new_path_clone).map_err(|e| match e {
                    crate::file_system::VolumeError::AlreadyExists(_) => {
                        format!("'{}' already exists", name_owned)
                    }
                    crate::file_system::VolumeError::PermissionDenied(_) => {
                        format!(
                            "Permission denied: cannot create '{}' in '{}'",
                            name_owned, parent_path_owned
                        )
                    }
                    _ => format!("Couldn't create folder: {}", e),
                })
            }),
        )
        .await
        .map_err(|_| IpcError::timeout())?
        .map_err(|e| IpcError::from_err(format!("Task failed: {}", e)))?
        .map_err(IpcError::from_err)?;

        return Ok((new_path, expanded_path));
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let mut new_path = PathBuf::from(&expanded_path);
    new_path.push(name);
    std::fs::create_dir(&new_path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!("'{}' already exists", name),
            std::io::ErrorKind::PermissionDenied => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Couldn't create folder: {}", e),
        })
        .map_err(IpcError::from_err)?;

    Ok((new_path, expanded_path))
}

/// Core file creation logic, separated from the Tauri command so it can be tested without `AppHandle`.
async fn create_file_core(
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), IpcError> {
    if name.is_empty() {
        return Err(IpcError::from_err("File name cannot be empty"));
    }
    if name.contains('/') || name.contains('\0') {
        return Err(IpcError::from_err("File name contains invalid characters"));
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" {
        expand_tilde(parent_path)
    } else {
        parent_path.to_string()
    };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(name);
        let new_path_clone = new_path.clone();
        let parent_path_owned = parent_path.to_string();
        let name_owned = name.to_string();

        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                volume.create_file(&new_path_clone, b"").map_err(|e| match e {
                    crate::file_system::VolumeError::AlreadyExists(_) => {
                        format!("'{}' already exists", name_owned)
                    }
                    crate::file_system::VolumeError::PermissionDenied(_) => {
                        format!(
                            "Permission denied: cannot create '{}' in '{}'",
                            name_owned, parent_path_owned
                        )
                    }
                    _ => format!("Couldn't create file: {}", e),
                })
            }),
        )
        .await
        .map_err(|_| IpcError::timeout())?
        .map_err(|e| IpcError::from_err(format!("Task failed: {}", e)))?
        .map_err(IpcError::from_err)?;

        return Ok((new_path, expanded_path));
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let mut new_path = PathBuf::from(&expanded_path);
    new_path.push(name);
    std::fs::File::create_new(&new_path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!("'{}' already exists", name),
            std::io::ErrorKind::PermissionDenied => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Couldn't create file: {}", e),
        })
        .map_err(IpcError::from_err)?;

    Ok((new_path, expanded_path))
}

// ============================================================================
// On-demand virtual scrolling API
// ============================================================================

/// Synchronous version — prefer `list_directory_start_streaming` for non-blocking operation.
#[tauri::command]
pub async fn list_directory_start(
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
) -> Result<ListingStartResult, IpcError> {
    let expanded_path = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded_path);
    let dir_sort_mode = directory_sort_mode.unwrap_or_default();
    blocking_result_with_timeout(Duration::from_secs(2), move || {
        ops_list_directory_start_with_volume("root", &path_buf, include_hidden, sort_by, sort_order, dir_sort_mode)
            .map_err(|e| format!("Failed to start directory listing '{}': {}", path, e))
    })
    .await
}

/// Returns immediately; reads in background.
/// Emits listing-progress, listing-complete, listing-error, listing-cancelled.
#[tauri::command]
#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: String,
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
    listing_id: String,
) -> Result<StreamingListingStartResult, String> {
    // Only expand tilde for local volumes (not MTP)
    let expanded_path = if volume_id == "root" {
        expand_tilde(&path)
    } else {
        path.clone()
    };
    let path_buf = PathBuf::from(&expanded_path);
    let dir_sort_mode = directory_sort_mode.unwrap_or_default();
    ops_list_directory_start_streaming(
        app,
        &volume_id,
        &path_buf,
        include_hidden,
        sort_by,
        sort_order,
        dir_sort_mode,
        listing_id,
    )
    .await
    .map_err(|e| format!("Failed to start directory listing '{}': {}", path, e))
}

#[tauri::command]
pub fn cancel_listing(listing_id: String) {
    ops_cancel_listing(&listing_id);
}

#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
#[tauri::command]
pub fn resort_listing(
    listing_id: String,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: Option<DirectorySortMode>,
    cursor_filename: Option<String>,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    all_selected: Option<bool>,
) -> Result<ResortResult, String> {
    ops_resort_listing(
        &listing_id,
        sort_by,
        sort_order,
        directory_sort_mode.unwrap_or_default(),
        cursor_filename.as_deref(),
        include_hidden,
        selected_indices.as_deref(),
        all_selected.unwrap_or(false),
    )
}

#[tauri::command]
pub fn get_file_range(
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_file_range(&listing_id, start, count, include_hidden)
}

#[tauri::command]
pub fn get_total_count(listing_id: String, include_hidden: bool) -> Result<usize, String> {
    ops_get_total_count(&listing_id, include_hidden)
}

/// Recalculates using font metrics — call after file watcher updates.
#[tauri::command]
pub fn get_max_filename_width(listing_id: String, include_hidden: bool) -> Result<Option<f32>, String> {
    ops_get_max_filename_width(&listing_id, include_hidden)
}

#[tauri::command]
pub fn find_file_index(listing_id: String, name: String, include_hidden: bool) -> Result<Option<usize>, String> {
    ops_find_file_index(&listing_id, &name, include_hidden)
}

#[tauri::command]
pub fn find_file_indices(
    listing_id: String,
    names: Vec<String>,
    include_hidden: bool,
) -> Result<std::collections::HashMap<String, usize>, String> {
    ops_find_file_indices(&listing_id, &names, include_hidden)
}

#[tauri::command]
pub fn get_file_at(listing_id: String, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    ops_get_file_at(&listing_id, index, include_hidden)
}

/// Gets file paths at specific frontend indices from a cached listing (batch version of path extraction).
/// Handles the parent ".." offset internally — callers pass frontend indices.
#[tauri::command]
pub fn get_paths_at_indices(
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
    has_parent: bool,
) -> Result<Vec<String>, String> {
    ops_get_paths_at_indices(&listing_id, &selected_indices, include_hidden, has_parent)
        .map(|paths| paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect())
}

/// Gets full FileEntry objects at specific backend indices from a cached listing.
/// Callers are responsible for any parent offset adjustment before passing indices.
#[tauri::command]
pub fn get_files_at_indices(
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_files_at_indices(&listing_id, &selected_indices, include_hidden)
}

#[tauri::command]
pub fn list_directory_end(listing_id: String) {
    ops_list_directory_end(&listing_id);
}

/// Force a re-read of a watched directory listing, emitting any diff.
/// Used after write operations (move) when the file watcher may not fire promptly.
#[tauri::command]
pub async fn refresh_listing(listing_id: String) -> TimedOut<()> {
    blocking_with_timeout_flag(Duration::from_secs(2), (), move || {
        crate::file_system::watcher::handle_directory_change(&listing_id);
    })
    .await
}

/// Returns total file/dir counts and sizes, plus selection stats if `selected_indices` is given.
#[tauri::command]
pub fn get_listing_stats(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
) -> Result<ListingStats, String> {
    ops_get_listing_stats(&listing_id, include_hidden, selected_indices.as_deref())
}

/// Re-enriches cached listing entries with fresh drive index data.
#[tauri::command]
pub fn refresh_listing_index_sizes(listing_id: String) -> Result<(), String> {
    ops_refresh_listing_index_sizes(&listing_id)
}

// ============================================================================
// Benchmarking support
// ============================================================================

/// Logs a frontend benchmark event to stderr (unified timeline with Rust events).
/// Only logs if RUSTY_COMMANDER_BENCHMARK=1 is set.
#[tauri::command]
#[allow(
    clippy::print_stderr,
    reason = "Benchmark output intentionally bypasses log framework"
)]
pub fn benchmark_log(message: String) {
    if crate::benchmark::is_enabled() {
        eprintln!("{}", message);
    }
}

// ============================================================================
// Write operations (copy, move, delete)
// ============================================================================

/// Emits write-progress, write-complete, write-error, write-cancelled.
#[tauri::command]
pub async fn copy_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    ops_copy_files_start(app, sources, destination, config).await
}

/// Uses rename() for same-filesystem (instant), copy+delete for cross-filesystem.
/// Same events as `copy_files`.
#[tauri::command]
pub async fn move_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    ops_move_files_start(app, sources, destination, config).await
}

/// Recursively deletes files and directories. Same events as `copy_files`.
/// When `volume_id` is provided and is not "root", routes through the Volume trait.
#[tauri::command]
pub async fn delete_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    volume_id: Option<String>,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };
    let config = config.unwrap_or_default();

    ops_delete_files_start(app, sources, config, volume_id).await
}

/// Moves files to macOS Trash. Same events as `copy_files` but with `operationType: trash`.
#[tauri::command]
pub async fn trash_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    item_sizes: Option<Vec<u64>>,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let config = config.unwrap_or_default();

    ops_trash_files_start(app, sources, item_sizes, config).await
}

#[tauri::command]
pub fn cancel_write_operation(operation_id: String, rollback: bool) {
    ops_cancel_write_operation(&operation_id, rollback);
}

#[tauri::command]
pub fn cancel_all_write_operations() {
    ops_cancel_all_write_operations();
}

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/// Scans source files for Copy dialog stats. Results are cached for reuse by the actual copy.
/// Emits scan-preview-progress, scan-preview-complete, scan-preview-error, scan-preview-cancelled.
///
/// When `source_volume_id` is provided and is not "root", the scan uses the Volume trait
/// (enabling MTP and other non-local volumes). Otherwise, uses `std::fs` for local scanning.
#[tauri::command]
pub async fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<String>,
    source_volume_id: Option<String>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: Option<u64>,
) -> ScanPreviewStartResult {
    let volume_id = source_volume_id.unwrap_or_else(|| "root".to_string());
    let is_local = volume_id == "root";

    // Only expand tilde for local paths
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };

    let source_volume = if is_local {
        None
    } else {
        get_volume_manager().get(&volume_id)
    };

    // Volume scans need a Tokio runtime handle (MtpVolume uses Handle::block_on).
    // Async Tauri commands run on the Tokio runtime, so Handle::current() works here.
    let runtime_handle = if source_volume.is_some() {
        Some(tokio::runtime::Handle::current())
    } else {
        None
    };

    let progress_interval = progress_interval_ms.unwrap_or(500);
    ops_start_scan_preview(
        app,
        sources,
        source_volume,
        sort_column,
        sort_order,
        progress_interval,
        runtime_handle,
    )
}

#[tauri::command]
pub fn cancel_scan_preview(preview_id: String) {
    ops_cancel_scan_preview(&preview_id);
}

/// Checks whether scan preview results are cached (scan completed successfully).
/// Used by TransferProgressDialog to handle the race condition where the scan completes
/// between TransferDialog closing and TransferProgressDialog mounting.
#[tauri::command]
pub fn check_scan_preview_status(preview_id: String) -> bool {
    ops_is_scan_preview_complete(&preview_id)
}

/// In Stop mode, the operation pauses on conflict and waits for this call to proceed.
#[tauri::command]
pub fn resolve_write_conflict(operation_id: String, resolution: ConflictResolution, apply_to_all: bool) {
    ops_resolve_write_conflict(&operation_id, resolution, apply_to_all);
}

#[tauri::command]
pub fn list_active_operations() -> Vec<OperationSummary> {
    ops_list_active_operations()
}

#[tauri::command]
pub fn get_operation_status(operation_id: String) -> Option<OperationStatus> {
    ops_get_operation_status(&operation_id)
}

// ============================================================================
// Drag operations
// ============================================================================

/// Initiates native drag from Rust directly, looking up paths from LISTING_CACHE (macOS only).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn start_selection_drag(
    app: tauri::AppHandle,
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
    has_parent: bool,
    mode: String,
    icon_path: String,
) -> Result<(), String> {
    // Get file paths from the cached listing
    let paths = ops_get_paths_at_indices(&listing_id, &selected_indices, include_hidden, has_parent)?;

    if paths.is_empty() {
        return Err("No valid files to drag".to_string());
    }

    // Get the main window
    let window = app.get_webview_window("main").ok_or("Main window not found")?;

    // Determine drag mode (Send-safe)
    let is_copy_mode = mode == "copy";

    // Store icon path for use in closure (PathBuf is Send)
    let icon_path_buf = PathBuf::from(icon_path);

    // Use a channel to get the result from the main thread
    let (tx, rx) = channel();

    // Run on main thread (required by macOS for drag operations)
    // Create DragItem inside the closure since it's not Send
    app.run_on_main_thread(move || {
        // Build DragItem inside the closure (not Send due to Data variant)
        let item = drag::DragItem::Files(paths);

        // Load icon from file path
        let icon = drag::Image::File(icon_path_buf);

        // Create options with the drag mode
        let options = drag::Options {
            skip_animatation_on_cancel_or_failure: false,
            mode: if is_copy_mode {
                drag::DragMode::Copy
            } else {
                drag::DragMode::Move
            },
        };

        let result = drag::start_drag(
            &window,
            item,
            icon,
            |_result, _cursor_pos| {
                // Callback when drag completes - we don't need to do anything here
            },
            options,
        );
        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to run on main thread: {}", e))?;

    // Wait for the result
    rx.recv()
        .map_err(|_| "Failed to receive drag result")?
        .map_err(|e| format!("Drag operation failed: {}", e))
}

/// Stub for non-macOS platforms. Returns an error since drag is not yet implemented.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn start_selection_drag(
    _app: tauri::AppHandle,
    _listing_id: String,
    _selected_indices: Vec<usize>,
    _include_hidden: bool,
    _has_parent: bool,
    _mode: String,
    _icon_path: String,
) -> Result<(), String> {
    Err("Drag operation is not yet supported on this platform".to_string())
}

// ============================================================================
// Unified volume copy commands
// ============================================================================

/// Unified copy across volume types (local, MTP, etc.). Same events as `copy_files`.
#[tauri::command]
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let source_volume = get_volume_manager()
        .get(&source_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    let dest_volume = get_volume_manager()
        .get(&dest_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: dest_volume_id.clone(),
            message: format!("Destination volume '{}' not found", dest_volume_id),
        })?;

    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);
    let config = config.unwrap_or_default();

    ops_copy_between_volumes(app, source_volume, source_paths, dest_volume, dest_path, config).await
}

/// Unified move across volume types. Same events as `copy_between_volumes`.
/// Handles same-volume (native rename/move), both-local (native move), and cross-volume (copy+delete).
#[tauri::command]
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let source_volume = get_volume_manager()
        .get(&source_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: source_volume_id.clone(),
            message: format!("Source volume '{}' not found", source_volume_id),
        })?;

    let dest_volume = get_volume_manager()
        .get(&dest_volume_id)
        .ok_or_else(|| WriteOperationError::IoError {
            path: dest_volume_id.clone(),
            message: format!("Destination volume '{}' not found", dest_volume_id),
        })?;

    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);
    let config = config.unwrap_or_default();

    ops_move_between_volumes(app, source_volume, source_paths, dest_volume, dest_path, config).await
}

/// Pre-flight scan: total count/bytes, available space, conflicts. Doesn't copy anything.
#[tauri::command]
pub async fn scan_volume_for_copy(
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    max_conflicts: Option<usize>,
) -> Result<VolumeCopyScanResult, IpcError> {
    let source_volume = get_volume_manager()
        .get(&source_volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Source volume '{}' not found", source_volume_id)))?;

    let dest_volume = get_volume_manager()
        .get(&dest_volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Destination volume '{}' not found", dest_volume_id)))?;

    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);
    let max_conflicts = max_conflicts.unwrap_or(100);

    // Run scan in blocking context for MTP volume support
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            ops_scan_for_volume_copy(&*source_volume, &source_paths, &*dest_volume, &dest_path, max_conflicts)
                .map_err(|e| e.to_string())
        }),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(|e| IpcError::from_err(format!("Scan task failed: {}", e)))?
    .map_err(IpcError::from_err)
}

/// Checks which source items already exist at the destination. Returns conflict details for UI.
#[tauri::command]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
) -> Result<Vec<ScanConflict>, IpcError> {
    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| IpcError::from_err(format!("Volume '{}' not found", volume_id)))?;

    let source_items: Vec<crate::file_system::SourceItemInfo> = source_items
        .into_iter()
        .map(|item| crate::file_system::SourceItemInfo {
            name: item.name,
            size: item.size,
            modified: item.modified,
        })
        .collect();
    let dest_path = PathBuf::from(dest_path);

    // Run in blocking context for MTP volume support
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            volume
                .scan_for_conflicts(&source_items, &dest_path)
                .map_err(|e| e.to_string())
        }),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(|e| IpcError::from_err(format!("Conflict scan task failed: {}", e)))?
    .map_err(IpcError::from_err)
}

/// Input type for source item information (used by scan_volume_for_conflicts).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItemInput {
    /// File/directory name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix timestamp in seconds).
    pub modified: Option<i64>,
}

// ============================================================================
// Self-drag overlay (dynamic drag image swapping)
// ============================================================================

/// Marks a self-drag as active and stores the rich image path so the native swizzle can:
/// - Hide the OS drag image over our window (swap to transparent in `draggingEntered:`)
/// - Show the rich image outside the window (swap back in `draggingExited:`)
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn prepare_self_drag_overlay(rich_image_path: String) {
    crate::drag_image_swap::set_self_drag_active(rich_image_path);
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn prepare_self_drag_overlay(_rich_image_path: String) {}

/// Clears self-drag state after drop or cancellation.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn clear_self_drag_overlay() {
    crate::drag_image_swap::clear_self_drag_state();
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn clear_self_drag_overlay() {}

/// Returns true if a synthetic entry diff should be emitted for this volume.
/// Protocol-only volumes (like MTP) don't support `std::fs` access, so synthetic
/// diffs would fail. These volumes handle UI updates through their own event systems.
fn should_emit_synthetic_diff(volume_id: Option<&str>) -> bool {
    match volume_id {
        None => true, // No volume_id means local filesystem
        Some(id) => get_volume_manager()
            .get(id)
            .is_none_or(|v| v.supports_local_fs_access()),
    }
}

/// Emits a synthetic `directory-diff` event for a newly created entry (file or directory).
///
/// Best-effort: if any step fails (stat, cache lookup, etc.) we log a warning
/// and return — the watcher will pick up the change later.
fn emit_synthetic_entry_diff(app: &tauri::AppHandle, entry_path: &Path, parent_path: &Path) {
    use crate::file_system::listing::reading::get_single_entry;
    use crate::file_system::listing::{find_listings_for_path, insert_entry_sorted};
    use crate::file_system::watcher::{DiffChange, DirectoryDiff};
    use tauri::Emitter;

    // 1. Construct a FileEntry for the new entry
    let mut entry = match get_single_entry(entry_path) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Synthetic entry diff: couldn't stat new entry: {}", e);
            return;
        }
    };

    // 2. Enrich with index data
    crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut entry));

    // 3. Find affected listings
    let listings = find_listings_for_path(parent_path);
    if listings.is_empty() {
        return;
    }

    // 4. For each listing, insert and emit
    for (listing_id, _sort_by, _sort_order, _dir_sort_mode) in listings {
        // insert_entry_sorted acquires LISTING_CACHE write lock and releases it on return
        let Some(index) = insert_entry_sorted(&listing_id, entry.clone()) else {
            continue; // Already exists or listing gone
        };

        // Increment sequence on CachedListing (after LISTING_CACHE write lock is released)
        let Some(sequence) = crate::file_system::listing::increment_sequence(&listing_id) else {
            continue;
        };

        let diff = DirectoryDiff {
            listing_id: listing_id.clone(),
            sequence,
            changes: vec![DiffChange {
                change_type: "add".to_string(),
                entry: entry.clone(),
                index,
            }],
        };

        if let Err(e) = app.emit("directory-diff", &diff) {
            log::warn!("Synthetic entry diff: couldn't emit event: {}", e);
        }
    }
}

// ============================================================================
// E2E test support (feature-gated)
// ============================================================================

/// Injects a listing error into an in-memory volume so the next `list_directory` call
/// returns a `VolumeError::IoError` with the given errno. The error is cleared after
/// one use, enabling retry testing.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
pub fn inject_listing_error(volume_id: String, error_code: i32) -> Result<(), String> {
    let volume = get_volume_manager()
        .get(&volume_id)
        .ok_or_else(|| format!("Volume `{}` not found", volume_id))?;
    volume.inject_error(error_code);
    Ok(())
}

/// Debug-only command that generates a real `FriendlyError` for the debug error pane preview.
///
/// Accepts either an errno code (for `IoError` variants) or a `VolumeError` variant name.
/// Optionally enriches with provider-specific suggestions when `provider_path` is set.
#[cfg(debug_assertions)]
#[tauri::command]
pub fn preview_friendly_error(
    error_code: Option<i32>,
    variant: Option<String>,
    provider_path: Option<String>,
) -> Result<crate::file_system::volume::friendly_error::FriendlyError, String> {
    use crate::file_system::volume::VolumeError;
    use crate::file_system::volume::friendly_error::{enrich_with_provider, friendly_error_from_volume_error};
    use std::path::Path;

    let path_str = provider_path
        .clone()
        .unwrap_or_else(|| "/Users/demo/Documents/test".to_string());
    let path = Path::new(&path_str);

    let volume_error = if let Some(code) = error_code {
        VolumeError::IoError {
            message: format!("os error {}", code),
            raw_os_error: Some(code),
        }
    } else if let Some(ref name) = variant {
        match name.as_str() {
            "NotFound" => VolumeError::NotFound(path_str.clone()),
            "PermissionDenied" => VolumeError::PermissionDenied(path_str.clone()),
            "AlreadyExists" => VolumeError::AlreadyExists(path_str.clone()),
            "NotSupported" => VolumeError::NotSupported,
            "DeviceDisconnected" => VolumeError::DeviceDisconnected("device went away".into()),
            "ReadOnly" => VolumeError::ReadOnly(path_str.clone()),
            "StorageFull" => VolumeError::StorageFull {
                message: "not enough space".into(),
            },
            "ConnectionTimeout" => VolumeError::ConnectionTimeout("timed out".into()),
            "Cancelled" => VolumeError::Cancelled("cancelled by user".into()),
            "IoError (no errno)" => VolumeError::IoError {
                message: "unknown I/O problem".into(),
                raw_os_error: None,
            },
            _ => return Err(format!("Unknown VolumeError variant: {}", name)),
        }
    } else {
        return Err("Provide either error_code or variant".into());
    };

    let mut friendly = friendly_error_from_volume_error(&volume_error, path);

    if provider_path.is_some() {
        enrich_with_provider(&mut friendly, path);
    }

    Ok(friendly)
}

/// Expands tilde (~) to the user's home directory.
pub(crate) fn expand_tilde(path: &str) -> String {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = dirs::home_dir()
    {
        return path.replacen("~", &home.to_string_lossy(), 1);
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_fs_cmd_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create test directory");
        dir
    }

    fn cleanup_test_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/Documents");
        assert!(expanded.starts_with('/'));
        assert!(expanded.contains("Documents"));
        assert!(!expanded.contains('~'));
    }

    #[test]
    fn test_expand_tilde_alone() {
        let expanded = expand_tilde("~");
        assert!(expanded.starts_with('/'));
        assert!(!expanded.contains('~'));
    }

    #[test]
    fn test_no_tilde() {
        let path = "/usr/local/bin";
        assert_eq!(expand_tilde(path), path);
    }

    #[tokio::test]
    async fn test_create_directory_success() {
        let tmp = create_test_dir("create_success");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "new-folder").await;
        assert!(result.is_ok());
        let (created_path, _) = result.unwrap();
        assert!(created_path.is_dir());
        assert!(created_path.to_string_lossy().ends_with("new-folder"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_already_exists() {
        let tmp = create_test_dir("create_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::create_dir(tmp.join("existing")).unwrap();
        let result = create_directory_core(None, &parent, "existing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_empty_name() {
        let tmp = create_test_dir("create_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_invalid_chars() {
        let tmp = create_test_dir("create_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory_core(None, &parent, "foo/bar").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));

        let result = create_directory_core(None, &parent, "foo\0bar").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_directory_nonexistent_parent() {
        let result = create_directory_core(None, "/nonexistent_path_12345", "test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_file_success() {
        let tmp = create_test_dir("create_file_success");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "new-file.txt").await;
        assert!(result.is_ok());
        let (created_path, _) = result.unwrap();
        assert!(created_path.is_file());
        assert!(created_path.to_string_lossy().ends_with("new-file.txt"));
        assert_eq!(fs::read(&created_path).unwrap(), b"");
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_already_exists() {
        let tmp = create_test_dir("create_file_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::write(tmp.join("existing.txt"), b"hello").unwrap();
        let result = create_file_core(None, &parent, "existing.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_empty_name() {
        let tmp = create_test_dir("create_file_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_create_file_invalid_chars() {
        let tmp = create_test_dir("create_file_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_file_core(None, &parent, "foo/bar.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));

        let result = create_file_core(None, &parent, "foo\0bar.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid characters"));
        cleanup_test_dir(&tmp);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_fast_closure_returns_value() {
        let result = blocking_with_timeout(Duration::from_secs(2), false, || true).await;
        assert!(result);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_slow_closure_returns_fallback() {
        let result = blocking_with_timeout(Duration::from_millis(50), false, || {
            std::thread::sleep(Duration::from_secs(2));
            true
        })
        .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_blocking_with_timeout_returns_custom_fallback() {
        let result = blocking_with_timeout(Duration::from_millis(50), 42, || {
            std::thread::sleep(Duration::from_secs(2));
            99
        })
        .await;
        assert_eq!(result, 42);
    }
}
