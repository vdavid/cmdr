//! In-memory file system volume for testing.
//!
//! Provides a fully in-memory file system that supports all Volume operations,
//! including create, delete, and list. Useful for unit and integration tests
//! without touching the real file system.

use super::{Volume, VolumeError};
use crate::file_system::FileEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Entry in the in-memory file system.
struct InMemoryEntry {
    metadata: FileEntry,
    #[allow(dead_code)] // Will be used for future read_file support
    content: Option<Vec<u8>>,
}

/// An in-memory volume for testing without touching the real file system.
///
/// This implementation stores all entries in a HashMap, allowing full control
/// over the file system state for testing. It supports:
/// - Listing directories
/// - Getting single entry metadata
/// - Creating files and directories
/// - Deleting entries
/// - Stress testing with large file counts
pub struct InMemoryVolume {
    name: String,
    root: PathBuf,
    entries: RwLock<HashMap<PathBuf, InMemoryEntry>>,
}

impl InMemoryVolume {
    /// Creates a new empty in-memory volume.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root: PathBuf::from("/"),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Creates an in-memory volume pre-populated with entries.
    pub fn with_entries(name: impl Into<String>, entries: Vec<FileEntry>) -> Self {
        let volume = Self::new(name);
        {
            let mut map = volume.entries.write().unwrap();
            for entry in entries {
                let path = PathBuf::from(&entry.path);
                map.insert(
                    path,
                    InMemoryEntry {
                        metadata: entry,
                        content: None,
                    },
                );
            }
        }
        volume
    }

    /// Creates an in-memory volume with N auto-generated files for stress testing.
    ///
    /// Generated entries:
    /// - Every 10th entry is a directory
    /// - Every 50th entry is a symlink
    /// - File sizes increase linearly
    pub fn with_file_count(name: impl Into<String>, count: usize) -> Self {
        let entries: Vec<FileEntry> = (0..count)
            .map(|i| {
                let is_dir = i % 10 == 0;
                let file_name = format!("file_{:06}.txt", i);
                FileEntry {
                    name: file_name.clone(),
                    path: format!("/{}", file_name),
                    is_directory: is_dir,
                    is_symlink: i % 50 == 0,
                    size: Some(1024 * (i as u64)),
                    modified_at: Some(1_640_000_000 + i as u64),
                    created_at: Some(1_639_000_000 + i as u64),
                    added_at: None,
                    opened_at: None,
                    permissions: 0o644,
                    owner: "testuser".to_string(),
                    group: "staff".to_string(),
                    icon_id: if is_dir {
                        "dir".to_string()
                    } else {
                        "ext:txt".to_string()
                    },
                    extended_metadata_loaded: true,
                }
            })
            .collect();
        Self::with_entries(name, entries)
    }

    /// Normalizes a path relative to the volume root.
    fn normalize(&self, path: &Path) -> PathBuf {
        if path.as_os_str().is_empty() || path == Path::new(".") {
            PathBuf::from("/")
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            PathBuf::from("/").join(path)
        }
    }

    /// Gets the parent path of a given path.
    fn parent_of(path: &Path) -> PathBuf {
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Gets current timestamp as seconds since Unix epoch.
    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

impl Volume for InMemoryVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let target_dir = self.normalize(path);

        // Find all entries whose parent matches this directory
        let mut result: Vec<FileEntry> = entries
            .iter()
            .filter(|(entry_path, _)| {
                let parent = Self::parent_of(entry_path);
                parent == target_dir
            })
            .map(|(_, entry)| entry.metadata.clone())
            .collect();

        // Sort: directories first, then alphabetically
        result.sort_by(|a, b| match (a.is_directory, b.is_directory) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        Ok(result)
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let entries = self
            .entries
            .read()
            .map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = self.normalize(path);

        entries
            .get(&normalized)
            .map(|e| e.metadata.clone())
            .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        let entries = match self.entries.read() {
            Ok(e) => e,
            Err(_) => return false,
        };

        let normalized = self.normalize(path);
        entries.contains_key(&normalized)
    }

    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = self.normalize(path);

        let name = normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let metadata = FileEntry {
            name: name.clone(),
            path: normalized.display().to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(content.len() as u64),
            modified_at: Some(Self::now_secs()),
            created_at: Some(Self::now_secs()),
            added_at: None,
            opened_at: None,
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "file".to_string(),
            extended_metadata_loaded: true,
        };

        entries.insert(
            normalized,
            InMemoryEntry {
                metadata,
                content: Some(content.to_vec()),
            },
        );

        Ok(())
    }

    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = self.normalize(path);

        let name = normalized
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let metadata = FileEntry {
            name,
            path: normalized.display().to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: Some(Self::now_secs()),
            created_at: Some(Self::now_secs()),
            added_at: None,
            opened_at: None,
            permissions: 0o755,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
        };

        entries.insert(
            normalized,
            InMemoryEntry {
                metadata,
                content: None,
            },
        );

        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let mut entries = self
            .entries
            .write()
            .map_err(|_| VolumeError::IoError("Lock poisoned".into()))?;

        let normalized = self.normalize(path);

        entries
            .remove(&normalized)
            .map(|_| ())
            .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
    }
}
