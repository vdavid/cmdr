//! IPC pass-throughs for ADB devices.

use serde::{Deserialize, Serialize};

use cmdr_adb::{AdbConnectError, AdbDevice};

use super::volume_wiring::AdbInstallStatus;

/// Why a connect didn't produce a volume, as the frontend branches on it.
///
/// A typed mirror of `cmdr_adb::AdbConnectError`, ❌ never prose: the
/// frontend's own copy is what a person reads. The backend's diagnostic string
/// goes to the log.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AdbConnectOutcomeError {
    /// No `adb` binary on this machine, so no server could be started.
    AdbNotInstalled,
    /// The server socket couldn't be reached.
    ServerUnreachable,
    /// The device isn't attached (or vanished mid-connect).
    DeviceGone {
        /// The serial asked for.
        serial: String,
    },
    /// The phone hasn't accepted this computer's key; "Allow" on the device.
    Unauthorized {
        /// The serial asked for.
        serial: String,
    },
    /// The device's ADB predates `shell_v2` (Android 7).
    DeviceTooOld {
        /// The serial asked for.
        serial: String,
    },
    /// The connect ran past its budget.
    TimedOut,
    /// The user called it off.
    Cancelled,
    /// The transport refused or broke in a way none of the above names.
    Transport,
}

impl From<AdbConnectError> for AdbConnectOutcomeError {
    fn from(error: AdbConnectError) -> Self {
        match error {
            AdbConnectError::AdbNotInstalled => Self::AdbNotInstalled,
            AdbConnectError::ServerUnreachable(what) => {
                log::info!(target: "volume", "adb server unreachable: {what}");
                Self::ServerUnreachable
            }
            AdbConnectError::DeviceGone(serial) => Self::DeviceGone { serial },
            AdbConnectError::Unauthorized(serial) => Self::Unauthorized { serial },
            AdbConnectError::DeviceTooOld { serial } => Self::DeviceTooOld { serial },
            AdbConnectError::TimedOut => Self::TimedOut,
            AdbConnectError::Cancelled => Self::Cancelled,
            AdbConnectError::Transport(what) => {
                log::info!(target: "volume", "adb connect didn't come up: {what}");
                Self::Transport
            }
        }
    }
}

/// The ADB devices the server last reported, from the cache the tracker keeps.
#[tauri::command]
#[specta::specta]
pub async fn list_adb_devices() -> Vec<AdbDevice> {
    super::device_provider::cached_devices()
}

/// Where the `adb` binary is and whether the device list is live, as the
/// settings screen renders it. Reads what is already known; ❌ no re-check.
#[tauri::command]
#[specta::specta]
pub async fn get_adb_install_status() -> AdbInstallStatus {
    super::volume_wiring::adb_install_status()
}

/// Looks for `adb` again and restarts the device tracker if it turns up.
///
/// ❗ User-driven only ("I installed it now"): this is the one path allowed to
/// retry `adb start-server`, and a caller must not poll it.
#[tauri::command]
#[specta::specta]
pub async fn recheck_adb_install(app: tauri::AppHandle) -> AdbInstallStatus {
    super::volume_wiring::recheck_adb_install(&app).await
}

/// Dials the device with `serial` and answers its volume id.
#[tauri::command]
#[specta::specta]
pub async fn connect_adb_device(serial: String) -> Result<String, AdbConnectOutcomeError> {
    super::volume_wiring::connect_adb_device(&serial)
        .await
        .map_err(AdbConnectOutcomeError::from)
}
