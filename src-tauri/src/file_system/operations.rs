//! File system operations: read, list, copy, move, delete.

#![allow(dead_code)] // Boilerplate for future use

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::LazyLock;
use std::sync::RwLock;
use uuid::Uuid;
use uzers::{get_group_by_gid, get_user_by_uid};

use super::watcher::{start_watching, stop_watching};
use crate::benchmark;

// ============================================================================
// Sorting configuration
// ============================================================================

/// Column to sort files by.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    #[default]
    Name,
    Extension,
    Size,
    Modified,
    Created,
}

/// Sort order (ascending or descending).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

/// Cache for uid→username and gid→groupname resolution.
static OWNER_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
static GROUP_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cache for directory listings (on-demand virtual scrolling).
/// Key: listing_id, Value: cached listing with all entries.
#[cfg(not(test))]
static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
#[cfg(test)]
pub(super) static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cached directory listing for on-demand virtual scrolling.
#[cfg(not(test))]
struct CachedListing {
    /// Volume ID this listing belongs to (e.g., "root", "dropbox")
    volume_id: String,
    /// Path within the volume (absolute path for now)
    path: std::path::PathBuf,
    /// Cached file entries
    entries: Vec<FileEntry>,
    /// Current sort column
    sort_by: SortColumn,
    /// Current sort order
    sort_order: SortOrder,
}

/// Cached directory listing for on-demand virtual scrolling.
#[cfg(test)]
pub(super) struct CachedListing {
    /// Volume ID this listing belongs to (e.g., "root", "dropbox")
    pub volume_id: String,
    /// Path within the volume (absolute path for now)
    pub path: std::path::PathBuf,
    /// Cached file entries
    pub entries: Vec<FileEntry>,
    /// Current sort column
    pub sort_by: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
}

// ============================================================================
// Sorting implementation
// ============================================================================

/// Extracts file extension for sorting purposes.
/// Returns: (is_dotfile, has_extension, extension_lowercase)
/// Dotfiles (names starting with .) sort first, then files without extension, then by extension.
fn extract_extension_for_sort(name: &str) -> (bool, bool, String) {
    // Dotfiles (e.g., .gitignore) sort first
    if name.starts_with('.') && !name[1..].contains('.') {
        return (true, false, String::new());
    }

    // Check for extension
    if let Some(dot_pos) = name.rfind('.')
        && dot_pos > 0
        && dot_pos < name.len() - 1
    {
        let ext = name[dot_pos + 1..].to_lowercase();
        return (false, true, ext);
    }

    // No extension
    (false, false, String::new())
}

/// Sorts file entries by the specified column and order.
/// Directories always come first, then files.
/// Uses natural sorting for string comparisons (e.g., "img_2" before "img_10").
pub fn sort_entries(entries: &mut [FileEntry], sort_by: SortColumn, sort_order: SortOrder) {
    entries.sort_by(|a, b| {
        // Directories always come first
        match (a.is_directory, b.is_directory) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // Compare by the selected column
        let primary = match sort_by {
            SortColumn::Name => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
            SortColumn::Extension => {
                let (a_dotfile, a_has_ext, a_ext) = extract_extension_for_sort(&a.name);
                let (b_dotfile, b_has_ext, b_ext) = extract_extension_for_sort(&b.name);

                // Dotfiles first, then no extension, then by extension
                match (a_dotfile, b_dotfile) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, true) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                    (false, false) => match (a_has_ext, b_has_ext) {
                        (false, true) => std::cmp::Ordering::Less,
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, false) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                        (true, true) => {
                            let ext_cmp = alphanumeric_sort::compare_str(&a_ext, &b_ext);
                            if ext_cmp == std::cmp::Ordering::Equal {
                                alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase())
                            } else {
                                ext_cmp
                            }
                        }
                    },
                }
            }
            SortColumn::Size => {
                // For directories, size is None - sort them by name among themselves
                match (a.size, b.size) {
                    (None, None) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(a_size), Some(b_size)) => a_size.cmp(&b_size),
                }
            }
            SortColumn::Modified => match (a.modified_at, b.modified_at) {
                (None, None) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            },
            SortColumn::Created => match (a.created_at, b.created_at) {
                (None, None) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            },
        };

        // Apply sort order
        match sort_order {
            SortOrder::Ascending => primary,
            SortOrder::Descending => primary.reverse(),
        }
    });
}

