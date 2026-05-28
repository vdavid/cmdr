//! Eject command: unmounts ejectable volumes (USB, SD, DMG, SMB, MTP).
//!
//! Dispatches by volume kind:
//! - **MTP device** (volume ID shaped `{device_id}:{storage_id}`): closes the
//!   MTP session via `mtp::connection_manager().disconnect`. The
//!   `mtp-device-disconnected` event removes the volume from the picker.
//! - **SMB volume** (registered `SmbVolume` in `VolumeManager`): runs `diskutil
//!   unmount`. FSEvents fires `NSWorkspaceDidUnmount`, which calls
//!   `Volume::on_unmount` (drops the smb2 session, stops the watcher) and the
//!   volume manager unregisters it. Same pattern as `disconnect_smb_volume`.
//! - **Physical or disk-image volume** (NSURL reports `isEjectable`): runs
//!   `diskutil eject`. On USB drives this also powers the device down so it's
//!   safe to unplug; on DMG-mounted disk images, `eject` is the verb that
//!   detaches the image (`unmount` would leave it attached).
//!
//! Non-ejectable volumes return an error.

use std::time::Duration;

use crate::commands::util::{IpcError, blocking_result_with_timeout};

/// Action the eject pipeline takes for a given volume.
#[derive(Debug, PartialEq, Eq)]
pub enum EjectAction {
    /// Run `diskutil eject <mount_path>`. Powers down USB devices, detaches DMGs.
    DiskutilEject,
    /// Run `diskutil unmount <mount_path>`. SMB: FSEvents handles smb2 teardown.
    DiskutilUnmount,
    /// Close the MTP session for this device.
    MtpDisconnect { device_id: String },
}

/// Reasons `decide_eject_action` can't pick an action. Kept as a typed enum so
/// callers and tests classify the failure by variant instead of substring-
/// matching a free-form message.
#[derive(Debug, PartialEq, Eq)]
pub enum EjectDecisionError {
    /// MTP volume id is shaped wrong: missing the `{device_id}:{storage_id}`
    /// separator. Carries the bad id verbatim for diagnostics / UI rendering.
    MtpIdMissingDevicePrefix { volume_id: String },
    /// Volume can't be ejected (not SMB, not MTP, and NSURL/`/sys/block`
    /// reports `is_ejectable = false`). Typical for the boot volume or other
    /// internal disks.
    NotEjectable { volume_id: String },
}

impl std::fmt::Display for EjectDecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MtpIdMissingDevicePrefix { volume_id } => {
                write!(f, "MTP volume id {} is missing a device prefix", volume_id)
            }
            Self::NotEjectable { volume_id } => {
                write!(f, "Volume {} isn't ejectable", volume_id)
            }
        }
    }
}

impl std::error::Error for EjectDecisionError {}

/// Inputs the decision needs. Kept as primitives so the decision is a pure
/// function that can be tested without touching `VolumeManager` or the FS.
#[derive(Debug)]
pub struct EjectContext<'a> {
    pub volume_id: &'a str,
    /// NSURL-derived ejectability for physical/DMG volumes. Always `false` for
    /// SMB and MTP (those route via their own branches).
    pub is_ejectable: bool,
    /// True if this is an SMB volume (any state: Direct, OsMount, Disconnected).
    pub is_smb: bool,
    /// True if this is an MTP/mobile-device volume.
    pub is_mtp: bool,
}

/// Decides what to do for a given volume. Pure function; the impure parts
/// (looking up the volume, running `diskutil`, calling MTP disconnect) live
/// in the Tauri command wrapper.
pub fn decide_eject_action(ctx: &EjectContext) -> Result<EjectAction, EjectDecisionError> {
    if ctx.is_mtp {
        let device_id = ctx
            .volume_id
            .split_once(':')
            .map(|(d, _)| d.to_string())
            .ok_or_else(|| EjectDecisionError::MtpIdMissingDevicePrefix {
                volume_id: ctx.volume_id.to_string(),
            })?;
        return Ok(EjectAction::MtpDisconnect { device_id });
    }
    if ctx.is_smb {
        return Ok(EjectAction::DiskutilUnmount);
    }
    if ctx.is_ejectable {
        return Ok(EjectAction::DiskutilEject);
    }
    Err(EjectDecisionError::NotEjectable {
        volume_id: ctx.volume_id.to_string(),
    })
}

/// Ejects a volume. Picks the right teardown for the volume's kind.
///
/// Returns `Ok(())` once the unmount or disconnect is initiated. The frontend
/// shouldn't wait for the volume to fully disappear — `volume-unmounted` (for
/// disk volumes) or `mtp-device-disconnected` (for MTP) will fire shortly
/// after and panes rooted at the volume redirect to root.
#[tauri::command]
#[specta::specta]
pub async fn eject_volume(volume_id: String) -> Result<(), IpcError> {
    use crate::file_system::get_volume_manager;

    // MTP volumes use ID format `{device_id}:{storage_id}` and aren't
    // registered in VolumeManager; check the live MTP device list first.
    let is_mtp = is_mtp_volume_id(&volume_id).await;

    let (mount_path, is_smb) = if is_mtp {
        (String::new(), false)
    } else {
        let volume = get_volume_manager()
            .get(&volume_id)
            .ok_or_else(|| IpcError::from_err(format!("Volume not found: {}", volume_id)))?;
        let mount_path = volume.root().to_string_lossy().to_string();
        let is_smb = volume.smb_connection_state().is_some();
        (mount_path, is_smb)
    };

    // For physical volumes, ejectability comes from NSURL (macOS) /
    // `/sys/block/*/removable` (Linux). Look it up via the fast statfs-based
    // resolver instead of enumerating all volumes.
    let is_ejectable = if is_mtp || is_smb {
        false
    } else {
        resolve_is_ejectable(&mount_path).await
    };

    let action = decide_eject_action(&EjectContext {
        volume_id: &volume_id,
        is_ejectable,
        is_smb,
        is_mtp,
    })
    .map_err(|e| IpcError::from_err(e.to_string()))?;

    match action {
        EjectAction::MtpDisconnect { device_id } => mtp_disconnect(&device_id).await,
        EjectAction::DiskutilUnmount => diskutil_run("unmount", &mount_path).await,
        EjectAction::DiskutilEject => diskutil_run("eject", &mount_path).await,
    }
}

