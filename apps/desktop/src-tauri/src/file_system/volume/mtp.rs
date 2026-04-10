//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream};
use crate::file_system::listing::FileEntry;
use crate::mtp::connection::{MtpConnectionError, connection_manager};
use log::debug;
use mtp_rs::FileDownload;
use std::path::{Path, PathBuf};

/// Wrapper to assert Send + Sync on a progress callback reference.
///
/// SAFETY: MtpVolume methods are called from `spawn_blocking` contexts (single OS thread).
/// The callback never crosses thread boundaries — `block_on` runs the async download on the
/// same thread. The Volume trait's `Fn` callbacks use atomics for interior mutation, so they
/// are effectively Send + Sync even though the trait doesn't declare it.
struct SendSyncProgress<'a>(&'a dyn Fn(u64, u64) -> std::ops::ControlFlow<()>);

// SAFETY: See above — callback is only accessed from the single spawn_blocking thread.
unsafe impl Send for SendSyncProgress<'_> {}
unsafe impl Sync for SendSyncProgress<'_> {}

impl SendSyncProgress<'_> {
    fn call(&self, bytes_done: u64, bytes_total: u64) -> std::ops::ControlFlow<()> {
        (self.0)(bytes_done, bytes_total)
    }
}

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

    fn list_directory_with_progress(
        &self,
        path: &Path,
        on_progress: &dyn Fn(usize),
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let mtp_path = self.to_mtp_path(path);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        debug!(
            "MtpVolume::list_directory_with_progress: device={}, storage={}, input_path={}, mtp_path={}",
            device_id,
            storage_id,
            path.display(),
            mtp_path
        );

        let handle = tokio::runtime::Handle::current();

        let start = std::time::Instant::now();
        let result = handle.block_on(async move {
            connection_manager()
                .list_directory_with_progress(&device_id, storage_id, &mtp_path, on_progress)
                .await
        });

        match &result {
            Ok(entries) => debug!(
                "MtpVolume::list_directory_with_progress: completed in {:?}, {} entries",
                start.elapsed(),
                entries.len()
            ),
            Err(e) => debug!(
                "MtpVolume::list_directory_with_progress: failed in {:?}, error={:?}",
                start.elapsed(),
                e
            ),
        }

        result.map_err(map_mtp_error)
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
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

        let entries = self.list_directory(parent)?;
        entries
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.get_metadata(path).is_ok()
    }

    fn is_directory(&self, path: &Path) -> Result<bool, VolumeError> {
        self.get_metadata(path).map(|e| e.is_directory)
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

    fn notify_mutation(&self, _volume_id: &str, parent_path: &Path, mutation: super::MutationEvent) {
        use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

        // MTP's get_metadata lists the parent dir to find the entry, which is expensive
        // but correct. The MTP event loop (connection/event_loop.rs) also handles
        // change notifications, so this is belt-and-suspenders for self-mutations.
        match mutation {
            super::MutationEvent::Created(ref name) | super::MutationEvent::Modified(ref name) => {
                let entry_path = parent_path.join(name);
                match self.get_metadata(&entry_path) {
                    Ok(entry) => {
                        let change = if matches!(mutation, super::MutationEvent::Created(_)) {
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
            super::MutationEvent::Deleted(name) => {
                notify_directory_changed(&self.volume_id, parent_path, DirectoryChange::Removed(name));
            }
            super::MutationEvent::Renamed { from, to } => {
                let new_path = parent_path.join(&to);
                match self.get_metadata(&new_path) {
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
    }

    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let Some(parent) = path.parent() else {
            return Err(VolumeError::IoError("Cannot create root directory".into()));
        };
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Err(VolumeError::IoError("Invalid directory name".into()));
        };

        let parent_mtp_path = self.to_mtp_path(parent);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;
        let folder_name = name.to_string();

        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .create_folder(&device_id, storage_id, &parent_mtp_path, &folder_name)
                    .await
            })
            .map(|_| ())
            .map_err(map_mtp_error)?;

        self.notify_mutation(&self.volume_id, parent, super::MutationEvent::Created(name.to_string()));
        Ok(())
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
            .map_err(map_mtp_error)?;

        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            self.notify_mutation(
                &self.volume_id,
                parent,
                super::MutationEvent::Deleted(name.to_string_lossy().to_string()),
            );
        }
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        // MTP doesn't support atomic overwrite, so check for conflicts when not forced.
        if !force && self.exists(to) {
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
            .ok_or_else(|| VolumeError::IoError("Invalid source path".into()))?;
        let to_name = Path::new(&to_mtp)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| VolumeError::IoError("Invalid destination path".into()))?;
        let same_name = from_name == to_name;

        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;
        let handle = tokio::runtime::Handle::current();

        if same_parent {
            // Same directory — just rename
            let new_name = to_name.to_string();
            handle
                .block_on(async {
                    connection_manager()
                        .rename_object(&device_id, storage_id, &from_mtp, &new_name)
                        .await
                })
                .map(|_| ())
                .map_err(map_mtp_error)?;

            // Notify listing cache about same-directory rename
            if let Some(from_parent_path) = from.parent() {
                self.notify_mutation(
                    &self.volume_id,
                    from_parent_path,
                    super::MutationEvent::Renamed {
                        from: from_name.to_string(),
                        to: to_name.to_string(),
                    },
                );
            }
        } else {
            // Different directory — use MTP MoveObject
            let to_parent_str = to_parent.to_string_lossy().to_string();
            handle
                .block_on(async {
                    connection_manager()
                        .move_object(&device_id, storage_id, &from_mtp, &to_parent_str)
                        .await
                })
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
                handle
                    .block_on(async {
                        connection_manager()
                            .rename_object(&device_id, storage_id, &moved_path, &new_name)
                            .await
                    })
                    .map(|_| ())
                    .map_err(map_mtp_error)?;
            }

            // Cross-directory move: remove from source dir, add in dest dir
            if let Some(from_parent_path) = from.parent() {
                self.notify_mutation(
                    &self.volume_id,
                    from_parent_path,
                    super::MutationEvent::Deleted(from_name.to_string()),
                );
            }
            if let Some(to_parent_path) = to.parent() {
                self.notify_mutation(
                    &self.volume_id,
                    to_parent_path,
                    super::MutationEvent::Created(to_name.to_string()),
                );
            }
        }
        Ok(())
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

    fn export_to_local_with_progress(
        &self,
        source: &Path,
        local_dest: &Path,
        on_progress: &dyn Fn(u64, u64) -> std::ops::ControlFlow<()>,
    ) -> Result<u64, VolumeError> {
        let mtp_path = self.to_mtp_path(source);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;
        let local_dest = local_dest.to_path_buf();

        debug!(
            "MtpVolume::export_to_local_with_progress: device={}, storage={}, source={}, dest={}",
            device_id,
            storage_id,
            mtp_path,
            local_dest.display()
        );

        let handle = tokio::runtime::Handle::current();
        let progress = SendSyncProgress(on_progress);
        let operation_id = format!("export-{}", uuid::Uuid::new_v4());

        handle
            .block_on(async {
                connection_manager()
                    .download_file_with_progress(
                        &device_id,
                        storage_id,
                        &mtp_path,
                        &local_dest,
                        None,
                        &operation_id,
                        Some(&|bytes_done, bytes_total| progress.call(bytes_done, bytes_total)),
                    )
                    .await
            })
            .map(|result| result.bytes_transferred)
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
        let operation_id = format!("export-{}", uuid::Uuid::new_v4());

        handle
            .block_on(async move {
                connection_manager()
                    .download_file(&device_id, storage_id, &mtp_path, &local_dest, None, &operation_id)
                    .await
            })
            .map(|result| result.bytes_transferred)
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
    ) -> Result<Vec<ScanConflict>, VolumeError> {
        // List destination directory to check for conflicts
        let entries = self.list_directory(dest_path)?;
        let mut conflicts = Vec::new();

        for item in source_items {
            // Check if a file with the same name exists at destination
            if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
                // Convert modified_at (milliseconds u64) to i64 seconds
                let dest_modified = existing.modified_at.map(|ms| (ms / 1000) as i64);
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
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied(e.to_string())
        }
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(e.to_string()),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(e.to_string()),
        MtpConnectionError::Timeout { .. } => VolumeError::ConnectionTimeout(e.to_string()),
        MtpConnectionError::StorageFull { .. } => VolumeError::StorageFull { message: e.to_string() },
        MtpConnectionError::StoreReadOnly { .. } => VolumeError::ReadOnly(e.to_string()),
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