/// Resolves a uid to a username, with caching.
fn get_owner_name(uid: u32) -> String {
    // Try read lock first
    if let Ok(cache) = OWNER_CACHE.read()
        && let Some(name) = cache.get(&uid)
    {
        return name.clone();
    }
    // Cache miss, resolve and store
    let name = get_user_by_uid(uid)
        .map(|u| u.name().to_string_lossy().into_owned())
        .unwrap_or_else(|| uid.to_string());
    if let Ok(mut cache) = OWNER_CACHE.write() {
        cache.insert(uid, name.clone());
    }
    name
}

/// Resolves a gid to a group name, with caching.
fn get_group_name(gid: u32) -> String {
    if let Ok(cache) = GROUP_CACHE.read()
        && let Some(name) = cache.get(&gid)
    {
        return name.clone();
    }
    let name = get_group_by_gid(gid)
        .map(|g| g.name().to_string_lossy().into_owned())
        .unwrap_or_else(|| gid.to_string());
    if let Ok(mut cache) = GROUP_CACHE.write() {
        cache.insert(gid, name.clone());
    }
    name
}

/// Generates icon ID based on file type and extension.
fn get_icon_id(is_dir: bool, is_symlink: bool, name: &str) -> String {
    if is_symlink {
        // Distinguish symlinks to directories vs files
        return if is_dir {
            "symlink-dir".to_string()
        } else {
            "symlink-file".to_string()
        };
    }
    if is_dir {
        return "dir".to_string();
    }
    // Extract extension
    if let Some(ext) = Path::new(name).extension() {
        return format!("ext:{}", ext.to_string_lossy().to_lowercase());
    }
    "file".to_string()
}

/// Represents a file or directory entry with extended metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
    pub created_at: Option<u64>,
    /// When the file was added to its current directory (macOS only)
    pub added_at: Option<u64>,
    /// When the file was last opened (macOS only)
    pub opened_at: Option<u64>,
    pub permissions: u32,
    pub owner: String,
    pub group: String,
    pub icon_id: String,
    /// Whether extended metadata (addedAt, openedAt) has been loaded
    /// Always true for legacy list_directory(), false for list_directory_core()
    #[serde(default = "default_extended_loaded")]
    pub extended_metadata_loaded: bool,
}

/// Default value for extended_metadata_loaded (for backwards compatibility)
fn default_extended_loaded() -> bool {
    true
}

