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

use crate::reading::{ListingTally, get_single_entry, list_directory_core_with_tally};
use crate::space::{get_space_info_for_path, live_space_info_for_path};
use cmdr_fs::entry::FileEntry;
#[cfg(feature = "playwright-e2e")]
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataCaps, MetadataClasses, MetadataReport, Unblock, UserFlags};
use cmdr_fs::volume::MountClass;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::listing_progress::report_while;
use cmdr_fs::volume::{
    CopyScanResult, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream, WatchCoverage,
};
use cmdr_fs::volume::{
    EntryKind, ListingProgress, NodeIdentity, StreamLength, StreamWriteProgress, StreamWritten, WriteMode,
};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
// An ambient `spawn_blocking`, not the host's runtime: a `Volume` method runs on its
// caller's. `DETAILS.md` § "Decisions".
use tokio::task::spawn_blocking;

/// How often a local listing samples its running tally and reports progress.
///
/// Sets the cadence of the pane's "Loaded N files..." readout: fast enough that a
/// multi-second folder visibly climbs, slow enough that the number stays readable.
/// Shortened in a test build (`crate::TEST_FAST_PATHS`) so a scratch dir
/// produces ticks without needing to be huge.
const PROGRESS_SAMPLE_INTERVAL: Duration = if crate::TEST_FAST_PATHS {
    Duration::from_millis(1)
} else {
    Duration::from_millis(200)
};

/// Which of a rename's two paths an `io::Error` is talking about.
///
/// `ENOENT` is the SOURCE that isn't there; `EEXIST` and `ENOTEMPTY` are the
/// DESTINATION that already is. Handing both to the same path would name the
/// wrong file in "there's already something called that", which is the one
/// sentence in the rename flow the user acts on.
pub fn rename_volume_error(err: &io::Error, from: &Path, to: &Path) -> VolumeError {
    match err.kind() {
        io::ErrorKind::AlreadyExists | io::ErrorKind::DirectoryNotEmpty => VolumeError::from_io_at(err, to),
        _ => VolumeError::from_io_at(err, from),
    }
}

/// Renames a local path only when `destination` is unoccupied.
///
/// **The one no-replace rename primitive in the app**: every rename that isn't
/// meant to overwrite comes through here, from the pane's F2 to a copy landing
/// its temp on a name the conflict check found free. A plain POSIX `rename(2)`
/// replaces silently, and both callers have a window between the check and the
/// rename in which a file can appear, so destroying it would be silent and
/// unrecoverable.
pub fn rename_local_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    rename_exclusive_with(source, destination, rename_exclusive_syscall)
}

/// The kernel's atomic no-replace rename: `RENAME_EXCL` on macOS,
/// `RENAME_NOREPLACE` on Linux.
fn rename_exclusive_syscall(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains a null byte"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination path contains a null byte"))?;
    // SAFETY: Both live C strings remain valid for the call. RENAME_EXCL makes
    // destination absence and the rename one kernel operation.
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    // SAFETY: as above; `AT_FDCWD` makes both paths cwd-relative, matching the
    // absolute paths we pass. RENAME_NOREPLACE is Linux's no-overwrite contract.
    #[cfg(target_os = "linux")]
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

/// [`rename_local_exclusive`] over an injectable `attempt`, which is how the
/// no-flag degradation is testable on a filesystem that has the flag.
///
/// ❗ **The flag is not universal, and a filesystem without it must still be
/// renameable.** APFS, HFS+, and ext4 take it; macOS's smbfs answers `ENOTSUP`,
/// and other non-native mounts answer `EINVAL` or `ENOSYS`. Only those three
/// degrade, to a check-then-rename that is racy but still strictly better than
/// the unconditional clobber a bare `rename(2)` performs. Every other errno is
/// the filesystem's own answer and reaches the caller unchanged.
pub(super) fn rename_exclusive_with(
    source: &Path,
    destination: &Path,
    attempt: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let Err(err) = attempt(source, destination) else {
        return Ok(());
    };
    let no_such_flag = matches!(
        err.raw_os_error(),
        Some(libc::ENOTSUP) | Some(libc::EINVAL) | Some(libc::ENOSYS)
    );
    if !no_such_flag {
        return Err(err);
    }
    log::debug!(
        target: "volume",
        "rename_local_exclusive: {} doesn't support an atomic no-replace rename ({err}); checking first instead",
        LogPrivate::path(destination)
    );

    rename_if_free(source, destination, name_state(std::fs::symlink_metadata(destination)))
}

