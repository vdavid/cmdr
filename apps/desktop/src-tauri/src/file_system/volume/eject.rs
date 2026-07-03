//! Volume eject: unmounts ejectable volumes (USB, SD, DMG, SMB, MTP).
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
//!
//! The `commands::eject` IPC layer is a thin delegate over [`eject`]: it maps the
//! typed [`EjectError`] to the wire `IpcError` (including the timeout flag).

use std::time::Duration;

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
/// in [`eject`].
pub fn decide_eject_action(ctx: &EjectContext) -> Result<EjectAction, EjectDecisionError> {
    if ctx.is_mtp {
        // rsplit-based parse (identity) so a `:` inside a serial-based device id
        // doesn't truncate it: the storage id is the trailing numeric component.
        let device_id = crate::mtp::identity::device_id_of_volume(ctx.volume_id)
            .map(str::to_string)
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

/// Errors from the eject pipeline. Typed variants so the command layer maps each
/// to `IpcError` — and distinguishes a genuine subprocess timeout — without
/// string-matching (`no-string-matching` rule).
#[derive(Debug)]
pub enum EjectError {
    /// A write op is reading from or writing to this volume; refuse to tear it
    /// down mid-transfer.
    Busy,
    /// `volume_id` isn't registered in `VolumeManager` (a race: unmounted mid-op).
    VolumeNotFound { volume_id: String },
    /// The kind dispatch couldn't pick an action.
    Decision(EjectDecisionError),
    /// The MTP disconnect, `diskutil`, or `umount` call failed. Carries the
    /// already-formatted, user-facing reason.
    Failed(String),
    /// The `diskutil` / `umount` subprocess didn't finish within the timeout.
    TimedOut,
}

impl std::fmt::Display for EjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // The picker disables Eject for busy volumes, so reaching here means a
            // race (or an MCP / automation caller); the message tells the user to
            // retry once the transfer finishes.
            Self::Busy => write!(
                f,
                "operations are in progress on this device. Eject again once they finish"
            ),
            Self::VolumeNotFound { volume_id } => write!(f, "Volume not found: {}", volume_id),
            Self::Decision(e) => write!(f, "{}", e),
            Self::Failed(msg) => write!(f, "{}", msg),
            Self::TimedOut => write!(f, "Eject timed out (the volume may be slow or unresponsive)"),
        }
    }
}

impl std::error::Error for EjectError {}

/// Ejects a volume. Picks the right teardown for the volume's kind.
///
/// Returns `Ok(())` once the unmount or disconnect is initiated. The frontend
/// shouldn't wait for the volume to fully disappear — `volume-unmounted` (for
/// disk volumes) or `mtp-device-disconnected` (for MTP) will fire shortly
/// after and panes rooted at the volume redirect to root.
pub async fn eject(volume_id: &str) -> Result<(), EjectError> {
    use crate::file_system::get_volume_manager;

    // Safety gate: never tear down a volume while a write op is reading from or
    // writing to it. The picker disables Eject for busy volumes, so reaching
    // here means a race (or an MCP / automation caller); refuse rather than
    // disconnect mid-transfer and risk a truncated file. See the volume picker's
    // `volumes-busy-changed` wiring.
    if crate::file_system::busy_volume_ids().iter().any(|id| id == volume_id) {
        return Err(EjectError::Busy);
    }

    // MTP volumes use ID format `{device_id}:{storage_id}` and aren't
    // registered in VolumeManager; check the live MTP device list first.
    let is_mtp = is_mtp_volume_id(volume_id).await;

    let (mount_path, is_smb) = if is_mtp {
        (String::new(), false)
    } else {
        let volume = get_volume_manager()
            .get(volume_id)
            .ok_or_else(|| EjectError::VolumeNotFound {
                volume_id: volume_id.to_string(),
            })?;
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
        volume_id,
        is_ejectable,
        is_smb,
        is_mtp,
    })
    .map_err(EjectError::Decision)?;

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
    let Some(device_id) = crate::mtp::identity::device_id_of_volume(volume_id) else {
        return false;
    };
    crate::mtp::connection_manager()
        .get_device_info(device_id)
        .await
        .is_some()
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
}

async fn mtp_disconnect(device_id: &str) -> Result<(), EjectError> {
    crate::mtp::connection_manager()
        .disconnect(
            device_id,
            None::<&tauri::AppHandle>,
            crate::mtp::MtpDisconnectReason::User,
        )
        .await
        .map_err(|e| EjectError::Failed(format!("Couldn't disconnect {}: {}", device_id, e)))
}

/// Runs a blocking eject subprocess with a 15 s timeout, mapping the outcome to
/// [`EjectError`] (a real timeout becomes [`EjectError::TimedOut`] so the wire
/// error carries the timeout flag).
async fn run_eject_subprocess(
    timeout: Duration,
    f: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), EjectError> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(e))) => Err(EjectError::Failed(e)),
        Ok(Err(join_err)) => Err(EjectError::Failed(join_err.to_string())),
        Err(_elapsed) => Err(EjectError::TimedOut),
    }
}

#[cfg(target_os = "macos")]
async fn diskutil_run(verb: &'static str, mount_path: &str) -> Result<(), EjectError> {
    let path_for_cmd = mount_path.to_string();
    run_eject_subprocess(Duration::from_secs(15), move || {
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

#[cfg(target_os = "linux")]
async fn diskutil_run(verb: &'static str, mount_path: &str) -> Result<(), EjectError> {
    // Linux: shell out to `umount`. The physical-drive eject UX is rare on
    // Linux dev machines; `umount` covers the SMB and removable cases.
    let path_for_cmd = mount_path.to_string();
    let _ = verb;
    run_eject_subprocess(Duration::from_secs(15), move || {
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
            volume_id: "mtp-336592896:65537",
            is_ejectable: false,
            is_smb: false,
            is_mtp: true,
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap(),
            EjectAction::MtpDisconnect {
                device_id: "mtp-336592896".to_string()
            }
        );
    }

    #[test]
    fn mtp_volume_with_colon_in_serial_keeps_the_full_device_id() {
        // Regression for the identity fix: a serial-based device id can contain a
        // `:`. The rsplit-based parse must keep the WHOLE device id, splitting
        // only on the trailing numeric storage id.
        let ctx = EjectContext {
            volume_id: "mtp-AA:BB:CC:65537",
            is_ejectable: false,
            is_smb: false,
            is_mtp: true,
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap(),
            EjectAction::MtpDisconnect {
                device_id: "mtp-AA:BB:CC".to_string()
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