/// Lists the contents of a directory.
///
/// # Arguments
/// * `path` - The directory path to list
///
/// # Returns
/// A vector of FileEntry representing the directory contents, sorted with directories first,
/// then files, both alphabetically.
pub fn list_directory(path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
    let overall_start = std::time::Instant::now();
    let mut entries = Vec::new();

    let mut metadata_time = std::time::Duration::ZERO;
    let mut owner_lookup_time = std::time::Duration::ZERO;
    let mut entry_creation_time = std::time::Duration::ZERO;

    let read_start = std::time::Instant::now();
    let dir_entries: Vec<_> = fs::read_dir(path)?.collect();
    let read_dir_time = read_start.elapsed();

    for entry in dir_entries {
        let entry = entry?;

        let meta_start = std::time::Instant::now();
        let file_type = entry.file_type()?;
        let is_symlink = file_type.is_symlink();

        // For symlinks, check if the TARGET is a directory by following the link
        // fs::metadata follows symlinks, fs::symlink_metadata does not
        let target_is_dir = if is_symlink {
            fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false) // Broken symlink = treat as file
        } else {
            false
        };

        // For symlinks, get metadata of the link itself (not target) for size/timestamps
        let metadata = if is_symlink {
            fs::symlink_metadata(entry.path())
        } else {
            entry.metadata()
        };
        metadata_time += meta_start.elapsed();

        match metadata {
            Ok(metadata) => {
                let name = entry.file_name().to_string_lossy().to_string();
                // is_directory: true if it's a real dir OR a symlink pointing to a dir
                let is_dir = metadata.is_dir() || target_is_dir;

                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());

                let created = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());

                let uid = metadata.uid();
                let gid = metadata.gid();

                let owner_start = std::time::Instant::now();
                let owner = get_owner_name(uid);
                let group = get_group_name(gid);
                owner_lookup_time += owner_start.elapsed();

                let create_start = std::time::Instant::now();
                // Get macOS-specific metadata (added_at, opened_at)
                #[cfg(target_os = "macos")]
                let (added_at, opened_at) = {
                    let macos_meta = super::macos_metadata::get_macos_metadata(&entry.path());
                    (macos_meta.added_at, macos_meta.opened_at)
                };
                #[cfg(not(target_os = "macos"))]
                let (added_at, opened_at) = (None, None);

                entries.push(FileEntry {
                    name: name.clone(),
                    path: entry.path().to_string_lossy().to_string(),
                    is_directory: is_dir,
                    is_symlink,
                    size: if metadata.is_file() { Some(metadata.len()) } else { None },
                    modified_at: modified,
                    created_at: created,
                    added_at,
                    opened_at,
                    permissions: metadata.permissions().mode(),
                    owner,
                    group,
                    icon_id: get_icon_id(is_dir, is_symlink, &name),
                    extended_metadata_loaded: true,
                });
                entry_creation_time += create_start.elapsed();
            }
            Err(_) => {
                // Permission denied or broken symlink—return minimal entry
                let name = entry.file_name().to_string_lossy().to_string();
                entries.push(FileEntry {
                    name: name.clone(),
                    path: entry.path().to_string_lossy().to_string(),
                    is_directory: false,
                    is_symlink,
                    size: None,
                    modified_at: None,
                    created_at: None,
                    added_at: None,
                    opened_at: None,
                    permissions: 0,
                    owner: String::new(),
                    group: String::new(),
                    icon_id: if is_symlink {
                        "symlink-broken".to_string()
                    } else {
                        "file".to_string()
                    },
                    extended_metadata_loaded: true,
                });
            }
        }
    }

    let sort_start = std::time::Instant::now();
    // Sort: directories first, then files, both alphabetically (using natural sort)
    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);
    let sort_time = sort_start.elapsed();

    let total_time = overall_start.elapsed();
    eprintln!(
        "[RUST TIMING] list_directory: path={}, entries={}, read_dir={}ms, metadata={}ms, owner={}ms, create={}ms, sort={}ms, total={}ms",
        path.display(),
        entries.len(),
        read_dir_time.as_millis(),
        metadata_time.as_millis(),
        owner_lookup_time.as_millis(),
        entry_creation_time.as_millis(),
        sort_time.as_millis(),
        total_time.as_millis()
    );

    Ok(entries)
}

// ============================================================================
// On-demand virtual scrolling API (listing-based, fetch by range)
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
///
/// # Arguments
/// * `path` - The directory path to list
/// * `include_hidden` - Whether to include hidden files in total count
///
/// # Returns
/// A `ListingStartResult` with listing ID and total count.
pub fn list_directory_start(path: &Path, include_hidden: bool) -> Result<ListingStartResult, std::io::Error> {
    // Use the default volume from VolumeManager with default sorting
    list_directory_start_with_volume("root", path, include_hidden, SortColumn::Name, SortOrder::Ascending)
}