/// What a stat of a rename destination proved about the name.
///
/// ❗ **Three answers, never two.** A stat that failed for any reason other than
/// the name being free has proved nothing, and folding that into [`Self::Free`]
/// is how a check-then-rename destroys the file it exists to protect. The
/// exposed filesystems are the ones most likely to answer with something else:
/// a reconnecting share, a marginal stick, a mount on its way out.
pub(super) enum NameState {
    /// Something is there, so an exclusive rename has to refuse.
    Occupied,
    /// Nothing is there, so the rename may go ahead.
    Free,
    /// The filesystem wouldn't say, carrying the reason it gave instead.
    Unknown(io::Error),
}

/// Reads a destination stat as the answer it actually carries.
///
/// Takes the result rather than the path so the three-way fold stays pure, and
/// so [`rename_if_free`] can be handed an `Unknown` a real filesystem only
/// produces transiently.
pub(super) fn name_state(stat: io::Result<std::fs::Metadata>) -> NameState {
    match stat {
        Ok(_) => NameState::Occupied,
        Err(err) if err.kind() == io::ErrorKind::NotFound => NameState::Free,
        Err(err) => NameState::Unknown(err),
    }
}

/// Renames onto a name a stat has just reported on, refusing unless it is free.
///
/// Still racy, irreducibly: the name can be taken in the gap between the stat
/// and the rename, and without the kernel flag there is no way to close that.
/// What it does guarantee is that the gap is the ONLY way through.
pub(super) fn rename_if_free(source: &Path, destination: &Path, state: NameState) -> io::Result<()> {
    match state {
        NameState::Occupied => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        )),
        NameState::Unknown(err) => Err(err),
        NameState::Free => std::fs::rename(source, destination),
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
    /// The mount this volume sits on, which decides whether a drive index can
    /// serve it (`capabilities`).
    mount: MountClass,
    /// Whether the kernel keeps AppleDouble sidecars on the volume holding a
    /// path. The real probe outside tests, which inject one to play a FAT
    /// volume on APFS. See [`Volume::list_directory_as_source`].
    sidecar_probe: fn(&Path) -> bool,
    /// The app around this volume: the pane listings it patches after a
    /// mutation, the watch coverage it reports, and the name order its listings
    /// come back in. ❗ A required argument, never a default: a volume built with
    /// a detached host silently turns off the scan oracle's fast path and leaves
    /// panes stale after every local copy.
    host: VolumeHost,
    /// Raw errno to inject on the next `list_directory` call. Cleared after use.
    #[cfg(feature = "playwright-e2e")]
    injected_error: std::sync::Mutex<Option<i32>>,
}

impl LocalPosixVolume {
    /// A volume for a folder on a local disk: the boot volume, a cloud-sync
    /// folder.
    ///
    /// # Arguments
    /// * `name` - Display name (like "Macintosh HD", "Dropbox")
    /// * `root` - Absolute path to the volume root (like "/", "/Users/you/Dropbox")
    /// * `host` - The app around it (`volume_host::host()` in Cmdr)
    ///
    /// ❗ Named for what it assumes. A MOUNT goes through [`Self::on_mount`] with
    /// the class its registration probed: a network mount built here would
    /// publish itself as indexable, and the user would be offered indexing that
    /// could only be refused.
    pub fn local_folder(name: impl Into<String>, root: impl Into<PathBuf>, host: VolumeHost) -> Self {
        Self::on_mount(name, root, MountClass::LocalDisk, host)
    }

