//! The cached ADB device state and the `DeviceVolumeProvider` built on it.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use cmdr_adb::{AdbDevice, AdbDeviceState, AdbVolume};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::{DeviceReadiness, DeviceUnavailableReason, Volume};

use crate::device_volumes::{DeviceVolumeEntry, DeviceVolumeProvider, ProviderFuture, notify_devices_changed};
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

/// The `adb://<serial>` root a pane navigates to: the dialed volume's own
/// `root()`, minted in one place so the row and the volume can't disagree.
pub(crate) fn device_path(serial: &str) -> String {
    cmdr_fs::volume::adb_app_root(serial)
}

/// The serial an `adb://<serial>[/…]` path names.
pub(crate) fn serial_of_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("adb://")?;
    let serial = rest.split('/').next()?;
    (!serial.is_empty()).then_some(serial)
}

/// How ready a device in `state` is, or `None` for one that has no filesystem
/// to offer and must not become a row at all.
///
/// ❗ `recovery`, `bootloader`, and `sideload` are a phone that isn't running
/// Android, and `Unknown` is a state word this crate can't read: listing either
/// would put a row on screen that can never open. Everything else IS listed,
/// because the states that need the user (the "Allow USB debugging?" tap) are
/// exactly the ones a hidden row would leave silent.
fn readiness_of(state: AdbDeviceState) -> Option<DeviceReadiness> {
    match state {
        AdbDeviceState::Ready => Some(DeviceReadiness::Ready),
        // A TCP device mid-handshake and an RSA handshake in flight both end at
        // the same place the user is looking: the phone's own prompt.
        AdbDeviceState::Unauthorized | AdbDeviceState::Authorizing | AdbDeviceState::Connecting => {
            Some(DeviceReadiness::WaitingForAuthorization)
        }
        AdbDeviceState::Offline => Some(DeviceReadiness::Unavailable {
            reason: DeviceUnavailableReason::Offline,
        }),
        AdbDeviceState::NoPermissions => Some(DeviceReadiness::Unavailable {
            reason: DeviceUnavailableReason::NoPermissions,
        }),
        AdbDeviceState::Recovery | AdbDeviceState::Bootloader | AdbDeviceState::Sideload | AdbDeviceState::Unknown => {
            None
        }
    }
}

/// One entry per device that has a filesystem to offer, dialed or not.
///
/// Pure on purpose: the provider's own state is process-wide, and the mapping
/// is what the cells assert on.
fn entries_for(devices: &[AdbDevice]) -> Vec<DeviceVolumeEntry> {
    devices
        .iter()
        .filter_map(|d| {
            Some(DeviceVolumeEntry {
                id: cmdr_fs::volume::adb_volume_id(&d.serial),
                name: d.display_name(),
                path: device_path(&d.serial),
                fs_type: "adb",
                mount_is_read_only: false,
                device_readiness: Some(readiness_of(d.state)?),
                usb_speed: None,
            })
        })
        .collect()
}

/// ADB's answer to `device_volumes::DeviceVolumeProvider`.
pub(crate) struct AdbDeviceProvider;

impl DeviceVolumeProvider for AdbDeviceProvider {
    fn id(&self) -> &'static str {
        "adb"
    }

    /// One entry per device with a filesystem to offer, dialed or not and
    /// whatever it is waiting for: a device with no volume yet is listed so the
    /// user can click it, and the first `adb://` navigation connects it
    /// (`commands/volumes.rs`). [`readiness_of`] says which states are listed.
    ///
    /// Follow-up: an " (ADB)" suffix when an MTP entry shares the name.
    fn entries(&self) -> ProviderFuture<'_, Vec<DeviceVolumeEntry>> {
        Box::pin(async { entries_for(&cached_devices()) })
    }

    fn owns_volume_id<'a>(&'a self, volume_id: &'a str) -> ProviderFuture<'a, bool> {
        Box::pin(async move {
            cached_devices()
                .iter()
                .any(|d| cmdr_fs::volume::adb_volume_id(&d.serial) == volume_id)
        })
    }

    /// Live space from the connected volume; `None` until it's dialed.
    fn space_for_path<'a>(&'a self, path: &'a str) -> ProviderFuture<'a, Option<(u64, u64)>> {
        Box::pin(async move {
            let volume = connected_volume(serial_of_path(path)?)?;
            let space = volume.get_space_info().await.ok()?;
            Some((space.total_bytes()?, space.available_bytes()?))
        })
    }

    /// Retires the volume. ❗ Nothing is detached: `adb` has no per-client
    /// detach, so the device stays in the server's list and is listed again on
    /// the next `volumes-changed`.
    fn eject<'a>(&'a self, volume_id: &'a str) -> ProviderFuture<'a, Result<(), String>> {
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

    fn device(serial: &str, state: AdbDeviceState) -> AdbDevice {
        AdbDevice {
            serial: serial.to_string(),
            state,
            product: None,
            model: None,
            device: None,
            transport_id: None,
        }
    }

    #[test]
    fn a_ready_device_is_listed_ready() {
        let entries = entries_for(&[device("R58M1", AdbDeviceState::Ready)]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].device_readiness, Some(DeviceReadiness::Ready));
        assert_eq!(entries[0].path, "adb://R58M1");
        assert_eq!(entries[0].id, cmdr_fs::volume::adb_volume_id("R58M1"));
    }

    #[test]
    fn a_phone_still_owing_its_allow_tap_is_listed_and_waiting() {
        for state in [
            AdbDeviceState::Unauthorized,
            AdbDeviceState::Authorizing,
            AdbDeviceState::Connecting,
        ] {
            let entries = entries_for(&[device("R58M1", state)]);
            assert_eq!(
                entries.first().map(|e| e.device_readiness),
                Some(Some(DeviceReadiness::WaitingForAuthorization)),
                "{state:?} is the moment the user has to act, so the row has to be there"
            );
        }
    }

    #[test]
    fn a_device_the_daemon_cant_use_is_listed_with_its_reason() {
        let offline = entries_for(&[device("R58M1", AdbDeviceState::Offline)]);
        assert_eq!(
            offline.first().map(|e| e.device_readiness),
            Some(Some(DeviceReadiness::Unavailable {
                reason: DeviceUnavailableReason::Offline
            }))
        );
        let no_permissions = entries_for(&[device("R58M1", AdbDeviceState::NoPermissions)]);
        assert_eq!(
            no_permissions.first().map(|e| e.device_readiness),
            Some(Some(DeviceReadiness::Unavailable {
                reason: DeviceUnavailableReason::NoPermissions
            }))
        );
    }

    #[test]
    fn a_phone_that_is_not_running_android_is_not_a_row() {
        for state in [
            AdbDeviceState::Recovery,
            AdbDeviceState::Bootloader,
            AdbDeviceState::Sideload,
            AdbDeviceState::Unknown,
        ] {
            assert!(
                entries_for(&[device("R58M1", state)]).is_empty(),
                "{state:?} has no filesystem to offer, so it must not become a row"
            );
        }
    }

    #[test]
    fn serial_of_path_reads_the_host_segment_only() {
        assert_eq!(serial_of_path("adb://ZY22ABC"), Some("ZY22ABC"));
        assert_eq!(serial_of_path("adb://ZY22ABC/sdcard/DCIM"), Some("ZY22ABC"));
        assert_eq!(serial_of_path("adb://"), None);
        assert_eq!(serial_of_path("mtp://dev/1"), None);
    }
}