/// Starts a new directory listing using a specific volume.
///
/// This is the internal implementation that supports multi-volume access.
///
/// # Arguments
/// * `volume_id` - The volume ID to use (e.g., "root", "dropbox")
/// * `path` - The directory path to list (relative to volume root)
/// * `include_hidden` - Whether to include hidden files in total count
/// * `sort_by` - Column to sort by
/// * `sort_order` - Ascending or descending
///
/// # Returns
/// A `ListingStartResult` with listing ID and total count.
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
    let volume = super::get_volume_manager().get(volume_id).ok_or_else(|| {
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
        all_entries.iter().filter(|e| !e.name.starts_with('.')).count()
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
    // For now, we still use the absolute path for watching
    // TODO: Update watcher to be volume-aware
    if volume.supports_watching() {
        // For LocalPosixVolume, the path is already absolute or needs to be resolved
        // We use the original path since LocalPosixVolume root is "/"
        if let Err(e) = start_watching(&listing_id, path) {
            eprintln!("[LISTING] Failed to start watcher: {}", e);
            // Continue anyway - watcher is optional enhancement
        }
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

/// Gets a range of entries from a cached listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `start` - Start index (0-based)
/// * `count` - Number of entries to return
/// * `include_hidden` - Whether to include hidden files
///
/// # Returns
/// Vector of FileEntry for the requested range.
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
        let visible: Vec<&FileEntry> = listing.entries.iter().filter(|e| !e.name.starts_with('.')).collect();
        let end = (start + count).min(visible.len());
        Ok(visible[start..end].iter().cloned().cloned().collect())
    }
}

/// Gets total count of entries in a cached listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `include_hidden` - Whether to include hidden files in count
///
/// # Returns
/// Total count of (visible) entries.
pub fn get_total_count(listing_id: &str, include_hidden: bool) -> Result<usize, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    if include_hidden {
        Ok(listing.entries.len())
    } else {
        Ok(listing.entries.iter().filter(|e| !e.name.starts_with('.')).count())
    }
}

/// Finds the index of a file by name in a cached listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `name` - File name to find
/// * `include_hidden` - Whether to include hidden files when calculating index
///
/// # Returns
/// Index of the file, or None if not found.
pub fn find_file_index(listing_id: &str, name: &str, include_hidden: bool) -> Result<Option<usize>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    if include_hidden {
        Ok(listing.entries.iter().position(|e| e.name == name))
    } else {
        // Find index in filtered list
        let visible: Vec<&FileEntry> = listing.entries.iter().filter(|e| !e.name.starts_with('.')).collect();
        Ok(visible.iter().position(|e| e.name == name))
    }
}

/// Gets a single file at the given index.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `index` - Index of the file to get
/// * `include_hidden` - Whether to include hidden files when calculating index
///
/// # Returns
/// FileEntry at the index, or None if out of bounds.
pub fn get_file_at(listing_id: &str, index: usize, include_hidden: bool) -> Result<Option<FileEntry>, String> {
    let cache = LISTING_CACHE.read().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    if include_hidden {
        Ok(listing.entries.get(index).cloned())
    } else {
        let visible: Vec<&FileEntry> = listing.entries.iter().filter(|e| !e.name.starts_with('.')).collect();
        Ok(visible.get(index).cloned().cloned())
    }
}

/// Ends a directory listing and cleans up the cache.
///
/// # Arguments
/// * `listing_id` - The listing ID to clean up
pub fn list_directory_end(listing_id: &str) {
    // Stop the file watcher
    stop_watching(listing_id);

    // Remove from listing cache
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.remove(listing_id);
    }
}

/// Result of re-sorting a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResortResult {
    /// New index of the file that was at the cursor position before re-sorting.
    /// None if the filename wasn't provided or wasn't found.
    pub new_cursor_index: Option<usize>,
}

/// Re-sorts an existing cached listing in-place.
///
/// This is more efficient than creating a new listing when you just want to change the sort order.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `sort_by` - Column to sort by
/// * `sort_order` - Ascending or descending
/// * `cursor_filename` - Optional filename to track; returns its new index after sorting
/// * `include_hidden` - Whether to include hidden files when calculating cursor index
///
/// # Returns
/// A `ResortResult` with the new cursor index (if filename was provided and found).
pub fn resort_listing(
    listing_id: &str,
    sort_by: SortColumn,
    sort_order: SortOrder,
    cursor_filename: Option<&str>,
    include_hidden: bool,
) -> Result<ResortResult, String> {
    let mut cache = LISTING_CACHE.write().map_err(|_| "Failed to acquire cache lock")?;

    let listing = cache
        .get_mut(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

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
                .filter(|e| !e.name.starts_with('.'))
                .position(|e| e.name == name)
        }
    });

    Ok(ResortResult { new_cursor_index })
}

