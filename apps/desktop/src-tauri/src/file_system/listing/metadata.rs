//! File entry metadata types and helper functions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, RwLock};
use uzers::{get_group_by_gid, get_user_by_uid};

/// Cache for uid→username resolution.
pub(crate) static OWNER_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
/// Cache for gid→groupname resolution.
pub(crate) static GROUP_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Resolves a uid to a username, with caching.
pub(crate) fn get_owner_name(uid: u32) -> String {
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
pub(crate) fn get_group_name(gid: u32) -> String {
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
pub(crate) fn get_icon_id(is_dir: bool, is_symlink: bool, name: &str) -> String {
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
    /// Physical size on disk in bytes (st_blocks * 512 on Unix, same as size on other platforms)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_size: Option<u64>,
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
    /// Recursive size in bytes (from drive index, None if not indexed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive_size: Option<u64>,
    /// Recursive physical size on disk in bytes (from drive index, None if not indexed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive_physical_size: Option<u64>,
    /// Recursive file count (from drive index, None if not indexed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive_file_count: Option<u64>,
    /// Recursive dir count (from drive index, None if not indexed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recursive_dir_count: Option<u64>,
}

impl FileEntry {
    /// Creates a `FileEntry` with the four essential fields set and everything else defaulted.
    pub(crate) fn new(name: String, path: String, is_dir: bool, is_symlink: bool) -> Self {
        Self {
            icon_id: get_icon_id(is_dir, is_symlink, &name),
            name,
            path,
            is_directory: is_dir,
            is_symlink,
            size: None,
            physical_size: None,
            modified_at: None,
            created_at: None,
            added_at: None,
            opened_at: None,
            permissions: 0,
            owner: String::new(),
            group: String::new(),
            extended_metadata_loaded: false,
            recursive_size: None,
            recursive_physical_size: None,
            recursive_file_count: None,
            recursive_dir_count: None,
        }
    }
}

/// Default value for extended_metadata_loaded (for backwards compatibility)
fn default_extended_loaded() -> bool {
    true
}

/// Extended metadata for a single file (macOS-specific fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedMetadata {
    /// Key for merging with FileEntry.
    pub path: String,
    /// macOS only.
    pub added_at: Option<u64>,
    /// macOS only.
    pub opened_at: Option<u64>,
}
