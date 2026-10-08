//! Following a mounted volume that was renamed (macOS).
//!
//! `NSWorkspaceDidRenameVolumeNotification` names the old and the new mount point, and the
//! observer in `watcher.rs` hands both here on a thread of their own. Why the volume moves rather
//! than leaving and arriving: `../file_system/volume/DETAILS.md` § "A renamed drive".

use log::debug;

use super::watcher::{handle_volume_mounted, handle_volume_unmounted};
use crate::file_system::volume::manager::RootMove;
use crate::volume_broadcast::{RootChangeKind, VolumeRootChanged};

/// Handle a rename notification: a mounted volume's mount point moved from `old_path` to
/// `new_path`, and the filesystem never left (macOS posts no unmount or mount around it).
///
/// The volume keeps its id, so the registry moves the root (`VolumeManager::move_root`), the
/// panes follow it (`volume-root-changed`, kind `Moved`, so a pane deep inside keeps its place),
/// and the drive's index restarts at the new root through the drive-release gate, like every
/// non-root start. ❌ Never model it as unmount + mount: that stops the index and sends every pane
/// on the drive home.
///
/// A volume whose id doesn't survive the rename (no UUID, so it keys on its path) IS a different
/// id afterwards, and for that one unmount + mount is the honest model.
///
/// Blocking (an NSURL read for the id, then the index's drain): the observer runs it on a thread
/// of its own. Public for tests so the handler logic can be exercised without posting real
/// `NSWorkspace` notifications.
pub(crate) fn handle_volume_renamed(old_path: &str, new_path: &str) {
    debug!("Volume renamed: {old_path} -> {new_path}");
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let keeps_its_id = manager
        .find_by_root(std::path::Path::new(old_path))
        .is_some_and(|(id, _)| super::volume_id_for_mount(new_path) == id);
    if !keeps_its_id {
        handle_volume_unmounted(old_path);
        handle_volume_mounted(new_path);
        return;
    }

    match manager.move_root(std::path::Path::new(old_path), std::path::Path::new(new_path)) {
        RootMove::Moved { id } => {
            crate::volume_broadcast::emit_volume_root_changed(VolumeRootChanged {
                volume_id: id.clone(),
                old_root: old_path.to_string(),
                new_root: new_path.to_string(),
                old_landing: old_path.to_string(),
                new_landing: new_path.to_string(),
                kind: RootChangeKind::Moved,
            });
            follow_with_the_index(&id);
        }
        RootMove::SiblingMoved { id } => debug!("{old_path} renamed; volume {id} still serves from its active root"),
        RootMove::BackendCantReroot { id } => log::warn!(
            target: "cmdr_lib::volumes",
            "{old_path} was renamed to {new_path}, and volume {id} can't move with it, so it stays on the old root"
        ),
        // Checked a moment ago, so only a concurrent unmount gets here: it's gone either way.
        RootMove::Unknown => handle_volume_mounted(new_path),
    }
    crate::volume_broadcast::emit_volumes_changed();
}

/// Restart a renamed drive's index at its new root, through the drive-release gate: an eject
/// landing beside the rename must not find a watcher this restart stood up behind its stop.
pub(super) fn follow_with_the_index(volume_id: &str) {
    use crate::file_system::volume::drive_release::{self, Gated, StartKind};

    let gated = drive_release::gate().start_blocking(volume_id, StartKind::DriveRenamed, || {
        crate::index_host::index().follow_volume_move(volume_id)
    });
    match gated {
        Gated::Ran(true) => log::info!(target: "cmdr_lib::volumes", "{volume_id}'s index follows it to its new name"),
        Gated::Ran(false) => debug!("{volume_id} was renamed with no index to follow it"),
        Gated::Skipped(reason) => {
            log::info!(target: "cmdr_lib::volumes", "{volume_id} was renamed but its index didn't follow: {reason:?}");
        }
    }
}