// ============================================================================
// Internal cache accessors for file watcher
// ============================================================================

/// Gets entries and path from the listing cache (for watcher diff computation).
/// Returns None if listing not found.
pub(super) fn get_listing_entries(listing_id: &str) -> Option<(std::path::PathBuf, Vec<FileEntry>)> {
    let cache = LISTING_CACHE.read().ok()?;
    let listing = cache.get(listing_id)?;
    Some((listing.path.clone(), listing.entries.clone()))
}

/// Updates the entries in the listing cache (after watcher detects changes).
pub(super) fn update_listing_entries(listing_id: &str, entries: Vec<FileEntry>) {
    if let Ok(mut cache) = LISTING_CACHE.write()
        && let Some(listing) = cache.get_mut(listing_id)
    {
        listing.entries = entries;
    }
}

// ============================================================================
// Two-phase metadata loading: Fast core data, then extended metadata
// ============================================================================

/// Lists the contents of a directory with CORE metadata only.
///
/// This is significantly faster than `list_directory()` because it skips
/// macOS-specific metadata (addedAt, openedAt) which require additional system calls.
///
/// Use `get_extended_metadata_batch()` to fetch extended metadata later.
///
/// # Arguments
/// * `path` - The directory path to list
///
/// # Returns
/// A vector of FileEntry with `extended_metadata_loaded = false`
pub fn list_directory_core(path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
    benchmark::log_event("list_directory_core START");
    let overall_start = std::time::Instant::now();
    let mut entries = Vec::new();

    benchmark::log_event("readdir START");
    let read_start = std::time::Instant::now();
    let dir_entries: Vec<_> = fs::read_dir(path)?.collect();
    let read_dir_time = read_start.elapsed();
    benchmark::log_event_value("readdir END, count", dir_entries.len());

    benchmark::log_event("stat_loop START");
    let mut metadata_time = std::time::Duration::ZERO;
    let mut owner_lookup_time = std::time::Duration::ZERO;

    for entry in dir_entries {
        let entry = entry?;

        let meta_start = std::time::Instant::now();
        let file_type = entry.file_type()?;
        let is_symlink = file_type.is_symlink();

        // For symlinks, check if the TARGET is a directory
        let target_is_dir = if is_symlink {
            fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            false
        };

        // For symlinks, get metadata of the link itself (not target)
        let metadata = if is_symlink {
            fs::symlink_metadata(entry.path())
        } else {
            entry.metadata()
        };
        metadata_time += meta_start.elapsed();

        match metadata {
            Ok(metadata) => {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = metadata.is_dir() || target_is_dir;

                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());

                let created = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());

                let uid = metadata.uid();
                let gid = metadata.gid();

                let owner_start = std::time::Instant::now();
                let owner = get_owner_name(uid);
                let group = get_group_name(gid);
                owner_lookup_time += owner_start.elapsed();

                // SKIP macOS metadata - that's the key optimization!
                entries.push(FileEntry {
                    name: name.clone(),
                    path: entry.path().to_string_lossy().to_string(),
                    is_directory: is_dir,
                    is_symlink,
                    size: if metadata.is_file() { Some(metadata.len()) } else { None },
                    modified_at: modified,
                    created_at: created,
                    added_at: None,  // Will be loaded later
                    opened_at: None, // Will be loaded later
                    permissions: metadata.permissions().mode(),
                    owner,
                    group,
                    icon_id: get_icon_id(is_dir, is_symlink, &name),
                    extended_metadata_loaded: false, // Not loaded yet!
                });
            }
            Err(_) => {
                // Permission denied or broken symlink
                let name = entry.file_name().to_string_lossy().to_string();
                entries.push(FileEntry {
                    name: name.clone(),
                    path: entry.path().to_string_lossy().to_string(),
                    is_directory: false,
                    is_symlink,
                    size: None,
                    modified_at: None,
                    created_at: None,
                    added_at: None,
                    opened_at: None,
                    permissions: 0,
                    owner: String::new(),
                    group: String::new(),
                    icon_id: if is_symlink {
                        "symlink-broken".to_string()
                    } else {
                        "file".to_string()
                    },
                    extended_metadata_loaded: true, // Nothing to load for broken entries
                });
            }
        }
    }
    benchmark::log_event_value("stat_loop END, entries", entries.len());

    // Sort: directories first, then files, both alphabetically (using natural sort)
    benchmark::log_event("sort START");
    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);
    benchmark::log_event("sort END");

    let total_time = overall_start.elapsed();
    eprintln!(
        "[RUST TIMING] list_directory_core: path={}, entries={}, read_dir={}ms, metadata={}ms, owner={}ms, total={}ms",
        path.display(),
        entries.len(),
        read_dir_time.as_millis(),
        metadata_time.as_millis(),
        owner_lookup_time.as_millis(),
        total_time.as_millis()
    );
    benchmark::log_event("list_directory_core END");

    Ok(entries)
}