/// MTP volume IDs are shaped `{device_id}:{storage_id}` (see
/// `commands/volumes.rs::append_mtp_volumes`). Confirm against the live device
/// list so we don't false-positive on any future ID containing a colon.
async fn is_mtp_volume_id(volume_id: &str) -> bool {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let Some((device_id, _)) = volume_id.split_once(':') else {
            return false;
        };
        crate::mtp::connection_manager()
            .get_device_info(device_id)
            .await
            .is_some()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = volume_id;
        false
    }
}

/// Looks up `is_ejectable` for the volume at `mount_path` via the per-path
/// statfs/NSURL fast resolver. Avoids the full volume enumeration.
async fn resolve_is_ejectable(mount_path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        let path = mount_path.to_string();
        tokio::task::spawn_blocking(move || {
            crate::volumes::resolve_path_volume_fast(&path)
                .map(|v| v.is_ejectable)
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
    #[cfg(target_os = "linux")]
    {
        let path = mount_path.to_string();
        tokio::task::spawn_blocking(move || {
            crate::volumes_linux::list_mounted_volumes()
                .into_iter()
                .find(|v| v.path == path)
                .map(|v| v.is_ejectable)
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = mount_path;
        false
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn mtp_disconnect(device_id: &str) -> Result<(), IpcError> {
    crate::mtp::connection_manager()
        .disconnect(
            device_id,
            None::<&tauri::AppHandle>,
            crate::mtp::MtpDisconnectReason::User,
        )
        .await
        .map_err(|e| IpcError::from_err(format!("Couldn't disconnect {}: {}", device_id, e)))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn mtp_disconnect(_device_id: &str) -> Result<(), IpcError> {
    Err(IpcError::from_err("MTP not supported on this platform"))
}

#[cfg(target_os = "macos")]
async fn diskutil_run(verb: &'static str, mount_path: &str) -> Result<(), IpcError> {
    let path_for_cmd = mount_path.to_string();
    blocking_result_with_timeout(Duration::from_secs(15), move || {
        let output = std::process::Command::new("diskutil")
            .args([verb, &path_for_cmd])
            .output()
            .map_err(|e| format!("Couldn't run diskutil: {}", e))?;
        if output.status.success() {
            log::info!(target: "eject", "diskutil {} succeeded for {}", verb, path_for_cmd);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("diskutil {} failed: {}", verb, stderr.trim()))
        }
    })
    .await
}

#[cfg(not(target_os = "macos"))]
async fn diskutil_run(verb: &'static str, mount_path: &str) -> Result<(), IpcError> {
    // Linux: shell out to `umount`. The physical-drive eject UX is rare on
    // Linux dev machines; `umount` covers the SMB and removable cases.
    let path_for_cmd = mount_path.to_string();
    let _ = verb;
    blocking_result_with_timeout(Duration::from_secs(15), move || {
        let output = std::process::Command::new("umount")
            .arg(&path_for_cmd)
            .output()
            .map_err(|e| format!("Couldn't run umount: {}", e))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("umount failed: {}", stderr.trim()))
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_volume_routes_to_mtp_disconnect() {
        let ctx = EjectContext {
            volume_id: "usb1234:0x10001",
            is_ejectable: false,
            is_smb: false,
            is_mtp: true,
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap(),
            EjectAction::MtpDisconnect {
                device_id: "usb1234".to_string()
            }
        );
    }

    #[test]
    fn mtp_without_colon_errors() {
        let ctx = EjectContext {
            volume_id: "no-colon-id",
            is_ejectable: false,
            is_smb: false,
            is_mtp: true,
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap_err(),
            EjectDecisionError::MtpIdMissingDevicePrefix {
                volume_id: "no-colon-id".to_string(),
            }
        );
    }

    #[test]
    fn smb_volume_routes_to_unmount() {
        let ctx = EjectContext {
            volume_id: "smb-naspolya-445-public",
            is_ejectable: false,
            is_smb: true,
            is_mtp: false,
        };
        assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilUnmount);
    }

    #[test]
    fn ejectable_disk_routes_to_eject() {
        let ctx = EjectContext {
            volume_id: "volumes-usb-drive",
            is_ejectable: true,
            is_smb: false,
            is_mtp: false,
        };
        assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilEject);
    }

    #[test]
    fn non_ejectable_local_volume_errors() {
        let ctx = EjectContext {
            volume_id: "root",
            is_ejectable: false,
            is_smb: false,
            is_mtp: false,
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap_err(),
            EjectDecisionError::NotEjectable {
                volume_id: "root".to_string(),
            }
        );
    }

    #[test]
    fn smb_wins_over_ejectable_flag() {
        // Belt-and-braces: if anything ever sets is_ejectable on an SMB
        // volume, the SMB branch should still win so we run `unmount` (no
        // hardware to power down) instead of `eject`.
        let ctx = EjectContext {
            volume_id: "smb-foo",
            is_ejectable: true,
            is_smb: true,
            is_mtp: false,
        };
        assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilUnmount);
    }
}
