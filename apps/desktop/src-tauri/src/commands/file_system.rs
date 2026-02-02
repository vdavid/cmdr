//! Tauri commands for file system operations.

#[cfg(target_os = "macos")]
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
use crate::file_system::write_operations::{
    ConflictResolution, ScanPreviewStartResult, cancel_scan_preview as ops_cancel_scan_preview,
    resolve_write_conflict as ops_resolve_write_conflict, start_scan_preview as ops_start_scan_preview,
};
use crate::file_system::{
    FileEntry, ListingStartResult, ListingStats, OperationStatus, OperationSummary, ResortResult, SortColumn,
    SortOrder, StreamingListingStartResult, WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    cancel_listing as ops_cancel_listing, cancel_write_operation as ops_cancel_write_operation,
    copy_files_start as ops_copy_files_start, delete_files_start as ops_delete_files_start,
    find_file_index as ops_find_file_index, get_file_at as ops_get_file_at, get_file_range as ops_get_file_range,
    get_listing_stats as ops_get_listing_stats, get_max_filename_width as ops_get_max_filename_width,
    get_operation_status as ops_get_operation_status, get_total_count as ops_get_total_count,
    get_volume_manager, list_active_operations as ops_list_active_operations,
    list_directory_end as ops_list_directory_end, list_directory_start_streaming as ops_list_directory_start_streaming,
    list_directory_start_with_volume as ops_list_directory_start_with_volume, move_files_start as ops_move_files_start,
    resort_listing as ops_resort_listing,
};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::mpsc::channel;
#[cfg(target_os = "macos")]
use tauri::Manager;

/// Checks if a path exists.
///
/// # Arguments
/// * `volume_id` - Optional volume ID. Defaults to "root" for local filesystem.
/// * `path` - The path to check. Supports tilde expansion (~) for local volumes.
///
/// # Returns
/// True if the path exists.
#[tauri::command]
pub fn path_exists(volume_id: Option<String>, path: String) -> bool {
    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        return volume.exists(Path::new(&expanded_path));
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let path_buf = PathBuf::from(expanded_path);
    path_buf.exists()
}

/// Creates a new directory.
///
/// # Arguments
/// * `volume_id` - Optional volume ID. Defaults to "root" for local filesystem.
/// * `parent_path` - The parent directory path. Supports tilde expansion (~) for local volumes.
/// * `name` - The folder name to create.
///
/// # Returns
/// The full path of the created directory, or an error message.
#[tauri::command]
pub fn create_directory(volume_id: Option<String>, parent_path: String, name: String) -> Result<String, String> {
    if name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\0') {
        return Err("Folder name contains invalid characters".to_string());
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" { expand_tilde(&parent_path) } else { parent_path.clone() };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(&name);
        volume.create_directory(&new_path).map_err(|e| match e {
            crate::file_system::VolumeError::AlreadyExists(_) => format!("'{}' already exists", name),
            crate::file_system::VolumeError::PermissionDenied(_) => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Failed to create folder: {}", e),
        })?;
        return Ok(new_path.to_string_lossy().to_string());
    }

    // Fallback for unknown volumes (shouldn't happen in practice)
    let mut new_path = PathBuf::from(&expanded_path);
    new_path.push(&name);
    std::fs::create_dir(&new_path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => format!("'{}' already exists", name),
        std::io::ErrorKind::PermissionDenied => {
            format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
        }
        _ => format!("Failed to create folder: {}", e),
    })?;
    Ok(new_path.to_string_lossy().to_string())
}

// ============================================================================
// On-demand virtual scrolling API
// ============================================================================

/// Starts a new directory listing (synchronous version).
///
/// Reads the directory once, caches it, and returns listing ID + total count.
/// Frontend then fetches visible ranges on demand via `get_file_range`.
///
/// NOTE: This is the synchronous version. For non-blocking operation, use
/// `list_directory_start_streaming` instead.
///
/// # Arguments
/// * `path` - The directory path to list. Supports tilde expansion (~).
/// * `include_hidden` - Whether to include hidden files in total count.
/// * `sort_by` - Column to sort by (name, extension, size, modified, created).
/// * `sort_order` - Ascending or descending.
#[tauri::command]
pub fn list_directory_start(
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
) -> Result<ListingStartResult, String> {
    let expanded_path = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded_path);
    ops_list_directory_start_with_volume("root", &path_buf, include_hidden, sort_by, sort_order)
        .map_err(|e| format!("Failed to start directory listing '{}': {}", path, e))
}

