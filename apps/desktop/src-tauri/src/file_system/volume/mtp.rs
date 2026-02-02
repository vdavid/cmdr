//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{Volume, VolumeError};
use crate::file_system::FileEntry;
use crate::mtp::connection::{connection_manager, MtpConnectionError};
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
            root: PathBuf::from(format!("/mtp-volume/{}/{}", device_id, storage_id)),
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

        // Get the tokio runtime handle - we're inside spawn_blocking,
        // so block_on is safe here
        let handle = tokio::runtime::Handle::current();

        handle
            .block_on(async move {
                connection_manager()
                    .list_directory(&device_id, storage_id, &mtp_path)
                    .await
            })
            .map_err(map_mtp_error)
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

    fn supports_watching(&self) -> bool {
        // MTP doesn't support file watching - the protocol has no notification mechanism
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
        assert_eq!(vol.root().to_string_lossy(), "/mtp-volume/mtp-20-5/65537");
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
    fn test_supports_watching() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert!(!vol.supports_watching());
    }
}
