//! Tauri commands for directory listing and virtual-scroll API.

use crate::file_system::get_files_at_indices as ops_get_files_at_indices;
use crate::file_system::get_paths_at_indices as ops_get_paths_at_indices;
use crate::file_system::{
    BriefColumnsError, DirectorySortMode, FileEntry, ListingStartResult, ListingStats, ResortResult, SortColumn,
    SortOrder, StreamingListingStartResult, cancel_listing as ops_cancel_listing,
    compute_brief_column_text_widths as ops_compute_brief_column_text_widths, find_file_index as ops_find_file_index,
    find_file_indices as ops_find_file_indices,
    fuzzy_find_first_match_in_listing as ops_fuzzy_find_first_match_in_listing, get_file_at as ops_get_file_at,
    get_file_range as ops_get_file_range, get_listing_stats as ops_get_listing_stats,
    get_max_filename_width as ops_get_max_filename_width, get_total_count as ops_get_total_count, get_volume_manager,
    list_directory_end as ops_list_directory_end, list_directory_start_streaming as ops_list_directory_start_streaming,
    list_directory_start_with_volume as ops_list_directory_start_with_volume,
    refresh_listing_index_sizes as ops_refresh_listing_index_sizes, resort_listing as ops_resort_listing,
};
use std::path::{Path, PathBuf};
use tokio::time::Duration;

use crate::commands::util::{IpcError, TimedOut, blocking_result_with_timeout};
use crate::file_system::validation::{MAX_NAME_BYTES, MAX_PATH_BYTES};

use super::expand_tilde;

const PATH_EXISTS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PathLimits {
    pub max_name_bytes: usize,
    pub max_path_bytes: usize,
}

#[tauri::command]
#[specta::specta]
pub fn get_path_limits() -> PathLimits {
    PathLimits {
        max_name_bytes: MAX_NAME_BYTES,
        max_path_bytes: MAX_PATH_BYTES,
    }
}

/// Returns `TimedOut<bool>` so the frontend can distinguish a real "doesn't exist"
/// from "we couldn't tell" (timeout, or SMB volume in `Disconnected` state). Without this
/// distinction, the directory-eviction poll in `FilePane.svelte` evicts users from a
/// network folder on any transient connection blip.
#[tauri::command]
#[specta::specta]
pub async fn path_exists(volume_id: Option<String>, path: String) -> TimedOut<bool> {
    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        // For SMB volumes, an immediate `false` from `exists()` may be the connection
        // being dead (`clone_session` returns `Err`) rather than the path actually missing.
        // Snapshot whether this is an SMB volume by whether it reports an SMB connection state.
        let is_smb = volume.smb_connection_state().is_some();

        let path_for_check = expanded_path.clone();
        match tokio::time::timeout(PATH_EXISTS_TIMEOUT, volume.exists(Path::new(&path_for_check))).await {
            Ok(exists) => {
                // SMB volume just transitioned to `Disconnected`? The `false` we got back
                // is meaningless — surface it as a timeout-equivalent so callers know.
                if !exists && is_smb && volume.smb_connection_state().is_none() {
                    return TimedOut {
                        data: false,
                        timed_out: true,
                    };
                }
                TimedOut {
                    data: exists,
                    timed_out: false,
                }
            }
            Err(_) => TimedOut {
                data: false,
                timed_out: true,
            },
        }
    } else {
        // Fallback for unknown volumes (shouldn't happen in practice)
        let path_buf = PathBuf::from(expanded_path);
        let result = tokio::time::timeout(
            PATH_EXISTS_TIMEOUT,
            tokio::task::spawn_blocking(move || path_buf.exists()),
        )
        .await;
        match result {
            Ok(Ok(exists)) => TimedOut {
                data: exists,
                timed_out: false,
            },
            _ => TimedOut {
                data: false,
                timed_out: true,
            },
        }
    }
}

// ============================================================================
// On-demand virtual scrolling API
// ============================================================================

/// Synchronous version — prefer `list_directory_start_streaming` for non-blocking operation.
#[tauri::command]
#[specta::specta]
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
    match tokio::time::timeout(
        Duration::from_secs(2),
        ops_list_directory_start_with_volume("root", &path_buf, include_hidden, sort_by, sort_order, dir_sort_mode),
    )
    .await
    {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(IpcError::from_err(format!(
            "Failed to start directory listing '{}': {}",
            path, e
        ))),
        Err(_) => Err(IpcError::timeout()),
    }
}

/// Returns immediately; reads in background.
/// Emits listing-progress, listing-complete, listing-error, listing-cancelled.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub fn cancel_listing(listing_id: String) {
    ops_cancel_listing(&listing_id);
}

