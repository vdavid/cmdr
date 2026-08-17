//! Volume and location discovery for Linux.
//!
//! Provides a sidebar location picker with:
//! - Favorites (Home, Desktop, Documents, Downloads)
//! - Main volume (root `/`)
//! - Mounted volumes (real filesystems from /proc/mounts)
//! - Cloud drives (Dropbox, Google Drive, Nextcloud, OneDrive)
//! - Network mounts (GVFS SMB shares under `/run/user/<uid>/gvfs/`)
//! - Removable media under /run/media/ or /media/
//!
//! The discovery primitives are split across themed submodules, mirroring macOS
//! `volumes/`; this module holds the shared model types, consts, and the
//! orchestrators that assemble them, and re-exports every submodule item so
//! `crate::volumes_linux::X` paths stay stable.

pub mod watcher;

mod cloud;
mod fs_type;
mod ids;
mod mounts;
mod smb;

#[cfg(test)]
mod test_support;

pub use fs_type::{get_volume_space, supports_trash_for_fs_type};
pub use mounts::get_mounted_volumes;
pub use smb::{SmbMountInfo, enrich_from_volume_registry, get_smb_mount_info};

pub(crate) use fs_type::get_mount_point;
pub(crate) use ids::volume_id_for_mount;
pub(crate) use smb::parse_gvfs_smb_dirname;

#[allow(
    unused_imports,
    reason = "API parity with macOS volumes module; used once SMB enrichment lands on Linux"
)]
pub use crate::file_system::volume::SmbConnectionState;

use crate::file_system::linux_mounts::{self, MountEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Category of a location item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LocationCategory {
    Favorite,
    MainVolume,
    AttachedVolume,
    CloudDrive,
    Network,
    MobileDevice,
}

/// Information about a location (volume, folder, or cloud drive).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: LocationCategory,
    pub icon: Option<String>,
    pub is_ejectable: bool,
    pub fs_type: Option<String>,
    pub supports_trash: bool,
    /// Whether the MOUNT behind this location refuses writes right now (for example, an MTP
    /// device reporting locked storage). Orthogonal to `capabilities.backend_can_write`, which
    /// answers for the BACKEND. Mirrors the macOS field; see `volumes/mod.rs`.
    pub mount_is_read_only: bool,
    /// Whether this volume is a mounted disk image (`.dmg`). Always `false` on Linux;
    /// mirrors the macOS shape so the shared `LocationInfo`/`VolumeInfo` type stays identical.
    pub is_disk_image: bool,
    /// SMB connection state indicator. Always `None` on Linux (no smb2 session tracking yet).
    pub smb_connection_state: Option<String>,
    /// Negotiated USB link speed. Set only for MTP/mobile volumes; everything
    /// else carries `None`. Frontend maps to a label like "USB 3.2 Gen 1".
    pub usb_speed: Option<crate::usb_speed::UsbSpeed>,
    /// What the backend registered for this volume can do (writable? can it be a
    /// copy source?), straight from `Volume::capabilities()`, so the frontend
    /// never re-derives capability from an id, an `fsType`, or a category.
    /// `None` when no backend is registered for this id (a favorite, or a volume
    /// discovery found before registration): the frontend falls back to its
    /// per-kind defaults. Filled by `enrich_from_volume_registry`, never by a
    /// discovery constructor.
    pub capabilities: Option<cmdr_fs::volume::VolumeCapabilities>,
}

/// Lets discovery collapse a filesystem mounted at several paths down to one
/// published location (`cmdr_fs::volume::canonical_root`). The macOS twin
/// implements the same trait on its own `LocationInfo`, which is what keeps the
/// rule shared without merging the two types.
impl cmdr_fs::volume::canonical_root::MountRootCandidate for LocationInfo {
    fn volume_id(&self) -> &str {
        &self.id
    }

    fn mount_root(&self) -> &str {
        &self.path
    }
}

/// Information about volume space.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

