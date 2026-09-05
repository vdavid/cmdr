//! Directory listing lifecycle, cache API, sorting, and statistics.
//!
//! This is the synchronous, frontend-facing API. Low-level disk I/O is in reading.rs,
//! async streaming is in streaming.rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::benchmark;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};
use crate::file_system::listing::visible_rows::VisibleRows;
use crate::file_system::watcher::{start_watching_detached, stop_watching};
use crate::index_host::index;

// ============================================================================
// Listing lifecycle
// ============================================================================

/// Result of starting a new directory listing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListingStartResult {
    pub listing_id: String,
    pub total_count: usize,
}

/// Starts a new directory listing using a specific volume.
///
/// This is the internal implementation that supports multi-volume access.
pub async fn list_directory_start_with_volume(
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

    // Resolve the volume, routing a `.zip`-crossing path to its read-only
    // `ArchiveVolume`. The cache keeps the FE-provided `volume_id` (parent drive);
    // the downstream re-read sites re-resolve the archive from `(volume_id, path)`.
    let resolved = crate::file_system::volume::manager::get_volume_manager()
        .resolve(volume_id, path)
        .await;
    let is_routed = resolved.is_routed();
    let volume = resolved.volume.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Volume '{}' not found", volume_id),
        )
    })?;

    // Use the Volume trait to list the directory
    let all_entries = volume.list_directory(path, None).await.map_err(|e| {
        // A stale-mount errno is the only evidence we get that this volume's
        // active mount died; if the filesystem is reachable through another
        // mount, this moves the volume there. See `volume::note_root_failure`.
        crate::file_system::volume::note_root_failure(volume_id, &e);
        std::io::Error::other(e.to_string())
    })?;
    benchmark::log_event_value("volume.list_directory COMPLETE, entries", all_entries.len());

    // Generate listing ID
    let listing_id = Uuid::new_v4().to_string();

    // Enrich directory entries with index data (recursive_size etc.) before sorting,
    // so that sort-by-size works correctly for directories. A routed volume has no
    // drive index (its paths aren't real FS paths), so enrich/verify are skipped.
    let mut all_entries = all_entries;
    if !is_routed {
        index().enrich(volume_id, &mut all_entries);
        index().verify_directory(volume_id, &path.to_string_lossy());
    }

    // Sort the entries
    sort_entries(&mut all_entries, sort_by, sort_order, dir_sort_mode);

    // Cache the entries FIRST (watcher will read from here), then read the row
    // count back off the listing's own map so the number the frontend sizes its
    // scroller with comes from the same filter every later fetch goes through.
    let listing = CachedListing::new(
        volume_id.to_string(),
        path.to_path_buf(),
        all_entries,
        sort_by,
        sort_order,
        dir_sort_mode,
    );
    let total_count = listing.rows(include_hidden).len();
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.insert(listing_id.clone(), listing);
    }

    // Start watching the directory (only if volume supports it)
    // TODO: Update watcher to be volume-aware
    if volume.can_watch_listings() {
        start_watching_detached(&listing_id, path);
    }

    benchmark::log_event("list_directory_start RETURNING");
    Ok(ListingStartResult {
        listing_id,
        total_count,
    })
}

/// Ends a directory listing and cleans up the cache.
pub fn list_directory_end(listing_id: &str) {
    // Stop the file watcher
    stop_watching(listing_id);

    // Drop any pending coalesced diff for this listing
    crate::file_system::listing::diff_emitter::drop_pending(listing_id);

    // Remove from listing cache
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.remove(listing_id);
    }
}

// ============================================================================
// On-demand virtual scrolling API (cache accessors)
// ============================================================================

/// Runs `f` against a cached listing, or reports that it's gone.
///
/// Every read accessor below shares this preamble: take the cache read lock, find
/// the listing, and stamp it so the six-hour orphan reaper knows a live pane is
/// still behind it.
fn with_listing<R>(listing_id: &str, f: impl FnOnce(&CachedListing) -> R) -> Result<R, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    listing.touch();

    Ok(f(listing))
}

/// Gets a range of entries from a cached listing.
pub fn get_file_range(
    listing_id: &str,
    start: usize,
    count: usize,
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    with_listing(listing_id, |listing| {
        let rows = listing.rows(include_hidden);
        let end = start.saturating_add(count).min(rows.len());
        (start..end)
            .filter_map(|row| rows.get(row).cloned())
            .collect::<Vec<FileEntry>>()
    })
}

/// Gets total count of entries in a cached listing.
pub fn get_total_count(listing_id: &str, include_hidden: bool) -> Result<usize, String> {
    with_listing(listing_id, |listing| listing.rows(include_hidden).len())
}

