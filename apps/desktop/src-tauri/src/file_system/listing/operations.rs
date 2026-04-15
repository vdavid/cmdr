//! Directory listing lifecycle, cache API, sorting, and statistics.
//!
//! This is the synchronous, frontend-facing API. Low-level disk I/O is in reading.rs,
//! async streaming is in streaming.rs.

#![allow(dead_code, reason = "Boilerplate for future use")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::benchmark;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};
use crate::file_system::watcher::{start_watching, stop_watching};

/// Returns true if the entry is not a hidden dotfile.
fn is_visible(entry: &FileEntry) -> bool {
    !entry.name.starts_with('.')
}

fn visible_entries<'a>(entries: &'a [FileEntry], include_hidden: bool) -> Box<dyn Iterator<Item = &'a FileEntry> + 'a> {
    if include_hidden {
        Box::new(entries.iter())
    } else {
        Box::new(entries.iter().filter(|e| is_visible(e)))
    }
}

// ============================================================================
// Listing lifecycle
// ============================================================================

/// Result of starting a new directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingStartResult {
    pub listing_id: String,
    pub total_count: usize,
    /// In pixels, for Brief mode columns. None if font metrics are not available.
    pub max_filename_width: Option<f32>,
}

/// Starts a new directory listing.
///
/// Reads the directory once, caches it, and returns listing ID + total count.
/// Frontend then fetches visible ranges on demand via `get_file_range`.
pub fn list_directory_start(path: &Path, include_hidden: bool) -> Result<ListingStartResult, std::io::Error> {
    list_directory_start_with_volume(
        "root",
        path,
        include_hidden,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
}

/// Starts a new directory listing using a specific volume.
///
/// This is the internal implementation that supports multi-volume access.
pub fn list_directory_start_with_volume(
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> Result<ListingStartResult, std::io::Error> {
    // Reset benchmark epoch for this navigation
    benchmark::reset_epoch();
    benchmark::log_event_value("list_directory_start CALLED", path.display());

    // Get the volume from VolumeManager
    let volume = crate::file_system::get_volume_manager().get(volume_id).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Volume '{}' not found", volume_id),
        )
    })?;

    // Use the Volume trait to list the directory
    let all_entries = tokio::runtime::Handle::current()
        .block_on(volume.list_directory(path))
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    benchmark::log_event_value("volume.list_directory COMPLETE, entries", all_entries.len());

    // Generate listing ID
    let listing_id = Uuid::new_v4().to_string();

    let total_count = visible_entries(&all_entries, include_hidden).count();

    // Enrich directory entries with index data (recursive_size etc.) before sorting,
    // so that sort-by-size works correctly for directories.
    let mut all_entries = all_entries;
    crate::indexing::enrich_entries_with_index(&mut all_entries);
    crate::indexing::trigger_verification(&path.to_string_lossy());

    // Sort the entries
    sort_entries(&mut all_entries, sort_by, sort_order, dir_sort_mode);

    // Cache the entries FIRST (watcher will read from here)
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.insert(
            listing_id.clone(),
            CachedListing {
                volume_id: volume_id.to_string(),
                path: path.to_path_buf(),
                entries: all_entries.clone(),
                sort_by,
                sort_order,
                directory_sort_mode: dir_sort_mode,
                sequence: std::sync::atomic::AtomicU64::new(0),
            },
        );
    }

    // Start watching the directory (only if volume supports it)
    // TODO: Update watcher to be volume-aware
    if volume.supports_watching()
        && let Err(e) = start_watching(&listing_id, path)
    {
        log::warn!("Failed to start watcher: {}", e);
        // Continue anyway - watcher is optional enhancement
    }

    // Calculate max filename width if font metrics are available
    let max_filename_width = {
        let font_id = "system-400-12"; // Default font for now
        let filenames: Vec<&str> = all_entries.iter().map(|e| e.name.as_str()).collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    };

    benchmark::log_event("list_directory_start RETURNING");
    Ok(ListingStartResult {
        listing_id,
        total_count,
        max_filename_width,
    })
}

/// Ends a directory listing and cleans up the cache.
pub fn list_directory_end(listing_id: &str) {
    // Stop the file watcher
    stop_watching(listing_id);

    // Remove from listing cache
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.remove(listing_id);
    }
}