    /// A volume rooted at a mount, with the [`MountClass`] its registration found
    /// (`index_provider::mount_class_at`).
    pub fn on_mount(name: impl Into<String>, root: impl Into<PathBuf>, mount: MountClass, host: VolumeHost) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
            mount,
            sidecar_probe: cmdr_fs::apple_double::volume_keeps_sidecars,
            host,
            #[cfg(feature = "playwright-e2e")]
            injected_error: std::sync::Mutex::new(None),
        }
    }

    /// This volume with an injected sidecar probe, so a test on APFS can read
    /// it as a FAT volume.
    #[cfg(any(test, feature = "testing"))]
    pub fn with_sidecar_probe(mut self, probe: fn(&Path) -> bool) -> Self {
        self.sidecar_probe = probe;
        self
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

impl LocalPosixVolume {
    /// What local disk keeps of a copy routed through the volume engine, and
    /// what its streams report off the open's `stat` and fd. Every user flag
    /// waits for the landing (`staged_flags` is empty): the copy puts them on
    /// with `apply_metadata` once the file has its final name, since Locked
    /// refuses the rename and the set goes on as one. Linux can't set a
    /// creation date (`statx` reads it) and has no user flags.
    #[cfg(target_os = "macos")]
    pub const METADATA_CAPS: MetadataCaps = MetadataCaps {
        stores: MetadataClasses::ALL,
        reports: MetadataClasses::ALL,
        folder_stores: MetadataClasses::ALL,
        folder_dates: true,
        staged_flags: UserFlags::NONE,
    };
    /// See the macOS twin.
    #[cfg(not(target_os = "macos"))]
    pub const METADATA_CAPS: MetadataCaps = MetadataCaps {
        stores: MetadataClasses::MODE.union(MetadataClasses::XATTRS),
        reports: MetadataClasses::CREATED
            .union(MetadataClasses::MODE)
            .union(MetadataClasses::XATTRS),
        folder_stores: MetadataClasses::MODE.union(MetadataClasses::XATTRS),
        folder_dates: true,
        staged_flags: UserFlags::NONE,
    };
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
    /// that's still live when the active one goes away. The new instance keeps
    /// this one's host.
    fn rerooted(&self, new_root: &Path) -> Option<Arc<dyn Volume>> {
        Some(Arc::new(LocalPosixVolume::on_mount(
            self.name.clone(),
            new_root,
            self.mount,
            self.host.clone(),
        )))
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
        // The one place Cmdr knows a directory IS a volume root, not merely
        // somewhere under one: `/.hidden` (`read_dot_hidden_names`) means nothing
        // anywhere else, so this is threaded through rather than re-derived below.
        let is_volume_root = abs_path == self.root;
        Box::pin(async move {
            // `on_progress` is `Sync` but not `Send`, so it can't ride into `spawn_blocking`
            // with the lister. Instead the lister publishes into a shared `ListingTally` and
            // this side samples it on a timer, leaving the callback on the async task that
            // owns it. Without this the pane sits on "Opening folder…" for a big folder's
            // whole read. `DETAILS.md` § "The local reader".
            let tally = Arc::new(ListingTally::default());
            let tally_for_listing = Arc::clone(&tally);
            let host = self.host.clone();
            let listing = spawn_blocking(move || {
                list_directory_core_with_tally(&abs_path, &tally_for_listing, is_volume_root, host.listings())
                    .map_err(|e| VolumeError::from_io_at(&e, &abs_path))
            });
            report_while(listing, PROGRESS_SAMPLE_INTERVAL, || tally.snapshot(), on_progress)
                .await
                .expect("spawn_blocking listing closure doesn't panic and the task is uncancelable")
        })
    }

    fn list_directory_as_source<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_as_source_impl(path)
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

    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        // One bare `lstat`: `get_metadata` also stats a link's target and looks
        // up owner names, none of which answers this.
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                let file_type = std::fs::symlink_metadata(&abs_path)
                    .map_err(|e| VolumeError::from_io_at(&e, &abs_path))?
                    .file_type();
                Ok(if file_type.is_symlink() {
                    EntryKind::Symlink
                } else if file_type.is_dir() {
                    EntryKind::Directory
                } else {
                    EntryKind::File
                })
            })
            .await
            .expect("spawn_blocking entry_kind closure doesn't panic and the task is uncancelable")
        })
    }

    fn node_identity<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Option<NodeIdentity>, VolumeError>> + Send + 'a>> {
        // One bare `lstat`, the same read the trash journals the identity from.
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                std::fs::symlink_metadata(&abs_path)
                    .map(|metadata| NodeIdentity::of(&metadata))
                    .map_err(|e| VolumeError::from_io_at(&e, &abs_path))
            })
            .await
            .expect("spawn_blocking node_identity closure doesn't panic and the task is uncancelable")
        })
    }

    fn notify_mutation<'a>(
        &'a self,
        volume_id: &'a str,
        parent_path: &'a Path,
        mutation: cmdr_fs::volume::MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            crate::mutation::patch_listing_after_local_mutation(self.host.listings(), volume_id, parent_path, mutation);
        })
    }

    fn can_watch_listings(&self) -> bool {
        true
    }

    fn listing_watch_coverage(&self, path: &Path) -> WatchCoverage {
        // Resolve relative-to-volume paths to their absolute form so the comparison
        // against the listing cache (which stores absolute paths) lines up.
        let abs_path = self.resolve(path);
        // By path: this volume doesn't know its own id (the registry assigns it).
        self.host.listings().local_watch_coverage(&abs_path)
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

    /// The folder's xattrs (Finder tags, comments, the FinderInfo of a custom
    /// icon) and its flags (Hidden, Locked), the classes a folder reports beyond
    /// its listing entry: one `lstat`, which answers the flags, and one
    /// `listxattr` plus a read per name when the xattrs are asked for.
    fn folder_metadata<'a>(
        &'a self,
        path: &'a Path,
        wanted: MetadataClasses,
    ) -> Pin<Box<dyn Future<Output = CarriedMetadata> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            let wanted = wanted.intersection(MetadataClasses::XATTRS | MetadataClasses::FLAGS);
            if wanted.is_empty() {
                return CarriedMetadata::default();
            }
            spawn_blocking(move || match std::fs::symlink_metadata(&abs_path) {
                Ok(meta) if meta.is_dir() && wanted.contains(MetadataClasses::XATTRS) => {
                    CarriedMetadata::of_local_dir(&abs_path, &meta).restricted_to(wanted)
                }
                Ok(meta) if meta.is_dir() => CarriedMetadata::of_local_stat(&meta).restricted_to(wanted),
                _ => CarriedMetadata::default(),
            })
            .await
            .expect("spawn_blocking folder_metadata closure doesn't panic and the task is uncancelable")
        })
    }

    /// Every class, through the host helper: xattrs, the folded mode, then
    /// birth and mtime in one call, then the flags last (Locked would refuse the
    /// others).
    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Pin<Box<dyn Future<Output = MetadataReport> + Send + 'a>> {
        let abs_path = self.resolve(path);
        let carried = carried.clone();
        Box::pin(async move {
            spawn_blocking(move || streams::apply_metadata_at(&abs_path, &carried, modified))
                .await
                .expect("spawn_blocking apply_metadata closure doesn't panic and the task is uncancelable")
        })
    }

    /// Clears the item's own `uchg`/`uappnd` and strips its ACL
    /// (`cmdr_fs::metadata::clear_blockers`), never through a symlink. A
    /// Locked FOLDER refusing its child's removal is the caller's to open
    /// (`open_parent_folder`), since only it knows when to close it again.
    fn clear_blockers<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Unblock> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || cmdr_fs::metadata::clear_blockers(&abs_path))
                .await
                .expect("spawn_blocking clear_blockers closure doesn't panic and the task is uncancelable")
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
                        LogPrivate::path(&abs_path),
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
                        LogPrivate::path(&abs_path),
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

    fn mount_class(&self) -> Option<MountClass> {
        Some(self.mount)
    }

    fn supports_export(&self) -> bool {
        true
    }

    /// [`Self::METADATA_CAPS`].
    fn metadata_caps(&self) -> MetadataCaps {
        Self::METADATA_CAPS
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.scan_subtree(path, cmdr_fs::volume::ScanStop::none(), None)
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

    fn supports_unknown_length_writes(&self) -> bool {
        true
    }

    fn supports_atomic_replace_rename(&self) -> bool {
        true
    }

    fn operations_are_local(&self) -> bool {
        // Every operation here is a syscall against a mounted filesystem, so a
        // per-file `get_metadata` is a microsecond `stat` and the cap below is a
        // statement about this Mac, not about any peer. Both facts matter to the
        // transfer driver; see `Volume::operations_are_local`.
        true
    }

    fn matches_names_in_any_unicode_form(&self) -> bool {
        // APFS keeps the spelling a name was created with and finds it in either
        // form (verified on macOS 27.0: a decomposed `café.txt` stats under its
        // composed spelling and lists decomposed, 2026-09-23). A Linux disk
        // matches bytes, so it gets the byte-exact default.
        cfg!(target_os = "macos")
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
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        self.write_from_stream_impl(dest, mode, length, stream, on_progress)
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

    fn get_live_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        let root = self.root.clone();
        Box::pin(async move {
            spawn_blocking(move || live_space_info_for_path(&root))
                .await
                .expect("spawn_blocking get_live_space_info closure doesn't panic and the task is uncancelable")
        })
    }

    /// `statvfs` and `access(W_OK)` on the nearest folder at or above `path` that
    /// exists, off the async runtime (`write_access_for_path`).
    fn write_access_at<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = cmdr_fs::volume::WriteAccess> + Send + 'a>> {
        let target = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || write_access_for_path(&target))
                .await
                .expect("spawn_blocking write_access closure doesn't panic and the task is uncancelable")
        })
    }
}

