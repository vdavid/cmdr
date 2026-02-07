//! Directory listing lifecycle, cache API, sorting, and statistics.
//!
//! This is the synchronous, frontend-facing API. Low-level disk I/O is in reading.rs,
//! async streaming is in streaming.rs.

#![allow(dead_code, reason = "Boilerplate for future use")]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::benchmark;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{SortColumn, SortOrder, sort_entries};
use crate::file_system::watcher::{start_watching, stop_watching};

/// Returns true if the entry is not a hidden dotfile.
fn is_visible(entry: &FileEntry) -> bool {
    !entry.name.starts_with('.')
}

// ============================================================================
// Listing lifecycle
// ============================================================================

/// Result of starting a new directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingStartResult {
    /// Unique listing ID for subsequent API calls
    pub listing_id: String,
    /// Total number of entries in the directory
    pub total_count: usize,
    /// Maximum filename width in pixels (for Brief mode columns)
    /// None if font metrics are not available
    pub max_filename_width: Option<f32>,
}

/// Starts a new directory listing.
///
/// Reads the directory once, caches it, and returns listing ID + total count.
/// Frontend then fetches visible ranges on demand via `get_file_range`.
pub fn list_directory_start(path: &Path, include_hidden: bool) -> Result<ListingStartResult, std::io::Error> {
    list_directory_start_with_volume("root", path, include_hidden, SortColumn::Name, SortOrder::Ascending)
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
    let all_entries = volume
        .list_directory(path)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    benchmark::log_event_value("volume.list_directory COMPLETE, entries", all_entries.len());

    // Generate listing ID
    let listing_id = Uuid::new_v4().to_string();

    // Count visible entries based on include_hidden setting
    let total_count = if include_hidden {
        all_entries.len()
    } else {
        all_entries.iter().filter(|e| is_visible(e)).count()
    };

    // Sort the entries
    let mut all_entries = all_entries;
    sort_entries(&mut all_entries, sort_by, sort_order);

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

    // Filter entries if not including hidden
    if include_hidden {
        let end = (start + count).min(listing.entries.len());
        Ok(listing.entries[start..end].to_vec())
    } else {
        // Need to filter and then slice
        let visible: Vec<&FileEntry> = listing.entries.iter().filter(|e| is_visible(e)).collect();
        let end = (start + count).min(visible.len());
        Ok(visible[start..end].iter().cloned().cloned().collect())
    }
}

/// Gets total count of entries in a cached listing.
pub fn get_total_count(listing_id: &str, include_hidden: bool) -> Result<usize, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    if include_hidden {
        Ok(listing.entries.len())
    } else {
        Ok(listing.entries.iter().filter(|e| is_visible(e)).count())
    }
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

    let max_width = if include_hidden {
        let filenames: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    } else {
        let filenames: Vec<&str> = listing
            .entries
            .iter()
            .filter(|e| is_visible(e))
            .map(|e| e.name.as_str())
            .collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    };

    Ok(max_width)
}

/// Finds the index of a file by name in a cached listing.
pub fn find_file_index(listing_id: &str, name: &str, include_hidden: bool) -> Result<Option<usize>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    if include_hidden {
        Ok(listing.entries.iter().position(|e| e.name == name))
    } else {
        // Find index in filtered list
        let visible: Vec<&FileEntry> = listing.entries.iter().filter(|e| is_visible(e)).collect();
        Ok(visible.iter().position(|e| e.name == name))
    }
}

