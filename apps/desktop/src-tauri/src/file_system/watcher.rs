//! File system watcher with debouncing, incremental processing, and diff computation.
//!
//! Watches directories for changes and emits `directory-diff` events to frontend.
//! Uses the unified LISTING_CACHE from operations.rs (no duplicate cache).
//! Two processing paths: incremental (stat + classify individual events, patch cache
//! in-place via cache helpers) and full re-read fallback (> 500 events or unknown
//! event kinds).

use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode, event::EventKind},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::file_system::listing::{
    FileEntry, ModifyResult, get_listing_entries, get_listing_path, get_single_entry, has_entry, increment_sequence,
    insert_entry_sorted, list_directory_core, remove_entry_by_path, update_entry_sorted, update_listing_entries,
};

/// Default debounce duration in milliseconds (used if not configured)
const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Configured debounce duration in milliseconds (set by frontend via update_debounce_ms)
static DEBOUNCE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(DEFAULT_DEBOUNCE_MS);

/// Updates the file watcher debounce duration.
/// This affects newly started watchers; existing watchers keep their original duration.
pub fn update_debounce_ms(ms: u64) {
    DEBOUNCE_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
    log::debug!("File watcher debounce updated to {} ms", ms);
}

/// Gets the current debounce duration in milliseconds.
fn get_debounce_ms() -> u64 {
    DEBOUNCE_MS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Global watcher manager
pub(crate) static WATCHER_MANAGER: LazyLock<RwLock<WatcherManager>> =
    LazyLock::new(|| RwLock::new(WatcherManager::new()));

/// A single directory diff change
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffChange {
    /// `"add"`, `"remove"`, or `"modify"`.
    #[serde(rename = "type")]
    pub change_type: String,
    pub entry: FileEntry,
    /// Position in the sorted listing: old listing for `"remove"`, new listing for `"add"`/`"modify"`.
    pub index: usize,
}

/// Diff event sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDiff {
    pub listing_id: String,
    /// Monotonic.
    pub sequence: u64,
    pub changes: Vec<DiffChange>,
}

/// Event sent when the watched directory itself is deleted
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDeletedEvent {
    pub listing_id: String,
    pub path: String,
}

/// State for a watched directory.
/// NOTE: No `entries` field - we use the unified LISTING_CACHE instead.
pub(crate) struct WatchedDirectory {
    #[allow(dead_code, reason = "Debouncer must be held to keep watching")]
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

/// Manages file watchers for directories
pub(crate) struct WatcherManager {
    pub(crate) watches: HashMap<String, WatchedDirectory>,
    pub(crate) app_handle: Option<AppHandle>,
}

impl WatcherManager {
    fn new() -> Self {
        Self {
            watches: HashMap::new(),
            app_handle: None,
        }
    }
}

/// Initialize the watcher manager with the app handle.
/// Must be called during app setup.
pub fn init_watcher_manager(app: AppHandle) {
    if let Ok(mut manager) = WATCHER_MANAGER.write() {
        manager.app_handle = Some(app);
    }
}

/// Start watching a directory for a given listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from list_directory_start
/// * `path` - The directory path to watch
///
/// Note: Initial entries are read from LISTING_CACHE when needed.
pub fn start_watching(listing_id: &str, path: &Path) -> Result<(), String> {
    log::debug!("start_watching: listing_id={}, path={}", listing_id, path.display());
    let listing_id_owned = listing_id.to_string();
    let listing_for_closure = listing_id_owned.clone();

    // Create the debouncer with a callback that handles changes
    let debounce_duration = Duration::from_millis(get_debounce_ms());
    let mut debouncer = new_debouncer(
        debounce_duration,
        None, // No tick rate limit
        move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    handle_directory_change_incremental(&listing_for_closure, events);
                }
                Err(_errors) => {
                    // Watcher errors often mean the watched directory was deleted.
                    // Try to re-read; if it fails with NotFound, we'll emit directory-deleted.
                    let lid = listing_for_closure.clone();
                    tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Start watching the path (Debouncer implements Watcher trait)
    debouncer
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    // Store in manager (no entries - we use LISTING_CACHE)
    let mut manager = WATCHER_MANAGER.write().map_err(|_| "Failed to acquire watcher lock")?;

    manager.watches.insert(listing_id_owned, WatchedDirectory { debouncer });

    Ok(())
}

/// Stop watching a directory for a given listing.
pub fn stop_watching(listing_id: &str) {
    if let Ok(mut manager) = WATCHER_MANAGER.write() {
        // Dropping the WatchedDirectory will drop the debouncer
        manager.watches.remove(listing_id);
    }
}

/// Processes individual file-system events incrementally instead of re-reading the whole directory.
///
/// Falls back to `handle_directory_change` when events are too numerous or ambiguous.
fn handle_directory_change_incremental(listing_id: &str, events: Vec<DebouncedEvent>) {
    // Fallback: too many events or ambiguous event kinds
    if events.len() > 500
        || events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Any | EventKind::Other))
    {
        let lid = listing_id.to_string();
        // `tauri::async_runtime::spawn` instead of `tokio::spawn` because this
        // closure runs on the notify-rs debouncer thread, which has no Tokio
        // runtime context. Tauri's async runtime works from any thread.
        tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
        return;
    }