/// Whether a write into `path` would be taken, answered for the nearest folder at
/// or above it that exists (a copy creates the rest).
///
/// A read-only filesystem is the `statvfs` `ST_RDONLY` flag, or `access(W_OK)`
/// refused with `EROFS`; a missing permission is `access` refused with `EACCES` or
/// `EPERM`. Anything the two calls can't answer is `Unknown`, ❌ never a guess.
pub(crate) fn write_access_for_path(path: &Path) -> cmdr_fs::volume::WriteAccess {
    use cmdr_fs::volume::{UnwritableReason, WriteAccess};
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Some(folder) = path.ancestors().find(|ancestor| ancestor.is_dir()) else {
        return WriteAccess::Unknown;
    };
    let Ok(c_path) = CString::new(folder.as_os_str().as_bytes()) else {
        return WriteAccess::Unknown;
    };
    let read_only = WriteAccess::Unwritable {
        reason: UnwritableReason::ReadOnlyFilesystem,
    };

    // SAFETY: `c_path` is a valid NUL-terminated C string; `stat` is a zeroed,
    // correctly-typed `libc::statvfs` out-buffer, and its flags are read only on the
    // `== 0` branch, where the kernel filled them.
    let mount_is_read_only = unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        libc::statvfs(c_path.as_ptr(), &mut stat) == 0 && (stat.f_flag & libc::ST_RDONLY) != 0
    };
    if mount_is_read_only {
        return read_only;
    }
    // SAFETY: `c_path` is a valid NUL-terminated C string, and `access` only reads it.
    if unsafe { libc::access(c_path.as_ptr(), libc::W_OK) } == 0 {
        return WriteAccess::Writable;
    }
    match io::Error::last_os_error().raw_os_error() {
        Some(libc::EROFS) => read_only,
        Some(libc::EACCES | libc::EPERM) => WriteAccess::Unwritable {
            reason: UnwritableReason::NoPermission,
        },
        _ => WriteAccess::Unknown,
    }
}