/// Gets a single file at the given index.
pub fn get_file_at(listing_id: &str, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    if include_hidden {
        let result = listing.entries.get(index).cloned();
        if result.is_none() {
            log::error!(
                "get_file_at: index {} out of bounds (listing has {} entries) - frontend/backend index mismatch!",
                index,
                listing.entries.len()
            );
        }
        Ok(result)
    } else {
        let visible: Vec<&FileEntry> = listing.entries.iter().filter(|e| is_visible(e)).collect();
        let result = visible.get(index).cloned().cloned();
        if result.is_none() {
            log::error!(
                "get_file_at: index {} out of bounds (listing has {} visible entries) - frontend/backend index mismatch!",
                index,
                visible.len()
            );
        }
        Ok(result)
    }
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

    // Build visible entries view (with or without hidden files)
    let visible: Vec<&FileEntry> = if include_hidden {
        listing.entries.iter().collect()
    } else {
        listing.entries.iter().filter(|e| is_visible(e)).collect()
    };

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
pub fn resort_listing(
    listing_id: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
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
            let entries_for_index = if include_hidden {
                listing.entries.iter().collect::<Vec<_>>()
            } else {
                listing.entries.iter().filter(|e| is_visible(e)).collect()
            };
            indices
                .iter()
                .filter_map(|&idx| entries_for_index.get(idx).map(|e| e.name.clone()))
                .collect()
        })
    };

    // Re-sort the entries
    sort_entries(&mut listing.entries, sort_by, sort_order);
    listing.sort_by = sort_by;
    listing.sort_order = sort_order;

    // Find the new cursor position
    let new_cursor_index = cursor_filename.and_then(|name| {
        if include_hidden {
            listing.entries.iter().position(|e| e.name == name)
        } else {
            listing
                .entries
                .iter()
                .filter(|e| is_visible(e))
                .position(|e| e.name == name)
        }
    });

    // Find new indices of selected files
    let new_selected_indices = if all_selected {
        // All files are still selected after re-sort
        let count = if include_hidden {
            listing.entries.len()
        } else {
            listing.entries.iter().filter(|e| is_visible(e)).count()
        };
        Some((0..count).collect())
    } else {
        selected_filenames.map(|filenames| {
            let entries_for_lookup: Vec<_> = if include_hidden {
                listing.entries.iter().collect()
            } else {
                listing.entries.iter().filter(|e| is_visible(e)).collect()
            };
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
pub(crate) fn update_listing_entries(listing_id: &str, entries: Vec<FileEntry>) {
    if let Ok(mut cache) = LISTING_CACHE.write()
        && let Some(listing) = cache.get_mut(listing_id)
    {
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
    /// Total number of files (not directories)
    pub total_files: usize,
    /// Total number of directories
    pub total_dirs: usize,
    /// Total size of all files in bytes
    pub total_file_size: u64,
    /// Number of selected files (if selected_indices provided)
    pub selected_files: Option<usize>,
    /// Number of selected directories (if selected_indices provided)
    pub selected_dirs: Option<usize>,
    /// Total size of selected files in bytes (if selected_indices provided)
    pub selected_file_size: Option<u64>,
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

    // Get visible entries based on include_hidden setting
    let visible_entries: Vec<&FileEntry> = if include_hidden {
        listing.entries.iter().collect()
    } else {
        listing.entries.iter().filter(|e| is_visible(e)).collect()
    };

    // Calculate totals
    let mut total_files: usize = 0;
    let mut total_dirs: usize = 0;
    let mut total_file_size: u64 = 0;

    for entry in &visible_entries {
        if entry.is_directory {
            total_dirs += 1;
        } else {
            total_files += 1;
            if let Some(size) = entry.size {
                total_file_size += size;
            }
        }
    }

    // Calculate selection stats if indices provided
    let (selected_files, selected_dirs, selected_file_size) = if let Some(indices) = selected_indices {
        let mut sel_files: usize = 0;
        let mut sel_dirs: usize = 0;
        let mut sel_size: u64 = 0;

        for &idx in indices {
            if let Some(entry) = visible_entries.get(idx) {
                if entry.is_directory {
                    sel_dirs += 1;
                } else {
                    sel_files += 1;
                    if let Some(size) = entry.size {
                        sel_size += size;
                    }
                }
            }
        }

        (Some(sel_files), Some(sel_dirs), Some(sel_size))
    } else {
        (None, None, None)
    };

    Ok(ListingStats {
        total_files,
        total_dirs,
        total_file_size,
        selected_files,
        selected_dirs,
        selected_file_size,
    })
}
