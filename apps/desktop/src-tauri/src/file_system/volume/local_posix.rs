//! Local POSIX file system volume implementation.

use super::{
    CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeScanner, VolumeWatcher,
};
use crate::file_system::listing::{FileEntry, get_single_entry, list_directory_core};
use crate::indexing::scanner::{self, ScanConfig, ScanError, ScanHandle, ScanSummary};
use crate::indexing::watcher::{DriveWatcher, FsChangeEvent, WatcherError};
use crate::indexing::writer::IndexWriter;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;
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
    /// Raw errno to inject on the next `list_directory` call. Cleared after use.
    #[cfg(feature = "playwright-e2e")]
    injected_error: std::sync::Mutex<Option<i32>>,
}

impl LocalPosixVolume {
    /// Creates a new local volume with the given name and root path.
    ///
    /// # Arguments
    /// * `name` - Display name (like "Macintosh HD", "Dropbox")
    /// * `root` - Absolute path to the volume root (like "/", "/Users/you/Dropbox")
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
            #[cfg(feature = "playwright-e2e")]
            injected_error: std::sync::Mutex::new(None),
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

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        #[cfg(feature = "playwright-e2e")]
        {
            let mut injected = self.injected_error.lock().unwrap();
            if let Some(errno) = injected.take() {
                return Box::pin(async move {
                    Err(VolumeError::IoError {
                        message: format!("Injected error for testing (os error {})", errno),
                        raw_os_error: Some(errno),
                    })
                });
            }
        }
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || list_directory_core(&abs_path).map_err(VolumeError::from))
                .await
                .unwrap()
        })
    }

    // list_directory_with_progress: delegate to the trait default (which calls list_directory).
    // The `on_progress` callback is not `Send`, so it can't go into `spawn_blocking`.

    #[cfg(feature = "playwright-e2e")]
    fn inject_error(&self, errno: i32) {
        *self.injected_error.lock().unwrap() = Some(errno);
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || get_single_entry(&abs_path).map_err(VolumeError::from))
                .await
                .unwrap()
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        // Use symlink_metadata instead of exists() to detect broken symlinks
        // Path::exists() follows symlinks and returns false for broken ones
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || std::fs::symlink_metadata(abs_path).is_ok())
                .await
                .unwrap()
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                let metadata = std::fs::symlink_metadata(&abs_path)?;
                Ok(metadata.is_dir())
            })
            .await
            .unwrap()
        })
    }

    fn supports_watching(&self) -> bool {
        true
    }

    fn local_path(&self) -> Option<PathBuf> {
        Some(self.root.clone())
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        let content = content.to_vec();
        Box::pin(async move {
            spawn_blocking(move || {
                std::fs::write(&abs_path, content)?;
                Ok(())
            })
            .await
            .unwrap()
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                std::fs::create_dir(&abs_path)?;
                Ok(())
            })
            .await
            .unwrap()
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                let metadata = std::fs::symlink_metadata(&abs_path)?;
                if metadata.is_dir() {
                    std::fs::remove_dir(&abs_path)?;
                } else {
                    std::fs::remove_file(&abs_path)?;
                }
                Ok(())
            })
            .await
            .unwrap()
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let from_abs = self.resolve(from);
        let to_abs = self.resolve(to);
        Box::pin(async move {
            spawn_blocking(move || {
                if !force && from_abs != to_abs && std::fs::symlink_metadata(&to_abs).is_ok() {
                    return Err(VolumeError::AlreadyExists(to_abs.display().to_string()));
                }
                std::fs::rename(&from_abs, &to_abs)?;
                Ok(())
            })
            .await
            .unwrap()
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                let mut file_count = 0;
                let mut dir_count = 0;
                let mut total_bytes = 0u64;

                for entry in WalkDir::new(&abs_path).min_depth(0) {
                    let entry = entry.map_err(|e| VolumeError::IoError {
                        message: e.to_string(),
                        raw_os_error: None,
                    })?;
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
            })
            .await
            .unwrap()
        })
    }

    fn export_to_local<'a>(
        &'a self,
        source: &'a Path,
        local_dest: &'a Path,
        _on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let src_abs = self.resolve(source);
        let local_dest = local_dest.to_path_buf();
        Box::pin(async move {
            spawn_blocking(move || copy_recursive(&src_abs, &local_dest))
                .await
                .unwrap()
        })
    }

    fn import_from_local<'a>(
        &'a self,
        local_source: &'a Path,
        dest: &'a Path,
        _on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let local_source = local_source.to_path_buf();
        let dest_abs = self.resolve(dest);
        Box::pin(async move {
            spawn_blocking(move || copy_recursive(&local_source, &dest_abs))
                .await
                .unwrap()
        })
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        let dest_abs = self.resolve(dest_path);
        let source_items: Vec<SourceItemInfo> = source_items.to_vec();
        Box::pin(async move {
            spawn_blocking(move || {
                let mut conflicts = Vec::new();

                for item in &source_items {
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
            })
            .await
            .unwrap()
        })
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        let root = self.root.clone();
        Box::pin(async move { spawn_blocking(move || get_space_info_for_path(&root)).await.unwrap() })
    }

    fn scanner(&self) -> Option<Box<dyn VolumeScanner>> {
        Some(Box::new(LocalPosixScanner))
    }

    fn watcher(&self) -> Option<Box<dyn VolumeWatcher>> {
        Some(Box::new(LocalPosixWatcher))
    }
}