    // Get watched directory path from the cache (without cloning all entries)
    let Some(dir_path) = get_listing_path(listing_id) else {
        return;
    };

    // Collect unique direct-child paths, skipping access events
    let mut unique_paths: HashSet<PathBuf> = HashSet::new();
    for event in &events {
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        for path in &event.paths {
            if path.parent() == Some(dir_path.as_path()) {
                unique_paths.insert(path.clone());
            }
        }
    }

    if unique_paths.is_empty() {
        return;
    }

    // Stat all paths BEFORE acquiring any locks
    let mut stat_results: HashMap<PathBuf, Option<FileEntry>> = HashMap::new();
    for path in &unique_paths {
        let entry = get_single_entry(path).ok();
        stat_results.insert(path.clone(), entry);
    }

    // Classify changes against the cache
    let mut adds: Vec<FileEntry> = Vec::new();
    let mut removes: Vec<PathBuf> = Vec::new();
    let mut modifies: Vec<FileEntry> = Vec::new();

    for (path, stat_entry) in &stat_results {
        let path_str = path.to_string_lossy();
        let in_cache = has_entry(listing_id, &path_str);
        match (in_cache, stat_entry) {
            (true, Some(entry)) => modifies.push(entry.clone()),
            (true, None) => removes.push(path.clone()),
            (false, Some(entry)) => adds.push(entry.clone()),
            (false, None) => {} // Not in cache and gone from disk — ignore
        }
    }

    if adds.is_empty() && removes.is_empty() && modifies.is_empty() {
        return;
    }

    // Enrich new/modified entries with index data
    for entry in &mut adds {
        crate::indexing::enrich_entries_with_index(std::slice::from_mut(entry));
    }
    for entry in &mut modifies {
        crate::indexing::enrich_entries_with_index(std::slice::from_mut(entry));
    }

    // Apply changes: removes first (indices refer to OLD listing), then adds, then modifies.
    // Look up original indices BEFORE mutating the cache, so all remove indices are in the
    // same (original) listing space. Then apply removes in reverse index order so earlier
    // removals don't shift later ones' positions.
    let mut changes: Vec<DiffChange> = Vec::new();

    // Collect original indices for removes before any mutations
    let mut remove_items: Vec<(usize, PathBuf)> = Vec::new();
    {
        use crate::file_system::listing::caching::LISTING_CACHE;
        let cache = match LISTING_CACHE.read() {
            Ok(c) => c,
            Err(_) => return,
        };
        if let Some(listing) = cache.get(listing_id) {
            for path in &removes {
                let path_str = path.to_string_lossy();
                if let Some(idx) = listing.entries.iter().position(|e| e.path == *path_str) {
                    remove_items.push((idx, path.clone()));
                }
            }
        }
    }

    // Sort removes by index descending so we remove from the end first (preserves indices)
    remove_items.sort_by_key(|item| std::cmp::Reverse(item.0));

