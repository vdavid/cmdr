//! File system operations: read, list, copy, move, delete.

#![allow(dead_code, reason = "Boilerplate for future use")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
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
    path: PathBuf,
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
    pub path: PathBuf,
    /// Cached file entries
    pub entries: Vec<FileEntry>,
    /// Current sort column
    pub sort_by: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
}

// ============================================================================
// Streaming directory listing types
// ============================================================================

/// Status of a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ListingStatus {
    /// Listing is in progress
    Loading,
    /// Listing completed successfully
    Ready,
    /// Listing was cancelled by the user
    Cancelled,
    /// Listing failed with an error
    Error { message: String },
}

/// Result of starting a streaming directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingListingStartResult {
    /// Unique listing ID for subsequent API calls
    pub listing_id: String,
    /// Initial status (always "loading")
    pub status: ListingStatus,
}

/// Progress event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingProgressEvent {
    pub listing_id: String,
    pub loaded_count: usize,
}

/// Completion event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
    pub max_filename_width: Option<f32>,
}

/// Error event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingErrorEvent {
    pub listing_id: String,
    pub message: String,
}

/// Cancelled event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingCancelledEvent {
    pub listing_id: String,
}

/// Read-complete event payload (emitted when read_dir finishes, before sorting/caching)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingReadCompleteEvent {
    pub listing_id: String,
    pub total_count: usize,
}

/// Opening event payload (emitted just before read_dir starts - the slow part for network folders)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingOpeningEvent {
    pub listing_id: String,
}

/// State for an in-progress streaming listing
pub struct StreamingListingState {
    /// Cancellation flag - checked periodically during iteration
    pub cancelled: AtomicBool,
}

