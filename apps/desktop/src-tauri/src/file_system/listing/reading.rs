//! Low-level directory reading and FileEntry construction.
//!
//! Pure I/O functions that read from disk and build FileEntry objects.
//! No Tauri or caching dependencies — consumed by operations.rs and streaming.rs.

#![allow(
    dead_code,
    reason = "list_directory and get_extended_metadata_batch are part of the two-phase loading API"
)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use crate::benchmark;
use crate::file_system::listing::metadata::{ExtendedMetadata, FileEntry, get_group_name, get_icon_id, get_owner_name};
use crate::file_system::listing::sorting::{SortColumn, SortOrder, sort_entries};

/// Lists the contents of a directory with full metadata (including macOS extended metadata).
///
/// Calls `list_directory_core()` for the fast path, then enriches entries
/// with macOS-specific metadata (addedAt, openedAt).
pub fn list_directory(path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
    let overall_start = std::time::Instant::now();

    let mut entries = list_directory_core(path)?;

    // Enrich with macOS-specific metadata
    #[cfg(target_os = "macos")]
    {
        let macos_start = std::time::Instant::now();
        for entry in &mut entries {
            let macos_meta = crate::file_system::macos_metadata::get_macos_metadata(Path::new(&entry.path));
            entry.added_at = macos_meta.added_at;
            entry.opened_at = macos_meta.opened_at;
            entry.extended_metadata_loaded = true;
        }
        log::debug!(
            "list_directory: macOS metadata enrichment={}ms",
            macos_start.elapsed().as_millis()
        );
    }

    #[cfg(not(target_os = "macos"))]
    for entry in &mut entries {
        entry.extended_metadata_loaded = true;
    }

    log::debug!(
        "list_directory: path={}, entries={}, total={}ms",
        path.display(),
        entries.len(),
        overall_start.elapsed().as_millis()
    );

    Ok(entries)
}

/// Lists the contents of a directory with CORE metadata only.
///
/// Significantly faster than `list_directory()` because it skips
/// macOS-specific metadata (addedAt, openedAt) which require additional system calls.
///
/// Use `get_extended_metadata_batch()` to fetch extended metadata later.
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
    for entry in dir_entries {
        let entry = entry?;
        match process_dir_entry(&entry) {
            Some(file_entry) => entries.push(file_entry),
            None => {
                // Permission denied or broken symlink — return minimal entry
                let is_symlink = entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false);
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
                    recursive_size: None,
                    recursive_file_count: None,
                    recursive_dir_count: None,
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
        "list_directory_core: path={}, entries={}, read_dir={}ms, total={}ms",
        path.display(),
        entries.len(),
        read_dir_time.as_millis(),
        total_time.as_millis()
    );
    benchmark::log_event("list_directory_core END");

    Ok(entries)
}

/// Gets metadata for a single file or directory path.
///
/// Used when we need metadata for a single path rather than listing
/// a directory. Useful for symlink target resolution and volume implementations.
pub fn get_single_entry(path: &Path) -> Result<FileEntry, std::io::Error> {
    let symlink_meta = fs::symlink_metadata(path)?;
    let is_symlink = symlink_meta.file_type().is_symlink();

    let target_is_dir = if is_symlink {
        fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        false
    };

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
        recursive_size: None,
        recursive_file_count: None,
        recursive_dir_count: None,
    })
}

/// Process a single directory entry into a FileEntry.
/// Returns None if the entry cannot be processed (permissions, etc).
pub(crate) fn process_dir_entry(entry: &fs::DirEntry) -> Option<FileEntry> {
    let file_type = entry.file_type().ok()?;
    let is_symlink = file_type.is_symlink();

    let target_is_dir = if is_symlink {
        fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        false
    };

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
        added_at: None,
        opened_at: None,
        permissions: metadata.permissions().mode(),
        owner,
        group,
        icon_id: get_icon_id(is_dir, is_symlink, &name),
        extended_metadata_loaded: false,
        recursive_size: None,
        recursive_file_count: None,
        recursive_dir_count: None,
    })
}

/// Fetches extended metadata for a batch of file paths.
///
/// Called after the initial directory listing to populate
/// macOS-specific metadata (addedAt, openedAt) without blocking initial render.
#[cfg(target_os = "macos")]
pub fn get_extended_metadata_batch(paths: Vec<String>) -> Vec<ExtendedMetadata> {
    benchmark::log_event_value("get_extended_metadata_batch START, count", paths.len());
    let result: Vec<ExtendedMetadata> = paths
        .into_iter()
        .map(|path_str| {
            let path = Path::new(&path_str);
            let macos_meta = crate::file_system::macos_metadata::get_macos_metadata(path);
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
    paths
        .into_iter()
        .map(|path_str| ExtendedMetadata {
            path: path_str,
            added_at: None,
            opened_at: None,
        })
        .collect()
}