/// Starts a new streaming directory listing (async version).
///
/// Returns immediately with a listing ID and "loading" status. The actual
/// directory reading happens in a background task, with progress events
/// emitted every 500ms.
///
/// # Events emitted
/// * `listing-progress` - Every 500ms with `{ listingId, loadedCount }`
/// * `listing-complete` - When done with `{ listingId, totalCount, maxFilenameWidth }`
/// * `listing-error` - On error with `{ listingId, message }`
/// * `listing-cancelled` - If cancelled with `{ listingId }`
///
/// # Arguments
/// * `app` - Tauri app handle (injected by Tauri).
/// * `volume_id` - The volume ID (e.g., "root", "mtp-20-5:65537").
/// * `path` - The directory path to list. Supports tilde expansion (~) for local volumes.
/// * `include_hidden` - Whether to include hidden files in total count.
/// * `sort_by` - Column to sort by (name, extension, size, modified, created).
/// * `sort_order` - Ascending or descending.
#[tauri::command]
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: String,
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    listing_id: String,
) -> Result<StreamingListingStartResult, String> {
    // Only expand tilde for local volumes (not MTP)
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path.clone() };
    let path_buf = PathBuf::from(&expanded_path);
    ops_list_directory_start_streaming(app, &volume_id, &path_buf, include_hidden, sort_by, sort_order, listing_id)
        .await
        .map_err(|e| format!("Failed to start directory listing '{}': {}", path, e))
}

/// Cancels an in-progress streaming directory listing.
///
/// Sets the cancellation flag, which will be checked by the background task.
/// The task will emit a `listing-cancelled` event when it stops.
///
/// # Arguments
/// * `listing_id` - The listing ID to cancel.
#[tauri::command]
pub fn cancel_listing(listing_id: String) {
    ops_cancel_listing(&listing_id);
}

/// Re-sorts an existing cached listing in-place.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `sort_by` - Column to sort by.
/// * `sort_order` - Ascending or descending.
/// * `cursor_filename` - Optional filename to track; returns its new index after sorting.
/// * `include_hidden` - Whether to include hidden files when calculating cursor index.
/// * `selected_indices` - Optional indices of selected files to track through re-sort.
/// * `all_selected` - If true, all files are selected (optimization).
#[tauri::command]
pub fn resort_listing(
    listing_id: String,
    sort_by: SortColumn,
    sort_order: SortOrder,
    cursor_filename: Option<String>,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    all_selected: Option<bool>,
) -> Result<ResortResult, String> {
    ops_resort_listing(
        &listing_id,
        sort_by,
        sort_order,
        cursor_filename.as_deref(),
        include_hidden,
        selected_indices.as_deref(),
        all_selected.unwrap_or(false),
    )
}

/// Gets a range of entries from a cached listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `start` - Start index (0-based).
/// * `count` - Number of entries to return.
/// * `include_hidden` - Whether to include hidden files.
#[tauri::command]
pub fn get_file_range(
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_file_range(&listing_id, start, count, include_hidden)
}

/// Gets total count of entries in a cached listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `include_hidden` - Whether to include hidden files in count.
#[tauri::command]
pub fn get_total_count(listing_id: String, include_hidden: bool) -> Result<usize, String> {
    ops_get_total_count(&listing_id, include_hidden)
}

/// Gets the maximum filename width for a cached listing.
///
/// Recalculates the width based on current entries using font metrics.
/// This is useful after files are added/removed by the file watcher.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `include_hidden` - Whether to include hidden files.
#[tauri::command]
pub fn get_max_filename_width(listing_id: String, include_hidden: bool) -> Result<Option<f32>, String> {
    ops_get_max_filename_width(&listing_id, include_hidden)
}

