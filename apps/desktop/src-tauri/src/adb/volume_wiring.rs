//! ADB volume wiring: turns "the user opened this device" into a registered
//! `AdbVolume`, and keeps the cached device list following the server.
//!
//! ❗ **A backend never registers itself.** This module knows both the backend
//! and the volume registry, and neither of those knows this module: the same
//! shape `mtp::volume_wiring` and `network::sftp_volume_wiring` take.

use std::sync::{Arc, Mutex};

use cmdr_adb::{AdbConnectError, AdbConnectionParams, AdbEndpoint, DeviceTracker};
use cmdr_fs::ignore_poison::IgnorePoison;
use tauri::AppHandle;

use super::device_provider::{self, AdbDeviceProvider};
use crate::device_volumes::{notify_devices_changed, register_device_provider};
use crate::file_system::volume::manager::get_volume_manager;
use crate::network::connect_wiring::AttemptTable;

/// The `host:track-devices` subscription. Replaceable, not a `OnceLock`: the
/// tracker gives up when no `adb` binary exists, and a re-check has to be able
/// to put a fresh one in its place.
static TRACKER: Mutex<Option<DeviceTracker>> = Mutex::new(None);

/// Files ADB as a device provider. Call once at startup.
pub(crate) fn install_device_provider() {
    register_device_provider(Arc::new(AdbDeviceProvider));
}

/// Starts following the ADB server's device list. A second call while one is
/// already running is a no-op.
///
/// Talks only to the local server socket, never to USB. With no `adb`
/// installed the tracker stops itself and says so at debug, so nothing reaches
/// the user at startup and nothing retries for the rest of the session;
/// [`recheck_adb_install`] is how it comes back.
pub fn start_adb_tracker(_app: &AppHandle) {
    let mut slot = TRACKER.lock_ignore_poison();
    if slot.as_ref().is_some_and(DeviceTracker::is_running) {
        return;
    }
    *slot = Some(cmdr_adb::track_devices(
        AdbEndpoint::default_local(),
        tauri::async_runtime::handle().inner().clone(),
        Arc::new(device_provider::apply_device_list),
    ));
}

/// Where Cmdr found the `adb` binary, and whether it is following the server.
///
/// Both halves are what a settings screen renders: a path to show, and whether
/// the device list is live. `binary_path` is `None` exactly when
/// `AdbConnectError::AdbNotInstalled` is what a connect would answer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AdbInstallStatus {
    /// The `adb` binary in use, if one was found.
    pub binary_path: Option<String>,
    /// Whether the `host:track-devices` subscription is live.
    pub tracking: bool,
}

/// What Cmdr currently knows about the ADB install, without looking again.
pub fn adb_install_status() -> AdbInstallStatus {
    AdbInstallStatus {
        binary_path: cmdr_adb::locate_adb_binary().map(|p| p.display().to_string()),
        tracking: TRACKER
            .lock_ignore_poison()
            .as_ref()
            .is_some_and(DeviceTracker::is_running),
    }
}

/// Looks for `adb` again and restarts the tracker if it found one.
///
/// ❗ The one entry point allowed to retry `adb start-server`: it stands for a
/// person saying "I installed it now", so it is one attempt per click, never a
/// loop.
pub async fn recheck_adb_install(app: &AppHandle) -> AdbInstallStatus {
    cmdr_adb::forget_start_attempt().await;
    start_adb_tracker(app);
    adb_install_status()
}

// ============================================================================
// Calling a connect off
// ============================================================================

/// The dials a user could still call off. ADB's OWN table, never a shared one,
/// for the reason `network/connect_wiring.rs` gives: one table would let a stray
/// cancel from another backend's sign-in reach in here.
static ATTEMPTS: AttemptTable = AttemptTable::new("an adb");

/// Calls off the dial filed under `attempt_id`, answering whether one was
/// running. An id nobody is holding is a plain `false`.
pub fn cancel_connect(attempt_id: &str) -> bool {
    ATTEMPTS.cancel(attempt_id)
}

/// The id a navigation's own dial is filed under.
///
/// A pane that walks onto `adb://<serial>` dials without anyone having minted an
/// attempt id, and per-serial keeps a second navigation onto the same device
/// from cancelling the first: [`AttemptTable`]'s serial makes the repeat replace
/// only its own entry.
fn navigation_attempt_id(serial: &str) -> String {
    format!("adb-navigation:{serial}")
}

/// Dials the device with `serial`, registers its volume, and answers the volume
/// id. Already connected is answered without a second dial.
///
/// `attempt_id` is the CALLER's own name for this dial, and what
/// [`cancel_connect`] needs to call it off: a phone can sit on its "Allow USB
/// debugging?" prompt for as long as nobody picks it up, so the pane has to be
/// able to arm a cancel button before this answers.
pub async fn connect_adb_device(serial: &str, attempt_id: &str) -> Result<String, AdbConnectError> {
    connect_device_at(AdbConnectionParams::new(serial), attempt_id).await
}

/// The dial itself, against whichever server `params` names.
///
/// `register_if_absent`, never `register`: an ADB device has no OS mount, so
/// nothing else can pre-register its id, and a repeated connect must not retire
/// a volume the pane is using. ❗ A called-off dial leaves nothing behind: no
/// volume registered, nothing remembered, no `volumes-changed`.
pub(crate) async fn connect_device_at(
    params: AdbConnectionParams,
    attempt_id: &str,
) -> Result<String, AdbConnectError> {
    if let Some(volume) = device_provider::connected_volume(&params.serial) {
        return Ok(volume.volume_id().to_string());
    }
    let serial = params.serial.clone();
    let (cancel, _attempt) = ATTEMPTS.register(attempt_id);
    let volume = cmdr_adb::connect_adb_volume(params, crate::volume_host::host(), cancel).await?;
    let volume_id = volume.volume_id().to_string();
    let volume = Arc::new(volume);
    get_volume_manager().register_if_absent(&volume_id, Arc::clone(&volume) as Arc<dyn cmdr_fs::volume::Volume>);
    device_provider::remember_volume(&serial, volume);
    log::info!(target: "volume", "registered ADB volume {volume_id}");
    notify_devices_changed("adb");
    Ok(volume_id)
}

/// The volume id for an `adb://<serial>[/…]` path, dialing the device on first
/// use. `None` for a path that isn't `adb://` at all.
pub async fn volume_id_for_path(path: &str) -> Option<Result<String, AdbConnectError>> {
    let serial = device_provider::serial_of_path(path)?;
    Some(connect_adb_device(serial, &navigation_attempt_id(serial)).await)
}

#[cfg(test)]
#[path = "volume_wiring_test.rs"]
mod volume_wiring_test;
