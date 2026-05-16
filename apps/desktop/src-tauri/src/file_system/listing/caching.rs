//! Directory listing cache for on-demand virtual scrolling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

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
    /// Unknown or bulk change: trigger a full re-read via the Volume trait.
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
    /// When this listing was created. Used by `snapshot_listings` for triage,
    /// surfacing orphan listings (e.g., volume dropdown previews) in error reports.
    pub created_at: Instant,
}

/// Lightweight summary of one cached listing, for `snapshot_listings`.
pub struct ListingSummary {
    pub listing_id: String,
    pub volume_id: String,
    pub path: PathBuf,
    pub entry_count: usize,
    pub age_ms: u128,
}

/// Returns a snapshot of every active listing in the cache. Used by `cmdr://state`
/// so triagers can spot orphan listings (started but never bound to a pane,
/// for example when a volume dropdown commits a navigation that the user then
/// abandons or that surfaces an error).
pub fn snapshot_listings() -> Vec<ListingSummary> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let now = Instant::now();
    let mut out: Vec<ListingSummary> = cache
        .iter()
        .map(|(id, listing)| ListingSummary {
            listing_id: id.clone(),
            volume_id: listing.volume_id.clone(),
            path: listing.path.clone(),
            entry_count: listing.entries.len(),
            age_ms: now.saturating_duration_since(listing.created_at).as_millis(),
        })
        .collect();
    out.sort_by_key(|a| a.age_ms);
    out
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

    // Skip if no AppHandle is registered yet (test or pre-init context).
    // Without it the `tokio::spawn` in FullRefresh has no runtime, and the
    // coalesced flush would never emit anyway.
    let has_app = WATCHER_MANAGER.read().ok().and_then(|m| m.app_handle.clone()).is_some();
    if !has_app {
        return;
    }

    match change {
        DirectoryChange::Added(entry) => {
            let mut entry = entry;
            crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_added(listing_id, entry.clone());
            }
        }
        DirectoryChange::Removed(name) => {
            let full_path = parent_path.join(&name);
            for (listing_id, ..) in &listings {
                notify_removed(listing_id, &full_path);
            }
        }
        DirectoryChange::Modified(entry) => {
            let mut entry = entry;
            crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_modified(listing_id, entry.clone());
            }
        }
        DirectoryChange::Renamed { old_name, new_entry } => {
            let mut new_entry = new_entry;
            crate::indexing::enrich_entries_with_index(std::slice::from_mut(&mut new_entry));
            let old_path = parent_path.join(&old_name);
            for (listing_id, ..) in &listings {
                notify_removed(listing_id, &old_path);
                notify_added(listing_id, new_entry.clone());
            }
        }
        DirectoryChange::FullRefresh => {
            if listings.is_empty() {
                // No listing matches this exact path. For STATUS_NOTIFY_ENUM_DIR the
                // path is the share root, but the user may be browsing a subdirectory.
                // Refresh all listings on this volume instead.
                let volume_listings = find_listings_on_volume(volume_id);
                for (lid, path, sort_by, sort_order, dir_sort_mode) in volume_listings {
                    let volume_id = volume_id.to_string();
                    tokio::spawn(notify_full_refresh(
                        volume_id,
                        path,
                        vec![(lid, sort_by, sort_order, dir_sort_mode)],
                    ));
                }
            } else {
                let volume_id = volume_id.to_string();
                let parent_path = parent_path.to_path_buf();
                tokio::spawn(notify_full_refresh(volume_id, parent_path, listings));
            }
        }
    }
}

/// Inserts an entry into the cache and queues a single-add change for the next
/// coalesced `directory-diff` flush.
fn notify_added(listing_id: &str, entry: FileEntry) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    let Some(index) = insert_entry_sorted(listing_id, entry.clone()) else {
        return; // Already exists or listing gone
    };

    enqueue_diff(
        listing_id,
        vec![DiffChange {
            change_type: "add".to_string(),
            entry,
            index,
        }],
    );
}

