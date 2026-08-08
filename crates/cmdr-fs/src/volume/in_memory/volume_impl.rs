//! The `Volume` impl for [`InMemoryVolume`].
//!
//! Split out of `in_memory.rs` so neither file carries the whole double: the
//! parent holds the store, the builders, and the fault knobs a test reaches for;
//! this holds the trait surface those knobs steer. Same module tree, so the
//! private fields stay private to `in_memory`.

use super::{InMemoryEntry, InMemoryReadStream, InMemoryVolume};
use crate::entry::FileEntry;
// Only the E2E error-injection paths take a lock here; everything else reads the
// store through the parent module's helpers.
#[cfg(feature = "playwright-e2e")]
use crate::ignore_poison::IgnorePoison;
use crate::volume::{
    CopyScanResult, LaneKey, ScanConflict, SmbConnectionState, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

impl Volume for InMemoryVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        self.smb_connection_state
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        match &self.lane_key {
            Some(key) => LaneKey::new(key.clone()),
            None => LaneKey::new(self.root.to_string_lossy().into_owned()),
        }
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(crate::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Check for injected error (E2E testing). Cleared after one use to enable retry testing.
            #[cfg(feature = "playwright-e2e")]
            {
                let mut injected = self.injected_error.lock_ignore_poison();
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
            if self.stat_fails_for(&normalized) {
                return Err(VolumeError::IoError {
                    message: format!("Stat unavailable for {}", normalized.display()),
                    raw_os_error: None,
                });
            }

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
            if self.stat_fails_for(&normalized) {
                return Err(VolumeError::IoError {
                    message: format!("Stat unavailable for {}", normalized.display()),
                    raw_os_error: None,
                });
            }

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

            if entries.contains_key(&normalized) {
                return Err(VolumeError::AlreadyExists(normalized.display().to_string()));
            }

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

            // Mirror `std::fs::create_dir`: error on an existing entry rather
            // than overwriting it. This is what lets the folder-merge walker use
            // `AlreadyExists` as the "this level pre-existed, merge into it"
            // signal (see `Volume::create_directory_errors_on_existing_dir`).
            if entries.contains_key(&normalized) {
                return Err(VolumeError::AlreadyExists(normalized.display().to_string()));
            }

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
            if self.delete_fails {
                return Err(VolumeError::IoError {
                    message: "injected delete failure".into(),
                    raw_os_error: None,
                });
            }

            let mut entries = self.entries.write().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;

            let normalized = self.normalize(path);

            // `Volume::delete`'s contract is "file or EMPTY directory only"
            // (LocalPosix uses `std::fs::remove_dir`, which fails `ENOTEMPTY`;
            // SMB returns STATUS_DIRECTORY_NOT_EMPTY). Honor it here, because
            // real data-safety logic LEANS on the refusal: the same-volume
            // rename-merge preserves a skipped child's source purely by letting
            // its parent's cleanup delete fail, and the volume move's source
            // sweep relies on the same shape. A test double that deleted a
            // non-empty directory would orphan the children in the map and let
            // every such regression pass silently.
            let is_dir = entries.get(&normalized).is_some_and(|e| e.metadata.is_directory);
            if is_dir && entries.keys().any(|k| k.parent() == Some(normalized.as_path())) {
                return Err(VolumeError::IoError {
                    message: "Directory not empty".into(),
                    raw_os_error: Some(66), // ENOTEMPTY on macOS
                });
            }

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
            let was_directory = entry.metadata.is_directory;

            entries.insert(to_normalized.clone(), entry);

            // Renaming a DIRECTORY carries its whole subtree along — that's the
            // single server-side call the same-volume move is built on ("a whole
            // subtree moves with one rename, never descended"). Re-key every
            // descendant. Moving only the directory node would silently orphan
            // the contents: they'd vanish from every listing while still sitting
            // in the map, so a test asserting by exact path would still find them
            // and a subtree walk would not — passing vacuously over exactly the
            // data-loss shape these tests exist to catch.
            if was_directory {
                let descendants: Vec<PathBuf> = entries
                    .keys()
                    .filter(|k| k.starts_with(&from_normalized) && *k != &from_normalized)
                    .cloned()
                    .collect();
                for old_key in descendants {
                    let Ok(suffix) = old_key.strip_prefix(&from_normalized) else {
                        continue;
                    };
                    let new_key = to_normalized.join(suffix);
                    if let Some(mut moved) = entries.remove(&old_key) {
                        moved.metadata.path = new_key.display().to_string();
                        entries.insert(new_key, moved);
                    }
                }
            }
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
                    // In-memory volume has no hardlinks: footprints are equal.
                    dedup_bytes: entry.metadata.size.unwrap_or(0),
                    top_level_is_directory: false,
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
                // In-memory volume has no hardlinks: footprints are equal.
                dedup_bytes: total_bytes,
                // We only reach this branch if the path isn't a known file
                // entry. In-memory roots and unknown paths both report `true`
                // to match how callers use the flag (fall through to the
                // directory-recursion path). Empty roots or unknown paths
                // behave like empty directories, which is the existing
                // contract on this backend.
                top_level_is_directory: true,
            })
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn operations_are_local(&self) -> bool {
        // A `HashMap` behind a lock: no transport, no round trip.
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

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.read_range_unsupported {
                return Err(VolumeError::NotSupported);
            }
            let normalized = self.normalize(path);
            self.record_read_range(offset, len);
            let entries = self.entries.read().map_err(|_| VolumeError::IoError {
                message: "Lock poisoned".into(),
                raw_os_error: None,
            })?;
            let entry = entries
                .get(&normalized)
                .ok_or_else(|| VolumeError::NotFound(normalized.display().to_string()))?;
            if entry.metadata.is_directory {
                return Err(VolumeError::IsADirectory(normalized.display().to_string()));
            }
            let content = entry.content.as_deref().unwrap_or_default();
            let start = (offset as usize).min(content.len());
            let end = start.saturating_add(len).min(content.len());
            Ok(content[start..end].to_vec())
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
        *self.injected_error.lock_ignore_poison() = Some(errno);
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.space_info.clone().ok_or(VolumeError::NotSupported) })
    }

    fn supports_local_fs_access(&self) -> bool {
        self.local_fs_access
    }

    fn create_directory_errors_on_existing_dir(&self) -> bool {
        !self.sibling_duplicates_allowed
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
                        source_is_directory: item.is_directory,
                        dest_is_directory: existing.is_directory,
                    });
                }
            }

            Ok(conflicts)
        })
    }
}
