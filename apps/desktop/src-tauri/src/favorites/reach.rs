//! The reach pass: stage 2 of a favorite row (`target.rs` header).
//!
//! Runs inside `volume_listing::complete`, AFTER registry enrichment, so every volume row's
//! `connection_state` and `capabilities` are final. ❗ Pure: it reads the rows beside each favorite
//! and two in-memory settings, ❌ never a disk or the wire, because the listing runs on every
//! `volumes-changed`.
//!
//! Per favorite row:
//!
//! 1. The target is the stored volume id, or for a legacy entry (`volume: None`) the volume row
//!    that claims it ([`claim_for`]).
//! 2. With a row for that id: `volume_root` is the row's path. A mount-rooted volume
//!    (`VolumeScheme::is_mount_rooted`) has the favorite's path rebased from the stored root onto
//!    the row's; a server's or phone's path stays verbatim, and one no longer under the root is
//!    `NotFound`. The reach comes from the row ([`reach_of_row`]).
//! 3. Without one: the reach comes from the id's scheme alone ([`reach_without_row`]).

use cmdr_fs::volume::app_paths::{path_is_under, rebase};
use cmdr_fs::volume::{ConnectionState, DEFAULT_VOLUME_ID, DeviceReadiness, VolumeScheme, mtp_ids};

use super::store::FavoriteVolume;
use super::target::{DeviceBackend, FavoriteReach, OnDisk, UnpluggedKind};
use crate::volume_listing::{LocationCategory, LocationInfo};

/// The two settings that decide whether an absent phone is unplugged or switched off. Both
/// in-memory, read once per listing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReachFacts {
    /// Whether MTP support is on (the MTP manager's own bit).
    pub mtp_enabled: bool,
    /// Whether ADB support is on (`adb::volume_wiring::is_adb_enabled`).
    pub adb_enabled: bool,
}

impl ReachFacts {
    /// The settings as they are now.
    pub(crate) fn now() -> Self {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        return Self {
            mtp_enabled: crate::mtp::connection_manager().is_enabled(),
            adb_enabled: crate::adb::volume_wiring::is_adb_enabled(),
        };
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        Self {
            mtp_enabled: false,
            adb_enabled: false,
        }
    }
}

/// Decides every favorite row's target from the volume rows beside it, rebasing a mount-rooted
/// favorite onto its volume's current root. Returns the claims it made for legacy entries
/// (`(favorite id, volume)`), for the caller to persist OFF the listing path
/// (`store::claim_volumes`).
pub(crate) fn annotate(rows: &mut [LocationInfo], facts: &ReachFacts) -> Vec<(String, FavoriteVolume)> {
    let volumes: Vec<LocationInfo> = rows
        .iter()
        .filter(|row| row.category != LocationCategory::Favorite)
        .cloned()
        .collect();
    let mut claims = Vec::new();
    for row in rows.iter_mut() {
        let Some(target) = row.favorite_target.as_mut() else {
            continue;
        };
        let stored = target.discovered.stored.clone();
        let on_disk = target.discovered.on_disk;

        let volume = match stored {
            Some(stored) => stored,
            None => match claim_for(&row.path, on_disk, &volumes) {
                Some(claimed) => {
                    let favorite_id = row.id.strip_prefix("fav-").unwrap_or(&row.id).to_string();
                    claims.push((favorite_id, claimed.clone()));
                    claimed
                }
                None => {
                    target.volume_id = None;
                    target.volume_name = None;
                    target.volume_root = None;
                    target.reach = FavoriteReach::NotFound;
                    continue;
                }
            },
        };

        target.volume_id = Some(volume.id.clone());
        let Some(volume_row) = volumes.iter().find(|candidate| candidate.id == volume.id) else {
            target.volume_name = Some(volume.name);
            target.volume_root = None;
            target.reach = reach_without_row(&volume.id, facts, &volumes);
            continue;
        };
        target.volume_name = Some(volume_row.name.clone());
        target.volume_root = Some(volume_row.path.clone());

        let rebased = VolumeScheme::of(&volume.id)
            .is_mount_rooted()
            .then(|| rebase(&row.path, &volume.root, &volume_row.path))
            .flatten();
        // Discovery probed the STORED path, so its "not there" says nothing about a moved one.
        let evidence = match rebased {
            Some(ref path) if *path != row.path => OnDisk::Unchecked,
            _ => on_disk,
        };
        if let Some(path) = rebased {
            row.path = path;
        }
        target.reach = match path_is_under(&row.path, &volume_row.path) {
            true => reach_of_row(volume_row, evidence),
            false => FavoriteReach::NotFound,
        };
    }
    claims
}

