//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{
    CopyScanResult, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream,
};
use crate::file_system::listing::FileEntry;
use crate::mtp::connection::{MtpConnectionError, connection_manager};
use log::debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

/// A volume backed by an MTP device storage.
///
/// This implementation wraps the MTP connection manager to provide file system
/// abstraction. All methods are natively async — MTP operations go through the
/// connection manager which uses async USB bulk transfers.
pub struct MtpVolume {
    /// Display name (typically the storage description like "Internal storage")
    name: String,
    /// MTP device ID (for example, "mtp-20-5")
    device_id: String,
    /// Storage ID within the device
    storage_id: u32,
    /// Virtual root path for this volume (for example, "/mtp-20-5/65537")
    root: PathBuf,
    /// Volume ID for listing cache lookups (format: "{device_id}:{storage_id}").
    volume_id: String,
}

impl MtpVolume {
    /// Creates a new MTP volume for a specific device storage.
    ///
    /// # Arguments
    /// * `device_id` - The MTP device ID (format: "mtp-{bus}-{address}")
    /// * `storage_id` - The storage ID within the device
    /// * `name` - Display name for the storage (for example, "Internal shared storage")
    pub fn new(device_id: &str, storage_id: u32, name: &str) -> Self {
        let volume_id = format!("{}:{}", device_id, storage_id);
        Self {
            name: name.to_string(),
            device_id: device_id.to_string(),
            storage_id,
            root: PathBuf::from(format!("mtp://{}/{}", device_id, storage_id)),
            volume_id,
        }
    }

    /// Converts a Volume path to an MTP inner path.
    ///
    /// The path can be in several formats:
    /// - MTP URL: `mtp://mtp-0-1/65537` or `mtp://mtp-0-1/65537/DCIM/Camera`
    /// - Absolute path: `/DCIM/Camera`
    /// - Relative path: `DCIM/Camera`
    ///
    /// The MTP API expects paths relative to the storage root (for example, `DCIM/Camera`).
    fn to_mtp_path(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy();

        // Handle MTP URLs (mtp://device-id/storage-id/optional/path)
        if path_str.starts_with("mtp://") {
            // Parse: mtp://mtp-0-1/65537/DCIM/Camera -> DCIM/Camera
            // The format is: mtp://{device_id}/{storage_id}/{path}
            let without_scheme = path_str.strip_prefix("mtp://").unwrap_or(&path_str);

            // Find the device_id/storage_id prefix and skip it
            // Device ID format: mtp-{bus}-{address} (like mtp-0-1)
            // So we need to skip: device_id/storage_id/
            let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
            // parts[0] = device_id (like "mtp-0-1")
            // parts[1] = storage_id (like "65537")
            // parts[2] = inner path (like "DCIM/Camera") or absent for root

            return if parts.len() >= 3 {
                parts[2].to_string()
            } else {
                String::new() // Root of storage
            };
        }

        // Handle empty or root paths
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return String::new();
        }

        // Strip leading slash if present
        path_str.strip_prefix('/').unwrap_or(&path_str).to_string()
    }
}

