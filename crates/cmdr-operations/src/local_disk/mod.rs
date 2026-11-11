//! The host's own disk, as the engine's shared code needs it: a file's device
//! and mode, the no-replace rename, mount and file-size questions, a
//! working-copy file, and a volume over a local folder.
//!
//! The one place (with the local-engine trees, `transfer/copy/` and friends)
//! that names `cmdr_local`, `libc`, or `std::os::unix`. Natively every function
//! forwards to the call the engine always made. The browser build has no local
//! disk, so there each answers what a host without one would; nothing reaches
//! them there, because that host's `OperationVolumes::local_disk` is `None` and
//! every local-disk branch asks it first (`host/DETAILS.md`).

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{MutationEvent, Volume, VolumeError};

mod posix;

#[cfg(target_os = "macos")]
pub(crate) use posix::{BsdFlags, bsd_flags, read_xattr};
pub(crate) use posix::{WriteProbe, home_dir, is_locked_item, is_mount_point, write_probe};

/// The permission bits of `permissions` (`st_mode`).
pub(crate) fn mode_of(permissions: &std::fs::Permissions) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.mode()
    }
    #[cfg(any(windows, target_family = "wasm"))]
    {
        if permissions.readonly() { 0o444 } else { 0o644 }
    }
}

/// `chmod(path, mode)`.
pub(crate) fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (path, mode);
        Err(no_local_disk())
    }
}

/// `mkdir(dir, mode)`.
pub(crate) fn create_dir_with_mode(dir: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(mode).create(dir)
    }
    #[cfg(any(windows, target_family = "wasm"))]
    {
        let _ = mode;
        std::fs::create_dir(dir)
    }
}

/// `options` opening a new file with `mode`.
pub(crate) fn with_mode(options: &mut std::fs::OpenOptions, mode: u32) -> &mut std::fs::OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode)
    }
    #[cfg(any(windows, target_family = "wasm"))]
    {
        let _ = mode;
        options
    }
}

/// `symlink(target, link)`.
#[cfg_attr(
    target_os = "macos",
    allow(
        dead_code,
        reason = "macOS copies a link with `copyfile`; the other local copies make it here"
    )
)]
pub(crate) fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (target, link);
        Err(no_local_disk())
    }
}

/// `rename(2)` that fails with `AlreadyExists` instead of clobbering `destination`
/// (`cmdr_local::rename_local_exclusive`).
pub(crate) fn rename_exclusive(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(not(target_family = "wasm"))]
    {
        cmdr_local::rename_local_exclusive(source, destination)
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (source, destination);
        Err(no_local_disk())
    }
}

/// A failed local rename as the volume vocabulary words it.
pub(crate) fn rename_error(err: &std::io::Error, from: &Path, to: &Path) -> VolumeError {
    #[cfg(not(target_family = "wasm"))]
    {
        cmdr_local::rename_volume_error(err, from, to)
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (err, from, to);
        VolumeError::NotSupported
    }
}

/// Every mount point in the OS mount table, or `None` when it can't be read
/// (and with no local disk, which has no mount table).
pub(crate) fn mount_points() -> Option<Vec<std::path::PathBuf>> {
    #[cfg(not(target_family = "wasm"))]
    {
        cmdr_local::mounts::list()
    }
    #[cfg(target_family = "wasm")]
    {
        None
    }
}

/// Whether `path` sits on a network mount.
pub(crate) fn is_on_network_mount(path: &Path) -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        cmdr_local::filesystem_kind::path_is_on_network_mount(path)
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = path;
        false
    }
}

/// What filesystem holds `path` (its kind and limits); `None` with no local disk.
pub(crate) fn filesystem_at(path: &Path) -> Option<cmdr_fs::filesystem_kind::FilesystemInfo> {
    #[cfg(not(target_family = "wasm"))]
    {
        Some(cmdr_local::filesystem_kind::detect_filesystem_for_path(path))
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = path;
        None
    }
}

/// Tells the panes about a create on the local disk.
pub(crate) fn patch_listings_after(listings: &dyn ListingHost, volume_id: &str, parent: &Path, event: MutationEvent) {
    #[cfg(not(target_family = "wasm"))]
    {
        cmdr_local::mutation::patch_listing_after_local_mutation(listings, volume_id, parent, event);
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = (listings, volume_id, parent, event);
    }
}

/// A volume over the local folder `root`, for a working copy or a spool.
pub(crate) fn folder_volume(name: &str, root: impl Into<std::path::PathBuf>, host: VolumeHost) -> Arc<dyn Volume> {
    #[cfg(not(target_family = "wasm"))]
    {
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(name, root, host))
    }
    #[cfg(target_family = "wasm")]
    {
        // Unreachable there (archive edits refuse on a host with no local disk);
        // an empty in-memory folder is the honest stand-in for "no disk here".
        let _ = host;
        Arc::new(cmdr_fs::volume::InMemoryVolume::new(name).with_root(root.into()))
    }
}

/// Every refusal a browser-build stand-in gives.
#[cfg(target_family = "wasm")]
fn no_local_disk() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, "no local disk")
}
