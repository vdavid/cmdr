//! Removing an item a reversal already verified as its operation's own, when
//! the item refuses: what a copy put on it from its source (Locked, a `deny
//! delete` ACL, SMB's Read-only) blocks even its owner's removal
//! (invariant 9, `transfer/volume/DETAILS.md` § "Copies keep the source's metadata").
//!
//! Every reversal that removes a copy goes through [`delete_clearing_blockers`]:
//! the journal rollback (`rollback.rs`) and the volume engine's in-flight one
//! (`transfer/volume/cleanup.rs`). A leaf, so both reach it without welding
//! the two into one module cycle.
//!
//! ❗ The clearing lives here, never in a volume's `delete`: the local volume's
//! delete is also F8's, which must never unlock the user's own files.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::metadata::{ParentOpening, Unblock, open_parent_folder};

use cmdr_fs::rt::Runtime as Handle;
use cmdr_fs::volume::{Volume, VolumeError};

/// What the item a reversal removes is: it decides `Volume::delete` or
/// `Volume::delete_directory`, so a folder's removal never reaches a file of
/// its name on an object store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeKind {
    File,
    Directory,
}

/// Why [`delete_clearing_blockers`] left the item where it is.
#[derive(Debug)]
pub(super) enum RemovalRefused {
    /// The volume refused it (gone, offline, a permission the clearing didn't
    /// touch).
    Volume(VolumeError),
    /// It wears a lock this process can't clear (`schg`, which only root can),
    /// or the clearing itself was refused.
    Locked,
}

impl From<VolumeError> for RemovalRefused {
    fn from(e: VolumeError) -> Self {
        Self::Volume(e)
    }
}

/// `volume.delete` (`delete_directory` for a folder), and when the item refuses with a permission refusal,
/// clearing what it wears (`Volume::clear_blockers`: Locked and the ACL on
/// local disk, Read-only on a share) before one retry. Then, on local disk,
/// opening the folder holding it (`open_parent_folder`: a copied folder carries
/// its source's Locked, `deny delete_child`, or `0500`) for one more, closing
/// it again right after.
///
/// A plain item costs exactly its delete: everything past it runs only after a
/// refusal. Call it only on an item the caller verified as its own. The folder
/// opening's syscalls run on `runtime`'s blocking pool (the engine's).
pub(super) async fn delete_clearing_blockers(
    runtime: &Handle,
    volume: &Arc<dyn Volume>,
    path: &Path,
    kind: NodeKind,
) -> Result<(), RemovalRefused> {
    let refused = match remove(volume, path, kind).await {
        Err(e @ VolumeError::PermissionDenied { .. }) => e,
        other => return Ok(other?),
    };
    let refused = match volume.clear_blockers(path).await {
        Unblock::Cleared(_) => match remove(volume, path, kind).await {
            Err(e @ VolumeError::PermissionDenied { .. }) => e,
            other => return Ok(other?),
        },
        Unblock::Refused => return Err(RemovalRefused::Locked),
        Unblock::NothingToClear => refused,
    };
    let Some(root) = volume.local_path() else {
        return Err(refused.into());
    };
    let local = cmdr_fs::volume::root_anchored(&root, path);
    // A few syscalls, but on a mounted share each one can block: off the runtime.
    match runtime.spawn_blocking(move || open_parent_folder(&local)).await {
        Ok(ParentOpening::Opened(folder)) => {
            let deleted = remove(volume, path, kind).await;
            // Dropping it closes it too, so even a failed join puts it back.
            if let Ok(report) = runtime.spawn_blocking(move || folder.close()).await
                && report.any_refused()
            {
                log::warn!(target: "rollback", "a folder opened for a removal didn't get all its metadata back: {report:?}");
            }
            Ok(deleted?)
        }
        Ok(ParentOpening::Refused) => Err(RemovalRefused::Locked),
        Ok(ParentOpening::NothingToOpen) | Err(_) => Err(refused.into()),
    }
}

/// One removal attempt of `path` as the `kind` it is.
async fn remove(volume: &Arc<dyn Volume>, path: &Path, kind: NodeKind) -> Result<(), VolumeError> {
    match kind {
        NodeKind::File => volume.delete(path).await,
        NodeKind::Directory => volume.delete_directory(path, None).await,
    }
}
