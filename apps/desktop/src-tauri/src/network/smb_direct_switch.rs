//! The per-share "Use Cmdr's fast direct connection" switch, as the volume switcher
//! asks about it: by volume id.
//!
//! The choice itself lives in `known_shares` (keyed by server and share, so it
//! outlives any one mount's volume id), and the auto upgrade reads it in
//! `smb_upgrade::register_smb_volume`. This module answers the two questions the UI
//! has, which both start from a volume: what the share behind it is set to, and what
//! flipping it does right now.

use crate::file_system::volume::manager::get_volume_manager;
use crate::network::known_shares;
use crate::network::os_mount_notice::withdraw_os_mount_notice;
use crate::network::smb_upgrade::{MOUNT_READ_LIMIT, return_to_os_mount};
use crate::volumes::{SmbMountInfo, get_smb_mount_info};
use cmdr_fs::deadline::blocking_with_timeout;
use std::time::Duration;

/// What switching a share's direct connection on or off did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DirectConnectionSwitch {
    /// The choice is saved, and nothing else needed to change now: the share was
    /// already on the OS mount when switched off, or switched on (the caller starts
    /// the connect it wants, with its own sign-in and toasts).
    Saved,
    /// Switched off while a direct session served the share, so the share went back
    /// to the macOS mount right away.
    ReturnedToOsMount,
    /// No SMB share is mounted behind this volume (anymore), so there's nothing to
    /// switch. Nothing was saved.
    NotAnSmbShare,
    /// The mount didn't answer a status read in time. Nothing was saved.
    MountNotResponding,
}

/// What reading the share behind a volume came back with.
#[derive(Debug)]
enum ShareRead {
    Smb(SmbMountInfo),
    NotSmb,
    NotResponding,
}

/// Reads the SMB share behind `volume_id` off its mount, bounded by `limit`.
///
/// Reads the mount for a direct share too: its OS mount stays up underneath the
/// session, and the mount is the one place the server and share are spelled the
/// way the auto upgrade will see them.
async fn read_share_within(
    volume_id: &str,
    limit: Duration,
    read: impl FnOnce(&str) -> Option<SmbMountInfo> + Send + 'static,
) -> ShareRead {
    let Some(volume) = get_volume_manager().get(volume_id) else {
        return ShareRead::NotSmb;
    };
    let root = volume.root().to_string_lossy().to_string();
    let Some(info) = blocking_with_timeout(limit, None, move || Some(read(&root))).await else {
        return ShareRead::NotResponding;
    };
    match info {
        Some(info) => ShareRead::Smb(info),
        None => ShareRead::NotSmb,
    }
}

/// Whether the share behind `volume_id` may use Cmdr's direct connection, or `None`
/// when there's no SMB share behind it to ask about (or its mount didn't answer).
pub(crate) async fn direct_connection_enabled_for(volume_id: &str) -> Option<bool> {
    direct_connection_enabled_within(volume_id, MOUNT_READ_LIMIT, get_smb_mount_info).await
}

async fn direct_connection_enabled_within(
    volume_id: &str,
    limit: Duration,
    read: impl FnOnce(&str) -> Option<SmbMountInfo> + Send + 'static,
) -> Option<bool> {
    match read_share_within(volume_id, limit, read).await {
        ShareRead::Smb(info) => Some(known_shares::direct_connection_enabled(&[&info.server], &info.share)),
        ShareRead::NotSmb | ShareRead::NotResponding => None,
    }
}

/// Saves the switch for the share behind `volume_id`, and when it goes OFF on a
/// share a direct session serves, hands the share back to the macOS mount now.
///
/// Now, and not at the next mount, because the switch's row sits beside the dot
/// that shows the connection: switched off on a green share, a dot that stays green
/// reads as the switch not working. The hand-back supersedes the session rather than
/// closing it (`smb_upgrade::return_to_os_mount`), so nothing in flight is cut.
///
/// Switching ON only saves: the caller runs "Connect directly" for a share on the OS
/// mount, which owns the credentials, the sign-in sheet, and the toasts.
pub(crate) async fn set_direct_connection_for(volume_id: &str, enabled: bool) -> DirectConnectionSwitch {
    set_direct_connection_within(volume_id, enabled, MOUNT_READ_LIMIT, get_smb_mount_info).await
}

async fn set_direct_connection_within(
    volume_id: &str,
    enabled: bool,
    limit: Duration,
    read: impl FnOnce(&str) -> Option<SmbMountInfo> + Send + 'static,
) -> DirectConnectionSwitch {
    let info = match read_share_within(volume_id, limit, read).await {
        ShareRead::Smb(info) => info,
        ShareRead::NotSmb => return DirectConnectionSwitch::NotAnSmbShare,
        ShareRead::NotResponding => return DirectConnectionSwitch::MountNotResponding,
    };
    // ❗ Saved BEFORE the hand-back takes the upgrade lock, so an auto upgrade
    // already waiting on that lock re-checks after us and sees the switch off.
    known_shares::set_direct_connection_enabled(&info.server, &info.share, enabled);
    // Off retires the share's slow-connection notice, whatever route flipped it:
    // its button would do exactly what the user just opted out of. On leaves it,
    // since the connect the caller starts next may still fail.
    if !enabled {
        withdraw_os_mount_notice(volume_id);
    }
    if !enabled && return_to_os_mount(volume_id).await {
        return DirectConnectionSwitch::ReturnedToOsMount;
    }
    DirectConnectionSwitch::Saved
}

#[cfg(test)]
#[path = "smb_direct_switch_test.rs"]
mod tests;
