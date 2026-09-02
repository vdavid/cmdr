//! Volume eject: unmounts ejectable volumes (USB, SD, DMG, SMB, MTP, ADB).
//!
//! Dispatches by volume kind:
//! - **Device volume** (a `device_volumes::DeviceVolumeProvider` owns the id):
//!   hands the eject to that provider. MTP closes the device session and the
//!   `mtp-device-disconnected` event removes its storages from the picker; ADB
//!   has nothing to detach (`adb` has no per-client detach), so it retires the
//!   volume and hides the device until it re-enumerates.
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
//! The `commands::eject` IPC layer is a thin delegate over [`eject`]: [`EjectError`]
//! IS the wire type, so nothing is flattened on the way out.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Action the eject pipeline takes for a given volume.
#[derive(Debug, PartialEq, Eq)]
pub enum EjectAction {
    /// Run `diskutil eject <mount_path>`. Powers down USB devices, detaches DMGs.
    DiskutilEject,
    /// Run `diskutil unmount <mount_path>`. SMB: FSEvents handles smb2 teardown.
    DiskutilUnmount,
    /// Hand the eject to the device provider that owns the volume.
    DeviceDisconnect { provider: &'static str, volume_id: String },
}

/// Reasons `decide_eject_action` can't pick an action. Kept as a typed enum so
/// callers and tests classify the failure by variant instead of substring-
/// matching a free-form message.
#[derive(Debug, PartialEq, Eq)]
pub enum EjectDecisionError {
    /// Volume can't be ejected (not SMB, not a device, and NSURL/`/sys/block`
    /// reports `is_ejectable = false`). Typical for the boot volume or other
    /// internal disks.
    NotEjectable { volume_id: String },
}

impl std::fmt::Display for EjectDecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
    /// SMB and device volumes (those route via their own branches).
    pub is_ejectable: bool,
    /// True if this is an SMB volume (any state: Direct, OsMount, Disconnected).
    pub is_smb: bool,
    /// The device provider (`"mtp"`, `"adb"`) that owns this volume, if one does.
    pub device_provider: Option<&'static str>,
}

