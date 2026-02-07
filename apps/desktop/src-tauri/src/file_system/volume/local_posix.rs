//! Local POSIX file system volume implementation.

use super::{CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError};
use crate::file_system::listing::{FileEntry, get_single_entry, list_directory_core};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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
            // If path already starts with our root, use it directly
            // This handles the case where frontend sends full absolute paths
            if path.starts_with(&self.root) {
                path.to_path_buf()
            } else if self.root == Path::new("/") {
                // For root volume, absolute paths are valid as-is
                path.to_path_buf()
            } else {
                // Treat absolute paths as relative to volume root
                // Strip the leading "/" and join with root
                let relative = path.strip_prefix("/").unwrap_or(path);
                self.root.join(relative)
            }
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

    fn is_directory(&self, path: &Path) -> Result<bool, VolumeError> {
        let abs_path = self.resolve(path);
        let metadata = std::fs::symlink_metadata(&abs_path)?;
        Ok(metadata.is_dir())
    }

    fn supports_watching(&self) -> bool {
        true
    }

    fn local_path(&self) -> Option<PathBuf> {
        Some(self.root.clone())
    }

    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let abs_path = self.resolve(path);
        std::fs::write(&abs_path, content)?;
        Ok(())
    }

    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let abs_path = self.resolve(path);
        std::fs::create_dir(&abs_path)?;
        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let abs_path = self.resolve(path);
        let metadata = std::fs::symlink_metadata(&abs_path)?;
        if metadata.is_dir() {
            std::fs::remove_dir(&abs_path)?;
        } else {
            std::fs::remove_file(&abs_path)?;
        }
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), VolumeError> {
        let from_abs = self.resolve(from);
        let to_abs = self.resolve(to);
        std::fs::rename(&from_abs, &to_abs)?;
        Ok(())
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy(&self, path: &Path) -> Result<CopyScanResult, VolumeError> {
        let abs_path = self.resolve(path);
        let mut file_count = 0;
        let mut dir_count = 0;
        let mut total_bytes = 0u64;

        for entry in WalkDir::new(&abs_path).min_depth(0) {
            let entry = entry.map_err(|e| VolumeError::IoError(e.to_string()))?;
            let ft = entry.file_type();
            if ft.is_file() {
                file_count += 1;
                if let Ok(meta) = entry.metadata() {
                    total_bytes += meta.len();
                }
            } else if ft.is_dir() {
                // Don't count the root itself if it's the starting point
                if entry.depth() > 0 {
                    dir_count += 1;
                }
            }
        }

        // If the path is a single file, count it
        if let Ok(meta) = std::fs::metadata(&abs_path) {
            if meta.is_file() && file_count == 0 {
                file_count = 1;
                total_bytes = meta.len();
            } else if meta.is_dir() && dir_count == 0 && file_count == 0 {
                dir_count = 1;
            }
        }

        Ok(CopyScanResult {
            file_count,
            dir_count,
            total_bytes,
        })
    }

    fn export_to_local(&self, source: &Path, local_dest: &Path) -> Result<u64, VolumeError> {
        let src_abs = self.resolve(source);
        copy_recursive(&src_abs, local_dest)
    }

    fn import_from_local(&self, local_source: &Path, dest: &Path) -> Result<u64, VolumeError> {
        let dest_abs = self.resolve(dest);
        copy_recursive(local_source, &dest_abs)
    }

    fn scan_for_conflicts(
        &self,
        source_items: &[SourceItemInfo],
        dest_path: &Path,
    ) -> Result<Vec<ScanConflict>, VolumeError> {
        let dest_abs = self.resolve(dest_path);
        let mut conflicts = Vec::new();

        for item in source_items {
            let dest_file_path = dest_abs.join(&item.name);
            if dest_file_path.exists()
                && let Ok(meta) = std::fs::metadata(&dest_file_path)
            {
                let dest_modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64));

                conflicts.push(ScanConflict {
                    source_path: item.name.clone(),
                    dest_path: dest_file_path.to_string_lossy().to_string(),
                    source_size: item.size,
                    dest_size: meta.len(),
                    source_modified: item.modified,
                    dest_modified,
                });
            }
        }

        Ok(conflicts)
    }

    fn get_space_info(&self) -> Result<SpaceInfo, VolumeError> {
        get_space_info_for_path(&self.root)
    }
}

/// Recursively copies a file or directory from source to destination.
/// Returns total bytes copied.
fn copy_recursive(source: &Path, dest: &Path) -> Result<u64, VolumeError> {
    let meta = std::fs::metadata(source)?;
    let mut total_bytes = 0;

    if meta.is_file() {
        // Copy single file
        std::fs::copy(source, dest)?;
        total_bytes = meta.len();
    } else if meta.is_dir() {
        // Create destination directory
        std::fs::create_dir_all(dest)?;

        // Copy all contents
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());
            total_bytes += copy_recursive(&src_path, &dest_path)?;
        }
    }

    Ok(total_bytes)
}

/// Gets space information for a path using statvfs.
fn get_space_info_for_path(path: &Path) -> Result<SpaceInfo, VolumeError> {
    use std::ffi::CString;

    let path_c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| VolumeError::IoError(e.to_string()))?;

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path_c.as_ptr(), &mut stat) == 0 {
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let block_size = stat.f_frsize as u64;
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let total_bytes = (stat.f_blocks as u64) * block_size;
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let available_bytes = (stat.f_bavail as u64) * block_size;
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let used_bytes = total_bytes.saturating_sub((stat.f_bfree as u64) * block_size);

            Ok(SpaceInfo {
                total_bytes,
                available_bytes,
                used_bytes,
            })
        } else {
            Err(VolumeError::IoError("Failed to get space info".into()))
        }
    }
}
