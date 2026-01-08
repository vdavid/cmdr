//! Local POSIX file system volume implementation.

use super::{Volume, VolumeError};
use crate::file_system::FileEntry;
use crate::file_system::operations::{get_single_entry, list_directory_core};
use std::path::{Path, PathBuf};

/// A volume backed by the local POSIX file system.
///
/// This implementation wraps the real filesystem, with a configurable root path.
/// For example:
/// - Root "/" represents "Macintosh HD"
/// - Root "/Users/you/Dropbox" represents "Dropbox" as a volume
pub struct LocalPosixVolume {
    name: String,
    root: PathBuf,
}

impl LocalPosixVolume {
    /// Creates a new local volume with the given name and root path.
    ///
    /// # Arguments
    /// * `name` - Display name (e.g., "Macintosh HD", "Dropbox")
    /// * `root` - Absolute path to the volume root (e.g., "/", "/Users/you/Dropbox")
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
        }
    }

    /// Resolves a path relative to this volume's root to an absolute path.
    ///
    /// Empty paths or "." resolve to the root itself.
    /// Absolute paths are always treated as relative to the volume root
    /// (the leading "/" is stripped).
    #[cfg(test)]
    pub(super) fn resolve(&self, path: &Path) -> PathBuf {
        self.resolve_internal(path)
    }

    #[cfg(not(test))]
    fn resolve(&self, path: &Path) -> PathBuf {
        self.resolve_internal(path)
    }

    fn resolve_internal(&self, path: &Path) -> PathBuf {
        if path.as_os_str().is_empty() || path == Path::new(".") {
            self.root.clone()
        } else if path.is_absolute() {
            // Treat absolute paths as relative to volume root
            // Strip the leading "/" and join with root
            let relative = path.strip_prefix("/").unwrap_or(path);
            self.root.join(relative)
        } else {
            self.root.join(path)
        }
    }
}

impl Volume for LocalPosixVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let abs_path = self.resolve(path);
        list_directory_core(&abs_path).map_err(VolumeError::from)
    }

    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let abs_path = self.resolve(path);
        get_single_entry(&abs_path).map_err(VolumeError::from)
    }

    fn exists(&self, path: &Path) -> bool {
        // Use symlink_metadata instead of exists() to detect broken symlinks
        // Path::exists() follows symlinks and returns false for broken ones
        std::fs::symlink_metadata(self.resolve(path)).is_ok()
    }

    fn supports_watching(&self) -> bool {
        true
    }
}
