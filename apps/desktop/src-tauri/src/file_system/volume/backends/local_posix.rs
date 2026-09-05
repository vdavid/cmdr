//! Local POSIX file system volume implementation.
//!
//! The struct, the rename primitive, and the query/mutation half of
//! `impl Volume` live here. The two concerns big enough to read on their own sit
//! beside it, the way the MTP backend splits: `scan` (the copy-scan family) and
//! `streams` (byte movement in and out of a file). A trait impl can't span
//! files, so those methods stay here as one-line delegations to inherent bodies
//! over there.

mod scan;
mod streams;

use super::{
    CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream, WatchCoverage,
};
use crate::file_system::listing::{FileEntry, ListingTally, get_single_entry, list_directory_core_with_tally};
use crate::file_system::volume::ListingProgress;
#[cfg(feature = "playwright-e2e")]
use crate::ignore_poison::IgnorePoison;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::spawn_blocking;

/// How often a local listing samples its running tally and reports progress.
///
/// Sets the cadence of the pane's "Loaded N files..." readout: fast enough that a
/// multi-second folder visibly climbs, slow enough that the number stays readable.
/// Shortened under test so a scratch dir produces ticks without needing to be huge.
#[cfg(not(test))]
const PROGRESS_SAMPLE_INTERVAL: Duration = Duration::from_millis(200);
#[cfg(test)]
const PROGRESS_SAMPLE_INTERVAL: Duration = Duration::from_millis(1);

/// Which of a rename's two paths an `io::Error` is talking about.
///
/// `ENOENT` is the SOURCE that isn't there; `EEXIST` and `ENOTEMPTY` are the
/// DESTINATION that already is. Handing both to the same path would name the
/// wrong file in "there's already something called that", which is the one
/// sentence in the rename flow the user acts on.
pub(crate) fn rename_volume_error(err: &io::Error, from: &Path, to: &Path) -> VolumeError {
    match err.kind() {
        io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty => VolumeError::from_io_at(err, to),
        _ => VolumeError::from_io_at(err, from),
    }
}

/// Atomically renames a local path only when `destination` is unoccupied.
#[cfg(target_os = "macos")]
pub(crate) fn rename_local_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains a null byte"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination path contains a null byte"))?;
    // SAFETY: Both live C strings remain valid for the call. RENAME_EXCL makes
    // destination absence and the rename one kernel operation.
    let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn rename_local_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains a null byte"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination path contains a null byte"))?;
    // SAFETY: Both live C strings remain valid for the call. RENAME_NOREPLACE
    // is Linux's atomic no-overwrite contract.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

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

    /// Resolves a caller's path to an absolute one under this volume's root.
    ///
    /// The rule (root itself, already-under-root as-is, otherwise anchored) is
    /// `cmdr_fs::volume::root_anchored`, shared with the IPC boundary so a
    /// destination resolves the same way here and on a remote backend.
    #[cfg(test)]
    pub(super) fn resolve(&self, path: &Path) -> PathBuf {
        self.resolve_internal(path)
    }

    #[cfg(not(test))]
    fn resolve(&self, path: &Path) -> PathBuf {
        self.resolve_internal(path)
    }

    fn resolve_internal(&self, path: &Path) -> PathBuf {
        cmdr_fs::volume::root_anchored(&self.root, path)
    }
}