impl Volume for MtpVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            debug!(
                "MtpVolume::list_directory: device={}, storage={}, input_path={}, mtp_path={}",
                self.device_id,
                self.storage_id,
                path.display(),
                mtp_path
            );

            let start = std::time::Instant::now();
            let result = if let Some(on_progress) = on_progress {
                connection_manager()
                    .list_directory_with_progress(&self.device_id, self.storage_id, &mtp_path, on_progress)
                    .await
            } else {
                connection_manager()
                    .list_directory(&self.device_id, self.storage_id, &mtp_path)
                    .await
            };

            match &result {
                Ok(entries) => debug!(
                    "MtpVolume::list_directory: completed in {:?}, {} entries",
                    start.elapsed(),
                    entries.len()
                ),
                Err(e) => debug!(
                    "MtpVolume::list_directory: failed in {:?}, error={:?}",
                    start.elapsed(),
                    e
                ),
            }

            result.map_err(map_mtp_error)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // MTP has no single-file stat — list the parent directory and find the entry.
            let path_str = path.to_string_lossy();
            if path_str.is_empty() || path_str == "/" || path_str == "." {
                // Root: synthesize a directory entry
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.root.display().to_string(),
                    true,
                    false,
                ));
            }

            let Some(parent) = path.parent() else {
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.root.display().to_string(),
                    true,
                    false,
                ));
            };

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(VolumeError::NotFound(path.display().to_string()));
            };

            let entries = self.list_directory(parent, None).await?;
            entries
                .into_iter()
                .find(|e| e.name == name)
                .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.is_ok() })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.map(|e| e.is_directory) })
    }

    fn supports_watching(&self) -> bool {
        // Return false because MTP has its OWN file watching mechanism that is
        // independent of the listing pipeline. The MtpConnectionManager starts an
        // event loop when a device connects (see start_event_loop) that polls for
        // USB interrupt endpoint events (ObjectAdded/ObjectRemoved/ObjectInfoChanged).
        // These events emit `mtp-directory-changed` directly to the frontend.
        //
        // The `supports_watching()` check in operations.rs is used to decide whether
        // to start the local notify-based file watcher, which only works for POSIX
        // paths. MTP paths like "/DCIM/Camera" don't exist on the local filesystem,
        // so we must return false to prevent the notify watcher from failing.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        false
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

            // MTP's get_metadata lists the parent dir to find the entry, which is expensive
            // but correct. The MTP event loop (connection/event_loop.rs) also handles
            // change notifications, so this is belt-and-suspenders for self-mutations.
            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_path.join(name);
                    match self.get_metadata(&entry_path).await {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(&self.volume_id, parent_path, change);
                        }
                        Err(e) => {
                            debug!(
                                "MtpVolume::notify_mutation: couldn't stat {}: {}",
                                entry_path.display(),
                                e
                            );
                        }
                    }
                }
                MutationEvent::Deleted(name) => {
                    notify_directory_changed(&self.volume_id, parent_path, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_path.join(&to);
                    match self.get_metadata(&new_path).await {
                        Ok(entry) => {
                            notify_directory_changed(
                                &self.volume_id,
                                parent_path,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            debug!(
                                "MtpVolume::notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let Some(parent) = path.parent() else {
                return Err(VolumeError::IoError {
                    message: "Cannot create root directory".into(),
                    raw_os_error: None,
                });
            };
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(VolumeError::IoError {
                    message: "Invalid directory name".into(),
                    raw_os_error: None,
                });
            };

            let parent_mtp_path = self.to_mtp_path(parent);
            let folder_name = name.to_string();

            connection_manager()
                .create_folder(&self.device_id, self.storage_id, &parent_mtp_path, &folder_name)
                .await
                .map(|_| ())
                .map_err(map_mtp_error)?;

            self.notify_mutation(&self.volume_id, parent, MutationEvent::Created(name.to_string()))
                .await;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            connection_manager()
                .delete_object(&self.device_id, self.storage_id, &mtp_path)
                .await
                .map_err(map_mtp_error)?;

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                self.notify_mutation(
                    &self.volume_id,
                    parent,
                    MutationEvent::Deleted(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // MTP doesn't support atomic overwrite, so check for conflicts when not forced.
            if !force && self.exists(to).await {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }

            let from_mtp = self.to_mtp_path(from);
            let to_mtp = self.to_mtp_path(to);

            let from_parent = Path::new(&from_mtp).parent().unwrap_or(Path::new(""));
            let to_parent = Path::new(&to_mtp).parent().unwrap_or(Path::new(""));
            let same_parent = from_parent == to_parent;

            let from_name = Path::new(&from_mtp)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| VolumeError::IoError {
                    message: "Invalid source path".into(),
                    raw_os_error: None,
                })?;
            let to_name =
                Path::new(&to_mtp)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| VolumeError::IoError {
                        message: "Invalid destination path".into(),
                        raw_os_error: None,
                    })?;
            let same_name = from_name == to_name;

            if same_parent {
                // Same directory — just rename
                let new_name = to_name.to_string();
                connection_manager()
                    .rename_object(&self.device_id, self.storage_id, &from_mtp, &new_name)
                    .await
                    .map(|_| ())
                    .map_err(map_mtp_error)?;

                // Notify listing cache about same-directory rename
                if let Some(from_parent_path) = from.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        from_parent_path,
                        MutationEvent::Renamed {
                            from: from_name.to_string(),
                            to: to_name.to_string(),
                        },
                    )
                    .await;
                }
            } else {
                // Different directory — use MTP MoveObject
                let to_parent_str = to_parent.to_string_lossy().to_string();
                connection_manager()
                    .move_object(&self.device_id, self.storage_id, &from_mtp, &to_parent_str)
                    .await
                    .map(|_| ())
                    .map_err(map_mtp_error)?;

                // If the name also changed, rename after moving
                if !same_name {
                    let moved_path = format!(
                        "{}{}{}",
                        to_parent_str,
                        if to_parent_str.is_empty() || to_parent_str.ends_with('/') {
                            ""
                        } else {
                            "/"
                        },
                        from_name
                    );
                    let new_name = to_name.to_string();
                    connection_manager()
                        .rename_object(&self.device_id, self.storage_id, &moved_path, &new_name)
                        .await
                        .map(|_| ())
                        .map_err(map_mtp_error)?;
                }

                // Cross-directory move: remove from source dir, add in dest dir
                if let Some(from_parent_path) = from.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        from_parent_path,
                        MutationEvent::Deleted(from_name.to_string()),
                    )
                    .await;
                }
                if let Some(to_parent_path) = to.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        to_parent_path,
                        MutationEvent::Created(to_name.to_string()),
                    )
                    .await;
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
            let mtp_path = self.to_mtp_path(path);

            debug!(
                "MtpVolume::scan_for_copy: device={}, storage={}, path={}",
                self.device_id, self.storage_id, mtp_path
            );

            connection_manager()
                .scan_for_copy(&self.device_id, self.storage_id, &mtp_path)
                .await
                .map_err(map_mtp_error)
        })
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if paths.is_empty() {
                return Ok(CopyScanResult {
                    file_count: 0,
                    dir_count: 0,
                    total_bytes: 0,
                    top_level_is_directory: false,
                });
            }

            // Group paths by parent directory so we list each parent at most once
            let mut by_parent: std::collections::HashMap<PathBuf, Vec<&PathBuf>> = std::collections::HashMap::new();
            for path in paths {
                let mtp_path = self.to_mtp_path(path);
                let mtp_path_buf = PathBuf::from(&mtp_path);
                let parent = mtp_path_buf.parent().unwrap_or(Path::new("")).to_path_buf();
                by_parent.entry(parent).or_default().push(path);
            }

            debug!(
                "MtpVolume::scan_for_copy_batch: {} paths across {} unique parent dirs",
                paths.len(),
                by_parent.len()
            );

            let mut result = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                // Aggregate over multiple paths — not meaningful for a batch.
                top_level_is_directory: false,
            };

            for (parent, children) in &by_parent {
                // List the parent directory once (goes through the listing cache)
                let parent_str = parent.to_string_lossy();
                let entries = self.list_directory(Path::new(parent_str.as_ref()), None).await?;

                for child_path in children {
                    let mtp_path = self.to_mtp_path(child_path);
                    let name = Path::new(&mtp_path).file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if let Some(entry) = entries.iter().find(|e| e.name == name) {
                        if entry.is_directory {
                            let scan = self.scan_for_copy(child_path).await?;
                            result.file_count += scan.file_count;
                            result.dir_count += scan.dir_count;
                            result.total_bytes += scan.total_bytes;
                        } else {
                            result.file_count += 1;
                            result.total_bytes += entry.size.unwrap_or(0);
                        }
                    }
                }
            }

            Ok(result)
        })
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // List destination directory to check for conflicts
            let entries = self.list_directory(dest_path, None).await?;
            let mut conflicts = Vec::new();

            for item in source_items {
                // Check if a file with the same name exists at destination
                if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
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

    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(5))
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let info = connection_manager()
                .get_device_info(&self.device_id)
                .await
                .ok_or_else(|| {
                    map_mtp_error(MtpConnectionError::NotConnected {
                        device_id: self.device_id.clone(),
                    })
                })?;

            // Find this storage in the device info
            let storage = info.storages.iter().find(|s| s.id == self.storage_id).ok_or_else(|| {
                map_mtp_error(MtpConnectionError::Other {
                    device_id: self.device_id.clone(),
                    message: format!("Storage {} not found", self.storage_id),
                })
            })?;

            Ok(SpaceInfo {
                total_bytes: storage.total_bytes,
                available_bytes: storage.available_bytes,
                used_bytes: storage.total_bytes.saturating_sub(storage.available_bytes),
            })
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // MTP is a single USB bulk transport — parallel ops would just
        // serialize on the wire with extra overhead.
        1
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            let (download, total_size) = connection_manager()
                .open_download_stream(&self.device_id, self.storage_id, &mtp_path)
                .await
                .map_err(map_mtp_error)?;

            Ok(Box::new(MtpReadStream {
                download: Some(download),
                total_size,
                bytes_read: 0,
            }) as Box<dyn VolumeReadStream>)
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        _on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let dest_folder = dest.parent().map(|p| self.to_mtp_path(p)).unwrap_or_default();
            let filename = dest
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| VolumeError::IoError {
                    message: "Invalid filename".into(),
                    raw_os_error: None,
                })?
                .to_string();

            // Stream chunks directly with .await — no need to pre-collect since
            // we're fully async now (no nested block_on risk).
            let mut chunks: Vec<bytes::Bytes> = Vec::new();
            while let Some(result) = stream.next_chunk().await {
                let data = result?;
                chunks.push(bytes::Bytes::from(data));
            }

            connection_manager()
                .upload_from_chunks(&self.device_id, self.storage_id, &dest_folder, &filename, size, chunks)
                .await
                .map_err(map_mtp_error)
        })
    }
}