/// Decides what to do for a given volume. Pure function; the impure parts
/// (looking up the volume, running `diskutil`, calling the provider's eject)
/// live in [`eject`].
pub fn decide_eject_action(ctx: &EjectContext) -> Result<EjectAction, EjectDecisionError> {
    if let Some(provider) = ctx.device_provider {
        return Ok(EjectAction::DeviceDisconnect {
            provider,
            volume_id: ctx.volume_id.to_string(),
        });
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

/// Why an eject or an SMB disconnect didn't happen, as a value rather than a
/// sentence.
///
/// ❌ **Nothing in this enum is prose a user reads.** It IS the wire type: the
/// frontend renders every word from the typed variant through the
/// `errors.eject.*` catalog in nine locales
/// (`src/lib/file-explorer/eject-error-messages.ts`). The `detail` fields carry
/// `diskutil`'s own stderr, which says useful non-enumerable things ("in use by
/// process 1234 (mds)"); they render as technical detail beside the message,
/// ❌ never as the message. Same split as `MutationError` on the write path;
/// `docs/guides/error-handling.md` is the map.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EjectError {
    /// A write op is reading from or writing to this volume; refuse to tear it
    /// down mid-transfer. The picker disables Eject for busy volumes, so
    /// reaching here means a race (or an MCP / automation caller).
    Busy,
    /// `volume_id` isn't registered in `VolumeManager` (a race: unmounted mid-op).
    VolumeNotFound {
        /// The id that no longer resolves.
        volume_id: String,
    },
    /// The volume can't be ejected at all: not SMB, not a device, and the OS reports
    /// it as fixed. Typical for the boot volume and other internal disks.
    NotEjectable {
        /// The volume asked about.
        volume_id: String,
    },
    /// Disconnect was asked of a volume that isn't a network share. The UI only
    /// offers Disconnect for SMB volumes, so this is a race or an automation
    /// caller.
    NotAnSmbVolume {
        /// The volume asked about.
        volume_id: String,
    },
    /// The device provider wouldn't retire the volume (MTP: the device wouldn't
    /// close its session).
    DeviceDisconnectRefused {
        /// Which provider refused (`"mtp"`, `"adb"`).
        provider: String,
        /// What the provider reported, for the log and the details line.
        detail: String,
    },
    /// `diskutil` / `umount` turned the unmount down. The overwhelmingly common
    /// case is an open file somewhere, and `detail` usually names the process.
    UnmountRefused {
        /// The tool's own stderr, for the log and the details line.
        detail: String,
    },
    /// The `diskutil` / `umount` subprocess didn't finish within the timeout.
    /// ❗ The unmount was NOT cancelled; it may still land.
    TimedOut,
    /// The one honest fallback, for a failure nothing above classifies (a
    /// panicked task). ❌ `detail` is never the message.
    Unexpected {
        /// What the layer below reported, for the log and the details line.
        detail: String,
    },
}

impl std::fmt::Display for EjectError {
    /// ❗ For logs, MCP replies, and debugging only; every user-facing word
    /// comes from the typed variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("operations are in progress on this device"),
            Self::VolumeNotFound { volume_id } => write!(f, "volume not found: {volume_id}"),
            Self::NotEjectable { volume_id } => write!(f, "volume {volume_id} isn't ejectable"),
            Self::NotAnSmbVolume { volume_id } => write!(f, "volume {volume_id} isn't an SMB volume"),
            Self::DeviceDisconnectRefused { provider, detail } => {
                write!(f, "{provider} disconnect refused: {detail}")
            }
            Self::UnmountRefused { detail } => write!(f, "unmount refused: {detail}"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for EjectError {}

impl From<EjectDecisionError> for EjectError {
    fn from(error: EjectDecisionError) -> Self {
        match error {
            EjectDecisionError::NotEjectable { volume_id } => Self::NotEjectable { volume_id },
        }
    }
}

/// Ejects a volume. Picks the right teardown for the volume's kind.
///
/// Returns `Ok(())` once the unmount or disconnect is initiated. The frontend
/// shouldn't wait for the volume to fully disappear — `volume-unmounted` (for
/// disk volumes) or `mtp-device-disconnected` (for MTP) will fire shortly
/// after and panes rooted at the volume redirect to root.
pub async fn eject(volume_id: &str) -> Result<(), EjectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    // Safety gate: never tear down a volume while a write op is reading from or
    // writing to it. The picker disables Eject for busy volumes, so reaching
    // here means a race (or an MCP / automation caller); refuse rather than
    // disconnect mid-transfer and risk a truncated file. See the volume picker's
    // `volumes-busy-changed` wiring.
    if crate::file_system::busy_volume_ids().iter().any(|id| id == volume_id) {
        return Err(EjectError::Busy);
    }

    // A device provider answers for its own volumes from live state, so an id
    // that merely looks device-shaped can't route an eject at nothing. Asked
    // first: MTP storages aren't registered under a mount path at all.
    let provider = crate::device_volumes::provider_for_volume_id(volume_id).await;

    let (mount_path, is_smb) = if provider.is_some() {
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
    let is_ejectable = if provider.is_some() || is_smb {
        false
    } else {
        resolve_is_ejectable(&mount_path).await
    };

    let action = decide_eject_action(&EjectContext {
        volume_id,
        is_ejectable,
        is_smb,
        device_provider: provider.as_ref().map(|p| p.id()),
    })
    .map_err(EjectError::from)?;

    match action {
        EjectAction::DeviceDisconnect {
            provider: id,
            volume_id,
        } => {
            let provider = provider.ok_or_else(|| EjectError::VolumeNotFound {
                volume_id: volume_id.clone(),
            })?;
            provider
                .eject(&volume_id)
                .await
                .map_err(|detail| EjectError::DeviceDisconnectRefused {
                    provider: id.to_string(),
                    detail,
                })
        }
        // For disk volumes, stop the index BEFORE the unmount (the wedge-safe point).
        // A device provider tears its index down through its own disconnect hook,
        // so it isn't stopped here.
        EjectAction::DiskutilUnmount => {
            stop_index_then_unmount(volume_id, || diskutil_run("unmount", &mount_path)).await
        }
        EjectAction::DiskutilEject => stop_index_then_unmount(volume_id, || diskutil_run("eject", &mount_path)).await,
    }
}

/// Stop the volume's index (if any) BEFORE running the unmount/eject.
///
/// This is the ONE reliable wedge-safe point: releasing the FSEvents watcher +
/// open SQLite handles while the filesystem is still healthy is the only thing that
/// keeps an open stream/handle from wedging a FSKit (`msdos`) unmount (see
/// `indexing/DETAILS.md` § the unmount/eject lifecycle and the 2026-07-15 kernel
/// panic). The ordering is unconditional: the index stop is awaited to completion,
/// then the unmount runs. `unmount` is a parameter so the ordering can be asserted
/// in a test without a real volume or `diskutil`. No-op stop for an unindexed volume.
async fn stop_index_then_unmount<F, Fut>(volume_id: &str, unmount: F) -> Result<(), EjectError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), EjectError>>,
{
    stop_index_blocking(volume_id).await;
    unmount().await
}

/// Stop `volume_id`'s index on the blocking pool, awaited so the stop COMPLETES
/// before the caller unmounts. `stop_indexing` drains the writer/live-event task
/// (up to a few seconds of blocking work), so it must not run on the async executor
/// directly.
///
/// Only a `LocalExternal` index is stopped here: it's the one carrying an FSEvents
/// watcher + open SQLite handles that can wedge a FSKit (`msdos`) unmount, and it's
/// the kind whose DB stays usable via a later reconcile. SMB/MTP indexes tear down
/// through their own disconnect paths and stay registered (Stale, offline-browsable)
/// across an eject, so this must not remove them. No-op for a non-`LocalExternal` or
/// unindexed volume.
async fn stop_index_blocking(volume_id: &str) {
    let vid = volume_id.to_string();
    if let Err(join_err) =
        tokio::task::spawn_blocking(move || crate::index_host::index().stop_removable_volume(&vid)).await
    {
        log::warn!(target: "eject", "index-stop task for '{volume_id}' failed to join: {join_err}");
    }
}

/// Disconnects a single SMB volume by tearing down its OS mount.
///
/// The "Disconnect" affordance in `SmbReconnectingView` / the gave-up
/// `VolumeUnreachableBanner` calls this (via the `disconnect_smb_volume`
/// command). On macOS it runs `diskutil unmount`; FSEvents then drives the
/// standard `Volume::on_unmount` → `VolumeManager`-removal pipeline (same as an
/// SMB [`eject`]): `SmbVolume::on_unmount` flips `unmounted=true`, stops the
/// watcher task, and drops the smb2 session, then a `volumes-changed` event
/// flows to the frontend.
///
/// On other platforms the OS-level unmount isn't wired up yet (mirrors
/// `network::mount::unmount_smb_shares_from_host`), so it drops the smb2 session
/// directly via `Volume::on_unmount`; the OS mount stays alive for the user to
/// eject from the file manager.
///
/// Unlike [`eject`], this has no busy gate: the Disconnect affordance targets a
/// reconnecting or unreachable volume, so there's nothing actively transferring.
///
/// Errors:
/// - [`EjectError::VolumeNotFound`] if the id isn't registered (a race).
/// - [`EjectError::NotAnSmbVolume`] when the volume isn't SMB (a race or
///   automation caller; the UI only offers Disconnect for SMB volumes).
pub async fn disconnect_smb(volume_id: &str) -> Result<(), EjectError> {
    use crate::file_system::volume::manager::get_volume_manager;

    let volume = get_volume_manager()
        .get(volume_id)
        .ok_or_else(|| EjectError::VolumeNotFound {
            volume_id: volume_id.to_string(),
        })?;

    if volume.smb_connection_state().is_none() {
        return Err(EjectError::NotAnSmbVolume {
            volume_id: volume_id.to_string(),
        });
    }

    #[cfg(target_os = "macos")]
    {
        let mount_path = volume.root().to_string_lossy().to_string();
        diskutil_run("unmount", &mount_path).await?;
        log::info!(target: "eject", "Disconnected SMB volume {} (unmounted {})", volume_id, mount_path);
        // FSEvents will fire shortly and trigger on_unmount + volume-manager removal.
    }

    #[cfg(not(target_os = "macos"))]
    {
        volume.on_unmount();
        log::info!(
            target: "eject",
            "Dropped smb2 session for {} (OS unmount not yet implemented on this platform)",
            volume_id
        );
    }

    Ok(())
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
            crate::volumes_linux::list_locations()
                .into_iter()
                .find(|v| v.path == path)
                .map(|v| v.is_ejectable)
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }
}

/// Runs a blocking eject subprocess with a 15 s timeout, mapping the outcome to
/// [`EjectError`]. A real deadline becomes [`EjectError::TimedOut`], which the
/// frontend words differently from a refusal because the unmount may still land.
async fn run_eject_subprocess(
    timeout: Duration,
    f: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), EjectError> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(detail))) => Err(EjectError::UnmountRefused { detail }),
        Ok(Err(join_err)) => Err(EjectError::Unexpected {
            detail: join_err.to_string(),
        }),
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
            .map_err(|e| format!("couldn't run diskutil: {e}"))?;
        if output.status.success() {
            log::info!(target: "eject", "diskutil {} succeeded for {}", verb, path_for_cmd);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("diskutil {}: {}", verb, stderr.trim()))
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
            .map_err(|e| format!("couldn't run umount: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("umount: {}", stderr.trim()))
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_volume_routes_to_its_provider() {
        // The id is handed over whole: the provider owns its parse (MTP splits
        // `{device_id}:{storage_id}` on the LAST colon, so a serial with `:` in
        // it survives).
        let ctx = EjectContext {
            volume_id: "mtp-AA:BB:CC:65537",
            is_ejectable: false,
            is_smb: false,
            device_provider: Some("mtp"),
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap(),
            EjectAction::DeviceDisconnect {
                provider: "mtp",
                volume_id: "mtp-AA:BB:CC:65537".to_string()
            }
        );
    }

    #[test]
    fn device_provider_wins_over_every_other_flag() {
        let ctx = EjectContext {
            volume_id: "adb-serial",
            is_ejectable: true,
            is_smb: true,
            device_provider: Some("adb"),
        };
        assert!(matches!(
            decide_eject_action(&ctx).unwrap(),
            EjectAction::DeviceDisconnect { provider: "adb", .. }
        ));
    }

    #[test]
    fn smb_volume_routes_to_unmount() {
        let ctx = EjectContext {
            volume_id: "smb-naspolya-445-public",
            is_ejectable: false,
            is_smb: true,
            device_provider: None,
        };
        assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilUnmount);
    }

    #[test]
    fn ejectable_disk_routes_to_eject() {
        let ctx = EjectContext {
            volume_id: "volumes-usb-drive",
            is_ejectable: true,
            is_smb: false,
            device_provider: None,
        };
        assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilEject);
    }

    #[test]
    fn non_ejectable_local_volume_errors() {
        let ctx = EjectContext {
            volume_id: "root",
            is_ejectable: false,
            is_smb: false,
            device_provider: None,
        };
        assert_eq!(
            decide_eject_action(&ctx).unwrap_err(),
            EjectDecisionError::NotEjectable {
                volume_id: "root".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn eject_stops_the_index_before_the_unmount() {
        use cmdr_index::IndexVolumeKind;
        use cmdr_index::testing::{is_index_active, reserve_initializing_index_for_test};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        // A LocalExternal drive with a live index is ejected: the index MUST be
        // stopped (its FSEvents watcher + SQLite handles released) BEFORE the unmount
        // runs — the wedge-safe ordering. Pre-fix this would have passed wrongly: the
        // eject path never touched indexing, so a live index survived into the unmount.
        // This drives the REAL
        // `stop_indexing` through the ordering seam with a fake unmount that records
        // whether the index was still active when it ran.
        let vid = "volumes-cmdr-test-eject-stop-order";
        let _tmp = reserve_initializing_index_for_test(vid, IndexVolumeKind::LocalExternal);
        assert!(is_index_active(vid), "precondition: the index is active");

        let active_when_unmount_ran = Arc::new(AtomicBool::new(true));
        let observed = Arc::clone(&active_when_unmount_ran);
        let vid_for_unmount = vid.to_string();

        let result = stop_index_then_unmount(vid, || async move {
            // Record the index state at the exact moment the unmount would run.
            observed.store(is_index_active(&vid_for_unmount), Ordering::SeqCst);
            Ok(())
        })
        .await;

        assert!(result.is_ok(), "the ordering seam must propagate the unmount result");
        assert!(
            !active_when_unmount_ran.load(Ordering::SeqCst),
            "the index must be stopped BEFORE the unmount runs"
        );
        assert!(!is_index_active(vid), "the index instance is gone after eject");
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
            device_provider: None,
        };
        assert_eq!(decide_eject_action(&ctx).unwrap(), EjectAction::DiskutilUnmount);
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    /// The wire shape the frontend matches on. Internally tagged, camelCase, with
    /// the fields intact — the same contract `MutationError` keeps.
    #[test]
    fn eject_error_crosses_the_wire_as_a_tagged_value() {
        let json = serde_json::to_value(EjectError::VolumeNotFound {
            volume_id: "volumes-usb".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "volumeNotFound", "volumeId": "volumes-usb" })
        );

        let json = serde_json::to_value(EjectError::UnmountRefused {
            detail: "in use by process 1234 (mds)".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "unmountRefused", "detail": "in use by process 1234 (mds)" })
        );

        assert_eq!(
            serde_json::to_value(EjectError::TimedOut).unwrap(),
            serde_json::json!({ "type": "timedOut" })
        );
    }

    /// A decision refusal keeps its own identity instead of collapsing into a
    /// generic "couldn't".
    #[test]
    fn a_decision_refusal_keeps_its_variant() {
        let err: EjectError = EjectDecisionError::NotEjectable {
            volume_id: "root".to_string(),
        }
        .into();
        assert!(matches!(err, EjectError::NotEjectable { ref volume_id } if volume_id == "root"));

        let json = serde_json::to_value(EjectError::DeviceDisconnectRefused {
            provider: "mtp".to_string(),
            detail: "PTP CloseSession timed out".to_string(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "deviceDisconnectRefused", "provider": "mtp", "detail": "PTP CloseSession timed out" })
        );
    }
}