/// Removes an entry from the cache and queues a single-remove change.
fn notify_removed(listing_id: &str, full_path: &Path) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    let Some((index, removed_entry)) = remove_entry_by_path(listing_id, full_path) else {
        return; // Not in cache or listing gone
    };

    enqueue_diff(
        listing_id,
        vec![DiffChange {
            change_type: "remove".to_string(),
            entry: removed_entry,
            index,
        }],
    );
}

/// Updates an entry in the cache and queues a modify (or remove+add) change.
fn notify_modified(listing_id: &str, entry: FileEntry) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    let result = match update_entry_sorted(listing_id, entry.clone()) {
        Some(r) => r,
        None => return,
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

    enqueue_diff(listing_id, changes);
}

/// Re-reads a directory via the Volume trait, computes a diff, and queues it.
async fn notify_full_refresh(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::listing::sorting::sort_entries;
    use crate::file_system::watcher::compute_diff;

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

        enqueue_diff(listing_id, changes);
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

/// Returns cached entries for `(volume_id, path)` when the volume reports that this
/// listing is being kept in sync by an active watcher. Otherwise `None`.
///
/// **Freshness contract (read carefully)**: a `Some(_)` result means the volume has
/// an active change-notification channel and the cache reflects the volume's most
/// recently observed state. It does NOT mean the cache is byte-perfect with the
/// device right now: every backend has a debounce or settling window between a real
/// change and the cache reflecting it.
///
/// - Local FS: FSEvents coalesce window (~10 ms).
/// - SMB: 200 ms watcher debounce; > 50 events per directory triggers a
///   `FullRefresh` which arrives via a real re-read.
/// - MTP: 500 ms event debouncer plus per-device polling. Many MTP devices
///   (cameras especially) never emit per-object events, so "watched" there means
///   only "the device is reachable and would forward changes if it sent any."
///
/// Callers must treat the result as "fresh as our most recent observation," which
/// is the same guarantee a `list_directory` call gives: it sees the device's state
/// at the moment the call returned, not at the moment the caller reads its result.
/// The contract intentionally accepts this window; a tighter one would force us to
/// re-validate every walk, defeating the whole point of the oracle.
///
/// When multiple cached listings exist for the same `(volume_id, path)` pair (two
/// panes browsing the same directory), the picker is deterministic: highest
/// `sequence`, ties broken by the latest `created_at`. Both listings receive watcher
/// events, so they're equally fresh; the tiebreaker is just to keep the result
/// stable across calls.
#[allow(dead_code, reason = "M1 plumbing: callers (scan walker, scan-preview) wire up in M2")]
pub fn try_get_watched_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
    // Step 1: find all listings on this (volume_id, path) and pick the most-recently-updated
    // one (highest sequence, ties broken by latest created_at). Read the entries out
    // under the cache lock and drop the lock before crossing any async / volume boundary.
    let entries: Vec<FileEntry> = {
        let cache = LISTING_CACHE.read().ok()?;
        let mut best: Option<(&String, &CachedListing, u64, Instant)> = None;
        for (id, listing) in cache.iter() {
            if listing.volume_id != volume_id || listing.path != path {
                continue;
            }
            let seq = listing.sequence.load(Ordering::Relaxed);
            let created = listing.created_at;
            best = match best {
                None => Some((id, listing, seq, created)),
                Some((_, _, best_seq, best_created))
                    if seq > best_seq || (seq == best_seq && created > best_created) =>
                {
                    Some((id, listing, seq, created))
                }
                Some(other) => Some(other),
            };
        }
        let (_, listing, ..) = best?;
        listing.entries.clone()
    };

    // Step 2: ask the volume whether this listing is being kept fresh by a watcher.
    // VolumeManager::get returns an Arc<dyn Volume> which we hold for the duration of
    // the sync `listing_is_watched` call. No await between this and the entries return.
    let volume = crate::file_system::get_volume_manager().get(volume_id)?;
    if volume.listing_is_watched(path) {
        Some(entries)
    } else {
        None
    }
}
