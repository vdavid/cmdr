//! ADB volume wiring: turns "the user opened this device" into a registered
//! `AdbVolume`, and keeps the cached device list following the server.
//!
//! ❗ **A backend never registers itself.** This module knows both the backend
//! and the volume registry, and neither of those knows this module: the same
//! shape `mtp::volume_wiring` and `network::sftp_volume_wiring` take.

use std::sync::{Arc, OnceLock};

use cmdr_adb::{AdbConnectError, AdbConnectionParams, AdbEndpoint, DeviceTracker};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use super::device_provider::{self, AdbDeviceProvider};
use crate::device_volumes::{notify_devices_changed, register_device_provider};
use crate::file_system::volume::manager::get_volume_manager;

/// The one `host:track-devices` subscription, alive for the process.
static TRACKER: OnceLock<DeviceTracker> = OnceLock::new();

/// Files ADB as a device provider. Call once at startup.
pub(crate) fn install_device_provider() {
    register_device_provider(Arc::new(AdbDeviceProvider));
}

/// Starts following the ADB server's device list. Once; a second call is a
/// no-op.
///
/// Talks only to the local server socket, never to USB. With no `adb`
/// installed the tracker logs at debug and idles, so nothing reaches the user
/// at startup.
pub fn start_adb_tracker(_app: &AppHandle) {
    if TRACKER.get().is_some() {
        return;
    }
    let tracker = cmdr_adb::track_devices(
        AdbEndpoint::default_local(),
        tauri::async_runtime::handle().inner().clone(),
        Arc::new(device_provider::apply_device_list),
    );
    if TRACKER.set(tracker).is_err() {
        log::debug!(target: "volume", "adb tracker already running");
    }
}

/// Dials the device with `serial`, registers its volume, and answers the volume
/// id. Already connected is answered without a second dial.
///
/// `register_if_absent`, never `register`: an ADB device has no OS mount, so
/// nothing else can pre-register its id, and a repeated connect must not retire
/// a volume the pane is using.
pub async fn connect_adb_device(serial: &str) -> Result<String, AdbConnectError> {
    if let Some(volume) = device_provider::connected_volume(serial) {
        return Ok(volume.volume_id().to_string());
    }
    let volume = cmdr_adb::connect_adb_volume(
        AdbConnectionParams::new(serial),
        crate::volume_host::host(),
        CancellationToken::new(),
    )
    .await?;
    let volume_id = volume.volume_id().to_string();
    let volume = Arc::new(volume);
    get_volume_manager().register_if_absent(&volume_id, Arc::clone(&volume) as Arc<dyn cmdr_fs::volume::Volume>);
    device_provider::remember_volume(serial, volume);
    log::info!(target: "volume", "registered ADB volume {volume_id}");
    notify_devices_changed("adb");
    Ok(volume_id)
}

/// The volume id for an `adb://<serial>[/…]` path, dialing the device on first
/// use. `None` for a path that isn't `adb://` at all.
pub async fn volume_id_for_path(path: &str) -> Option<Result<String, AdbConnectError>> {
    let serial = device_provider::serial_of_path(path)?;
    Some(connect_adb_device(serial).await)
}
