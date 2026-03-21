//! Directory listing cache for on-demand virtual scrolling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, entry_comparator};

/// Result of updating an entry in-place or moving it to a new sorted position.
pub enum ModifyResult {
    /// Entry was updated without changing its sorted position.
    UpdatedInPlace { index: usize },
    /// Entry was removed from `old_index` and re-inserted at `new_index` because sort-relevant fields changed.
    Moved { old_index: usize, new_index: usize },
}

/// Cache for directory listings (on-demand virtual scrolling).
/// Key: listing_id, Value: cached listing with all entries.
pub(crate) static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cached directory listing for on-demand virtual scrolling.
pub(crate) struct CachedListing {
    /// Volume ID this listing belongs to (like "root", "dropbox")
    pub volume_id: String,
    /// Path within the volume (absolute path for now)
    pub path: PathBuf,
    /// Cached file entries
    pub entries: Vec<FileEntry>,
    /// Current sort column
    pub sort_by: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
    /// How directories are sorted relative to the current sort column
    pub directory_sort_mode: DirectorySortMode,
}

/// Finds all cached listings whose directory path matches `parent_path`.
///
/// Returns `(listing_id, sort_by, sort_order, directory_sort_mode)` for each match.
/// Typically 0 (no pane showing that dir), 1, or 2 (both panes showing the same dir).
pub fn find_listings_for_path(parent_path: &Path) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .filter(|(_, listing)| listing.path == parent_path)
        .map(|(id, listing)| {
            (
                id.clone(),
                listing.sort_by,
                listing.sort_order,
                listing.directory_sort_mode,
            )
        })
        .collect()
}

/// Inserts a `FileEntry` into a cached listing at the correct sorted position.
///
/// Uses `partition_point` with the listing's sort comparator to find the insertion index.
/// Returns the insertion index, or `None` if the listing wasn't found or the entry
/// already exists (checked by path).
pub fn insert_entry_sorted(listing_id: &str, entry: FileEntry) -> Option<usize> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;

    // Don't insert if an entry with this path already exists
    if listing.entries.iter().any(|e| e.path == entry.path) {
        return None;
    }

    let cmp = entry_comparator(listing.sort_by, listing.sort_order, listing.directory_sort_mode);
    let pos = listing
        .entries
        .partition_point(|existing| cmp(existing, &entry).is_lt());
    listing.entries.insert(pos, entry);
    Some(pos)
}

/// Returns the directory path for a cached listing, without cloning entries.
pub fn get_listing_path(listing_id: &str) -> Option<PathBuf> {
    let cache = LISTING_CACHE.read().ok()?;
    cache.get(listing_id).map(|listing| listing.path.clone())
}

/// Removes an entry by its path from the cached listing.
///
/// Returns `(old_index, removed_entry)` or `None` if the listing or entry wasn't found.
pub fn remove_entry_by_path(listing_id: &str, path: &Path) -> Option<(usize, FileEntry)> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;
    let path_str = path.to_string_lossy();

    let idx = listing.entries.iter().position(|e| e.path == *path_str)?;
    let entry = listing.entries.remove(idx);
    Some((idx, entry))
}

/// Checks whether a cached listing contains an entry with the given path.
pub fn has_entry(listing_id: &str, path: &str) -> bool {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return false,
    };
    cache
        .get(listing_id)
        .is_some_and(|listing| listing.entries.iter().any(|e| e.path == path))
}

/// Updates an existing entry in the cached listing.
///
/// If sort-relevant fields changed (size, modified_at, is_directory), removes the old entry
/// and re-inserts at the correct sorted position. Otherwise updates in place.
/// Returns `None` if the listing or entry wasn't found.
pub fn update_entry_sorted(listing_id: &str, new_entry: FileEntry) -> Option<ModifyResult> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;

    let idx = listing.entries.iter().position(|e| e.path == new_entry.path)?;
    let old = &listing.entries[idx];

    let sort_relevant_changed = old.size != new_entry.size
        || old.modified_at != new_entry.modified_at
        || old.is_directory != new_entry.is_directory;

    if sort_relevant_changed {
        listing.entries.remove(idx);
        let cmp = entry_comparator(listing.sort_by, listing.sort_order, listing.directory_sort_mode);
        let new_pos = listing
            .entries
            .partition_point(|existing| cmp(existing, &new_entry).is_lt());
        listing.entries.insert(new_pos, new_entry);
        Some(ModifyResult::Moved {
            old_index: idx,
            new_index: new_pos,
        })
    } else {
        listing.entries[idx] = new_entry;
        Some(ModifyResult::UpdatedInPlace { index: idx })
    }
}