/// Cache for streaming state (separate from completed listings cache)
pub(crate) static STREAMING_STATE: LazyLock<RwLock<HashMap<String, Arc<StreamingListingState>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

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

        // Compare by the active sorting column
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
    log::debug!(
        "list_directory: path={}, entries={}, read_dir={}ms, metadata={}ms, owner={}ms, create={}ms, sort={}ms, total={}ms",
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
            log::warn!("Failed to start watcher: {}", e);
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

/// Gets the maximum filename width for a cached listing.
///
/// Recalculates the width based on current entries using font metrics.
/// This is useful after files are added/removed by the file watcher.
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `include_hidden` - Whether to include hidden files
///
/// # Returns
/// Maximum filename width in pixels, or None if font metrics are not available.
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
            .filter(|e| !e.name.starts_with('.'))
            .map(|e| e.name.as_str())
            .collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    };

    Ok(max_width)
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
    /// New indices of previously selected files after re-sorting.
    /// None if no selected_indices were provided.
    pub new_selected_indices: Option<Vec<usize>>,
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
/// * `selected_indices` - Optional indices of selected files to track through re-sort
/// * `all_selected` - If true, all files are selected (optimization to avoid passing huge arrays)
///
/// # Returns
/// A `ResortResult` with the new cursor index and new selected indices.
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
                listing.entries.iter().filter(|e| !e.name.starts_with('.')).collect()
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
                .filter(|e| !e.name.starts_with('.'))
                .position(|e| e.name == name)
        }
    });

    // Find new indices of selected files
    let new_selected_indices = if all_selected {
        // All files are still selected after re-sort
        let count = if include_hidden {
            listing.entries.len()
        } else {
            listing.entries.iter().filter(|e| !e.name.starts_with('.')).count()
        };
        Some((0..count).collect())
    } else {
        selected_filenames.map(|filenames| {
            let entries_for_lookup: Vec<_> = if include_hidden {
                listing.entries.iter().collect()
            } else {
                listing.entries.iter().filter(|e| !e.name.starts_with('.')).collect()
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
pub(super) fn get_listing_entries(listing_id: &str) -> Option<(PathBuf, Vec<FileEntry>)> {
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
    log::debug!(
        "list_directory_core: path={}, entries={}, read_dir={}ms, metadata={}ms, owner={}ms, total={}ms",
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

// ============================================================================
// Streaming directory listing implementation
// ============================================================================

/// Starts a streaming directory listing that returns immediately and emits progress events.
///
/// This is non-blocking - the actual directory reading happens in a background task.
/// Progress is reported via Tauri events every 500ms.
///
/// # Arguments
/// * `app` - Tauri app handle for emitting events
/// * `volume_id` - The volume ID to use (e.g., "root", "dropbox")
/// * `path` - The directory path to list
/// * `include_hidden` - Whether to include hidden files in total count
/// * `sort_by` - Column to sort by
/// * `sort_order` - Ascending or descending
///
/// # Returns
/// A `StreamingListingStartResult` with listing ID and initial status.
pub async fn list_directory_start_streaming(
    app: tauri::AppHandle,
    volume_id: &str,
    path: &Path,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
    listing_id: String,
) -> Result<StreamingListingStartResult, std::io::Error> {
    // Reset benchmark epoch for this navigation
    benchmark::reset_epoch();
    benchmark::log_event_value("list_directory_start_streaming CALLED", path.display());

    // Create streaming state with cancellation flag
    let state = Arc::new(StreamingListingState {
        cancelled: AtomicBool::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = STREAMING_STATE.write() {
        cache.insert(listing_id.clone(), Arc::clone(&state));
    }

    // Clone values for the spawned task
    let listing_id_for_spawn = listing_id.clone();
    let path_owned = path.to_path_buf();
    let volume_id_owned = volume_id.to_string();
    let app_for_spawn = app.clone();

    // Spawn background task
    tokio::spawn(async move {
        // Clone again for use after spawn_blocking
        let listing_id_for_cleanup = listing_id_for_spawn.clone();
        let app_for_error = app_for_spawn.clone();

        // Run blocking I/O on dedicated thread pool
        let result = tokio::task::spawn_blocking(move || {
            read_directory_with_progress(
                &app_for_spawn,
                &listing_id_for_spawn,
                &state,
                &volume_id_owned,
                &path_owned,
                include_hidden,
                sort_by,
                sort_order,
            )
        })
        .await;

        // Clean up streaming state
        if let Ok(mut cache) = STREAMING_STATE.write() {
            cache.remove(&listing_id_for_cleanup);
        }

        // Handle task result
        if let Err(e) = result {
            // Task panicked or was cancelled
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "listing-error",
                ListingErrorEvent {
                    listing_id: listing_id_for_cleanup,
                    message: format!("Task failed: {}", e),
                },
            );
        }
        // Note: read_directory_with_progress handles its own event emission for success/error/cancel
    });

    benchmark::log_event("list_directory_start_streaming RETURNING");
    Ok(StreamingListingStartResult {
        listing_id,
        status: ListingStatus::Loading,
    })
}

/// Reads a directory with progress reporting.
///
/// This function runs on a blocking thread pool and emits progress events.
#[allow(
    clippy::too_many_arguments,
    reason = "Streaming operation requires many state parameters"
)]
fn read_directory_with_progress(
    app: &tauri::AppHandle,
    listing_id: &str,
    state: &Arc<StreamingListingState>,
    volume_id: &str,
    path: &PathBuf,
    include_hidden: bool,
    sort_by: SortColumn,
    sort_order: SortOrder,
) -> Result<(), std::io::Error> {
    use tauri::Emitter;

    let mut entries = Vec::new();
    let mut last_progress_time = std::time::Instant::now();
    const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    benchmark::log_event("read_directory_with_progress START");

    // Emit opening event - this is the slow part for network folders
    // (SMB connection establishment, directory handle creation)
    let _ = app.emit(
        "listing-opening",
        ListingOpeningEvent {
            listing_id: listing_id.to_string(),
        },
    );

    // Read directory entries one by one
    let read_start = std::time::Instant::now();
    for entry_result in fs::read_dir(path)? {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            benchmark::log_event("read_directory_with_progress CANCELLED");
            let _ = app.emit(
                "listing-cancelled",
                ListingCancelledEvent {
                    listing_id: listing_id.to_string(),
                },
            );
            return Ok(());
        }

        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue, // Skip unreadable entries
        };

        // Process entry (same logic as list_directory_core)
        if let Some(file_entry) = process_dir_entry(&entry) {
            entries.push(file_entry);
        }

        // Emit progress every 500ms
        if last_progress_time.elapsed() >= PROGRESS_INTERVAL {
            let _ = app.emit(
                "listing-progress",
                ListingProgressEvent {
                    listing_id: listing_id.to_string(),
                    loaded_count: entries.len(),
                },
            );
            last_progress_time = std::time::Instant::now();
        }
    }
    let read_dir_time = read_start.elapsed();
    benchmark::log_event_value("read_dir COMPLETE, entries", entries.len());

    // Emit read-complete event (before sorting/caching) so UI can show "All N files loaded"
    let _ = app.emit(
        "listing-read-complete",
        ListingReadCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count: entries.len(),
        },
    );

    // Check cancellation one more time before finalizing
    if state.cancelled.load(Ordering::Relaxed) {
        benchmark::log_event("read_directory_with_progress CANCELLED (after read)");
        let _ = app.emit(
            "listing-cancelled",
            ListingCancelledEvent {
                listing_id: listing_id.to_string(),
            },
        );
        return Ok(());
    }

    // Sort entries
    benchmark::log_event("sort START");
    sort_entries(&mut entries, sort_by, sort_order);
    benchmark::log_event("sort END");

    // Calculate counts based on include_hidden setting
    let total_count = if include_hidden {
        entries.len()
    } else {
        entries.iter().filter(|e| !e.name.starts_with('.')).count()
    };

    // Calculate max filename width if font metrics are available
    let max_filename_width = {
        let font_id = "system-400-12"; // Default font (must match list_directory_start_with_volume)
        let filenames: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        crate::font_metrics::calculate_max_width(&filenames, font_id)
    };

    // Cache the completed listing
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.insert(
            listing_id.to_string(),
            CachedListing {
                volume_id: volume_id.to_string(),
                path: path.clone(),
                entries,
                sort_by,
                sort_order,
            },
        );
    }

    // Get the volume from VolumeManager to check if it supports watching
    if let Some(volume) = super::get_volume_manager().get(volume_id)
        && volume.supports_watching()
        && let Err(e) = start_watching(listing_id, path)
    {
        log::warn!("Failed to start watcher: {}", e);
        // Continue anyway - watcher is optional enhancement
    }

    // Emit completion event
    let _ = app.emit(
        "listing-complete",
        ListingCompleteEvent {
            listing_id: listing_id.to_string(),
            total_count,
            max_filename_width,
        },
    );

    benchmark::log_event_value(
        "read_directory_with_progress COMPLETE, read_dir_time_ms",
        read_dir_time.as_millis(),
    );
    Ok(())
}

/// Process a single directory entry into a FileEntry.
/// Returns None if the entry cannot be processed (permissions, etc).
pub(crate) fn process_dir_entry(entry: &fs::DirEntry) -> Option<FileEntry> {
    let file_type = entry.file_type().ok()?;
    let is_symlink = file_type.is_symlink();

    // For symlinks, check if the TARGET is a directory
    let target_is_dir = if is_symlink {
        fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        false
    };

    // For symlinks, get metadata of the link itself (not target)
    let metadata = if is_symlink {
        fs::symlink_metadata(entry.path()).ok()?
    } else {
        entry.metadata().ok()?
    };

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
    let owner = get_owner_name(uid);
    let group = get_group_name(gid);

    Some(FileEntry {
        name: name.clone(),
        path: entry.path().to_string_lossy().to_string(),
        is_directory: is_dir,
        is_symlink,
        size: if metadata.is_file() { Some(metadata.len()) } else { None },
        modified_at: modified,
        created_at: created,
        added_at: None,  // Will be loaded later if needed
        opened_at: None, // Will be loaded later if needed
        permissions: metadata.permissions().mode(),
        owner,
        group,
        icon_id: get_icon_id(is_dir, is_symlink, &name),
        extended_metadata_loaded: false, // Not loaded yet
    })
}

/// Cancels an in-progress streaming listing.
///
/// Sets the cancellation flag, which will be checked by the background task.
pub fn cancel_listing(listing_id: &str) {
    if let Ok(cache) = STREAMING_STATE.read()
        && let Some(state) = cache.get(listing_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
        benchmark::log_event_value("cancel_listing", listing_id);
    }
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
///
/// # Arguments
/// * `listing_id` - The listing ID from `list_directory_start`
/// * `include_hidden` - Whether to include hidden files in calculations
/// * `selected_indices` - Optional indices of selected files to calculate selection stats
///
/// # Returns
/// Statistics about the listing (totals and optionally selection stats).
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
        listing.entries.iter().filter(|e| !e.name.starts_with('.')).collect()
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
