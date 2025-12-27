//! File system operations: read, list, copy, move, delete.

#![allow(dead_code)] // Boilerplate for future use

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::LazyLock;
use std::sync::RwLock;
use uzers::{get_group_by_gid, get_user_by_uid};

/// Cache for uid→username and gid→groupname resolution.
static OWNER_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
static GROUP_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

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
    pub permissions: u32,
    pub owner: String,
    pub group: String,
    pub icon_id: String,
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
    let mut entries = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
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

                entries.push(FileEntry {
                    name: name.clone(),
                    path: entry.path().to_string_lossy().to_string(),
                    is_directory: is_dir,
                    is_symlink,
                    size: if metadata.is_file() { Some(metadata.len()) } else { None },
                    modified_at: modified,
                    created_at: created,
                    permissions: metadata.permissions().mode(),
                    owner: get_owner_name(uid),
                    group: get_group_name(gid),
                    icon_id: get_icon_id(is_dir, is_symlink, &name),
                });
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
                    permissions: 0,
                    owner: String::new(),
                    group: String::new(),
                    icon_id: if is_symlink {
                        "symlink-broken".to_string()
                    } else {
                        "file".to_string()
                    },
                });
            }
        }
    }

    // Sort: directories first, then files, both alphabetically
    entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}
