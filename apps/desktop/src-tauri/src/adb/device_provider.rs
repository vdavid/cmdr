//! The cached ADB device state and the `DeviceVolumeProvider` built on it.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, RwLock};

use cmdr_adb::{AdbDevice, AdbVolume};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::Volume;

use crate::device_volumes::{DeviceVolumeEntry, DeviceVolumeProvider, notify_devices_changed};
use crate::file_system::volume::manager::get_volume_manager;

/// What the app knows about ADB devices without asking the server: the list
/// the tracker last pushed, and the volumes that have been dialed.
#[derive(Default)]
pub(crate) struct AdbDevices {
    /// The device list as `host:track-devices` last delivered it.
    devices: Vec<AdbDevice>,
    /// The connected volumes, by serial.
    volumes: HashMap<String, Arc<AdbVolume>>,
}

static STATE: LazyLock<RwLock<AdbDevices>> = LazyLock::new(|| RwLock::new(AdbDevices::default()));

/// The cached device list.
pub(crate) fn cached_devices() -> Vec<AdbDevice> {
    STATE.read_ignore_poison().devices.clone()
}

/// The connected volume for `serial`, if it has been dialed.
pub(crate) fn connected_volume(serial: &str) -> Option<Arc<AdbVolume>> {
    STATE.read_ignore_poison().volumes.get(serial).cloned()
}

/// Files a freshly dialed volume under its serial.
pub(crate) fn remember_volume(serial: &str, volume: Arc<AdbVolume>) {
    STATE.write_ignore_poison().volumes.insert(serial.to_string(), volume);
}

/// Forgets the volume for `serial`, handing it back so the caller can retire it.
pub(crate) fn forget_volume(serial: &str) -> Option<Arc<AdbVolume>> {
    STATE.write_ignore_poison().volumes.remove(serial)
}

/// Stores a tracker push and retires the volume of every serial that left.
///
/// ❗ Synchronous on purpose: it runs inside the tracker's callback, and the
/// registry is sync. A volume that lost its device is told so
/// (`note_device_gone`) and unregistered, which is what retires it.
pub(crate) fn apply_device_list(devices: Vec<AdbDevice>) {
    let gone: Vec<(String, Arc<AdbVolume>)> = {
        let mut state = STATE.write_ignore_poison();
        state.devices = devices;
        let still_here: Vec<&str> = state.devices.iter().map(|d| d.serial.as_str()).collect();
        let gone_serials: Vec<String> = state
            .volumes
            .keys()
            .filter(|serial| !still_here.contains(&serial.as_str()))
            .cloned()
            .collect();
        gone_serials
            .into_iter()
            .filter_map(|serial| state.volumes.remove(&serial).map(|v| (serial, v)))
            .collect()
    };
    for (serial, volume) in gone {
        log::info!(target: "volume", "adb device {serial} left; retiring its volume");
        volume.note_device_gone();
        get_volume_manager().unregister(volume.volume_id());
    }
    notify_devices_changed("adb");
}

/// The `adb://<serial>` root a pane navigates to.
pub(crate) fn device_path(serial: &str) -> String {
    format!("adb://{serial}")
}

/// The serial an `adb://<serial>[/…]` path names.
pub(crate) fn serial_of_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("adb://")?;
    let serial = rest.split('/').next()?;
    (!serial.is_empty()).then_some(serial)
}

/// ADB's answer to `device_volumes::DeviceVolumeProvider`.
pub(crate) struct AdbDeviceProvider;

impl DeviceVolumeProvider for AdbDeviceProvider {
    fn id(&self) -> &'static str {
        "adb"
    }

    /// One entry per `Ready` device, dialed or not: a device with no volume yet
    /// is listed so the user can click it, and the first `adb://` navigation
    /// connects it (`commands/volumes.rs`).
    ///
    /// Follow-up: an " (ADB)" suffix when an MTP entry shares the name.
    fn entries(&self) -> Pin<Box<dyn Future<Output = Vec<DeviceVolumeEntry>> + Send + '_>> {
        Box::pin(async {
            cached_devices()
                .iter()
                .filter(|d| d.is_ready())
                .map(|d| DeviceVolumeEntry {
                    id: cmdr_fs::volume::adb_volume_id(&d.serial),
                    name: d.display_name(),
                    path: device_path(&d.serial),
                    fs_type: "adb",
                    mount_is_read_only: false,
                    usb_speed: None,
                })
                .collect()
        })
    }

    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            cached_devices()
                .iter()
                .any(|d| cmdr_fs::volume::adb_volume_id(&d.serial) == volume_id)
        })
    }

    /// Live space from the connected volume; `None` until it's dialed.
    fn space_for_path<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Option<(u64, u64)>> + Send + 'a>> {
        Box::pin(async move {
            let volume = connected_volume(serial_of_path(path)?)?;
            let space = volume.get_space_info().await.ok()?;
            Some((space.total_bytes, space.available_bytes))
        })
    }

    /// Retires the volume. ❗ Nothing is detached: `adb` has no per-client
    /// detach, so the device stays in the server's list and is listed again on
    /// the next `volumes-changed`.
    fn eject<'a>(&'a self, volume_id: &'a str) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let serial = cached_devices()
                .into_iter()
                .find(|d| cmdr_fs::volume::adb_volume_id(&d.serial) == volume_id)
                .map(|d| d.serial)
                .ok_or_else(|| format!("no adb device owns volume {volume_id}"))?;
            if forget_volume(&serial).is_some() {
                get_volume_manager().unregister(volume_id);
            }
            notify_devices_changed("adb");
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_of_path_reads_the_host_segment_only() {
        assert_eq!(serial_of_path("adb://ZY22ABC"), Some("ZY22ABC"));
        assert_eq!(serial_of_path("adb://ZY22ABC/sdcard/DCIM"), Some("ZY22ABC"));
        assert_eq!(serial_of_path("adb://"), None);
        assert_eq!(serial_of_path("mtp://dev/1"), None);
    }
}
