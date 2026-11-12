//! MTP volume wiring: turns "a storage attached" into a registered `MtpVolume`,
//! and answers the volume list as a `DeviceVolumeProvider`.
//!
//! This is the MTP twin of `network::smb_upgrade`, which registers `SmbVolume`
//! the same way. Both exist so a backend never registers itself: the wiring
//! knows both the backend and the volume registry, and neither of those knows
//! the wiring. New backends (FTP, S3, SFTP) copy this shape. The rule and its
//! rationale: `DETAILS.md` § "Backends never register themselves".

use std::sync::Arc;

use log::debug;

use crate::device_volumes::{DeviceVolumeEntry, DeviceVolumeProvider, ProviderFuture, register_device_provider};
use cmdr_fs::volume::DeviceReadiness;
use cmdr_mtp::MtpVolume;

use crate::file_system::volume::manager::get_volume_manager;
use crate::mtp::{MtpDeviceInfo, MtpVolumeRegistrar};

/// How this app turns an attached storage into a browsable volume.
///
/// Handed to `MtpConnectionManager::new` at startup. Without it a device
/// connects and its storages never appear as volumes.
///
/// ❗ Both callbacks run synchronously inside the session layer, and must stay
/// that way. `connect()` attaches every storage before it starts the device's
/// event loop, and the loop's consumers (open listings, looked up by volume id,
/// and the per-volume index) route through the volume registry: an event
/// arriving ahead of the volumes has nothing to land on and the update is lost.
/// ❌ Never spawn from here, never make these async. See
/// `connection/volume_registrar.rs`.
///
/// The `volumes-changed` broadcast rides along here, and this is the only place
/// MTP asks for one. Attaching and detaching a storage is exactly what changes
/// the volume list, so the two can't drift apart; the session layer, which knows
/// nothing about the app's volume list, would have to remember to ask at every
/// path that touches a storage, and one that forgot would leave the picker
/// showing a phone that has gone. `emit_volumes_changed` coalesces on a 150 ms
/// window, so a device arriving with four storages still costs one broadcast.
pub(crate) fn volume_registrar() -> MtpVolumeRegistrar {
    MtpVolumeRegistrar {
        attach: |manager, device_id, storage_id, storage_name| {
            let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(device_id, storage_id);
            let volume = Arc::new(MtpVolume::new(Arc::clone(manager), device_id, storage_id, storage_name));
            get_volume_manager().register(&volume_id, volume);
            debug!(
                "Registered MTP volume: {volume_id} (volumeName={storage_name:?})",
                storage_name = cmdr_fs::log_private::LogPrivate::volume(&storage_name)
            );
            crate::volume_broadcast::emit_volumes_changed();
        },
        detach: |device_id, storage_id| {
            let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(device_id, storage_id);
            get_volume_manager().unregister(&volume_id);
            debug!("Unregistered MTP volume: {volume_id}");
            crate::volume_broadcast::emit_volumes_changed();
        },
    }
}

// ============================================================================
// The device provider
// ============================================================================

/// What a device is called: its product, else its manufacturer.
fn device_name(device: &MtpDeviceInfo) -> &str {
    device
        .product
        .as_deref()
        .or(device.manufacturer.as_deref())
        .unwrap_or("Mobile device")
}

/// The row of a phone whose session hasn't opened (`auto_connect.rs`).
///
/// It stands for the device, since its storages aren't known yet. ❗ The id and path name no
/// storage on purpose. `waiting` fails `split_volume_id`'s numeric tail, so nothing can read this
/// row as a storage, and the path sits BESIDE the phone's `mtp://<device>/<storage>` rather than
/// over it, so `device_volume_for_path` can't resolve a remembered path on the phone to this row
/// and tell a pane the phone is here. Carries the serial so an unauthorized ADB row of the same
/// phone takes its name (`name_serial_only_entries`).
fn waiting_entry(device: &MtpDeviceInfo) -> DeviceVolumeEntry {
    DeviceVolumeEntry {
        id: format!("{}:waiting", device.id),
        name: device_name(device).to_string(),
        path: format!("mtp://{}/waiting", device.id),
        fs_type: "mtp",
        mount_is_read_only: false,
        device_readiness: Some(DeviceReadiness::Connecting),
        usb_speed: None,
        device_serial: device.serial_number.clone(),
    }
}

/// MTP's answer to `device_volumes::DeviceVolumeProvider`: the connected
/// devices' storages, from the connection manager's cached device list, plus a
/// row for each phone still connecting.
struct MtpDeviceProvider;

