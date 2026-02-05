//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{ConflictInfo, CopyScanResult, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream};
use crate::file_system::metadata::FileEntry;
use crate::mtp::connection::{MtpConnectionError, connection_manager};
use log::debug;
use mtp_rs::FileDownload;
use std::path::{Path, PathBuf};

/// A volume backed by an MTP device storage.
///
/// This implementation wraps the MTP connection manager to provide file system
/// abstraction. The Volume trait is synchronous, so async MTP calls are executed
/// using tokio's `block_on` from within the blocking thread pool context.
///
/// # Thread safety
///
/// MtpVolume methods are called from within `tokio::task::spawn_blocking` contexts,
/// which run on a separate OS thread pool. This makes it safe to use `block_on`
/// to execute async MTP operations.
pub struct MtpVolume {
    /// Display name (typically the storage description like "Internal storage")
    name: String,
    /// MTP device ID (for example, "mtp-20-5")
    device_id: String,
    /// Storage ID within the device
    storage_id: u32,
    /// Virtual root path for this volume (for example, "/mtp-20-5/65537")
    root: PathBuf,
}

impl MtpVolume {
    /// Creates a new MTP volume for a specific device storage.
    ///
    /// # Arguments
    /// * `device_id` - The MTP device ID (format: "mtp-{bus}-{address}")
    /// * `storage_id` - The storage ID within the device
    /// * `name` - Display name for the storage (for example, "Internal shared storage")
    pub fn new(device_id: &str, storage_id: u32, name: &str) -> Self {
        Self {
            name: name.to_string(),
            device_id: device_id.to_string(),
            storage_id,
            root: PathBuf::from(format!("mtp://{}/{}", device_id, storage_id)),
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
            // Device ID format: mtp-{bus}-{address} (e.g., mtp-0-1)
            // So we need to skip: device_id/storage_id/
            let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
            // parts[0] = device_id (e.g., "mtp-0-1")
            // parts[1] = storage_id (e.g., "65537")
            // parts[2] = inner path (e.g., "DCIM/Camera") or absent for root

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

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let mtp_path = self.to_mtp_path(path);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        debug!(
            "MtpVolume::list_directory: device={}, storage={}, input_path={}, mtp_path={}",
            device_id,
            storage_id,
            path.display(),
            mtp_path
        );

        // Get the tokio runtime handle - we're inside spawn_blocking,
        // so block_on is safe here
        let handle = tokio::runtime::Handle::current();

        let start = std::time::Instant::now();
        let result = handle.block_on(async move {
            connection_manager()
                .list_directory(&device_id, storage_id, &mtp_path)
                .await
        });

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
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        // MTP doesn't have a direct "get metadata" API - we need to list the parent
        // and find the entry. For now, return NotSupported.
        // The listing pipeline doesn't use get_metadata for directory browsing.
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    fn exists(&self, path: &Path) -> bool {
        // Check by trying to list the parent directory and finding the entry
        let Some(parent) = path.parent() else {
            // Root always exists
            return true;
        };

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };

        match self.list_directory(parent) {
            Ok(entries) => entries.iter().any(|e| e.name == name),
            Err(_) => false,
        }
    }

    fn is_directory(&self, path: &Path) -> Result<bool, VolumeError> {
        // Empty path or root is always a directory
        let path_str = path.to_string_lossy();
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return Ok(true);
        }

        // Check by listing the parent directory and finding the entry
        let Some(parent) = path.parent() else {
            // Root is a directory
            return Ok(true);
        };

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Err(VolumeError::NotFound(path.display().to_string()));
        };

