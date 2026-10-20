//! Where a tree removal must stop: a directory another filesystem is mounted on.
//!
//! A recursive delete that walks into a mount empties that volume, often a NAS
//! or a server other people use, and the mount point's own `rmdir` then fails
//! quietly, after its contents are gone. Both delete walkers ask
//! [`is_mount_boundary`] of every directory they're about to descend, an
//! Overwrite that replaces a folder asks [`refuse_a_mount_inside`] before it
//! sets the folder aside, and a move onto another volume or into a zip asks
//! [`refuse_moving_a_mount`] of its sources before it copies anything (it would
//! otherwise copy the drive's files off, then delete them there). Each refuses
//! the whole operation on a yes, before anything is removed, which is what
//! Finder does.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use super::state::WriteOperationState;
use super::types::{MountedVolumeAction, WriteOperationError};

/// Whether the directory at `path` is where another filesystem is mounted.
/// `dev` is the device its own `lstat` reported.
///
/// A device that differs from the parent's is the cheap first test, so an
/// ordinary directory costs one `stat` of its parent and nothing else. The OS
/// mount table settles the rest: a macOS firmlink (`/Users` under `/`) crosses
/// from the system volume to the data volume with no mount at that path. A
/// parent that can't be read skips the shortcut and asks the table too, and a
/// table that can't be read answers "boundary": the caller is about to delete
/// on the answer, so it's never "no" without evidence.
///
/// ❗ Blocks on a `stat`, so an async caller runs it on the blocking pool.
pub(crate) fn is_mount_boundary(path: &Path, dev: u64) -> bool {
    use std::os::unix::fs::MetadataExt;

    // `/` has no parent to cross from.
    let Some(parent) = path.parent() else {
        return false;
    };
    // Following: the parent may be spelled through a link (`/tmp`), and what
    // holds `path` is the link's target.
    if std::fs::metadata(parent).is_ok_and(|parent_meta| parent_meta.dev() == dev) {
        return false;
    }
    // The table spells a mount point with its links resolved (`/private/tmp/…`).
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    super::transfer_sides::root_is_listed(&resolved).unwrap_or(true)
}

/// The first mount point at or under any of the directories `dirs`, spelled
/// under that directory, read from the OS mount table: the caller is about to
/// remove (or carry off) each whole tree, and the table answers without walking
/// one. A path that isn't a directory (or isn't there) holds no mount. A table
/// that can't be read answers the first directory itself, since the caller is
/// about to act on the answer.
///
/// ❗ Blocks on a `stat` per path, so an async caller runs it on the blocking pool.
pub(crate) fn mount_at_or_under<'a>(dirs: impl IntoIterator<Item = &'a Path>) -> Option<PathBuf> {
    let dirs: Vec<(&Path, PathBuf)> = dirs
        .into_iter()
        .filter(|dir| std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir()))
        // The table spells mount points with links resolved.
        .filter_map(|dir| dir.canonicalize().ok().map(|resolved| (dir, resolved)))
        .collect();
    let (first, _) = dirs.first()?;
    let Some(mount_points) = mount_points() else {
        return Some(first.to_path_buf());
    };
    dirs.iter().find_map(|(dir, resolved)| {
        mount_points
            .iter()
            .find_map(|mount| mount.strip_prefix(resolved).ok().map(|below| dir.join(below)))
    })
}