    for (original_index, path) in &remove_items {
        if let Some((_mutated_index, removed_entry)) = remove_entry_by_path(listing_id, path) {
            // Emit the original (pre-mutation) index, not the mutated one
            changes.push(DiffChange {
                change_type: "remove".to_string(),
                entry: removed_entry,
                index: *original_index,
            });
        }
    }

    for entry in adds {
        if let Some(new_index) = insert_entry_sorted(listing_id, entry.clone()) {
            changes.push(DiffChange {
                change_type: "add".to_string(),
                entry,
                index: new_index,
            });
        }
    }

    for entry in modifies {
        match update_entry_sorted(listing_id, entry.clone()) {
            Some(ModifyResult::UpdatedInPlace { index }) => {
                changes.push(DiffChange {
                    change_type: "modify".to_string(),
                    entry,
                    index,
                });
            }
            Some(ModifyResult::Moved { old_index, new_index }) => {
                // A moved entry is a remove + add from the frontend's perspective
                changes.push(DiffChange {
                    change_type: "remove".to_string(),
                    entry: entry.clone(),
                    index: old_index,
                });
                changes.push(DiffChange {
                    change_type: "add".to_string(),
                    entry,
                    index: new_index,
                });
            }
            None => {}
        }
    }

    if changes.is_empty() {
        return;
    }

    // Increment sequence and emit
    let app_handle = {
        let manager = match WATCHER_MANAGER.read() {
            Ok(m) => m,
            Err(_) => return,
        };
        manager.app_handle.clone()
    };

    let Some(sequence) = increment_sequence(listing_id) else {
        return;
    };

    if let Some(app) = app_handle {
        let diff = DirectoryDiff {
            listing_id: listing_id.to_string(),
            sequence,
            changes,
        };
        if let Err(e) = app.emit("directory-diff", &diff) {
            log::warn!("Watcher: Failed to emit incremental diff event: {}", e);
        }
    }
}

