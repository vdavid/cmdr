//! The sides of a cross-volume move that sit on a local mount, and the question
//! its held sweeps ask before a delete: has one of them left the mount table?
//!
//! The local cross-FS move asks it through the operation's `TransferSides`
//! (`../move_op/cross_fs.rs`, `../move_op/source_sweep.rs`). The volume engine
//! carries none, because `state.sides` also homes its in-flight records, so it
//! keeps its own here, captured when the sweeps are set up. The rule and its
//! why: `../DETAILS.md` § Durability, "Cross-volume move".
//!
//! Only a side with a `local_path` that the mount table LISTED at capture is
//! watched (`TransferSide::at_start`). A side with no local path (direct SMB, MTP, S3, …) has no mount to
//! leave: its drop surfaces as a typed `VolumeError` from the call that met it.
//! A local volume rooted somewhere that isn't a mount point couldn't leave the
//! table either, and asking would read it as gone.

use cmdr_fs::volume::Volume;

use crate::transfer_sides::{TransferSide, TransferSides};
use crate::types::{TransferRole, WriteOperationError};

pub(super) struct LocalMounts {
    sides: TransferSides,
    watch_source: bool,
    watch_destination: bool,
}

impl LocalMounts {
    /// Both sides as they are now. `ids` are the REAL volume ids (the
    /// operation's journal volumes); without them (engine tests) a side is
    /// identified by its name.
    pub(super) fn capture(source: &dyn Volume, destination: &dyn Volume, ids: Option<&(String, String)>) -> Self {
        let (source, watch_source) = side(source, ids.map(|(id, _)| id));
        let (destination, watch_destination) = side(destination, ids.map(|(_, id)| id));
        Self {
            sides: TransferSides::new(source, destination),
            watch_source,
            watch_destination,
        }
    }

    /// The error to stop on when the mount table SAYS `role`'s drive has left,
    /// naming it. `None` while it's listed, for a side that isn't watched, and
    /// when the table can't be read: a guess never gates a delete.
    pub(super) fn left(&self, role: TransferRole) -> Option<WriteOperationError> {
        let watched = match role {
            TransferRole::Source => self.watch_source,
            TransferRole::Destination => self.watch_destination,
        };
        let side = self.sides.side(role);
        (watched && side.has_left()).then(|| WriteOperationError::DeviceDisconnected {
            path: side.root.display().to_string(),
            side: Some(self.sides.disconnected(role)),
        })
    }
}

/// One side, and whether it's a mount the table can watch.
fn side(volume: &dyn Volume, id: Option<&String>) -> (TransferSide, bool) {
    let id = id.cloned().unwrap_or_else(|| volume.name().to_string());
    let name = volume.name().to_string();
    match volume.local_path() {
        // Watched only if it's a mount the table listed (`TransferSide::at_start`).
        Some(mount) => (TransferSide::at_start(id, name, mount), true),
        None => (TransferSide::new(id, name, volume.root().to_path_buf()), false),
    }
}