#[allow(clippy::too_many_arguments, reason = "Tauri commands require top-level arguments")]
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub fn get_file_range(
    listing_id: String,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_file_range(&listing_id, start, count, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub fn get_total_count(listing_id: String, include_hidden: bool) -> Result<usize, String> {
    ops_get_total_count(&listing_id, include_hidden)
}

/// Recalculates using font metrics — call after file watcher updates.
#[tauri::command]
#[specta::specta]
pub fn get_max_filename_width(listing_id: String, include_hidden: bool) -> Result<Option<f32>, String> {
    ops_get_max_filename_width(&listing_id, include_hidden)
}

/// Returns the widest filename's text-only width (in px) per Brief-mode column.
///
/// Pure read path: takes a snapshot of `LISTING_CACHE` for `listing_id` and
/// measures each column's widest filename with `font_metrics::calculate_max_width`.
/// The FE applies chrome + clamp on top.
///
/// Error mapping (consumed by the FE):
/// - `font_metrics_not_ready` — at least one column had no measurable filename
///   in the font cache. FE retries after `ensureFontMetricsLoaded` resolves.
/// - `invalid_items_per_column` — caller sent 0; FE clamps to >= 1 normally.
/// - `listing_not_found:{id}` — listing already ended (or never started).
/// - Anything else is a pass-through (cache-lock poisoning etc.).
#[tauri::command]
#[specta::specta]
pub async fn get_brief_column_text_widths(
    listing_id: String,
    items_per_column: usize,
    has_parent: bool,
    font_id: String,
    include_hidden: bool,
) -> Result<Vec<f32>, IpcError> {
    blocking_result_with_timeout(Duration::from_secs(2), move || {
        ops_compute_brief_column_text_widths(&listing_id, items_per_column, has_parent, &font_id, include_hidden)
            .map_err(|e| match e {
                BriefColumnsError::FontMetricsNotReady => "font_metrics_not_ready".to_string(),
                BriefColumnsError::InvalidItemsPerColumn => "invalid_items_per_column".to_string(),
                BriefColumnsError::ListingNotFound(id) => format!("listing_not_found:{}", id),
                BriefColumnsError::Other(msg) => msg,
            })
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn find_file_index(listing_id: String, name: String, include_hidden: bool) -> Result<Option<usize>, String> {
    ops_find_file_index(&listing_id, &name, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub fn find_file_indices(
    listing_id: String,
    names: Vec<String>,
    include_hidden: bool,
) -> Result<std::collections::HashMap<String, usize>, String> {
    ops_find_file_indices(&listing_id, &names, include_hidden)
}

/// Returns the backend index of the highest-scoring fuzzy match for `query` in
/// the cached listing, or `None` when nothing matches. Powers the type-to-jump
/// feature in `FilePane.svelte`. Hidden entries are skipped when `include_hidden`
/// is false. The frontend adjusts for the synthetic `..` parent offset before
/// setting the cursor (the parent entry is never in `LISTING_CACHE`).
#[tauri::command]
#[specta::specta]
pub async fn find_first_fuzzy_match(
    listing_id: String,
    query: String,
    include_hidden: bool,
) -> Result<Option<usize>, IpcError> {
    ops_fuzzy_find_first_match_in_listing(&listing_id, &query, include_hidden).map_err(IpcError::from_err)
}

#[tauri::command]
#[specta::specta]
pub fn get_file_at(listing_id: String, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    ops_get_file_at(&listing_id, index, include_hidden)
}

/// Gets file paths at specific frontend indices from a cached listing (batch version of path extraction).
/// Handles the parent ".." offset internally — callers pass frontend indices.
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub fn get_files_at_indices(
    listing_id: String,
    selected_indices: Vec<usize>,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    ops_get_files_at_indices(&listing_id, &selected_indices, include_hidden)
}

#[tauri::command]
#[specta::specta]
pub fn list_directory_end(listing_id: String) {
    ops_list_directory_end(&listing_id);
}

/// Force a re-read of a watched directory listing, emitting any diff.
/// Used after write operations (move) when the file watcher may not fire promptly.
#[tauri::command]
#[specta::specta]
pub async fn refresh_listing(listing_id: String) -> TimedOut<()> {
    let timed_out = tokio::time::timeout(Duration::from_secs(2), async {
        crate::file_system::watcher::handle_directory_change(&listing_id).await;
    })
    .await
    .is_err();
    TimedOut { data: (), timed_out }
}

/// Returns total file/dir counts and sizes, plus selection stats if `selected_indices` is given.
#[tauri::command]
#[specta::specta]
pub fn get_listing_stats(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
) -> Result<ListingStats, String> {
    ops_get_listing_stats(&listing_id, include_hidden, selected_indices.as_deref())
}

/// Re-enriches cached listing entries with fresh drive index data.
#[tauri::command]
#[specta::specta]
pub fn refresh_listing_index_sizes(listing_id: String) -> Result<(), String> {
    ops_refresh_listing_index_sizes(&listing_id)
}

// ============================================================================
// Benchmarking support
// ============================================================================

/// Logs a frontend benchmark event to stderr (unified timeline with Rust events).
/// Only logs if RUSTY_COMMANDER_BENCHMARK=1 is set.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::print_stderr,
    reason = "Benchmark output intentionally bypasses log framework"
)]
pub fn benchmark_log(message: String) {
    if crate::benchmark::is_enabled() {
        eprintln!("{}", message);
    }
}
