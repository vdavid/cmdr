//! MTP volume wiring: turns "a storage attached" into a registered `MtpVolume`.
//!
//! This is the MTP twin of `network::smb_upgrade`, which registers `SmbVolume`
//! the same way. Both exist so a backend never registers itself: the wiring
//! knows both the backend and the volume registry, and neither of those knows
//! the wiring. New backends (FTP, S3, SFTP) copy this shape. The rule and its
//! rationale: `DETAILS.md` § "Backends never register themselves".

use crate::file_system::volume::MtpVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::mtp::connection::{MtpVolumeRegistrar, set_volume_registrar};
use log::debug;
use std::sync::Arc;

/// Teaches the MTP session layer how a storage becomes a volume.
///
/// Call once at startup, before anything can connect a device. Without it a
/// device connects and its storages never appear as volumes.
///
/// ❗ Both callbacks run synchronously inside the session layer, and must stay
/// that way. `connect()` attaches every storage before it starts the device's
/// event loop, and the loop's consumers (open listings, looked up by volume id,
/// and the per-volume index) route through the volume registry: an event
/// arriving ahead of the volumes has nothing to land on and the update is lost.
/// ❌ Never spawn from here, never make these async. See
/// `connection/volume_registrar.rs`.
pub(crate) fn install_volume_registrar() {
    set_volume_registrar(MtpVolumeRegistrar {
        attach: |device_id, storage_id, storage_name| {
            let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(device_id, storage_id);
            let volume = Arc::new(MtpVolume::new(device_id, storage_id, storage_name));
            get_volume_manager().register(&volume_id, volume);
            debug!("Registered MTP volume: {volume_id} ({storage_name})");
        },
        detach: |device_id, storage_id| {
            let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(device_id, storage_id);
            get_volume_manager().unregister(&volume_id);
            debug!("Unregistered MTP volume: {volume_id}");
        },
    });
}
