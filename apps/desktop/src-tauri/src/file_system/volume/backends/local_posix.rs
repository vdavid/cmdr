//! Local POSIX file system volume implementation.

use super::{
    CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream, VolumeScanner,
    VolumeWatcher,
};
use crate::file_system::git;
use crate::file_system::listing::{FileEntry, get_single_entry, list_directory_core};
#[cfg(feature = "playwright-e2e")]
use crate::ignore_poison::IgnorePoison;
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        #[cfg(feature = "playwright-e2e")]
        {
            let mut injected = self.injected_error.lock_ignore_poison();
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
            spawn_blocking(move || {
                if let Some(routed) = git::try_route_listing(&abs_path) {
                    return routed;
                }
                list_directory_core(&abs_path).map_err(VolumeError::from)
            })
            .await
            .unwrap()
        })
    }

    // list_directory_with_progress: delegate to the trait default (which calls list_directory).
    // The `on_progress` callback is not `Send`, so it can't go into `spawn_blocking`.

    #[cfg(feature = "playwright-e2e")]
    fn inject_error(&self, errno: i32) {
        *self.injected_error.lock_ignore_poison() = Some(errno);
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                if let Some(routed) = git::try_route_metadata(&abs_path) {
                    return routed;
                }
                get_single_entry(&abs_path).map_err(VolumeError::from)
            })
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

    fn notify_mutation<'a>(
        &'a self,
        volume_id: &'a str,
        parent_path: &'a Path,
        mutation: super::MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        // Virtual git paths receive their cache invalidations through the
        // `.git`-watcher pipeline (`file_system::git::watcher`), not via
        // mutation hooks. Mutating ops on them already return
        // `NotSupported` above, but a future caller might land here through
        // a different path; early-return rather than try to stat a path
        // that has no real-FS counterpart.
        if git::is_virtual(parent_path) {
            return Box::pin(async {});
        }
        // Fall through to the trait default.
        Box::pin(async move {
            use super::MutationEvent;
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};
            use crate::file_system::listing::reading::get_single_entry;

            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_path.join(name);
                    match get_single_entry(&entry_path) {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(volume_id, parent_path, change);
                        }
                        Err(e) => {
                            log::warn!("notify_mutation: couldn't stat {}: {}", entry_path.display(), e);
                        }
                    }
                }
                MutationEvent::Deleted(name) => {
                    notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_path.join(&to);
                    match get_single_entry(&new_path) {
                        Ok(entry) => {
                            notify_directory_changed(
                                volume_id,
                                parent_path,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    fn supports_watching(&self) -> bool {
        true
    }

    fn listing_is_watched(&self, path: &Path) -> bool {
        // Resolve relative-to-volume paths to their absolute form so the comparison
        // against `LISTING_CACHE` (which stores absolute paths) lines up.
        let abs_path = self.resolve(path);
        // Find any listing on this path (volume-agnostic: the listing cache is keyed
        // by listing_id and tagged with a volume_id, but LocalPosixVolume doesn't
        // store its own volume_id — the manager assigns it at registration time).
        let listings = crate::file_system::listing::caching::find_listings_for_path_on_volume(None, &abs_path);
        if listings.is_empty() {
            return false;
        }
        // A listing exists; check whether an FSEvents watcher is attached to any
        // matching listing_id. There's a race window between the listing being
        // populated and the watcher being registered, during which we deliberately
        // return false (the listing exists but isn't being kept fresh yet).
        match crate::file_system::watcher::WATCHER_MANAGER.read() {
            Ok(manager) => listings
                .iter()
                .any(|(lid, ..)| manager.watches.contains_key(lid.as_str())),
            Err(_) => false,
        }
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
        if git::is_virtual(&abs_path) {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        }
        let content = content.to_vec();
        Box::pin(async move {
            spawn_blocking(move || -> Result<(), VolumeError> {
                use std::io::Write;
                // `create_new(true)` is the no-clobber contract the IPC layer
                // and frontend assume: an `AlreadyExists` errno surfaces as
                // `VolumeError::AlreadyExists`, which the New File command
                // maps to a friendly "already exists" error. A plain
                // `std::fs::write` would silently truncate the user's file.
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&abs_path)?;
                file.write_all(&content)?;
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
        if git::is_virtual(&abs_path) {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        }
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
        if git::is_virtual(&abs_path) {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        }
        Box::pin(async move {
            spawn_blocking(move || {
                let metadata = std::fs::symlink_metadata(&abs_path).map_err(|e| {
                    log::warn!(
                        target: "local_posix",
                        "delete: stat failed for {}: {} (kind={:?}, errno={:?})",
                        abs_path.display(),
                        e,
                        e.kind(),
                        e.raw_os_error()
                    );
                    e
                })?;
                let result = if metadata.is_dir() {
                    std::fs::remove_dir(&abs_path)
                } else {
                    std::fs::remove_file(&abs_path)
                };
                result.map_err(|e| {
                    log::warn!(
                        target: "local_posix",
                        "delete: {} {} failed: {} (kind={:?}, errno={:?})",
                        if metadata.is_dir() { "remove_dir" } else { "remove_file" },
                        abs_path.display(),
                        e,
                        e.kind(),
                        e.raw_os_error()
                    );
                    e
                })?;
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
        if git::is_virtual(&from_abs) || git::is_virtual(&to_abs) {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        }
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
                use std::os::unix::fs::MetadataExt;

                let mut file_count = 0;
                let mut dir_count = 0;
                let mut total_bytes = 0u64;
                // `dedup_bytes` is the source's on-disk (`du`) footprint:
                // each inode counted once. `total_bytes` keeps counting every
                // hardlink (the cross-volume write footprint). The set is
                // scoped to this one top-level source; cross-source hardlinks
                // aren't deduped (rare; see `CopyScanResult::dedup_bytes`).
                let mut dedup_bytes = 0u64;
                let mut seen_inodes: std::collections::HashSet<u64> = std::collections::HashSet::new();

                for entry in WalkDir::new(&abs_path).min_depth(0) {
                    let entry = entry.map_err(|e| VolumeError::IoError {
                        message: e.to_string(),
                        raw_os_error: None,
                    })?;
                    let ft = entry.file_type();
                    if ft.is_file() {
                        file_count += 1;
                        if let Ok(meta) = entry.metadata() {
                            let len = meta.len();
                            total_bytes += len;
                            // `nlink == 1` is the overwhelmingly common case
                            // (no hardlinks): skip the set entirely. Only
                            // multiply-linked inodes pay the lookup.
                            if meta.nlink() <= 1 || seen_inodes.insert(meta.ino()) {
                                dedup_bytes += len;
                            }
                        }
                    } else if ft.is_dir() {
                        // Don't count the root itself if it's the starting point
                        if entry.depth() > 0 {
                            dir_count += 1;
                        }
                    }
                }

                // Top-level stat (also fills in single-file / empty-dir edge cases).
                // This runs regardless so we can populate `top_level_is_directory`
                // without re-statting downstream.
                let top_meta = std::fs::metadata(&abs_path).ok();
                let top_level_is_directory = top_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

                // If the path is a single file, count it
                if let Some(ref meta) = top_meta {
                    if meta.is_file() && file_count == 0 {
                        file_count = 1;
                        total_bytes = meta.len();
                        dedup_bytes = meta.len();
                    } else if meta.is_dir() && dir_count == 0 && file_count == 0 {
                        dir_count = 1;
                    }
                }

                Ok(CopyScanResult {
                    file_count,
                    dir_count,
                    total_bytes,
                    dedup_bytes,
                    top_level_is_directory,
                })
            })
            .await
            .unwrap()
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // Local disk can handle several concurrent I/O streams; clamp to
        // physical-ish core count so we never spawn hundreds of tasks for
        // huge batches. `available_parallelism` returns logical CPUs, so we
        // halve it as a cheap stand-in for "physical cores" (no num_cpus dep).
        // Minimum of 4 keeps the behavior reasonable on single-core boxes.
        let logical = std::thread::available_parallelism().map_or(4, |n| n.get());
        let approx_physical = (logical / 2).max(1);
        approx_physical.clamp(4, 16)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                if let Some(routed) = git::try_open_blob_stream(&abs_path) {
                    return routed;
                }
                let metadata = std::fs::metadata(&abs_path)?;
                if metadata.is_dir() {
                    return Err(VolumeError::IoError {
                        message: "Cannot stream a directory".into(),
                        raw_os_error: None,
                    });
                }
                let total_size = metadata.len();
                let file = std::fs::File::open(&abs_path)?;
                Ok(Box::new(LocalPosixReadStream {
                    file: Some(file),
                    total_size,
                    bytes_read: 0,
                }) as Box<dyn VolumeReadStream>)
            })
            .await
            .unwrap()
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let dest_abs = self.resolve(dest);
        if git::is_virtual(&dest_abs) {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        }
        Box::pin(async move {
            // Ensure parent directory exists
            if let Some(parent) = dest_abs.parent() {
                let parent = parent.to_path_buf();
                spawn_blocking(move || std::fs::create_dir_all(&parent))
                    .await
                    .unwrap()
                    .map_err(VolumeError::from)?;
            }

            // Open destination file on the blocking pool.
            let dest_for_open = dest_abs.clone();
            let mut file = spawn_blocking(move || std::fs::File::create(&dest_for_open))
                .await
                .unwrap()
                .map_err(VolumeError::from)?;

            let mut bytes_written = 0u64;
            while let Some(chunk_result) = stream.next_chunk().await {
                let chunk = chunk_result?;
                if chunk.is_empty() {
                    continue;
                }
                let chunk_len = chunk.len() as u64;

                // Write the chunk on the blocking pool.
                let (file_ret, write_res) = spawn_blocking(move || {
                    use std::io::Write;
                    let res = file.write_all(&chunk);
                    (file, res)
                })
                .await
                .unwrap();
                file = file_ret;
                write_res.map_err(VolumeError::from)?;

                bytes_written += chunk_len;

                if on_progress(bytes_written, size) == std::ops::ControlFlow::Break(()) {
                    // Drop the file handle and try to clean up the partial file.
                    drop(file);
                    let partial = dest_abs.clone();
                    let _ = spawn_blocking(move || std::fs::remove_file(&partial)).await;
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }

            // Make the file durable before signalling success. A bare
            // `file.flush()` is a userspace no-op on a raw `std::fs::File`, so
            // without `sync_data` the bytes would live only in the OS page
            // cache. A cross-volume copy/move landing on a local disk (MTP →
            // Local, SMB → Local, USB import) all flows through this method, so
            // reporting "complete" here without an fdatasync would let the user
            // eject / sleep and lose data (on a move, from both sides). This
            // gives the same "durable as each file completes" property the
            // local-FS chunked copy path already has (`transfer/chunked_copy.rs`
            // → `dst_file.sync_data()`): a crash mid-batch leaves earlier files
            // safe.
            //
            // Best-effort on error, matching `durability::flush_created_destinations`:
            // a failed `sync_data` is logged under `target: "write_durability"`,
            // NOT propagated. The bytes are written either way, and failing a
            // completed multi-GB transfer at the final fsync is worse UX than
            // accepting a small durability-window risk on a filesystem that
            // can't sync.
            let dest_for_sync = dest_abs.clone();
            file = spawn_blocking(move || {
                use std::io::Write;
                // Userspace flush first (harmless no-op on a raw File, but
                // correct if the writer is ever wrapped in a BufWriter).
                let _ = file.flush();
                if let Err(e) = file.sync_data() {
                    log::warn!(
                        target: "write_durability",
                        "write_from_stream: fdatasync failed for {}: {e}",
                        dest_for_sync.display()
                    );
                }
                file
            })
            .await
            .unwrap();
            drop(file);

            // Best-effort: fsync the parent directory so the new file's
            // directory entry (the create) is durable too. Some filesystems
            // reject directory fsync; log and continue.
            if let Some(parent) = dest_abs.parent() {
                let parent = parent.to_path_buf();
                let _ = spawn_blocking(move || match std::fs::File::open(&parent).and_then(|d| d.sync_all()) {
                    Ok(()) => {}
                    Err(e) => log::debug!(
                        target: "write_durability",
                        "write_from_stream: parent dir fsync skipped for {}: {e}",
                        parent.display()
                    ),
                })
                .await;
            }

            // No `notify_mutation` call here: `LocalPosixVolume`'s mutation
            // methods (create_file/delete/rename) all rely on FSEvents to
            // patch the cache, and FSEvents is reliable on local FS. The
            // SMB / MTP overrides need it because their out-of-band
            // notification channels can lose events; we don't.
            Ok(bytes_written)
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
                            source_is_directory: item.is_directory,
                            dest_is_directory: meta.is_dir(),
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

/// Streaming reader for `LocalPosixVolume` files.
///
/// Reads the file in 1 MiB chunks on the blocking thread pool via
/// `tokio::task::spawn_blocking`. Each `next_chunk` call hands the file handle
/// to the blocking pool, reads one chunk, and returns ownership along with the
/// data.
struct LocalPosixReadStream {
    file: Option<std::fs::File>,
    total_size: u64,
    bytes_read: u64,
}

/// 1 MiB chunks, matching `chunked_copy.rs`'s constant.
const LOCAL_STREAM_CHUNK_SIZE: usize = 1024 * 1024;

impl VolumeReadStream for LocalPosixReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let mut file = self.file.take()?;

            let (file_ret, result) = spawn_blocking(move || {
                use std::io::Read;
                let mut buf = vec![0u8; LOCAL_STREAM_CHUNK_SIZE];
                let n = match file.read(&mut buf) {
                    Ok(n) => n,
                    Err(e) => return (file, Err(VolumeError::from(e))),
                };
                buf.truncate(n);
                (file, Ok(buf))
            })
            .await
            .unwrap();

            match result {
                Ok(buf) if buf.is_empty() => {
                    // EOF: drop the file handle.
                    drop(file_ret);
                    None
                }
                Ok(buf) => {
                    self.bytes_read += buf.len() as u64;
                    self.file = Some(file_ret);
                    Some(Ok(buf))
                }
                Err(e) => {
                    drop(file_ret);
                    Some(Err(e))
                }
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

/// Gets space information for a path.
///
/// On macOS, uses `NSURLVolumeAvailableCapacityForImportantUsageKey` which includes purgeable
/// space (APFS snapshots, iCloud caches), matching what Finder reports. Falls back to `statvfs`
/// if the NSURL query fails. On Linux, uses `statvfs` directly (no purgeable space concept).
pub(crate) fn get_space_info_for_path(path: &Path) -> Result<SpaceInfo, VolumeError> {
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

    // SAFETY: `path_c` is a valid NUL-terminated C string from `path`; `stat` is a zeroed,
    // correctly-typed `libc::statvfs` out-buffer the kernel fills, and its fields are only read on
    // the `== 0` (success) branch where the kernel initialized them.
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