/// Gets metadata for a single file or directory path.
///
/// This is used when we need metadata for a single path rather than listing
/// a directory. Useful for symlink target resolution and volume implementations.
///
/// # Arguments
/// * `path` - Absolute path to the file or directory
///
/// # Returns
/// A FileEntry with metadata for the path
pub fn get_single_entry(path: &Path) -> Result<FileEntry, std::io::Error> {
    // Check if it's a symlink first
    let symlink_meta = fs::symlink_metadata(path)?;
    let is_symlink = symlink_meta.file_type().is_symlink();

    // For symlinks, check if the target is a directory
    let target_is_dir = if is_symlink {
        fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        false
    };

    // Use symlink metadata for the entry (not following the link)
    let metadata = &symlink_meta;
    let is_dir = metadata.is_dir() || target_is_dir;

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let uid = metadata.uid();
    let gid = metadata.gid();
    let owner = get_owner_name(uid);
    let group = get_group_name(gid);

    Ok(FileEntry {
        name: name.clone(),
        path: path.to_string_lossy().to_string(),
        is_directory: is_dir,
        is_symlink,
        size: if metadata.is_file() { Some(metadata.len()) } else { None },
        modified_at: modified,
        created_at: created,
        added_at: None,
        opened_at: None,
        permissions: metadata.permissions().mode(),
        owner,
        group,
        icon_id: get_icon_id(is_dir, is_symlink, &name),
        extended_metadata_loaded: false,
    })
}

/// Extended metadata for a single file (macOS-specific fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedMetadata {
    /// File path (key for merging)
    pub path: String,
    /// When the file was added to its current directory (macOS only)
    pub added_at: Option<u64>,
    /// When the file was last opened (macOS only)
    pub opened_at: Option<u64>,
}

/// Fetches extended metadata for a batch of file paths.
///
/// This is called after the initial directory listing to populate
/// macOS-specific metadata (addedAt, openedAt) without blocking initial render.
///
/// # Arguments
/// * `paths` - File paths to fetch extended metadata for
///
/// # Returns
/// Vector of ExtendedMetadata for each path
#[cfg(target_os = "macos")]
pub fn get_extended_metadata_batch(paths: Vec<String>) -> Vec<ExtendedMetadata> {
    use std::path::Path;

    benchmark::log_event_value("get_extended_metadata_batch START, count", paths.len());
    let result: Vec<ExtendedMetadata> = paths
        .into_iter()
        .map(|path_str| {
            let path = Path::new(&path_str);
            let macos_meta = super::macos_metadata::get_macos_metadata(path);
            ExtendedMetadata {
                path: path_str,
                added_at: macos_meta.added_at,
                opened_at: macos_meta.opened_at,
            }
        })
        .collect();
    benchmark::log_event_value("get_extended_metadata_batch END, count", result.len());
    result
}

#[cfg(not(target_os = "macos"))]
pub fn get_extended_metadata_batch(paths: Vec<String>) -> Vec<ExtendedMetadata> {
    benchmark::log_event_value("get_extended_metadata_batch (non-macOS), count", paths.len());
    // On non-macOS, there's no extended metadata to fetch
    paths
        .into_iter()
        .map(|path_str| ExtendedMetadata {
            path: path_str,
            added_at: None,
            opened_at: None,
        })
        .collect()
}