// ============================================================================
// On-demand virtual scrolling API (cache accessors)
// ============================================================================

/// Gets a range of entries from a cached listing.
pub fn get_file_range(
    listing_id: &str,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let entries: Vec<FileEntry> = visible_entries(&listing.entries, include_hidden)
        .skip(start)
        .take(count)
        .cloned()
        .collect();

    Ok(entries)
}

/// Gets total count of entries in a cached listing.
pub fn get_total_count(listing_id: &str, include_hidden: bool) -> Result<usize, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    Ok(visible_entries(&listing.entries, include_hidden).count())
}

/// Gets the maximum filename width for a cached listing.
///
/// Recalculates the width based on current entries using font metrics.
/// Useful after files are added/removed by the file watcher.
pub fn get_max_filename_width(listing_id: &str, include_hidden: bool) -> Result<Option<f32>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let font_id = "system-400-12"; // Default font (must match list_directory_start_with_volume)

    let filenames: Vec<&str> = visible_entries(&listing.entries, include_hidden)
        .map(|e| e.name.as_str())
        .collect();
    let max_width = crate::font_metrics::calculate_max_width(&filenames, font_id);

    Ok(max_width)
}

/// Finds the index of a file by name in a cached listing.
pub fn find_file_index(listing_id: &str, name: &str, include_hidden: bool) -> Result<Option<usize>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    Ok(visible_entries(&listing.entries, include_hidden).position(|e| e.name == name))
}

/// Finds the indices of multiple files by name in a cached listing (batch version of `find_file_index`).
///
/// Single pass over cached entries, O(entries + names). Returns only found names as keys.
pub fn find_file_indices(
    listing_id: &str,
    names: &[String],
    include_hidden: bool,
) -> Result<HashMap<String, usize>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let lookup: std::collections::HashSet<&str> = names.iter().map(|n| n.as_str()).collect();
    let mut result = HashMap::with_capacity(names.len());

    for (idx, entry) in visible_entries(&listing.entries, include_hidden).enumerate() {
        if lookup.contains(entry.name.as_str()) {
            result.insert(entry.name.clone(), idx);
        }
    }

    Ok(result)
}

/// Gets a single file at the given index.
pub fn get_file_at(listing_id: &str, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let result = visible_entries(&listing.entries, include_hidden).nth(index).cloned();
    if result.is_none() {
        let total = visible_entries(&listing.entries, include_hidden).count();
        log::error!(
            "get_file_at: index {} out of bounds (listing has {} entries) - frontend/backend index mismatch!",
            index,
            total
        );
    }
    Ok(result)
}

/// Gets file paths at specific indices from a cached listing.
///
/// Optimized for drag operations where we only need paths, not full FileEntry objects.
pub fn get_paths_at_indices(
    listing_id: &str,
    selected_indices: &[usize],
    include_hidden: bool,
    has_parent: bool,
) -> Result<Vec<PathBuf>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let visible: Vec<&FileEntry> = visible_entries(&listing.entries, include_hidden).collect();

    let mut paths = Vec::with_capacity(selected_indices.len());
    for &frontend_idx in selected_indices {
        // Skip ".." entry (frontend index 0 when has_parent is true)
        if has_parent && frontend_idx == 0 {
            continue;
        }

        // Convert frontend index to backend index
        let backend_idx = if has_parent { frontend_idx - 1 } else { frontend_idx };

        if let Some(entry) = visible.get(backend_idx) {
            paths.push(PathBuf::from(&entry.path));
        }
    }

    Ok(paths)
}

/// Gets full FileEntry objects at specific backend indices from a cached listing.
///
/// Unlike `get_paths_at_indices` (which takes frontend indices and handles the parent offset),
/// this takes backend indices directly — the caller is responsible for any offset adjustment.
/// Used by the delete dialog where full entry metadata (name, size, isDirectory, etc.) is needed.
pub fn get_files_at_indices(
    listing_id: &str,
    selected_indices: &[usize],
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let visible: Vec<&FileEntry> = visible_entries(&listing.entries, include_hidden).collect();

    let mut entries = Vec::with_capacity(selected_indices.len());
    for &idx in selected_indices {
        if let Some(entry) = visible.get(idx) {
            entries.push((*entry).clone());
        }
    }

    Ok(entries)
}