        let entries = self.list_directory(parent)?;
        entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.is_directory)
            .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))
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

    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let Some(parent) = path.parent() else {
            return Err(VolumeError::IoError("Cannot create root directory".into()));
        };
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Err(VolumeError::IoError("Invalid directory name".into()));
        };

        let parent_path = self.to_mtp_path(parent);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;
        let folder_name = name.to_string();

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .create_folder(&device_id, storage_id, &parent_path, &folder_name)
                    .await
            })
            .map(|_| ())
            .map_err(map_mtp_error)
    }

    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let mtp_path = self.to_mtp_path(path);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .delete_object(&device_id, storage_id, &mtp_path)
                    .await
            })
            .map_err(map_mtp_error)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), VolumeError> {
        let mtp_path = self.to_mtp_path(from);
        // Extract the new name from the destination path
        let new_name = to
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| VolumeError::IoError("Invalid destination path".into()))?
            .to_string();

        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .rename_object(&device_id, storage_id, &mtp_path, &new_name)
                    .await
            })
            .map(|_| ())
            .map_err(map_mtp_error)
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy(&self, path: &Path) -> Result<CopyScanResult, VolumeError> {
        let mtp_path = self.to_mtp_path(path);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        debug!(
            "MtpVolume::scan_for_copy: device={}, storage={}, path={}",
            device_id, storage_id, mtp_path
        );

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .scan_for_copy(&device_id, storage_id, &mtp_path)
                    .await
            })
            .map_err(map_mtp_error)
    }

    fn export_to_local(&self, source: &Path, local_dest: &Path) -> Result<u64, VolumeError> {
        let mtp_path = self.to_mtp_path(source);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;
        let local_dest = local_dest.to_path_buf();

        debug!(
            "MtpVolume::export_to_local: device={}, storage={}, source={}, dest={}",
            device_id,
            storage_id,
            mtp_path,
            local_dest.display()
        );

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .download_recursive(&device_id, storage_id, &mtp_path, &local_dest)
                    .await
            })
            .map_err(map_mtp_error)
    }

    fn import_from_local(&self, local_source: &Path, dest: &Path) -> Result<u64, VolumeError> {
        // upload_recursive expects the destination FOLDER, not the full path.
        // It derives the filename from the source. So we need to extract the parent.
        let dest_folder = dest.parent().map(|p| self.to_mtp_path(p)).unwrap_or_default();

        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;
        let local_source = local_source.to_path_buf();

        debug!(
            "MtpVolume::import_from_local: device={}, storage={}, source={}, dest_folder={}",
            device_id,
            storage_id,
            local_source.display(),
            dest_folder
        );

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .upload_recursive(&device_id, storage_id, &local_source, &dest_folder)
                    .await
            })
            .map_err(map_mtp_error)
    }

    fn scan_for_conflicts(
        &self,
        source_items: &[SourceItemInfo],
        dest_path: &Path,
    ) -> Result<Vec<ConflictInfo>, VolumeError> {
        // List destination directory to check for conflicts
        let entries = self.list_directory(dest_path)?;
        let mut conflicts = Vec::new();

        for item in source_items {
            // Check if a file with the same name exists at destination
            if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
                // Convert modified_at (milliseconds u64) to i64 seconds
                let dest_modified = existing.modified_at.map(|ms| (ms / 1000) as i64);
                conflicts.push(ConflictInfo {
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
    }

    fn get_space_info(&self) -> Result<SpaceInfo, VolumeError> {
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                let info = connection_manager().get_device_info(&device_id).await.ok_or_else(|| {
                    MtpConnectionError::NotConnected {
                        device_id: device_id.clone(),
                    }
                })?;

                // Find this storage in the device info
                let storage =
                    info.storages
                        .iter()
                        .find(|s| s.id == storage_id)
                        .ok_or_else(|| MtpConnectionError::Other {
                            device_id: device_id.clone(),
                            message: format!("Storage {} not found", storage_id),
                        })?;

                Ok(SpaceInfo {
                    total_bytes: storage.total_bytes,
                    available_bytes: storage.available_bytes,
                    used_bytes: storage.total_bytes.saturating_sub(storage.available_bytes),
                })
            })
            .map_err(map_mtp_error)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn open_read_stream(&self, path: &Path) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
        let mtp_path = self.to_mtp_path(path);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        let handle = tokio::runtime::Handle::current();

        // Get the file download stream from connection manager
        let (download, total_size) = handle
            .block_on(async {
                connection_manager()
                    .open_download_stream(&device_id, storage_id, &mtp_path)
                    .await
            })
            .map_err(map_mtp_error)?;

        Ok(Box::new(MtpReadStream {
            handle,
            download: Some(download),
            total_size,
            bytes_read: 0,
        }))
    }

    fn write_from_stream(
        &self,
        dest: &Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
    ) -> Result<u64, VolumeError> {
        let dest_folder = dest.parent().map(|p| self.to_mtp_path(p)).unwrap_or_default();
        let filename = dest
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| VolumeError::IoError("Invalid filename".into()))?
            .to_string();

        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        // IMPORTANT: Collect all chunks BEFORE entering block_on to avoid nested runtime error.
        // MtpReadStream::next_chunk() uses block_on internally, so we can't call it from
        // within another block_on (which upload_from_stream would do).
        let mut chunks: Vec<bytes::Bytes> = Vec::new();
        while let Some(result) = stream.next_chunk() {
            let data = result?;
            chunks.push(bytes::Bytes::from(data));
        }

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async {
                connection_manager()
                    .upload_from_chunks(&device_id, storage_id, &dest_folder, &filename, size, chunks)
                    .await
            })
            .map_err(map_mtp_error)
    }
}

/// Streaming reader for MTP files.
///
/// Wraps the mtp-rs FileDownload to provide sync iteration.
pub struct MtpReadStream {
    /// Tokio runtime handle for blocking on async operations.
    handle: tokio::runtime::Handle,
    /// The underlying async download (wrapped in Option for take semantics).
    download: Option<FileDownload>,
    /// Total file size.
    total_size: u64,
    /// Bytes read so far.
    bytes_read: u64,
}

impl VolumeReadStream for MtpReadStream {
    fn next_chunk(&mut self) -> Option<Result<Vec<u8>, VolumeError>> {
        let download = self.download.as_mut()?;

        self.handle.block_on(async {
            match download.next_chunk().await {
                Some(Ok(bytes)) => {
                    self.bytes_read += bytes.len() as u64;
                    Some(Ok(bytes.to_vec()))
                }
                Some(Err(e)) => Some(Err(VolumeError::IoError(e.to_string()))),
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
        MtpConnectionError::ExclusiveAccess { .. } => VolumeError::PermissionDenied(e.to_string()),
        _ => VolumeError::IoError(e.to_string()),
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
