//! File system watcher with debouncing and diff computation.
//!
//! Watches directories for changes, computes diffs, and emits events to frontend.
//! Uses the unified LISTING_CACHE from operations.rs (no duplicate cache).

use notify_debouncer_full::{
    DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use super::operations::{FileEntry, get_listing_entries, list_directory_core, update_listing_entries};

/// Debounce duration in milliseconds
const DEBOUNCE_MS: u64 = 200;

/// Global watcher manager
static WATCHER_MANAGER: LazyLock<RwLock<WatcherManager>> = LazyLock::new(|| RwLock::new(WatcherManager::new()));

/// A single directory diff change
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffChange {
    /// Type of change: add, remove, or modify
    #[serde(rename = "type")]
    pub change_type: String,
    /// The file entry
    pub entry: FileEntry,
}

/// Diff event sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDiff {
    /// Listing ID this diff belongs to
    pub listing_id: String,
    /// Monotonic sequence number
    pub sequence: u64,
    /// List of changes
    pub changes: Vec<DiffChange>,
}

/// State for a watched directory.
/// NOTE: No `entries` field - we use the unified LISTING_CACHE instead.
struct WatchedDirectory {
    sequence: u64,
    #[allow(dead_code)] // Debouncer must be held to keep watching
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

/// Manages file watchers for directories
pub struct WatcherManager {
    watches: HashMap<String, WatchedDirectory>,
    app_handle: Option<AppHandle>,
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
    let listing_id_owned = listing_id.to_string();
    let listing_for_closure = listing_id_owned.clone();

    // Create the debouncer with a callback that handles changes
    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_MS),
        None, // No tick rate limit
        move |result: DebounceEventResult| {
            if let Ok(_events) = result {
                // Events occurred - re-read directory and compute diff
                handle_directory_change(&listing_for_closure);
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

    manager
        .watches
        .insert(listing_id_owned, WatchedDirectory { sequence: 0, debouncer });

    Ok(())
}

/// Stop watching a directory for a given listing.
pub fn stop_watching(listing_id: &str) {
    if let Ok(mut manager) = WATCHER_MANAGER.write() {
        // Dropping the WatchedDirectory will drop the debouncer
        manager.watches.remove(listing_id);
    }
}

/// Handle a directory change event.
/// Re-reads the directory, computes diff, updates LISTING_CACHE, and emits event.
fn handle_directory_change(listing_id: &str) {
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

    // Re-read the directory using core metadata (extended metadata not needed for diffs)
    let new_entries = match list_directory_core(&path) {
        Ok(entries) => entries,
        Err(e) => {
            // Silently ignore permission denied - user may have revoked access
            if e.kind() != std::io::ErrorKind::PermissionDenied {
                eprintln!("[WATCHER] Failed to re-read directory: {}", e);
            }
            return;
        }
    };

    // Compute diff
    let changes = compute_diff(&old_entries, &new_entries);

    if changes.is_empty() {
        return; // No actual changes
    }

    // Update the unified LISTING_CACHE with new entries
    update_listing_entries(listing_id, new_entries);

    // Increment sequence and get current value
    let sequence = {
        let mut manager = match WATCHER_MANAGER.write() {
            Ok(m) => m,
            Err(_) => return,
        };

        let watch = match manager.watches.get_mut(listing_id) {
            Some(w) => w,
            None => return,
        };

        watch.sequence += 1;
        watch.sequence
    };

    // Emit event to frontend
    if let Some(app) = app_handle {
        let diff = DirectoryDiff {
            listing_id: listing_id.to_string(),
            sequence,
            changes,
        };

        if let Err(e) = app.emit("directory-diff", &diff) {
            eprintln!("[WATCHER] Failed to emit event: {}", e);
        }
    }
}

/// Compute the diff between old and new directory listings.
pub(crate) fn compute_diff(old: &[FileEntry], new: &[FileEntry]) -> Vec<DiffChange> {
    let mut changes = Vec::new();

    // Create lookup maps by path
    let old_map: HashMap<&str, &FileEntry> = old.iter().map(|e| (e.path.as_str(), e)).collect();
    let new_map: HashMap<&str, &FileEntry> = new.iter().map(|e| (e.path.as_str(), e)).collect();

    // Find additions and modifications
    for new_entry in new {
        match old_map.get(new_entry.path.as_str()) {
            None => {
                // New entry - addition
                changes.push(DiffChange {
                    change_type: "add".to_string(),
                    entry: new_entry.clone(),
                });
            }
            Some(old_entry) => {
                // Exists in both - check if modified
                if is_entry_modified(old_entry, new_entry) {
                    changes.push(DiffChange {
                        change_type: "modify".to_string(),
                        entry: new_entry.clone(),
                    });
                }
            }
        }
    }

    // Find removals
    for old_entry in old {
        if !new_map.contains_key(old_entry.path.as_str()) {
            changes.push(DiffChange {
                change_type: "remove".to_string(),
                entry: old_entry.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str, size: Option<u64>) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: format!("/test/{}", name),
            is_directory: false,
            is_symlink: false,
            size,
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "user".to_string(),
            group: "group".to_string(),
            icon_id: "ext:txt".to_string(),
            extended_metadata_loaded: true,
        }
    }

    #[test]
    fn test_compute_diff_addition() {
        let old = vec![make_entry("a.txt", Some(100))];
        let new = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];

        let diff = compute_diff(&old, &new);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].change_type, "add");
        assert_eq!(diff[0].entry.name, "b.txt");
    }

    #[test]
    fn test_compute_diff_removal() {
        let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];
        let new = vec![make_entry("a.txt", Some(100))];

        let diff = compute_diff(&old, &new);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].change_type, "remove");
        assert_eq!(diff[0].entry.name, "b.txt");
    }

    #[test]
    fn test_compute_diff_modification() {
        let old = vec![make_entry("a.txt", Some(100))];
        let new = vec![make_entry("a.txt", Some(200))]; // Size changed

        let diff = compute_diff(&old, &new);
        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].change_type, "modify");
        assert_eq!(diff[0].entry.size, Some(200));
    }

    #[test]
    fn test_compute_diff_no_change() {
        let old = vec![make_entry("a.txt", Some(100))];
        let new = vec![make_entry("a.txt", Some(100))];

        let diff = compute_diff(&old, &new);
        assert!(diff.is_empty());
    }
}