/// Finds the index of a file by name in a cached listing.
pub fn find_file_index(listing_id: &str, name: &str, include_hidden: bool) -> Result<Option<usize>, String> {
    with_listing(listing_id, |listing| listing.rows(include_hidden).row_of(name))
}

/// Which side of a named row to read: the one before it or the one after it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RowBeside {
    Previous,
    Next,
}

/// The entry sitting immediately before or after the one named `name`, or `None`
/// when that name isn't in the listing or the row beside it doesn't exist.
///
/// Resolving the anchor and reading its neighbour happen against ONE snapshot,
/// which is the whole point: a caller doing it in two calls (`find_file_index`,
/// then `get_file_at`) hands back a row that a rename landing between the two has
/// already moved. Callers ask this instead of an index when they have a row they
/// know and want the one beside it, because a name survives a re-sort that any
/// index they hold does not.
pub fn get_file_beside(
    listing_id: &str,
    name: &str,
    side: RowBeside,
    include_hidden: bool,
) -> Result<Option<FileEntry>, String> {
    with_listing(listing_id, |listing| {
        let rows = listing.rows(include_hidden);
        let anchor = rows.row_of(name)?;
        let beside = match side {
            RowBeside::Previous => anchor.checked_sub(1)?,
            RowBeside::Next => anchor + 1,
        };
        rows.get(beside).cloned()
    })
}

/// Finds the indices of multiple files by name in a cached listing (batch version of
/// `find_file_index`).
///
/// Single pass over the listing's rows, O(rows + names). Returns only found names as keys.
pub fn find_file_indices(
    listing_id: &str,
    names: &[String],
    include_hidden: bool,
) -> Result<HashMap<String, usize>, String> {
    with_listing(listing_id, |listing| {
        let lookup: std::collections::HashSet<&str> = names.iter().map(|n| n.as_str()).collect();
        let mut result = HashMap::with_capacity(names.len());

        for (row, entry) in listing.rows(include_hidden).iter().enumerate() {
            if lookup.contains(entry.name.as_str()) {
                result.insert(entry.name.clone(), row);
            }
        }

        result
    })
}

/// Gets a single file at the given index.
pub fn get_file_at(listing_id: &str, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    with_listing(listing_id, |listing| {
        let rows = listing.rows(include_hidden);
        let result = rows.get(index).cloned();
        if result.is_none() {
            // Out-of-bounds is expected briefly after a mutation: the FE iterates over a
            // cached `totalCount` that may lag the BE listing during the async refetch
            // window opened by a `directory-diff` event. The FE handles `None` gracefully
            // (skips the entry, breaks the loop). Logged at debug so we still have the
            // breadcrumb when investigating cursor/selection bugs without firing crash
            // reports for legitimate drift.
            log::debug!(
                "get_file_at: index {} out of bounds (listing {} has {} entries at {}): likely FE/BE drift after async listing refresh",
                index,
                listing_id,
                rows.len(),
                listing.path.display()
            );
        }
        result
    })
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
    with_listing(listing_id, |listing| {
        let rows = listing.rows(include_hidden);
        let mut paths = Vec::with_capacity(selected_indices.len());
        for &frontend_idx in selected_indices {
            // Skip ".." entry (frontend index 0 when has_parent is true)
            if has_parent && frontend_idx == 0 {
                continue;
            }

            // Convert frontend index to backend index
            let backend_idx = if has_parent { frontend_idx - 1 } else { frontend_idx };

            if let Some(entry) = rows.get(backend_idx) {
                paths.push(PathBuf::from(&entry.path));
            }
        }
        paths
    })
}

/// Gets full FileEntry objects at specific backend indices from a cached listing.
///
/// Unlike `get_paths_at_indices` (which takes frontend indices and handles the parent offset),
/// this takes backend indices directly. The caller is responsible for any offset adjustment.
/// Used by the delete dialog where full entry metadata (name, size, isDirectory, etc.) is needed.
pub fn get_files_at_indices(
    listing_id: &str,
    selected_indices: &[usize],
    include_hidden: bool,
) -> Result<Vec<FileEntry>, String> {
    with_listing(listing_id, |listing| {
        let rows = listing.rows(include_hidden);
        selected_indices
            .iter()
            .filter_map(|&idx| rows.get(idx).cloned())
            .collect::<Vec<FileEntry>>()
    })
}

// ============================================================================
// Re-sorting
// ============================================================================

