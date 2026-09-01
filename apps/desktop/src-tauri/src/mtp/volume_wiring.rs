//! MTP volume wiring: turns "a storage attached" into a registered `MtpVolume`,
//! and answers the volume list as a `DeviceVolumeProvider`.
//!
//! This is the MTP twin of `network::smb_upgrade`, which registers `SmbVolume`
//! the same way. Both exist so a backend never registers itself: the wiring
//! knows both the backend and the volume registry, and neither of those knows
//! the wiring. New backends (FTP, S3, SFTP) copy this shape. The rule and its
//! rationale: `DETAILS.md` § "Backends never register themselves".

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use log::debug;

use crate::device_volumes::{DeviceVolumeEntry, DeviceVolumeProvider, register_device_provider};
use crate::file_system::volume::MtpVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::mtp::connection::{MtpVolumeRegistrar, set_volume_registrar};

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

// ============================================================================
// The device provider
// ============================================================================

/// MTP's answer to `device_volumes::DeviceVolumeProvider`: the connected
/// devices' storages, from the connection manager's cached device list.
struct MtpDeviceProvider;

impl DeviceVolumeProvider for MtpDeviceProvider {
    fn id(&self) -> &'static str {
        "mtp"
    }

    /// Each storage becomes its own entry. One device with several storages
    /// spells each `"<device> - <storage>"`; a single-storage device is just
    /// the device name, because the storage name is noise the user didn't ask
    /// for ("Internal shared storage").
    fn entries(&self) -> Pin<Box<dyn Future<Output = Vec<DeviceVolumeEntry>> + Send + '_>> {
        Box::pin(async {
            let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
            let mut entries = Vec::new();
            for device in devices {
                let multi = device.storages.len() > 1;
                let device_name = device
                    .device
                    .product
                    .as_deref()
                    .or(device.device.manufacturer.as_deref())
                    .unwrap_or("Mobile device");
                for storage in &device.storages {
                    let name = if multi {
                        format!("{} - {}", device_name, storage.name)
                    } else {
                        device_name.to_string()
                    };
                    entries.push(DeviceVolumeEntry {
                        id: format!("{}:{}", device.device.id, storage.id),
                        name,
                        path: format!("mtp://{}/{}", device.device.id, storage.id),
                        fs_type: "mtp",
                        mount_is_read_only: storage.is_read_only,
                        usb_speed: device.device.usb_speed,
                    });
                }
            }
            entries
        })
    }

    /// MTP volume ids are shaped `{device_id}:{storage_id}`. Confirmed against
    /// the live device list so a future id containing a colon can't
    /// false-positive.
    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let Some(device_id) = cmdr_fs::volume::mtp_ids::device_id_of_volume(volume_id) else {
                return false;
            };
            crate::mtp::connection_manager()
                .get_device_info(device_id)
                .await
                .is_some()
        })
    }

    /// Live space from an `mtp://{device_id}/{storage_id}/...` path.
    fn space_for_path<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<(u64, u64)>> + Send + 'a>> {
        Box::pin(async move {
            let rest = path.strip_prefix("mtp://")?;
            let mut parts = rest.splitn(3, '/');
            let device_id = parts.next()?;
            let storage_id: u32 = parts.next()?.parse().ok()?;
            crate::mtp::connection_manager()
                .get_live_storage_space(device_id, storage_id)
                .await
        })
    }

    /// Closes the device's MTP session. The `mtp-device-disconnected` event
    /// then removes every one of its storages from the picker.
    fn eject<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            // rsplit-based parse (identity) so a `:` inside a serial-based device
            // id doesn't truncate it: the storage id is the trailing numeric part.
            let device_id = cmdr_fs::volume::mtp_ids::device_id_of_volume(volume_id)
                .ok_or_else(|| format!("MTP volume id {volume_id} is missing a device prefix"))?;
            crate::mtp::connection_manager()
                .disconnect(
                    device_id,
                    None::<&tauri::AppHandle>,
                    crate::mtp::MtpDisconnectReason::User,
                )
                .await
                .map_err(|e| e.to_string())
        })
    }
}

/// Files MTP as a device provider, so the volume list, eject, and path
/// resolution see its storages. Call once at startup.
pub(crate) fn install_device_provider() {
    register_device_provider(Arc::new(MtpDeviceProvider));
}
