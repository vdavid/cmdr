//! In-memory file system volume for testing.
//!
//! Provides a fully in-memory file system that supports all Volume operations,
//! including create, delete, and list. Useful for unit and integration tests
//! without touching the real file system.

use super::{CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream};
use crate::file_system::listing::FileEntry;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::RwLock;

/// Entry in the in-memory file system.
struct InMemoryEntry {
    metadata: FileEntry,
    #[allow(dead_code, reason = "Will be used for future read_file support")]
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
    /// Configurable space info for testing. None means get_space_info returns NotSupported.
    space_info: Option<SpaceInfo>,
    /// Raw errno to inject on the next `list_directory` call. Cleared after use.
    #[cfg(feature = "playwright-e2e")]
    injected_error: std::sync::Mutex<Option<i32>>,
}

impl InMemoryVolume {
    /// Creates a new empty in-memory volume.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root: PathBuf::from("/"),
            entries: RwLock::new(HashMap::new()),
            space_info: None,
            #[cfg(feature = "playwright-e2e")]
            injected_error: std::sync::Mutex::new(None),
        }
    }

    /// Sets configurable space info so get_space_info() works in tests.
    pub fn with_space_info(mut self, total_bytes: u64, available_bytes: u64) -> Self {
        self.space_info = Some(SpaceInfo {
            total_bytes,
            available_bytes,
            used_bytes: total_bytes.saturating_sub(available_bytes),
        });
        self
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
                    size: Some(1024 * (i as u64)),
                    modified_at: Some(1_640_000_000 + i as u64),
                    created_at: Some(1_639_000_000 + i as u64),
                    permissions: 0o644,
                    owner: "testuser".to_string(),
                    group: "staff".to_string(),
                    extended_metadata_loaded: true,
                    ..FileEntry::new(file_name.clone(), format!("/{}", file_name), is_dir, i % 50 == 0)
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

/// Chunk size for InMemoryReadStream (64 KB — small enough to test multi-chunk behavior
/// without needing large test data).
const IN_MEMORY_STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Streaming reader for in-memory files.
struct InMemoryReadStream {
    data: Vec<u8>,
    offset: usize,
}

impl VolumeReadStream for InMemoryReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.offset >= self.data.len() {
                return None;
            }
            let end = (self.offset + IN_MEMORY_STREAM_CHUNK_SIZE).min(self.data.len());
            let chunk = self.data[self.offset..end].to_vec();
            self.offset = end;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        self.data.len() as u64
    }

    fn bytes_read(&self) -> u64 {
        self.offset as u64
    }
}