// ============================================================================
// Re-sorting
// ============================================================================

/// Result of re-sorting a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResortResult {
    /// New index of the file that was at the cursor position before re-sorting.
    /// None if the filename wasn't provided or wasn't found.
    pub new_cursor_index: Option<usize>,
    /// New indices of previously selected files after re-sorting.
    /// None if no selected_indices were provided.
    pub new_selected_indices: Option<Vec<usize>>,
}

/// Re-sorts an existing cached listing in-place.
///
/// More efficient than creating a new listing when you just want to change the sort order.
#[allow(
    clippy::too_many_arguments,
    reason = "Resort requires sort params, cursor tracking, and selection state"
)]
pub fn resort_listing(
    listing_id: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
    cursor_filename: Option<&str>,
    include_hidden: bool,
    selected_indices: Option<&[usize]>,
    all_selected: bool,
) -> Result<ResortResult, String> {
    let mut cache = LISTING_CACHE.write().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get_mut(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    // Collect filenames of selected files before re-sorting
    let selected_filenames: Option<Vec<String>> = if all_selected {
        // All files selected - we'll rebuild the full set after sort
        None
    } else {
        selected_indices.map(|indices| {
            let entries_for_index: Vec<_> = visible_entries(&listing.entries, include_hidden).collect();
            indices
                .iter()
                .filter_map(|&idx| entries_for_index.get(idx).map(|e| e.name.clone()))
                .collect()
        })
    };

    // Refresh index data before re-sorting (cache entries may not have fresh sizes)
    crate::indexing::enrich_entries_with_index(&mut listing.entries);

    // Re-sort the entries
    sort_entries(&mut listing.entries, sort_by, sort_order, dir_sort_mode);
    listing.sort_by = sort_by;
    listing.directory_sort_mode = dir_sort_mode;
    listing.sort_order = sort_order;

    // Find the new cursor position
    let new_cursor_index =
        cursor_filename.and_then(|name| visible_entries(&listing.entries, include_hidden).position(|e| e.name == name));

    // Find new indices of selected files
    let new_selected_indices = if all_selected {
        let count = visible_entries(&listing.entries, include_hidden).count();
        Some((0..count).collect())
    } else {
        selected_filenames.map(|filenames| {
            let entries_for_lookup: Vec<_> = visible_entries(&listing.entries, include_hidden).collect();
            filenames
                .iter()
                .filter_map(|name| entries_for_lookup.iter().position(|e| e.name == *name))
                .collect()
        })
    };

    Ok(ResortResult {
        new_cursor_index,
        new_selected_indices,
    })
}

// ============================================================================
// Internal cache accessors for file watcher
// ============================================================================

/// Gets entries and path from the listing cache (for watcher diff computation).
/// Returns None if listing not found.
pub(crate) fn get_listing_entries(listing_id: &str) -> Option<(PathBuf, Vec<FileEntry>)> {
    let cache = LISTING_CACHE.read().ok()?;
    let listing = cache.get(listing_id)?;
    Some((listing.path.clone(), listing.entries.clone()))
}

/// Updates the entries in the listing cache (after watcher detects changes).
/// Re-sorts using the stored sort parameters so the cache stays consistent.
pub(crate) fn update_listing_entries(listing_id: &str, entries: Vec<FileEntry>) {
    if let Ok(mut cache) = LISTING_CACHE.write()
        && let Some(listing) = cache.get_mut(listing_id)
    {
        let mut entries = entries;
        crate::indexing::enrich_entries_with_index(&mut entries);
        sort_entries(
            &mut entries,
            listing.sort_by,
            listing.sort_order,
            listing.directory_sort_mode,
        );
        listing.entries = entries;
    }
}

/// Gets all listings for volumes matching a specific prefix.
///
/// Used by MTP file watching to find all listings belonging to a device.
/// MTP volume IDs have the format "mtp-{device_id}:{storage_id}".
///
/// Returns: Vec<(listing_id, volume_id, path, entries)>
pub(crate) fn get_listings_by_volume_prefix(prefix: &str) -> Vec<(String, String, PathBuf, Vec<FileEntry>)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .filter(|(_, listing)| listing.volume_id.starts_with(prefix))
        .map(|(listing_id, listing)| {
            (
                listing_id.clone(),
                listing.volume_id.clone(),
                listing.path.clone(),
                listing.entries.clone(),
            )
        })
        .collect()
}