/// Finds the index of a file by name in a cached listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `name` - File name to find.
/// * `include_hidden` - Whether to include hidden files when calculating index.
#[tauri::command]
pub fn find_file_index(listing_id: String, name: String, include_hidden: bool) -> Result<Option<usize>, String> {
    ops_find_file_index(&listing_id, &name, include_hidden)
}

/// Gets a single file at the given index.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `index` - Index of the file to get.
/// * `include_hidden` - Whether to include hidden files when calculating index.
#[tauri::command]
pub fn get_file_at(listing_id: String, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    ops_get_file_at(&listing_id, index, include_hidden)
}

/// Ends a directory listing and cleans up the cache.
///
/// # Arguments
/// * `listing_id` - The listing ID to clean up.
#[tauri::command]
pub fn list_directory_end(listing_id: String) {
    ops_list_directory_end(&listing_id);
}

/// Gets statistics about a cached listing.
///
/// Returns total file/dir counts and sizes. If `selected_indices` is provided,
/// also returns statistics for the selected items.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`.
/// * `include_hidden` - Whether to include hidden files in calculations.
/// * `selected_indices` - Optional indices of selected files to calculate selection stats.
#[tauri::command]
pub fn get_listing_stats(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
) -> Result<ListingStats, String> {
    ops_get_listing_stats(&listing_id, include_hidden, selected_indices.as_deref())
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

/// Starts a copy operation in the background.
///
/// # Events emitted
/// * `write-progress` - Every 200ms (configurable) with progress
/// * `write-complete` - On success
/// * `write-error` - On error
/// * `write-cancelled` - If cancelled
///
/// # Arguments
/// * `app` - Tauri app handle (injected by Tauri).
/// * `sources` - List of source file/directory paths. Supports tilde expansion (~).
/// * `destination` - Destination directory path. Supports tilde expansion (~).
/// * `config` - Optional configuration (progress interval, overwrite).
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

/// Starts a move operation in the background.
///
/// Uses rename() for same-filesystem moves (instant).
/// Falls back to copy+delete for cross-filesystem moves.
///
/// # Events emitted
/// * `write-progress` - Every 200ms (configurable) with progress
/// * `write-complete` - On success
/// * `write-error` - On error
/// * `write-cancelled` - If cancelled
///
/// # Arguments
/// * `app` - Tauri app handle (injected by Tauri).
/// * `sources` - List of source file/directory paths. Supports tilde expansion (~).
/// * `destination` - Destination directory path. Supports tilde expansion (~).
/// * `config` - Optional configuration (progress interval, overwrite).
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

/// Starts a delete operation in the background.
///
/// Recursively deletes files and directories.
///
/// # Events emitted
/// * `write-progress` - Every 200ms (configurable) with progress
/// * `write-complete` - On success
/// * `write-error` - On error
/// * `write-cancelled` - If cancelled
///
/// # Arguments
/// * `app` - Tauri app handle (injected by Tauri).
/// * `sources` - List of file/directory paths to delete. Supports tilde expansion (~).
/// * `config` - Optional configuration (progress interval).
#[tauri::command]
pub async fn delete_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let config = config.unwrap_or_default();

    ops_delete_files_start(app, sources, config).await
}

/// Cancels an in-progress write operation.
///
/// Sets the cancellation flag, which will be checked by the background task.
/// The task will emit a `write-cancelled` event when it stops.
///
/// # Arguments
/// * `operation_id` - The operation ID to cancel.
/// * `rollback` - If true, delete any partial files created. If false, keep them.
#[tauri::command]
pub fn cancel_write_operation(operation_id: String, rollback: bool) {
    ops_cancel_write_operation(&operation_id, rollback);
}

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/// Starts a scan preview for the Copy dialog.
///
/// This immediately starts scanning the source files in the background and emits
/// progress events. The scan results are cached and can be reused when starting
/// the actual copy operation.
///
/// # Events emitted
/// * `scan-preview-progress` - Based on progress_interval_ms setting
/// * `scan-preview-complete` - When scanning finishes
/// * `scan-preview-error` - On error
/// * `scan-preview-cancelled` - If cancelled
///
/// # Arguments
/// * `app` - Tauri app handle (injected by Tauri).
/// * `sources` - List of source file/directory paths. Supports tilde expansion (~).
/// * `sort_column` - Column to sort files by.
/// * `sort_order` - Sort order (ascending/descending).
/// * `progress_interval_ms` - Progress update interval in milliseconds (default: 500).
#[tauri::command]
pub fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<String>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: Option<u64>,
) -> ScanPreviewStartResult {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let progress_interval = progress_interval_ms.unwrap_or(500);
    ops_start_scan_preview(app, sources, sort_column, sort_order, progress_interval)
}