impl Volume for InMemoryVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Check for injected error (E2E testing). Cleared after one use to enable retry testing.
            #[cfg(feature = "playwright-e2e")]
            {
                let mut injected = self.injected_error.lock().unwrap();
                if let Some(errno) = injected.take() {
                    return Err(VolumeError::IoError {
                        message: format!("Injected error for testing (os error {})", errno),
                        raw_os_error: Some(errno),
                    });
                }
            }

            let entries = self.entries.read().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

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
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.entries.read().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let normalized = self.normalize(path);

            entries
                .get(&normalized)
                .map(|e| e.metadata.clone())
                .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let entries = match self.entries.read() {
                Ok(e) => e,
                Err(_) => return false,
            };

            let normalized = self.normalize(path);
            entries.contains_key(&normalized)
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = self.entries.read().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let normalized = self.normalize(path);

            entries
                .get(&normalized)
                .map(|e| e.metadata.is_directory)
                .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
        })
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = self.entries.write().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let normalized = self.normalize(path);

            let name = normalized
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let metadata = FileEntry {
                size: Some(content.len() as u64),
                modified_at: Some(Self::now_secs()),
                created_at: Some(Self::now_secs()),
                permissions: 0o644,
                owner: "testuser".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new(name, normalized.display().to_string(), false, false)
            };

            entries.insert(
                normalized,
                InMemoryEntry {
                    metadata,
                    content: Some(content.to_vec()),
                },
            );

            Ok(())
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = self.entries.write().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let normalized = self.normalize(path);

            let name = normalized
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let metadata = FileEntry {
                modified_at: Some(Self::now_secs()),
                created_at: Some(Self::now_secs()),
                permissions: 0o755,
                owner: "testuser".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new(name, normalized.display().to_string(), true, false)
            };

            entries.insert(
                normalized,
                InMemoryEntry {
                    metadata,
                    content: None,
                },
            );

            Ok(())
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = self.entries.write().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let normalized = self.normalize(path);

            entries
                .remove(&normalized)
                .map(|_| ())
                .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = self.entries.write().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let from_normalized = self.normalize(from);
            let to_normalized = self.normalize(to);

            if !force && from_normalized != to_normalized && entries.contains_key(&to_normalized) {
                return Err(VolumeError::AlreadyExists(to_normalized.display().to_string()));
            }

            let mut entry = entries
                .remove(&from_normalized)
                .ok_or_else(|| VolumeError::NotFound(from_normalized.display().to_string()))?;

            // Update the metadata to reflect the new name and path
            let new_name = to_normalized
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            entry.metadata.name = new_name;
            entry.metadata.path = to_normalized.display().to_string();

            entries.insert(to_normalized, entry);
            Ok(())
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let normalized = self.normalize(path);
            let entries = self.entries.read().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            // Check if the path is a file
            if let Some(entry) = entries.get(&normalized)
                && !entry.metadata.is_directory
            {
                return Ok(CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: entry.metadata.size.unwrap_or(0),
                });
            }

            // Recursively scan all descendants
            let mut file_count = 0;
            let mut dir_count = 0;
            let mut total_bytes = 0u64;

            for (entry_path, entry) in entries.iter() {
                // Skip the root path itself, only count descendants
                if entry_path == &normalized {
                    continue;
                }
                if !entry_path.starts_with(&normalized) {
                    continue;
                }
                if entry.metadata.is_directory {
                    dir_count += 1;
                } else {
                    file_count += 1;
                    total_bytes += entry.metadata.size.unwrap_or(0);
                }
            }

            Ok(CopyScanResult {
                file_count,
                dir_count,
                total_bytes,
            })
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // No backend bottleneck; return high and let the copy engine's
        // upper bound (32) clamp to sanity.
        32
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let normalized = self.normalize(path);
            let entries = self.entries.read().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let entry = entries
                .get(&normalized)
                .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))?;

            if entry.metadata.is_directory {
                return Err(VolumeError::IoError {
                    message: "Cannot stream a directory".into(),
                    raw_os_error: None,
                });
            }

            let data = entry.content.clone().unwrap_or_default();
            Ok(Box::new(InMemoryReadStream { data, offset: 0 }) as Box<dyn VolumeReadStream>)
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        _size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let total_size = stream.total_size();
            let mut data = Vec::new();
            let mut bytes_written = 0u64;

            while let Some(result) = stream.next_chunk().await {
                let chunk = result?;
                bytes_written += chunk.len() as u64;
                data.extend_from_slice(&chunk);

                if on_progress(bytes_written, total_size) == std::ops::ControlFlow::Break(()) {
                    return Err(VolumeError::IoError {
                        message: "Operation cancelled".into(),
                        raw_os_error: None,
                    });
                }
            }

            self.create_file(dest, &data).await?;
            Ok(bytes_written)
        })
    }

    #[cfg(feature = "playwright-e2e")]
    fn inject_error(&self, errno: i32) {
        *self.injected_error.lock().unwrap() = Some(errno);
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.space_info.clone().ok_or(VolumeError::NotSupported) })
    }

    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        None
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let dest_entries = self.list_directory(dest_path, None).await?;
            let mut conflicts = Vec::new();

            for item in source_items {
                if let Some(existing) = dest_entries.iter().find(|e| e.name == item.name) {
                    let dest_modified = existing.modified_at.map(|s| s as i64);
                    conflicts.push(ScanConflict {
                        source_path: item.name.clone(),
                        dest_path: existing.path.clone(),
                        source_size: item.size,
                        dest_size: existing.size.unwrap_or(0),
                        source_modified: item.modified,
                        dest_modified,
                    });
                }
            }

            Ok(conflicts)
        })
    }
}