// ============================================================================
// Listing statistics for selection info display
// ============================================================================

/// Statistics about a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingStats {
    /// Not including directories.
    pub total_files: usize,
    pub total_dirs: usize,
    /// Total logical size in bytes (files + directory recursive sizes).
    pub total_size: u64,
    /// Total physical (on-disk) size in bytes. Mirrors `total_size` but uses `physical_size` / `recursive_physical_size`.
    pub total_physical_size: u64,
    /// Present only if `selected_indices` was provided.
    pub selected_files: Option<usize>,
    /// Present only if `selected_indices` was provided.
    pub selected_dirs: Option<usize>,
    /// Total logical size of selected entries in bytes. Present only if `selected_indices` was provided.
    pub selected_size: Option<u64>,
    /// Total physical size of selected entries in bytes. Present only if `selected_indices` was provided.
    pub selected_physical_size: Option<u64>,
}

/// Gets statistics about a cached listing.
///
/// Returns total file/dir counts and sizes. If `selected_indices` is provided,
/// also returns statistics for the selected items.
pub fn get_listing_stats(
    listing_id: &str,
    include_hidden: bool,
    selected_indices: Option<&[usize]>,
) -> Result<ListingStats, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let visible: Vec<&FileEntry> = visible_entries(&listing.entries, include_hidden).collect();

    // Calculate totals
    let mut total_files: usize = 0;
    let mut total_dirs: usize = 0;
    let mut total_size: u64 = 0;
    let mut total_physical_size: u64 = 0;

    for entry in &visible {
        if entry.is_directory {
            total_dirs += 1;
            if let Some(size) = entry.recursive_size {
                total_size += size;
            }
            if let Some(size) = entry.recursive_physical_size {
                total_physical_size += size;
            }
        } else {
            total_files += 1;
            if let Some(size) = entry.size {
                total_size += size;
            }
            if let Some(size) = entry.physical_size {
                total_physical_size += size;
            }
        }
    }

    // Calculate selection stats if indices provided
    let (selected_files, selected_dirs, selected_size, selected_physical_size) = if let Some(indices) = selected_indices
    {
        let mut sel_files: usize = 0;
        let mut sel_dirs: usize = 0;
        let mut sel_size: u64 = 0;
        let mut sel_physical_size: u64 = 0;

        for &idx in indices {
            if let Some(entry) = visible.get(idx) {
                if entry.is_directory {
                    sel_dirs += 1;
                    if let Some(size) = entry.recursive_size {
                        sel_size += size;
                    }
                    if let Some(size) = entry.recursive_physical_size {
                        sel_physical_size += size;
                    }
                } else {
                    sel_files += 1;
                    if let Some(size) = entry.size {
                        sel_size += size;
                    }
                    if let Some(size) = entry.physical_size {
                        sel_physical_size += size;
                    }
                }
            }
        }

        (Some(sel_files), Some(sel_dirs), Some(sel_size), Some(sel_physical_size))
    } else {
        (None, None, None, None)
    };

    Ok(ListingStats {
        total_files,
        total_dirs,
        total_size,
        total_physical_size,
        selected_files,
        selected_dirs,
        selected_size,
        selected_physical_size,
    })
}

/// Re-enriches directory entries in a cached listing with fresh index data.
///
/// Called when `index-dir-updated` fires so that subsequent `get_listing_stats`
/// reads see up-to-date `recursive_size` values without needing a write lock.
pub fn refresh_listing_index_sizes(listing_id: &str) -> Result<(), String> {
    let mut cache = LISTING_CACHE.write().map_err(|_| "Failed to acquire cache lock")?;
    if let Some(listing) = cache.get_mut(listing_id) {
        crate::indexing::enrich_entries_with_index(&mut listing.entries);
    }
    Ok(())
}
