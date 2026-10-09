//! What a favorite row tells the frontend about where it points, and what discovery learned about
//! it on the way.
//!
//! The listing builds a favorite row in two stages, because neither alone sees enough:
//!
//! 1. **Discovery** (`volumes::get_favorites`, `volumes_linux::get_favorites`) runs blocking under
//!    its own timeout and can look at the disk, but sees no servers and no devices. It stats a
//!    favorite's folder only where that can't hang ([`on_disk`]) and seeds the row's
//!    [`FavoriteTarget`] from the store ([`FavoriteTarget::discovered`]).
//! 2. **The reach pass** (`super::reach`) runs inside `volume_listing::complete`, sees every row
//!    (mounts, saved shares, server places, device storages) with its final connection state, and
//!    must never touch a disk. It decides the [`FavoriteReach`].
//!
//! ❌ **Not `connection_state` on the favorite row.** A favorite is not a volume, and
//! `connection_state` is read by volume-shaped consumers (the switcher dot, the reconnect banner,
//! "answers now"). A favorite carrying `saved` would make each of them a question about a volume
//! that doesn't exist. The favorite's own vocabulary is [`FavoriteReach`].

use std::path::Path;

use cmdr_fs::volume::DeviceUnavailableReason;
use serde::{Deserialize, Serialize};

use super::store::FavoriteVolume;

/// Present only on a favorite row: where it points, and whether a pick can get there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteTarget {
    /// The volume the favorite lives on: the stored one, or the one the reach pass claimed for a
    /// legacy entry. `None` when nothing can say.
    pub volume_id: Option<String>,
    /// The volume's row name when it has a row, else the name stored with the favorite.
    pub volume_name: Option<String>,
    /// The volume's root as published NOW: the `volumePath` a pick enters with. `None` when the
    /// volume has no row.
    pub volume_root: Option<String>,
    /// Whether a pick gets there, and if not, why.
    pub reach: FavoriteReach,
    /// What discovery saw, for the reach pass. ❌ Never serialized: it's the listing's own
    /// bookkeeping between its two stages.
    #[serde(skip)]
    pub(crate) discovered: Discovered,
}

/// What discovery hands the reach pass about one favorite.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Discovered {
    /// The volume `favorites.json` holds for it, `None` on an unclaimed legacy entry.
    pub stored: Option<FavoriteVolume>,
    /// Whether discovery saw the folder, at the path the store holds.
    pub on_disk: OnDisk,
}

/// Whether discovery saw a favorite's folder on disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum OnDisk {
    /// It's there.
    Yes,
    /// The folder isn't there, on a local disk that answered.
    No,
    /// Taken on trust without a look: a TCC-protected folder while the Full Disk Access gate is
    /// pending (a stat there raises a popup). Those are home folders on the local disk, present on
    /// essentially every account, so this counts as seen: it reads `Ready` and lets the boot volume
    /// claim a legacy entry. ❗ Without it, the seeded `~/Desktop` read "not found" all through
    /// onboarding.
    Assumed,
    /// Nobody looked: a scheme path or a folder on a network mount (a stat there can hang).
    #[default]
    Unchecked,
}

/// Whether discovery may stat a favorite's folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Probe {
    /// A local disk: stat it.
    Stat,
    /// The containing mount (from the mount-table snapshot) is a network one. A stat there can
    /// block for minutes on a dead share, and one dead share would cost the whole listing.
    NetworkMount,
    /// A TCC-protected folder while the Full Disk Access gate is pending: even `exists()` raises
    /// a system popup over the onboarding modal.
    #[cfg_attr(
        all(not(target_os = "macos"), not(test)),
        expect(dead_code, reason = "only macOS has TCC")
    )]
    TakenOnTrust,
}

/// Whether a pick of a favorite gets there, and if not, why. One decision site: `favorites/reach.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum FavoriteReach {
    /// The volume is live: a pick opens the folder.
    Ready,
    /// The volume is a saved place that isn't connected (an unmounted share, a saved server, a
    /// dropped or signed-out session, a phone not dialed yet or waiting for "Allow"): a pick
    /// enters it and the pane's own connect view dials.
    Connects,
    /// The device or drive isn't there.
    Unplugged {
        /// What's missing, for the words.
        device: UnpluggedKind,
        /// Why a listed device can't be used, when that's the story.
        reason: Option<DeviceUnavailableReason>,
    },
    /// The backend that would reach it is switched off in Settings.
    AccessOff {
        /// Which one.
        backend: DeviceBackend,
    },
    /// Nothing saved knows how to dial the volume any more (a forgotten share or server). The
    /// same place coming back revives the favorite with no migration.
    Forgotten,
    /// The folder isn't there, or nothing can say which volume it was on.
    NotFound,
}

/// What an [`FavoriteReach::Unplugged`] favorite is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UnpluggedKind {
    /// A local drive (or a non-SMB network mount) that isn't mounted.
    Drive,
    /// A phone that isn't connected, or is listed but can't be used.
    Phone,
    /// The phone is connected, but this storage on it isn't (an SD card swapped out).
    Storage,
}

/// Which device backend an [`FavoriteReach::AccessOff`] favorite needs switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DeviceBackend {
    /// Phones and cameras over MTP.
    Mtp,
    /// Android devices over ADB.
    Adb,
}

impl FavoriteTarget {
    /// The target discovery publishes before the reach pass: the stored volume, and a provisional
    /// reach from what it saw (`NotFound` for a folder that's gone, `Ready` otherwise).
    pub(crate) fn discovered(stored: Option<FavoriteVolume>, on_disk: OnDisk) -> Self {
        Self {
            volume_id: stored.as_ref().map(|volume| volume.id.clone()),
            volume_name: stored.as_ref().map(|volume| volume.name.clone()),
            volume_root: None,
            reach: match on_disk {
                OnDisk::No => FavoriteReach::NotFound,
                OnDisk::Yes | OnDisk::Assumed | OnDisk::Unchecked => FavoriteReach::Ready,
            },
            discovered: Discovered { stored, on_disk },
        }
    }
}

/// What discovery learns about a favorite's folder, asking `exists` only where that can't hang.
///
/// A scheme path (`sftp://…`, `mtp://…`) is never stat'd: it isn't an OS path at all.
pub(crate) fn on_disk(path: &str, probe: Probe, exists: impl FnOnce(&Path) -> bool) -> OnDisk {
    let path = Path::new(path);
    if !path.is_absolute() {
        return OnDisk::Unchecked;
    }
    match probe {
        Probe::NetworkMount => OnDisk::Unchecked,
        Probe::TakenOnTrust => OnDisk::Assumed,
        Probe::Stat if exists(path) => OnDisk::Yes,
        Probe::Stat => OnDisk::No,
    }
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
