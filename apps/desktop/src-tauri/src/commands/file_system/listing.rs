//! Tauri commands for directory listing and virtual-scroll API.

use crate::file_system::get_files_at_indices as ops_get_files_at_indices;
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
use crate::file_system::{
    DirectorySortMode, FileEntry, ListingStartResult, ListingStats, ResortResult, SortColumn, SortOrder,
    StreamingListingStartResult, cancel_listing as ops_cancel_listing, find_file_index as ops_find_file_index,
    find_file_indices as ops_find_file_indices, get_file_at as ops_get_file_at, get_file_range as ops_get_file_range,
    get_listing_stats as ops_get_listing_stats, get_max_filename_width as ops_get_max_filename_width,
    get_total_count as ops_get_total_count, get_volume_manager, list_directory_end as ops_list_directory_end,
    list_directory_start_streaming as ops_list_directory_start_streaming,
    list_directory_start_with_volume as ops_list_directory_start_with_volume,
    refresh_listing_index_sizes as ops_refresh_listing_index_sizes, resort_listing as ops_resort_listing,
};
use std::path::{Path, PathBuf};
use tokio::time::Duration;

use crate::commands::util::{
    IpcError, TimedOut, blocking_result_with_timeout, blocking_with_timeout, blocking_with_timeout_flag,
};
use crate::file_system::validation::{MAX_NAME_BYTES, MAX_PATH_BYTES};

use super::expand_tilde;

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