// ── Indexing trait implementations ────────────────────────────────────

/// Scanner for local POSIX volumes using jwalk parallel directory traversal.
struct LocalPosixScanner;

impl VolumeScanner for LocalPosixScanner {
    fn scan_volume(
        &self,
        config: ScanConfig,
        writer: &IndexWriter,
    ) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
        scanner::scan_volume(config, writer)
    }

    fn scan_subtree(
        &self,
        root: &Path,
        writer: &IndexWriter,
        cancelled: &AtomicBool,
    ) -> Result<ScanSummary, ScanError> {
        scanner::scan_subtree(root, writer, cancelled)
    }
}

/// Watcher for local POSIX volumes using macOS FSEvents via cmdr-fsevent-stream.
struct LocalPosixWatcher;

impl VolumeWatcher for LocalPosixWatcher {
    fn watch(
        &self,
        root: &Path,
        since_when: u64,
        event_sender: mpsc::Sender<FsChangeEvent>,
    ) -> Result<DriveWatcher, WatcherError> {
        DriveWatcher::start(root, since_when, event_sender)
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

/// Gets space information for a path.
///
/// On macOS, uses `NSURLVolumeAvailableCapacityForImportantUsageKey` which includes purgeable
/// space (APFS snapshots, iCloud caches) — matching what Finder reports. Falls back to `statvfs`
/// if the NSURL query fails. On Linux, uses `statvfs` directly (no purgeable space concept).
fn get_space_info_for_path(path: &Path) -> Result<SpaceInfo, VolumeError> {
    // On macOS, prefer the NSURL API that accounts for purgeable space.
    #[cfg(target_os = "macos")]
    {
        if let Some(space) = crate::volumes::get_volume_space(&path.to_string_lossy()) {
            // NSURL doesn't give us used_bytes directly, compute from total - available.
            let used_bytes = space.total_bytes.saturating_sub(space.available_bytes);
            return Ok(SpaceInfo {
                total_bytes: space.total_bytes,
                available_bytes: space.available_bytes,
                used_bytes,
            });
        }
    }

    // Fallback (and Linux primary path): statvfs
    get_space_info_statvfs(path)
}

/// Gets space information using `statvfs`. Used as the primary method on Linux and as a
/// fallback on macOS.
fn get_space_info_statvfs(path: &Path) -> Result<SpaceInfo, VolumeError> {
    use std::ffi::CString;

    let path_c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| VolumeError::IoError {
        message: e.to_string(),
        raw_os_error: None,
    })?;

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
            Err(VolumeError::IoError {
                message: "Failed to get space info".into(),
                raw_os_error: None,
            })
        }
    }
}