/// Every mount point in the OS mount table, or `None` when it can't be read.
fn mount_points() -> Option<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        cmdr_local::mount_table::enumerate_mounts()
            .map(|mounts| mounts.into_iter().map(|m| PathBuf::from(m.mount_point)).collect())
    }
    #[cfg(target_os = "linux")]
    {
        cmdr_local::linux_mounts::parse_proc_mounts()
            .map(|mounts| mounts.into_iter().map(|m| PathBuf::from(m.mountpoint)).collect())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Refuses an Overwrite that would remove the local folder `dest` while a drive
/// is mounted at or under it.
pub(crate) fn refuse_a_mount_inside(dest: &Path) -> Result<(), WriteOperationError> {
    refuse(mount_at_or_under([dest]), MountedVolumeAction::Replace)
}

/// Refuses a move onto another volume when any of its local `sources` is, or
/// holds, a mount point: the move copies each tree, then deletes what it
/// copied, which would carry the drive's files off it.
pub(crate) fn refuse_moving_a_mount(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    refuse(
        mount_at_or_under(sources.iter().map(PathBuf::as_path)),
        MountedVolumeAction::Move,
    )
}

/// Refuses a permanent delete of local `sources` when any of them is, or
/// holds, a mount point, read off the mount table right before the delete
/// phase removes anything. The walk's per-directory test ran at SCAN time,
/// which may be a preview from before the drive was mounted; the table also
/// lists a Linux bind mount, whose device matches its parent's.
pub(crate) fn refuse_deleting_a_mount(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    refuse(
        mount_at_or_under(sources.iter().map(PathBuf::as_path)),
        MountedVolumeAction::Delete,
    )
}

/// [`refuse_deleting_a_mount`] for a delete on a volume.
pub(crate) async fn refuse_deleting_a_volume_mount(
    volume: &dyn Volume,
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    let found = on_local_disk(volume, sources, state, |on_disk| {
        mount_at_or_under(on_disk.iter().map(PathBuf::as_path))
    })
    .await?;
    refuse(found, MountedVolumeAction::Delete)
}

/// [`is_mount_boundary`] for a directory the volume walker is about to descend.
pub(crate) async fn refuse_a_volume_mount_point(
    volume: &dyn Volume,
    path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    let found = on_local_disk(volume, &[path.to_path_buf()], state, |on_disk| {
        use std::os::unix::fs::MetadataExt;
        let on_disk = on_disk.into_iter().next()?;
        // A path that can't be stat'd isn't refused here: the listing that
        // follows reports it.
        let dev = std::fs::symlink_metadata(&on_disk).ok()?.dev();
        is_mount_boundary(&on_disk, dev).then_some(on_disk)
    })
    .await?;
    refuse(found, MountedVolumeAction::Delete)
}

/// [`refuse_a_mount_inside`] for a volume folder an Overwrite is about to set
/// aside and remove.
pub(crate) async fn refuse_a_volume_mount_inside(
    volume: &dyn Volume,
    path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    let found = on_local_disk(volume, &[path.to_path_buf()], state, |on_disk| {
        mount_at_or_under(on_disk.iter().map(PathBuf::as_path))
    })
    .await?;
    refuse(found, MountedVolumeAction::Replace)
}

/// [`refuse_moving_a_mount`] for a move off a volume.
pub(crate) async fn refuse_moving_a_volume_mount(
    volume: &dyn Volume,
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
) -> Result<(), WriteOperationError> {
    let found = on_local_disk(volume, sources, state, |on_disk| {
        mount_at_or_under(on_disk.iter().map(PathBuf::as_path))
    })
    .await?;
    refuse(found, MountedVolumeAction::Move)
}

/// Runs `find_mount` on the blocking pool over `paths`' spots on disk, for a
/// volume backed by a local folder (`Volume::local_path`): a drive or share
/// mounted inside a `LocalPosixVolume` pane's tree is reachable through it.
/// Other backends have no local path to ask about; what's mounted on a server
/// is the server's.
async fn on_local_disk(
    volume: &dyn Volume,
    paths: &[PathBuf],
    state: &Arc<WriteOperationState>,
    find_mount: fn(Vec<PathBuf>) -> Option<PathBuf>,
) -> Result<Option<PathBuf>, WriteOperationError> {
    let (Some(root), Some(first)) = (volume.local_path(), paths.first()) else {
        return Ok(None);
    };
    let on_disk: Vec<PathBuf> = paths
        .iter()
        .map(|path| cmdr_fs::volume::root_anchored(&root, path))
        .collect();
    state
        .scope()
        .host()
        .runtime()
        .spawn_blocking(move || find_mount(on_disk))
        .await
        .map_err(|e| WriteOperationError::IoError {
            path: first.display().to_string(),
            message: format!("the mount check didn't finish: {e}").into(),
        })
}

fn refuse(mount_point: Option<PathBuf>, action: MountedVolumeAction) -> Result<(), WriteOperationError> {
    match mount_point {
        Some(mount_point) => Err(refused_at(&mount_point, action)),
        None => Ok(()),
    }
}

/// The refusal naming `mount_point`, for what the operation would have done
/// to the drive there.
pub(crate) fn refused_at(mount_point: &Path, action: MountedVolumeAction) -> WriteOperationError {
    WriteOperationError::MountedVolumeInside {
        mount_point: mount_point.display().to_string(),
        action,
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "mount_boundary_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
#[path = "mount_boundary_tests.rs"]
mod tests;