/// Result of re-sorting a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

    listing.touch();

    // Collect filenames of selected files before re-sorting
    let selected_filenames: Option<Vec<String>> = if all_selected {
        // All files selected - we'll rebuild the full set after sort
        None
    } else {
        selected_indices.map(|indices| {
            let rows = listing.rows(include_hidden);
            indices
                .iter()
                .filter_map(|&idx| rows.get(idx).map(|e| e.name.clone()))
                .collect()
        })
    };

    // Refresh index data before re-sorting (cache entries may not have fresh sizes)
    let volume_id = listing.volume_id.clone();
    index().enrich(&volume_id, listing.entries_mut());

    // Re-sort the entries
    sort_entries(listing.entries_mut(), sort_by, sort_order, dir_sort_mode);
    listing.sort_by = sort_by;
    listing.directory_sort_mode = dir_sort_mode;
    listing.sort_order = sort_order;

    let rows = listing.rows(include_hidden);

    // Find the new cursor position
    let new_cursor_index = cursor_filename.and_then(|name| rows.row_of(name));

    // Find new indices of selected files
    let new_selected_indices = if all_selected {
        Some((0..rows.len()).collect())
    } else {
        selected_filenames.map(|filenames| {
            let names_to_rows: HashMap<&str, usize> = rows
                .iter()
                .enumerate()
                .map(|(row, entry)| (entry.name.as_str(), row))
                .collect();
            filenames
                .iter()
                .filter_map(|name| names_to_rows.get(name.as_str()).copied())
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
    Some((listing.path.clone(), listing.entries().to_vec()))
}

/// Updates the entries in the listing cache (after watcher detects changes).
/// Re-sorts using the stored sort parameters so the cache stays consistent.
pub(crate) fn update_listing_entries(listing_id: &str, entries: Vec<FileEntry>) {
    if let Ok(mut cache) = LISTING_CACHE.write()
        && let Some(listing) = cache.get_mut(listing_id)
    {
        listing.touch();
        let mut entries = entries;
        index().enrich(&listing.volume_id, &mut entries);
        sort_entries(
            &mut entries,
            listing.sort_by,
            listing.sort_order,
            listing.directory_sort_mode,
        );
        listing.set_entries(entries);
    }
}

/// The distinct volume ids under `prefix` that have at least one cached listing.
///
/// What a device backend asks when its event names an object the protocol alone
/// can't place: "which of my storages is a pane showing?" Ids only, so nothing
/// clones an open directory's entries to answer it.
pub(crate) fn volume_ids_with_listings(prefix: &str) -> Vec<String> {
    // Recover rather than answer empty: an empty answer reads as "no pane is
    // showing anything", which sends a device backend down the blanket-refresh
    // path for as long as the process lives.
    let cache = LISTING_CACHE.read_ignore_poison();

    let mut ids: Vec<String> = cache
        .values()
        .filter(|listing| listing.volume_id.starts_with(prefix))
        .map(|listing| listing.volume_id.clone())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

// ============================================================================
// Listing statistics for selection info display
// ============================================================================

/// Statistics about a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListingStats {
    /// Not including directories.
    pub total_files: usize,
    pub total_dirs: usize,
    /// Total logical size in bytes (files + directory recursive sizes).
    pub total_size: u64,
    /// Total physical (on-disk) size in bytes. Mirrors `total_size` but uses `physical_size` /
    /// `recursive_physical_size`.
    pub total_physical_size: u64,
    /// Present only if `selected_indices` was provided.
    pub selected_files: Option<usize>,
    /// Present only if `selected_indices` was provided.
    pub selected_dirs: Option<usize>,
    /// Total logical size of selected entries in bytes. Present only if `selected_indices` was
    /// provided.
    pub selected_size: Option<u64>,
    /// Total physical size of selected entries in bytes. Present only if `selected_indices` was
    /// provided.
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
    with_listing(listing_id, |listing| {
        listing_stats(listing.rows(include_hidden), selected_indices)
    })
}

/// The counting half of `get_listing_stats`, over one snapshot of the pane's rows.
fn listing_stats(visible: VisibleRows<'_>, selected_indices: Option<&[usize]>) -> ListingStats {
    // Calculate totals
    let mut total_files: usize = 0;
    let mut total_dirs: usize = 0;
    let mut total_size: u64 = 0;
    let mut total_physical_size: u64 = 0;

    for entry in visible.iter() {
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

    ListingStats {
        total_files,
        total_dirs,
        total_size,
        total_physical_size,
        selected_files,
        selected_dirs,
        selected_size,
        selected_physical_size,
    }
}

/// Re-enriches directory entries in a cached listing with fresh index data.
///
/// Called when `index-dir-updated` fires so that subsequent `get_listing_stats`
/// reads see up-to-date `recursive_size` values without needing a write lock.
pub fn refresh_listing_index_sizes(listing_id: &str) -> Result<(), String> {
    let mut cache = LISTING_CACHE.write().map_err(|_| "Failed to acquire cache lock")?;
    if let Some(listing) = cache.get_mut(listing_id) {
        let volume_id = listing.volume_id.clone();
        index().enrich(&volume_id, listing.entries_mut());
    }
    Ok(())
}
