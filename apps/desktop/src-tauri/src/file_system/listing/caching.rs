//! Directory listing cache for on-demand virtual scrolling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};

use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, entry_comparator};

/// Describes a change to a directory's contents on a specific volume.
///
/// Used by `notify_directory_changed` to apply targeted cache updates
/// and emit `directory-diff` events to the frontend.
pub enum DirectoryChange {
    /// A single entry was added. Includes the full `FileEntry` to insert.
    Added(FileEntry),
    /// A single entry was removed by name.
    Removed(String),
    /// A single entry was modified. Includes the updated `FileEntry`.
    Modified(FileEntry),
    /// An entry was renamed within the same directory.
    Renamed { old_name: String, new_entry: FileEntry },
    /// Unknown or bulk change — trigger a full re-read via the Volume trait.
    #[allow(dead_code, reason = "M3 will use this for smb2 watcher's STATUS_NOTIFY_ENUM_DIR")]
    FullRefresh,
}

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
    /// Monotonic sequence number for `directory-diff` events. Incremented each time
    /// the cache is patched (by watcher, notify_mutation, or manual refresh).
    /// Lives on the listing so it works for all volume types, including SMB/MTP
    /// which don't use the FSEvents-based `WatchedDirectory`.
    pub sequence: AtomicU64,
}

/// Finds all cached listings whose directory path matches `parent_path`.
///
/// When `volume_id` is `Some`, also filters by volume. This prevents false matches
/// when two volumes serve overlapping paths.
///
/// Returns `(listing_id, sort_by, sort_order, directory_sort_mode)` for each match.
/// Typically 0 (no pane showing that dir), 1, or 2 (both panes showing the same dir).
pub fn find_listings_for_path(parent_path: &Path) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)> {
    find_listings_for_path_on_volume(None, parent_path)
}

/// Like `find_listings_for_path`, but also filters by `volume_id`.
pub fn find_listings_for_path_on_volume(
    volume_id: Option<&str>,
    parent_path: &Path,
) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .filter(|(_, listing)| listing.path == parent_path && volume_id.is_none_or(|vid| listing.volume_id == vid))
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

/// Finds all cached listings belonging to a volume, regardless of path.
///
/// Used by `FullRefresh` when the SMB watcher emits `STATUS_NOTIFY_ENUM_DIR` for
/// the share root but no listing matches that exact path (the user may be browsing
/// a subdirectory).
pub(crate) fn find_listings_on_volume(
    volume_id: &str,
) -> Vec<(String, PathBuf, SortColumn, SortOrder, DirectorySortMode)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .filter(|(_, listing)| listing.volume_id == volume_id)
        .map(|(id, listing)| {
            (
                id.clone(),
                listing.path.clone(),
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

/// Notifies the listing system that a directory's contents changed on a volume.
///
/// Finds all active listings matching `volume_id` and `parent_path`, applies the
/// change to the cache, and emits `directory-diff` events to the frontend.
///
/// For single-entry changes (add/remove/modify/rename), patches the cache in-place.
/// For `FullRefresh`, re-reads the directory via the Volume trait and computes a diff.
pub fn notify_directory_changed(volume_id: &str, parent_path: &Path, change: DirectoryChange) {
    use crate::file_system::watcher::WATCHER_MANAGER;

    let listings = find_listings_for_path_on_volume(Some(volume_id), parent_path);

    // For non-FullRefresh changes, bail early if no listing matches this path.
    // FullRefresh has a volume-wide fallback below (for STATUS_NOTIFY_ENUM_DIR).
    if listings.is_empty() && !matches!(change, DirectoryChange::FullRefresh) {
        return;
    }

    // Get app handle once (same pattern as handle_directory_change)
    let app_handle = {
        let manager = match WATCHER_MANAGER.read() {
            Ok(m) => m,
            Err(_) => return,
        };
        manager.app_handle.clone()
    };
    let Some(app) = app_handle else { return };

    match change {
        DirectoryChange::Added(entry) => {
            let mut entry = entry;
            crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_added(&app, listing_id, entry.clone());
            }
        }
        DirectoryChange::Removed(name) => {
            let full_path = parent_path.join(&name);
            for (listing_id, ..) in &listings {
                notify_removed(&app, listing_id, &full_path);
            }
        }
        DirectoryChange::Modified(entry) => {
            let mut entry = entry;
            crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_modified(&app, listing_id, entry.clone());
            }
        }
        DirectoryChange::Renamed { old_name, new_entry } => {
            let mut new_entry = new_entry;
            crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut new_entry));
            let old_path = parent_path.join(&old_name);
            for (listing_id, ..) in &listings {
                notify_removed(&app, listing_id, &old_path);
                notify_added(&app, listing_id, new_entry.clone());
            }
        }
        DirectoryChange::FullRefresh => {
            if listings.is_empty() {
                // No listing matches this exact path. For STATUS_NOTIFY_ENUM_DIR the
                // path is the share root, but the user may be browsing a subdirectory.
                // Refresh all listings on this volume instead.
                let volume_listings = find_listings_on_volume(volume_id);
                for (lid, path, sort_by, sort_order, dir_sort_mode) in volume_listings {
                    let app = app.clone();
                    let volume_id = volume_id.to_string();
                    tokio::spawn(notify_full_refresh(
                        app,
                        volume_id,
                        path,
                        vec![(lid, sort_by, sort_order, dir_sort_mode)],
                    ));
                }
            } else {
                let volume_id = volume_id.to_string();
                let parent_path = parent_path.to_path_buf();
                tokio::spawn(notify_full_refresh(app, volume_id, parent_path, listings));
            }
        }
    }
}

