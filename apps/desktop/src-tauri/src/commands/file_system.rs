//! Tauri commands for file system operations.

use crate::file_system::{
    FileEntry, ListingStartResult, ResortResult, SortColumn, SortOrder, StreamingListingStartResult,
    cancel_listing as ops_cancel_listing, find_file_index as ops_find_file_index, get_file_at as ops_get_file_at,
    get_file_range as ops_get_file_range, get_max_filename_width as ops_get_max_filename_width,
    get_total_count as ops_get_total_count, list_directory_end as ops_list_directory_end,
    list_directory_start_streaming as ops_list_directory_start_streaming,
    list_directory_start_with_volume as ops_list_directory_start_with_volume, resort_listing as ops_resort_listing,
};
use std::path::PathBuf;

/// Checks if a path exists.
///
/// # Arguments
/// * `path` - The path to check. Supports tilde expansion (~).
///
/// # Returns
/// True if the path exists.
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    let expanded_path = expand_tilde(&path);
    let path_buf = PathBuf::from(expanded_path);
    path_buf.exists()
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
/// * `path` - The directory path to list. Supports tilde expansion (~).
/// * `include_hidden` - Whether to include hidden files in total count.
/// * `sort_by` - Column to sort by (name, extension, size, modified, created).
/// * `sort_order` - Ascending or descending.
#[tauri::command]
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    path: String,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    listing_id: String,
) -> Result<StreamingListingStartResult, String> {
    let expanded_path = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded_path);
    ops_list_directory_start_streaming(app, "root", &path_buf, include_hidden, sort_by, sort_order, listing_id)
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
#[tauri::command]
pub fn resort_listing(
    listing_id: String,
    sort_by: SortColumn,
    sort_order: SortOrder,
    cursor_filename: Option<String>,
    include_hidden: bool,
) -> Result<ResortResult, String> {
    ops_resort_listing(
        &listing_id,
        sort_by,
        sort_order,
        cursor_filename.as_deref(),
        include_hidden,
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

// ============================================================================
// Benchmarking support
// ============================================================================

/// Logs a frontend benchmark event to stderr (unified timeline with Rust events).
/// Only logs if RUSTY_COMMANDER_BENCHMARK=1 is set.
#[tauri::command]
pub fn benchmark_log(message: String) {
    if crate::benchmark::is_enabled() {
        eprintln!("{}", message);
    }
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
}