// Legacy compat alias
pub use LocationInfo as VolumeInfo;

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Get all locations organized by category, deduplicated by path AND by volume
/// ID.
///
/// The ID half matters because a volume ID is identity: one filesystem reachable
/// through two categories (a CIFS mount that's also GVFS-mounted) derives one ID
/// at two paths, and everything downstream keys on the ID. `get_mounted_volumes`
/// already collapses double mounts within its own category; this catches the
/// cross-category case.
pub fn list_locations() -> Vec<LocationInfo> {
    let mounts = linux_mounts::parse_proc_mounts();
    let mut locations = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    let mut push_unique = |locations: &mut Vec<LocationInfo>, loc: LocationInfo| {
        // Both inserts must run, so the sets can't drift apart on a partial hit.
        let new_path = seen_paths.insert(loc.path.clone());
        let new_id = seen_ids.insert(loc.id.clone());
        if new_path && new_id {
            locations.push(loc);
        }
    };

    // 1. Favorites
    for loc in get_favorites(&mounts) {
        push_unique(&mut locations, loc);
    }

    // 2. Main volume
    if let Some(loc) = get_main_volume(&mounts) {
        push_unique(&mut locations, loc);
    }

    // 3. Mounted volumes (real filesystems, excluding root and virtual)
    for loc in get_mounted_volumes(&mounts) {
        push_unique(&mut locations, loc);
    }

    // 4. Cloud drives
    for loc in cloud::get_cloud_drives(&mounts) {
        push_unique(&mut locations, loc);
    }

    // 5. Network mounts (GVFS SMB shares)
    for loc in smb::get_network_mounts() {
        push_unique(&mut locations, loc);
    }

    locations
}

/// Legacy compatibility wrapper.
pub fn list_mounted_volumes() -> Vec<LocationInfo> {
    list_locations()
}

/// Get the user's favorites from the editable store (`favorites.json`).
///
/// Maps each stored `{ id, path, name }` to a `LocationInfo` with `category: Favorite`. Seeds the
/// platform defaults on first launch (file absent); see `favorites/CLAUDE.md`. Linux has no TCC, so
/// there's no FDA-pending skip: every favorite is existence-checked.
fn get_favorites(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    crate::favorites::store::list()
        .into_iter()
        .filter(|favorite| Path::new(&favorite.path).exists())
        .map(|favorite| {
            let fs_type = linux_mounts::fs_type_for_path_from_entries(Path::new(&favorite.path), mounts);
            let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
            LocationInfo {
                id: format!("fav-{}", favorite.id),
                name: favorite.name,
                path: favorite.path.clone(),
                category: LocationCategory::Favorite,
                icon: None,
                is_ejectable: false,
                fs_type,
                supports_trash,
                mount_is_read_only: false,
                is_disk_image: false,
                smb_connection_state: None,
                usb_speed: None,
                capabilities: None,
            }
        })
        .collect()
}

/// Get the root filesystem as the main volume.
fn get_main_volume(mounts: &[MountEntry]) -> Option<LocationInfo> {
    let fs_type = linux_mounts::fs_type_for_path_from_entries(Path::new("/"), mounts);
    let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
    Some(LocationInfo {
        id: DEFAULT_VOLUME_ID.to_string(),
        name: "Root".to_string(),
        path: "/".to_string(),
        category: LocationCategory::MainVolume,
        icon: None,
        is_ejectable: false,
        fs_type,
        supports_trash,
        mount_is_read_only: false,
        is_disk_image: false,
        smb_connection_state: None,
        usb_speed: None,
        capabilities: None,
    })
}

/// Build a `VolumeInfo` for the volume containing `path` using only
/// mount table data. Does NOT call `list_locations()`.
pub fn resolve_path_volume_fast(path: &str) -> Option<VolumeInfo> {
    let (mount_point, fs_type) = get_mount_point(path)?;

    let name = mounts::mount_display_name(&mount_point);
    let supports_trash = supports_trash_for_fs_type(Some(&fs_type));
    let category = if mount_point == "/" {
        LocationCategory::MainVolume
    } else {
        LocationCategory::AttachedVolume
    };

    Some(VolumeInfo {
        id: volume_id_for_mount(&mount_point),
        name,
        path: mount_point,
        category,
        icon: None,
        is_ejectable: false,
        fs_type: Some(fs_type),
        supports_trash,
        mount_is_read_only: false,
        is_disk_image: false,
        smb_connection_state: None,
        usb_speed: None,
        capabilities: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::parse_test_mounts;

    #[test]
    fn test_get_main_volume() {
        let mounts = parse_test_mounts();
        let main = get_main_volume(&mounts);
        assert!(main.is_some());
        let main = main.unwrap();
        assert_eq!(main.id, "root");
        assert_eq!(main.path, "/");
        assert_eq!(main.category, LocationCategory::MainVolume);
        assert_eq!(main.fs_type.as_deref(), Some("ext4"));
    }
}