/// The volume row a legacy favorite (no stored volume) lives on: the one whose path is the deepest
/// whole-segment prefix of the favorite's.
///
/// ❗ Claim only from evidence. The boot volume claims only a folder discovery SAW (`OnDisk::Yes`),
/// or took on trust as a TCC-protected home folder while the FDA gate is pending (`Assumed`):
/// otherwise `/Volumes/naspi/docs` on an unmounted share nobody saved would be claimed by `/`,
/// written down, and read "not found" forever after the share comes back. Any other row (a live
/// mount, a saved share at its last mount path, a server place, a device storage) names the volume
/// by itself being there.
fn claim_for(path: &str, on_disk: OnDisk, volumes: &[LocationInfo]) -> Option<FavoriteVolume> {
    let row = volumes
        .iter()
        .filter(|volume| path_is_under(path, &volume.path))
        .max_by_key(|volume| volume.path.trim_end_matches('/').len())?;
    let seen = match on_disk {
        OnDisk::Yes | OnDisk::Assumed => true,
        OnDisk::No | OnDisk::Unchecked => false,
    };
    if row.id == DEFAULT_VOLUME_ID && !seen {
        return None;
    }
    Some(FavoriteVolume {
        id: row.id.clone(),
        root: row.path.clone(),
        name: row.name.clone(),
    })
}

/// Whether a pick reaches a favorite whose volume HAS a row. `on_disk` is what discovery saw at
/// the favorite's path (`Unchecked` once rebased).
fn reach_of_row(row: &LocationInfo, on_disk: OnDisk) -> FavoriteReach {
    match row.device_readiness {
        Some(DeviceReadiness::Unavailable { reason }) => {
            return FavoriteReach::Unplugged {
                device: UnpluggedKind::Phone,
                reason: Some(reason),
            };
        }
        // The pane's device view waits for the tap.
        Some(DeviceReadiness::WaitingForAuthorization) => return FavoriteReach::Connects,
        Some(DeviceReadiness::Ready) | None => {}
    }
    // A device listed but not registered (an ADB phone nobody dialed yet): the pane dials it.
    if row.category == LocationCategory::MobileDevice && row.capabilities.is_none() {
        return FavoriteReach::Connects;
    }
    match row.connection_state {
        None | Some(ConnectionState::Direct | ConnectionState::OsMount) => {}
        // The pane's own views take it from there: place-connect dials a saved place, the
        // reconnect banner and the sign-in and host-key sheets mend a dropped session.
        Some(
            ConnectionState::Saved
            | ConnectionState::Disconnected
            | ConnectionState::NeedsSignIn
            | ConnectionState::NeedsHostKeyApproval,
        ) => return FavoriteReach::Connects,
    }
    match on_disk {
        OnDisk::No => FavoriteReach::NotFound,
        OnDisk::Yes | OnDisk::Assumed | OnDisk::Unchecked => FavoriteReach::Ready,
    }
}

/// Whether a pick reaches a favorite whose volume has NO row, from its id's scheme alone.
fn reach_without_row(volume_id: &str, facts: &ReachFacts, volumes: &[LocationInfo]) -> FavoriteReach {
    let phone = FavoriteReach::Unplugged {
        device: UnpluggedKind::Phone,
        reason: None,
    };
    match VolumeScheme::of(volume_id) {
        VolumeScheme::Mtp if !facts.mtp_enabled => FavoriteReach::AccessOff {
            backend: DeviceBackend::Mtp,
        },
        // The phone is listed under another storage id: the card the favorite was on is out.
        VolumeScheme::Mtp if phone_has_other_storage(volume_id, volumes) => FavoriteReach::Unplugged {
            device: UnpluggedKind::Storage,
            reason: None,
        },
        VolumeScheme::Mtp => phone,
        VolumeScheme::Adb if !facts.adb_enabled => FavoriteReach::AccessOff {
            backend: DeviceBackend::Adb,
        },
        VolumeScheme::Adb => phone,
        // Also an NFS or AFP mount that's gone ("isn't connected" is true for both).
        VolumeScheme::Root | VolumeScheme::Local | VolumeScheme::Path | VolumeScheme::Cloud => {
            FavoriteReach::Unplugged {
                device: UnpluggedKind::Drive,
                reason: None,
            }
        }
        // Nothing saved knows the id: a forgotten share or server.
        VolumeScheme::Smb | VolumeScheme::Sftp | VolumeScheme::Webdav | VolumeScheme::S3 => FavoriteReach::Forgotten,
        VolumeScheme::Favorite | VolumeScheme::Unknown => FavoriteReach::NotFound,
    }
}

/// Whether the MTP device behind `volume_id` is listed under another storage id.
fn phone_has_other_storage(volume_id: &str, volumes: &[LocationInfo]) -> bool {
    let Some(device) = mtp_ids::device_id_of_volume(volume_id) else {
        return false;
    };
    volumes
        .iter()
        .any(|volume| mtp_ids::device_id_of_volume(&volume.id) == Some(device))
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod tests;
