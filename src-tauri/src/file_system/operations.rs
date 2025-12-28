//! File system operations: read, list, copy, move, delete.

#![allow(dead_code)] // Boilerplate for future use

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::LazyLock;
use std::sync::RwLock;
use std::time::Instant;
use uuid::Uuid;
use uzers::{get_group_by_gid, get_user_by_uid};

/// Cache for uid→username and gid→groupname resolution.
static OWNER_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
static GROUP_CACHE: LazyLock<RwLock<HashMap<u32, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cache for directory listing sessions (cursor-based pagination).
/// Key: session_id, Value: cached directory with cursor position.
static SESSION_CACHE: LazyLock<RwLock<HashMap<String, CachedDirectory>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cached directory entries for cursor-based pagination.
struct CachedDirectory {
    entries: Vec<FileEntry>,
    cursor: usize,
    created_at: Instant,
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
                });
            }
        }
    }

    let sort_start = std::time::Instant::now();
    // Sort: directories first, then files, both alphabetically
    entries.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
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
// Cursor-based pagination API (session-based, reads directory only once)
// ============================================================================

/// Result of starting a new directory listing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartResult {
    /// Unique session ID for subsequent next/end calls
    pub session_id: String,
    /// Total number of entries in the directory
    pub total_count: usize,
    /// First chunk of entries
    pub entries: Vec<FileEntry>,
    /// Whether there are more entries to fetch
    pub has_more: bool,
}

/// Result of fetching the next chunk in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkNextResult {
    /// Chunk of entries
    pub entries: Vec<FileEntry>,
    /// Whether there are more entries to fetch
    pub has_more: bool,
}

/// Starts a new paginated directory listing session.
///
/// Reads the directory once, caches it, and returns the first chunk.
/// Use `list_directory_next` to get subsequent chunks.
/// Call `list_directory_end` to clean up when done.
///
/// # Arguments
/// * `path` - The directory path to list
/// * `chunk_size` - Number of entries to return in the first chunk
///
/// # Returns
/// A `SessionStartResult` with session ID, total count, and first chunk.
pub fn list_directory_start(path: &Path, chunk_size: usize) -> Result<SessionStartResult, std::io::Error> {
    // Read and sort the directory once
    let all_entries = list_directory(path)?;
    let total_count = all_entries.len();

    // Generate session ID
    let session_id = Uuid::new_v4().to_string();

    // Extract first chunk
    let first_chunk: Vec<FileEntry> = all_entries.iter().take(chunk_size).cloned().collect();
    let has_more = total_count > chunk_size;

    // Cache the entries with cursor position
    if let Ok(mut cache) = SESSION_CACHE.write() {
        cache.insert(
            session_id.clone(),
            CachedDirectory {
                entries: all_entries,
                cursor: chunk_size.min(total_count),
                created_at: Instant::now(),
            },
        );

        // Clean up old sessions (older than 60 seconds)
        cache.retain(|_, v| v.created_at.elapsed().as_secs() < 60);
    }

    Ok(SessionStartResult {
        session_id,
        total_count,
        entries: first_chunk,
        has_more,
    })
}

/// Gets the next chunk of entries from a cached session.
///
/// # Arguments
/// * `session_id` - The session ID from `list_directory_start`
/// * `chunk_size` - Number of entries to return
///
/// # Returns
/// A `ChunkNextResult` with entries and has_more flag.
pub fn list_directory_next(session_id: &str, chunk_size: usize) -> Result<ChunkNextResult, String> {
    let mut cache = SESSION_CACHE.write().map_err(|_| "Failed to acquire cache lock")?;

    let session = cache
        .get_mut(session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    let start = session.cursor;
    let end = (start + chunk_size).min(session.entries.len());

    let entries: Vec<FileEntry> = session.entries[start..end].to_vec();
    session.cursor = end;

    let has_more = end < session.entries.len();

    Ok(ChunkNextResult { entries, has_more })
}

/// Ends a directory listing session and cleans up the cache.
///
/// # Arguments
/// * `session_id` - The session ID to clean up
pub fn list_directory_end(session_id: &str) {
    if let Ok(mut cache) = SESSION_CACHE.write() {
        cache.remove(session_id);
    }
}