/// Cancels a running scan preview.
///
/// Sets the cancellation flag, which will stop the scan. A `scan-preview-cancelled`
/// event will be emitted when the scan stops.
///
/// # Arguments
/// * `preview_id` - The preview ID to cancel.
#[tauri::command]
pub fn cancel_scan_preview(preview_id: String) {
    ops_cancel_scan_preview(&preview_id);
}

/// Resolves a pending conflict for an in-progress write operation.
///
/// When an operation encounters a conflict in Stop mode, it emits a `write-conflict`
/// event and waits for this function to be called. The operation will then proceed
/// with the chosen resolution.
///
/// # Arguments
/// * `operation_id` - The operation ID that has a pending conflict.
/// * `resolution` - How to resolve the conflict (skip, overwrite, or rename).
/// * `apply_to_all` - If true, apply this resolution to all future conflicts in this operation.
#[tauri::command]
pub fn resolve_write_conflict(operation_id: String, resolution: ConflictResolution, apply_to_all: bool) {
    ops_resolve_write_conflict(&operation_id, resolution, apply_to_all);
}

/// Lists all active write operations.
///
/// Returns a list of operation summaries for all currently running operations.
/// This is useful for showing a global progress view or managing multiple concurrent operations.
#[tauri::command]
pub fn list_active_operations() -> Vec<OperationSummary> {
    ops_list_active_operations()
}

/// Gets the detailed status of a specific write operation.
///
/// Returns the current status including phase, progress, and file information.
/// Returns None if the operation is not found (either never existed or already completed).
///
/// # Arguments
/// * `operation_id` - The operation ID to query.
#[tauri::command]
pub fn get_operation_status(operation_id: String) -> Option<OperationStatus> {
    ops_get_operation_status(&operation_id)
}

// ============================================================================
// Drag operations
// ============================================================================

/// Starts a native drag operation for selected files from a cached listing (macOS only).
///
/// This initiates the drag from Rust directly, avoiding IPC transfer of file paths.
/// The paths are looked up from LISTING_CACHE using the provided indices.
///
/// # Arguments
/// * `app` - Tauri app handle for accessing the window
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `selected_indices` - Frontend indices of selected files
/// * `include_hidden` - Whether hidden files are shown (affects index mapping)
/// * `has_parent` - Whether the ".." entry is shown at index 0
/// * `mode` - Drag mode: "copy" or "move"
/// * `icon_path` - Path to the drag preview icon (temp file)
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

/// Expands tilde (~) to the user's home directory.
fn expand_tilde(path: &str) -> String {
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

    #[test]
    fn test_create_directory_success() {
        let tmp = create_test_dir("create_success");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory(None, parent, "new-folder".to_string());
        assert!(result.is_ok());
        let created_path = result.unwrap();
        assert!(PathBuf::from(&created_path).is_dir());
        assert!(created_path.ends_with("new-folder"));
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn test_create_directory_already_exists() {
        let tmp = create_test_dir("create_exists");
        let parent = tmp.to_string_lossy().to_string();
        fs::create_dir(tmp.join("existing")).unwrap();
        let result = create_directory(None, parent, "existing".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn test_create_directory_empty_name() {
        let tmp = create_test_dir("create_empty");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory(None, parent, "".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn test_create_directory_invalid_chars() {
        let tmp = create_test_dir("create_invalid");
        let parent = tmp.to_string_lossy().to_string();
        let result = create_directory(None, parent.clone(), "foo/bar".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));

        let result = create_directory(None, parent, "foo\0bar".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn test_create_directory_nonexistent_parent() {
        let result = create_directory(None, "/nonexistent_path_12345".to_string(), "test".to_string());
        assert!(result.is_err());
    }
}
