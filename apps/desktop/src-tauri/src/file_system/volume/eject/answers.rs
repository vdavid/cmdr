//! The eject pipeline's vocabulary: what it can do to a volume, what it did, and
//! why it didn't.
//!
//! Split from the pipeline (`mod.rs`) because these are the WIRE: the frontend
//! renders every refusal from [`EjectError`]'s typed variant through the
//! `errors.eject.*` catalog, and the MCP `eject` tool hands an agent both this
//! enum's tags and [`EjectOutcome`]'s. The pure [`decide_eject_action`] lives here
//! too, beside the answers it picks between, and is unit-tested without touching
//! `VolumeManager` or the filesystem.

use serde::{Deserialize, Serialize};

use super::holders::HolderScan;

/// Action the eject pipeline takes for a given volume.
#[derive(Debug, PartialEq, Eq)]
pub enum EjectAction {
    /// A disk's volume: `diskutil eject <mount_path>`, which powers down USB devices and detaches
    /// DMGs, or `diskutil unmount` for one volume of a disk that keeps others (`disk_reach`).
    DiskutilEject,
    /// Run `diskutil unmount <mount_path>`. SMB: FSEvents handles smb2 teardown.
    DiskutilUnmount,
    /// Hand the eject to the device provider that owns the volume.
    DeviceDisconnect { provider: &'static str, volume_id: String },
    /// Drop a remote server's session (SFTP, WebDAV) and unregister the volume.
    /// There is no mount to unmount: the session IS the volume.
    RemoteDisconnect { volume_id: String },
}

/// What a teardown actually did, once it went through.
///
/// ❗ The answer an automated caller reads: the MCP `eject` tool returns this as
/// a typed tag rather than a sentence, so an agent learns which teardown ran
/// (`tools/actions/eject.rs`). The frontend ignores it; its feedback is the
/// volume leaving the switcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EjectOutcome {
    /// A device provider (MTP, ADB) retired the volume.
    DeviceDisconnected,
    /// A remote session (SFTP, WebDAV) was dropped and the volume unregistered.
    RemoteDisconnected,
    /// One volume left the mount table, an SMB share and a macFUSE mount included.
    Unmounted,
    /// A whole physical disk was ejected, every volume on it included.
    DiskEjected,
    /// Nothing was left to do: the volume had already gone before the teardown ran.
    AlreadyGone,
    /// One volume of a disk left the mount table, and the disk stays attached because other
    /// volumes on it are still mounted. ❗ The drive is NOT safe to unplug, which is what the
    /// frontend tells the person.
    VolumeEjectedDiskStays,
}

/// How much of a physical disk an eject takes down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EjectScope {
    /// The volume asked about, unless it's the disk's last one, which ejects the disk too.
    Volume,
    /// Every volume on its disk, and then the disk.
    Disk,
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
    /// True if the volume is a remote server session Cmdr dials itself (SFTP,
    /// WebDAV), which detaches by dropping that session
    /// (`BackendKind::detaches_by_session_drop`).
    pub is_remote_session: bool,
    /// The device provider (`"mtp"`, `"adb"`) that owns this volume, if one does.
    pub device_provider: Option<&'static str>,
}

/// Decides what to do for a given volume. Pure function; the impure parts
/// (looking up the volume, running `diskutil`, calling the provider's eject)
/// live in [`super::eject`].
pub fn decide_eject_action(ctx: &EjectContext) -> Result<EjectAction, EjectDecisionError> {
    if let Some(provider) = ctx.device_provider {
        return Ok(EjectAction::DeviceDisconnect {
            provider,
            volume_id: ctx.volume_id.to_string(),
        });
    }
    // Before the ejectability test, which is a question about a MOUNT: a server
    // Cmdr dialed itself has no mount table row to answer it, so asking would
    // word an honest "no" as the wrong refusal (`ERR-P7F5Q`).
    if ctx.is_remote_session {
        return Ok(EjectAction::RemoteDisconnect {
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
    /// A remote server (SFTP, WebDAV) had no live session left to close. ❗ The
    /// ONE way a remote disconnect comes back unhappy: dropping the session IS
    /// the teardown and it can't refuse, so there's nothing else to report.
    RemoteNotConnected {
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
    /// case is an open file somewhere.
    UnmountRefused {
        /// Who held the drive when the last attempt was refused. ❗ Two answers:
        /// a scan that couldn't run names nobody, which is ❌ never "nobody is
        /// holding it".
        holders: HolderScan,
        /// The tool's own stderr, for the log and the details line.
        detail: String,
    },
    /// An eject taking several volumes down stopped before it unmounted anything, since someone
    /// is holding one of them. ❗ Nothing changed, where [`Self::UnmountRefused`] from the same
    /// eject can mean some volumes already went.
    HeldBeforeEject {
        /// Who's holding the disk.
        holders: HolderScan,
    },
    /// The `diskutil` / `umount` subprocess (or a device provider's eject) didn't
    /// finish within the timeout. ❗ The unmount was NOT cancelled; it may still land.
    TimedOut,
    /// A step that runs BEFORE any unmount didn't finish within its deadline, so
    /// nothing was unmounted and nothing may still land. A disk image whose backing
    /// file sits on a hung share can block these for good. ❌ Never word it as
    /// [`Self::TimedOut`], whose copy promises the eject may still happen.
    NotResponding {
        /// The step that stalled.
        step: EjectStep,
    },
    /// The one honest fallback, for a failure nothing above classifies (a
    /// panicked task). ❌ `detail` is never the message.
    Unexpected {
        /// What the layer below reported, for the log and the details line.
        detail: String,
    },
}

/// Which pre-unmount step stalled, for [`EjectError::NotResponding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EjectStep {
    /// Asking the OS whether the volume is ejectable (`statfs` + NSURL, or the
    /// Linux mount list).
    EjectabilityCheck,
    /// Working out which physical disk the volume sits on, and which of its volumes
    /// the eject takes down with it.
    DiskResolve,
    /// Stopping the drive's index, which must finish before any unmount runs.
    IndexStop,
}

impl std::fmt::Display for EjectStep {
    /// For logs and MCP replies only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EjectabilityCheck => "the ejectability check",
            Self::DiskResolve => "the disk lookup",
            Self::IndexStop => "the index stop",
        })
    }
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
            Self::RemoteNotConnected { volume_id } => write!(f, "volume {volume_id} has no open connection to close"),
            Self::DeviceDisconnectRefused { provider, detail } => {
                write!(f, "{provider} disconnect refused: {detail}")
            }
            Self::UnmountRefused { holders, detail } => write!(f, "unmount refused ({holders}): {detail}"),
            Self::HeldBeforeEject { holders } => write!(f, "nothing was unmounted, since the disk is held ({holders})"),
            Self::TimedOut => f.write_str("timed out"),
            Self::NotResponding { step } => write!(f, "{step} didn't finish in time, so nothing was unmounted"),
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