/// Direct async streaming reader for MTP files.
///
/// Calls `FileDownload::next_chunk().await` directly — possible because
/// `VolumeReadStream::next_chunk()` is async. No background task or channel
/// needed.
struct MtpReadStream {
    download: Option<mtp_rs::FileDownload>,
    total_size: u64,
    bytes_read: u64,
}

impl Drop for MtpReadStream {
    fn drop(&mut self) {
        if let Some(mut download) = self.download.take() {
            // Not fully consumed — cancel the USB transfer to prevent
            // ReceiveStream's Drop from panicking (and corrupting the session).
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let timeout = mtp_rs::DEFAULT_CANCEL_TIMEOUT;
                    if let Err(e) = download.cancel(timeout).await {
                        log::warn!("MTP download cancel on drop: {:?}", e);
                    }
                });
            }
        }
    }
}

impl VolumeReadStream for MtpReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let download = self.download.as_mut()?;
            match download.next_chunk().await {
                Some(Ok(bytes)) => {
                    self.bytes_read += bytes.len() as u64;
                    Some(Ok(bytes.to_vec()))
                }
                Some(Err(e)) => Some(Err(VolumeError::IoError {
                    message: e.to_string(),
                    raw_os_error: None,
                })),
                None => None,
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

/// Maps MTP connection errors to Volume errors.
fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::NotFound(e.to_string())
        }
        MtpConnectionError::ObjectNotFound { path, .. } => VolumeError::NotFound(path),
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied(e.to_string())
        }
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(e.to_string()),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(e.to_string()),
        MtpConnectionError::Timeout { .. } => VolumeError::ConnectionTimeout(e.to_string()),
        MtpConnectionError::StorageFull { .. } => VolumeError::StorageFull { message: e.to_string() },
        MtpConnectionError::StoreReadOnly { .. } => VolumeError::ReadOnly(e.to_string()),
        _ => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_volume() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Internal storage");
        assert_eq!(vol.name(), "Internal storage");
        assert_eq!(vol.device_id, "mtp-20-5");
        assert_eq!(vol.storage_id, 65537);
    }

    #[test]
    fn test_root_path() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Internal storage");
        assert_eq!(vol.root().to_string_lossy(), "mtp://mtp-20-5/65537");
    }

    #[test]
    fn test_to_mtp_path_empty() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert_eq!(vol.to_mtp_path(Path::new("")), "");
        assert_eq!(vol.to_mtp_path(Path::new("/")), "");
        assert_eq!(vol.to_mtp_path(Path::new(".")), "");
    }

    #[test]
    fn test_to_mtp_path_relative() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert_eq!(vol.to_mtp_path(Path::new("DCIM")), "DCIM");
        assert_eq!(vol.to_mtp_path(Path::new("DCIM/Camera")), "DCIM/Camera");
    }

    #[test]
    fn test_to_mtp_path_absolute() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert_eq!(vol.to_mtp_path(Path::new("/DCIM")), "DCIM");
        assert_eq!(vol.to_mtp_path(Path::new("/DCIM/Camera")), "DCIM/Camera");
    }

    #[test]
    fn test_to_mtp_path_mtp_url_root() {
        let vol = MtpVolume::new("mtp-0-1", 65537, "Test");
        // MTP URL for storage root
        assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537")), "");
    }

    #[test]
    fn test_to_mtp_path_mtp_url_with_path() {
        let vol = MtpVolume::new("mtp-0-1", 65537, "Test");
        // MTP URL with nested path
        assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM")), "DCIM");
        assert_eq!(
            vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM/Camera")),
            "DCIM/Camera"
        );
    }

    #[test]
    fn test_supports_watching_returns_false() {
        // MTP volumes return false for supports_watching because they have their
        // own event loop (in MtpConnectionManager) that handles file watching
        // independently. The supports_watching check in operations.rs is only
        // for the local notify-based watcher, which doesn't work for MTP paths.
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert!(!vol.supports_watching());
    }

    #[test]
    fn test_supports_streaming_returns_true() {
        // MTP volumes support streaming for direct MTP-to-MTP transfers.
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert!(vol.supports_streaming());
    }
}