impl DeviceVolumeProvider for MtpDeviceProvider {
    fn id(&self) -> &'static str {
        "mtp"
    }

    /// Each storage becomes its own entry. One device with several storages
    /// spells each `"<device> - <storage>"`; a single-storage device is just
    /// the device name, because the storage name is noise the user didn't ask
    /// for ("Internal shared storage").
    fn entries(&self) -> ProviderFuture<'_, Vec<DeviceVolumeEntry>> {
        Box::pin(async {
            let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
            let mut entries = Vec::new();
            // ❗ Against the list above: the row can outlive a connect by a moment, since the
            // storages attach inside `connect()` and the row is settled right after it.
            let waiting: Vec<_> = crate::mtp::watcher::waiting_devices()
                .iter()
                .filter(|waiting| !devices.iter().any(|d| d.device.id == waiting.id))
                .map(waiting_entry)
                .collect();
            for device in devices {
                let multi = device.storages.len() > 1;
                let device_name = device_name(&device.device);
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
                        // Every storage listed here belongs to a device the
                        // session layer already has open, so there is nothing
                        // left to wait for.
                        device_readiness: Some(DeviceReadiness::Ready),
                        usb_speed: device.device.usb_speed,
                        device_serial: device.device.serial_number.clone(),
                    });
                }
            }
            entries.extend(waiting);
            entries
        })
    }

    /// MTP volume ids are shaped `{device_id}:{storage_id}`. Confirmed against
    /// the live device list so a future id containing a colon can't
    /// false-positive.
    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> ProviderFuture<'a, bool> {
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
    fn space_for_path<'a>(&'a self, path: &'a str) -> ProviderFuture<'a, Option<(u64, u64)>> {
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
    fn eject<'a>(&'a self, volume_id: &'a str) -> ProviderFuture<'a, Result<(), String>> {
        Box::pin(async move {
            // rsplit-based parse (identity) so a `:` inside a serial-based device
            // id doesn't truncate it: the storage id is the trailing numeric part.
            let device_id = cmdr_fs::volume::mtp_ids::device_id_of_volume(volume_id)
                .ok_or_else(|| format!("MTP volume id {volume_id} is missing a device prefix"))?;
            crate::mtp::connection_manager()
                .disconnect(device_id, crate::mtp::MtpDisconnectReason::User)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn phone() -> MtpDeviceInfo {
        MtpDeviceInfo {
            id: "mtp-testserial0001".to_string(),
            location_id: 1,
            vendor_id: 0x18d1,
            product_id: 0x4ee1,
            manufacturer: Some("Google".to_string()),
            product: Some("Pixel 9 Pro XL".to_string()),
            serial_number: Some("TESTSERIAL0001".to_string()),
            usb_speed: None,
        }
    }

    #[test]
    fn a_waiting_row_names_the_phone_and_says_it_is_connecting() {
        let entry = waiting_entry(&phone());
        assert_eq!(entry.name, "Pixel 9 Pro XL");
        assert_eq!(entry.fs_type, "mtp");
        assert_eq!(entry.device_readiness, Some(DeviceReadiness::Connecting));
        assert_eq!(entry.device_serial.as_deref(), Some("TESTSERIAL0001"));
    }

    #[test]
    fn a_waiting_row_falls_back_to_the_manufacturer_like_a_connected_one() {
        let device = MtpDeviceInfo {
            product: None,
            ..phone()
        };
        assert_eq!(waiting_entry(&device).name, "Google");
    }

    /// Nothing may read the row as a storage, or an eject or an index lookup finds a volume there.
    #[test]
    fn a_waiting_row_is_not_a_storage() {
        let entry = waiting_entry(&phone());
        assert_eq!(cmdr_fs::volume::mtp_ids::split_volume_id(&entry.id), None);
        assert!(!cmdr_fs::volume::mtp_ids::is_mtp_volume_id(&entry.id));
    }

    /// `device_volume_for_path` answers with the row a path falls under. Over the phone's own
    /// paths, the row would tell a restored tab that a phone with no session is here.
    #[test]
    fn a_waiting_row_does_not_claim_the_phones_paths() {
        let entry = waiting_entry(&phone());
        let on_the_phone = "mtp://mtp-testserial0001/65537/DCIM";
        assert!(!cmdr_fs::volume::app_paths::path_is_under(on_the_phone, &entry.path));
    }
}