/// Inserts an entry into the cache and emits a single-add diff event.
fn notify_added(app: &tauri::AppHandle, listing_id: &str, entry: FileEntry) {
    use crate::file_system::watcher::{DiffChange, DirectoryDiff};
    use tauri::Emitter;

    let Some(index) = insert_entry_sorted(listing_id, entry.clone()) else {
        return; // Already exists or listing gone
    };

    let Some(sequence) = increment_sequence(listing_id) else {
        return;
    };

    let diff = DirectoryDiff {
        listing_id: listing_id.to_string(),
        sequence,
        changes: vec![DiffChange {
            change_type: "add".to_string(),
            entry,
            index,
        }],
    };
    if let Err(e) = app.emit("directory-diff", &diff) {
        log::warn!("notify_directory_changed: couldn't emit add event: {}", e);
    }
}

/// Removes an entry from the cache and emits a single-remove diff event.
fn notify_removed(app: &tauri::AppHandle, listing_id: &str, full_path: &Path) {
    use crate::file_system::watcher::{DiffChange, DirectoryDiff};
    use tauri::Emitter;

    let Some((index, removed_entry)) = remove_entry_by_path(listing_id, full_path) else {
        return; // Not in cache or listing gone
    };

    let Some(sequence) = increment_sequence(listing_id) else {
        return;
    };

    let diff = DirectoryDiff {
        listing_id: listing_id.to_string(),
        sequence,
        changes: vec![DiffChange {
            change_type: "remove".to_string(),
            entry: removed_entry,
            index,
        }],
    };
    if let Err(e) = app.emit("directory-diff", &diff) {
        log::warn!("notify_directory_changed: couldn't emit remove event: {}", e);
    }
}

/// Updates an entry in the cache and emits a modify (or remove+add) diff event.
fn notify_modified(app: &tauri::AppHandle, listing_id: &str, entry: FileEntry) {
    use crate::file_system::watcher::{DiffChange, DirectoryDiff};
    use tauri::Emitter;

    let result = match update_entry_sorted(listing_id, entry.clone()) {
        Some(r) => r,
        None => return,
    };

    let Some(sequence) = increment_sequence(listing_id) else {
        return;
    };

    let changes = match result {
        ModifyResult::UpdatedInPlace { index } => {
            vec![DiffChange {
                change_type: "modify".to_string(),
                entry,
                index,
            }]
        }
        ModifyResult::Moved { old_index, new_index } => {
            vec![
                DiffChange {
                    change_type: "remove".to_string(),
                    entry: entry.clone(),
                    index: old_index,
                },
                DiffChange {
                    change_type: "add".to_string(),
                    entry,
                    index: new_index,
                },
            ]
        }
    };

    let diff = DirectoryDiff {
        listing_id: listing_id.to_string(),
        sequence,
        changes,
    };
    if let Err(e) = app.emit("directory-diff", &diff) {
        log::warn!("notify_directory_changed: couldn't emit modify event: {}", e);
    }
}

/// Re-reads a directory via the Volume trait, computes a diff, and emits it.
async fn notify_full_refresh(
    app: tauri::AppHandle,
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    use crate::file_system::listing::sorting::sort_entries;
    use crate::file_system::watcher::{DirectoryDiff, compute_diff};
    use tauri::Emitter;

    let vol = match crate::file_system::get_volume_manager().get(&volume_id) {
        Some(v) => v,
        None => {
            log::warn!("notify_directory_changed: volume `{}` not found", volume_id);
            return;
        }
    };

    let mut new_entries = match vol.list_directory(&parent_path, None).await {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!(
                "notify_directory_changed: failed to re-read {}: {}",
                parent_path.display(),
                e
            );
            return;
        }
    };

    crate::indexing::enrich_entries_with_index(&mut new_entries);

    for (listing_id, sort_by, sort_order, dir_sort_mode) in &listings {
        // Re-sort to match this listing's sort params
        let mut sorted = new_entries.clone();
        sort_entries(&mut sorted, *sort_by, *sort_order, *dir_sort_mode);

        // Get old entries for diff computation
        let old_entries = {
            let cache = match LISTING_CACHE.read() {
                Ok(c) => c,
                Err(_) => continue,
            };
            match cache.get(listing_id.as_str()) {
                Some(listing) => listing.entries.clone(),
                None => continue,
            }
        };

        let changes = compute_diff(&old_entries, &sorted);
        if changes.is_empty() {
            continue;
        }

        // Update cache
        crate::file_system::listing::operations::update_listing_entries(listing_id, sorted);

        let Some(sequence) = increment_sequence(listing_id) else {
            continue;
        };

        let diff = DirectoryDiff {
            listing_id: listing_id.clone(),
            sequence,
            changes,
        };
        if let Err(e) = app.emit("directory-diff", &diff) {
            log::warn!("notify_directory_changed: couldn't emit refresh event: {}", e);
        }
    }
}

/// Increments and returns the sequence number for a cached listing.
///
/// Uses the `AtomicU64` on `CachedListing` so it works for all volume types,
/// including SMB/MTP which don't have a `WatchedDirectory` entry.
pub(crate) fn increment_sequence(listing_id: &str) -> Option<u64> {
    let cache = LISTING_CACHE.read().ok()?;
    let listing = cache.get(listing_id)?;
    let seq = listing.sequence.fetch_add(1, Ordering::Relaxed) + 1;
    Some(seq)
}