impl Volume for LocalPosixVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    /// The root is pure addressing here (every method `resolve`s against it and
    /// calls `std::fs`), so a re-root is a new instance at the new prefix. That
    /// is what lets the registry hand a doubly-mounted share's ID to the mount
    /// that's still live when the active one goes away.
    fn rerooted(&self, new_root: &Path) -> Option<Arc<dyn Volume>> {
        Some(Arc::new(LocalPosixVolume::new(self.name.clone(), new_root)))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
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
            // `on_progress` is `Sync` but not `Send`, so it can't ride into `spawn_blocking`
            // with the lister. Instead the lister publishes into a shared `ListingTally` and
            // this side samples it on a timer, leaving the callback on the async task that
            // owns it. Without this the pane sits on "Opening folder..." for a big folder's
            // whole read. `listing/DETAILS.md` § "Local listing progress".
            let tally = Arc::new(ListingTally::default());
            let tally_for_listing = Arc::clone(&tally);
            let mut listing = spawn_blocking(move || {
                list_directory_core_with_tally(&abs_path, &tally_for_listing)
                    .map_err(|e| VolumeError::from_io_at(&e, &abs_path))
            });

            let Some(on_progress) = on_progress else {
                return listing
                    .await
                    .expect("spawn_blocking listing closure doesn't panic and the task is uncancelable");
            };

            loop {
                tokio::select! {
                    // Biased so a listing that finished during the tick reports its real
                    // result instead of one more approximate count.
                    biased;
                    finished = &mut listing => {
                        return finished
                            .expect("spawn_blocking listing closure doesn't panic and the task is uncancelable");
                    }
                    () = tokio::time::sleep(PROGRESS_SAMPLE_INTERVAL) => {
                        let progress = tally.snapshot();
                        // Nothing stat'ed yet (the blocking pool hasn't picked the task up, or
                        // this is a slow `read_dir` on a network path): a zero would render as
                        // "Loaded 0 files...".
                        if progress.entries() > 0 {
                            on_progress(progress);
                        }
                    }
                }
            }
        })
    }

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
            spawn_blocking(move || get_single_entry(&abs_path).map_err(|e| VolumeError::from_io_at(&e, &abs_path)))
                .await
                .expect("spawn_blocking metadata closure doesn't panic and the task is uncancelable")
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        // Use symlink_metadata instead of exists() to detect broken symlinks
        // Path::exists() follows symlinks and returns false for broken ones
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || std::fs::symlink_metadata(abs_path).is_ok())
                .await
                .expect("spawn_blocking symlink_metadata closure doesn't panic and the task is uncancelable")
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                let metadata =
                    std::fs::symlink_metadata(&abs_path).map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                Ok(metadata.is_dir())
            })
            .await
            .expect("spawn_blocking is_directory closure doesn't panic and the task is uncancelable")
        })
    }

    fn notify_mutation<'a>(
        &'a self,
        volume_id: &'a str,
        parent_path: &'a Path,
        mutation: super::MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            crate::file_system::listing::mutation::patch_listing_after_local_mutation(volume_id, parent_path, mutation);
        })
    }

    fn can_watch_listings(&self) -> bool {
        true
    }

    fn listing_watch_coverage(&self, path: &Path) -> WatchCoverage {
        // Resolve relative-to-volume paths to their absolute form so the comparison
        // against `LISTING_CACHE` (which stores absolute paths) lines up.
        let abs_path = self.resolve(path);
        // Find any listing on this path (volume-agnostic: the listing cache is keyed
        // by listing_id and tagged with a volume_id, but LocalPosixVolume doesn't
        // store its own volume_id — the manager assigns it at registration time).
        let listings = crate::file_system::listing::caching::find_listings_for_path_on_volume(None, &abs_path);
        if listings.is_empty() {
            return WatchCoverage::None;
        }
        // A listing exists; report what the FSEvents watch attached to it covers.
        // There's a race window between the listing being populated and the watcher
        // being registered, during which this deliberately answers `None` (the
        // listing exists but isn't being kept fresh yet).
        //
        // The coverage was decided when the watch was armed, so this stays a pure
        // in-memory read: a `statfs` here would land in the middle of every
        // recursive scan walk. `watcher::coverage_for_watched_path` is where the
        // network-mount question is actually asked.
        let listing_ids: Vec<String> = listings.into_iter().map(|(lid, ..)| lid).collect();
        crate::file_system::watcher::coverage_for_listings(&listing_ids)
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
                    .open(&abs_path)
                    .map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                file.write_all(&content)
                    .map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                Ok(())
            })
            .await
            .expect("spawn_blocking create_file closure doesn't panic and the task is uncancelable")
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                std::fs::create_dir(&abs_path).map_err(|e| VolumeError::from_io_at(&e, &abs_path))?;
                Ok(())
            })
            .await
            .expect("spawn_blocking create_dir closure doesn't panic and the task is uncancelable")
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
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
                    VolumeError::from_io_at(&e, &abs_path)
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
                    VolumeError::from_io_at(&e, &abs_path)
                })?;
                Ok(())
            })
            .await
            .expect("spawn_blocking delete closure doesn't panic and the task is uncancelable")
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
                if !force && from_abs != to_abs {
                    rename_local_exclusive(&from_abs, &to_abs)
                        .map_err(|e| rename_volume_error(&e, &from_abs, &to_abs))?;
                } else {
                    std::fs::rename(&from_abs, &to_abs).map_err(|e| rename_volume_error(&e, &from_abs, &to_abs))?;
                }
                Ok(())
            })
            .await
            .expect("spawn_blocking rename closure doesn't panic and the task is uncancelable")
        })
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.scan_subtree(path, cmdr_fs::volume::ScanStop::none())
    }

    /// ❗ Overridden so `scan_subtree` gets the stop. The trait default loops
    /// `scan_for_copy` per path, which asks only BETWEEN paths — and one path here
    /// can be a whole mounted share: `/Volumes/naspi` over a sleeping NAS is
    /// minutes of `readdir` with nothing in between, which is precisely the scan
    /// somebody cancels.
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a cmdr_fs::volume::ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_with_boundary_impl(paths, boundary)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn operations_are_local(&self) -> bool {
        // Every operation here is a syscall against a mounted filesystem, so a
        // per-file `get_metadata` is a microsecond `stat` and the cap below is a
        // statement about this Mac, not about any peer. Both facts matter to the
        // transfer driver; see `Volume::operations_are_local`.
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // Local disk can handle several concurrent I/O streams; clamp to
        // physical-ish core count so we never spawn hundreds of tasks for
        // huge batches. `available_parallelism` returns logical CPUs, so we
        // halve it as a cheap stand-in for "physical cores" (no num_cpus dep).
        // Minimum of 4 keeps the behavior reasonable on single-core boxes.
        //
        // This is a guard-rail, NOT a capacity claim, which is why
        // `operations_are_local` above stops it from bounding a network peer.
        let logical = std::thread::available_parallelism().map_or(4, |n| n.get());
        let approx_physical = (logical / 2).max(1);
        approx_physical.clamp(4, 16)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_impl(path)
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        self.read_range_impl(path, offset, len)
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.write_from_stream_impl(dest, size, stream, on_progress)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.scan_for_conflicts_impl(source_items, dest_path)
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        let root = self.root.clone();
        Box::pin(async move {
            spawn_blocking(move || get_space_info_for_path(&root))
                .await
                .expect("spawn_blocking get_space_info closure doesn't panic and the task is uncancelable")
        })
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
            return Ok(space);
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

            // ❗ `used` is NOT the complement of `available` here: `statvfs`
            // reserves blocks that are neither free to a normal user nor holding
            // anything, so the two come from different fields and `bounded`
            // (which derives one from the other) would be wrong.
            Ok(SpaceInfo::Bounded {
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