/// Force a re-read of a directory listing, computing and emitting any diff.
/// Called by the file watcher on change events, and also available as a Tauri
/// command for cases where the watcher doesn't fire (e.g. rename-move on Linux).
///
/// Works for all volume types: reads via the Volume trait's `list_directory`,
/// not via `std::fs`.
pub async fn handle_directory_change(listing_id: &str) {
    log::debug!("handle_directory_change: listing_id={}", listing_id);

    // Look up volume for this listing so we can re-read through the Volume trait.
    let volume = {
        use crate::file_system::listing::caching::LISTING_CACHE;
        let cache = match LISTING_CACHE.read() {
            Ok(c) => c,
            Err(_) => return,
        };
        let listing = match cache.get(listing_id) {
            Some(l) => l,
            None => return,
        };
        crate::file_system::get_volume_manager().get(&listing.volume_id)
    };

    // Get old entries and path from the unified LISTING_CACHE
    let Some((path, old_entries)) = get_listing_entries(listing_id) else {
        return; // Listing no longer exists
    };

    // Get app handle for emitting events
    let app_handle = {
        let manager = match WATCHER_MANAGER.read() {
            Ok(m) => m,
            Err(_) => return,
        };
        manager.app_handle.clone()
    };

    // Re-read the directory via the Volume trait (works for all volume types).
    // Falls back to list_directory_core for listings whose volume was unregistered.
    let new_entries = if let Some(vol) = volume {
        match vol.list_directory(&path, None).await {
            Ok(entries) => entries,
            Err(crate::file_system::VolumeError::NotFound(_)) => {
                log::info!("Watcher: Directory deleted, notifying frontend: {}", path.display());
                if let Some(app) = &app_handle {
                    let event = DirectoryDeletedEvent {
                        listing_id: listing_id.to_string(),
                        path: path.to_string_lossy().to_string(),
                    };
                    if let Err(emit_err) = app.emit("directory-deleted", &event) {
                        log::warn!("Watcher: Failed to emit directory-deleted event: {}", emit_err);
                    }
                }
                stop_watching(listing_id);
                return;
            }
            Err(crate::file_system::VolumeError::PermissionDenied(_)) => return,
            Err(e) => {
                log::warn!("Watcher: Failed to re-read directory: {}", e);
                return;
            }
        }
    } else {
        // Volume unregistered — fall back to std::fs for local paths
        match list_directory_core(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("Watcher: Directory deleted, notifying frontend: {}", path.display());
                if let Some(app) = &app_handle {
                    let event = DirectoryDeletedEvent {
                        listing_id: listing_id.to_string(),
                        path: path.to_string_lossy().to_string(),
                    };
                    if let Err(emit_err) = app.emit("directory-deleted", &event) {
                        log::warn!("Watcher: Failed to emit directory-deleted event: {}", emit_err);
                    }
                }
                stop_watching(listing_id);
                return;
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::PermissionDenied {
                    log::warn!("Watcher: Failed to re-read directory: {}", e);
                }
                return;
            }
        }
    };

    // Re-sort new_entries by the listing's sort params so compute_diff compares
    // two lists in the same order (list_directory returns entries in Name/Asc).
    // Also enrich with index data so diff entries have recursive_size etc.
    let mut new_entries = new_entries;
    {
        use crate::file_system::listing::caching::LISTING_CACHE;
        use crate::file_system::listing::sorting::sort_entries;

        if let Ok(cache) = LISTING_CACHE.read()
            && let Some(listing) = cache.get(listing_id)
        {
            crate::indexing::enrich_entries_with_index(&mut new_entries);
            sort_entries(
                &mut new_entries,
                listing.sort_by,
                listing.sort_order,
                listing.directory_sort_mode,
            );
        }
    }

    // Compute diff
    let changes = compute_diff(&old_entries, &new_entries);

    if changes.is_empty() {
        return; // No actual changes
    }

    // Update the unified LISTING_CACHE with new entries
    update_listing_entries(listing_id, new_entries);

    // Increment sequence and get current value
    let Some(sequence) = increment_sequence(listing_id) else {
        return;
    };

    // Emit event to frontend
    if let Some(app) = app_handle {
        let diff = DirectoryDiff {
            listing_id: listing_id.to_string(),
            sequence,
            changes,
        };

        if let Err(e) = app.emit("directory-diff", &diff) {
            log::warn!("Watcher: Failed to emit event: {}", e);
        }
    }
}

/// Computes the diff between old and new directory listings.
///
/// Used by both local file watcher and MTP file watcher to generate
/// incremental updates for the frontend.
pub fn compute_diff(old: &[FileEntry], new: &[FileEntry]) -> Vec<DiffChange> {
    let mut changes = Vec::new();

    // Create lookup maps by path
    let old_map: HashMap<&str, &FileEntry> = old.iter().map(|e| (e.path.as_str(), e)).collect();
    let new_map: HashSet<&str> = new.iter().map(|e| e.path.as_str()).collect();

    // Find additions and modifications (index refers to position in new listing)
    for (new_index, new_entry) in new.iter().enumerate() {
        match old_map.get(new_entry.path.as_str()) {
            None => {
                changes.push(DiffChange {
                    change_type: "add".to_string(),
                    entry: new_entry.clone(),
                    index: new_index,
                });
            }
            Some(old_entry) => {
                if is_entry_modified(old_entry, new_entry) {
                    changes.push(DiffChange {
                        change_type: "modify".to_string(),
                        entry: new_entry.clone(),
                        index: new_index,
                    });
                }
            }
        }
    }

    // Find removals (index refers to position in old listing)
    for (old_index, old_entry) in old.iter().enumerate() {
        if !new_map.contains(old_entry.path.as_str()) {
            changes.push(DiffChange {
                change_type: "remove".to_string(),
                entry: old_entry.clone(),
                index: old_index,
            });
        }
    }

    changes
}

/// Check if a file entry has been modified.
fn is_entry_modified(old: &FileEntry, new: &FileEntry) -> bool {
    old.size != new.size
        || old.modified_at != new.modified_at
        || old.permissions != new.permissions
        || old.is_directory != new.is_directory
        || old.is_symlink != new.is_symlink
}
