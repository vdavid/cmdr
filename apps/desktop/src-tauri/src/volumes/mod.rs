//! Volume and location discovery for macOS.
//!
//! Provides a Finder-like location picker with:
//! - Favorites (from Finder sidebar)
//! - Main volume (Macintosh HD)
//! - Attached volumes (external drives)
//! - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
//! - Network locations
//!
//! The discovery primitives are split across theme submodules; this module holds
//! the shared model types, consts, and the orchestrators that assemble them, and
//! re-exports every submodule item so `crate::volumes::X` paths stay stable.

pub mod disk_image;
pub mod watcher;

mod cloud;
mod fs_type;
mod mounts;
mod nsurl;
mod smb;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

pub use crate::file_system::volume::SmbConnectionState;
pub(crate) use crate::file_system::volume::{path_to_id, smb_volume_id};

pub use cloud::get_cloud_drives;
pub(crate) use cloud::resolve_cloud_drive_for_path;
pub(crate) use fs_type::{get_fs_type, get_mount_point, read_only_from_statfs};
pub use fs_type::{is_network_fs_type, is_smb_fs_type, supports_trash_for_fs_type};
pub use mounts::get_attached_volumes;
pub use nsurl::{VolumeSpaceInfo, get_volume_space};
pub(crate) use nsurl::{get_bool_resource, get_icon_for_path, get_volume_name, volume_name_from_path};
pub use smb::{enrich_smb_connection_state, get_smb_mount_info};
pub(crate) use smb::{parse_smb_mount_source, volume_id_for_mount};

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
///
/// Serialized Rust → frontend. It also derives `Deserialize` because it rides inside
/// the typed `volumes-changed` event payload (`VolumesChanged`), and `tauri_specta::Event`
/// requires the payload (and its nested types) to round-trip.
/// Fields serialized as explicit `null` when absent so specta's `validate_exported_command`
/// accepts the type in Unified mode.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub category: LocationCategory,
    /// Base64-encoded WebP.
    pub icon: Option<String>,
    pub is_ejectable: bool,
    /// Filesystem type from `statfs` (for example, "apfs", "hfs", "smbfs").
    pub fs_type: Option<String>,
    /// Whether this volume supports macOS trash. Derived from `fs_type`.
    pub supports_trash: bool,
    /// Whether this location is read-only (for example, MTP devices with locked storage,
    /// or a read-only mounted volume). Powers the 🔒 indicator and the copy/move write guard.
    pub is_read_only: bool,
    /// Whether this volume is backed by a mounted disk image (a `.dmg`). Disk images are
    /// transient install-style mounts: the UI suppresses indexing (badge + first-connect
    /// prompt) and both free-space bars for them. Detected via DiskArbitration; see
    /// `disk_image::is_disk_image_mount`. Always `false` off macOS and for non-volume locations.
    pub is_disk_image: bool,
    /// SMB connection state indicator. Only set for volumes with an active `SmbVolume`.
    pub smb_connection_state: Option<SmbConnectionState>,
    /// Negotiated USB link speed. Set only for MTP/mobile volumes; everything
    /// else carries `None`. Frontend maps to a label like "USB 3.2 Gen 1" and a
    /// theoretical max MB/s for the volume switcher.
    pub usb_speed: Option<crate::usb_speed::UsbSpeed>,
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Volume ID for the iCloud Drive cloud drive entry. Hardcoded here so callers
/// outside this module (e.g. `friendly_error::friendly_error_for_restricted_empty_root`)
/// can match against it without a stringly-typed coupling. Renames break the build.
pub const ICLOUD_VOLUME_ID: &str = "cloud-icloud";

/// Build a `VolumeInfo` for the volume containing `path` using only
/// `statfs()` and per-path NSURL resource queries. Does NOT call
/// `list_locations()`. Avoids the blocking NSFileManager volume enumeration.
pub fn resolve_path_volume_fast(path: &str) -> Option<VolumeInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    // Cloud drives are plain folders on the data volume, so `statfs` below would
    // resolve them to `/` (Macintosh HD). Match the known cloud-drive roots first
    // so the switcher highlights the cloud drive the user is actually inside.
    if let Some(cloud) = resolve_cloud_drive_for_path(path) {
        return Some(cloud);
    }

    let (mount_point, fs_type) = get_mount_point(path)?;

    // Drain autoreleased ObjC objects (NSURL, NSString).
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&mount_point));

        let name = get_volume_name(&url, &mount_point);
        let is_ejectable = get_bool_resource(&url, "NSURLVolumeIsEjectableKey").unwrap_or(false);
        let supports_trash = supports_trash_for_fs_type(Some(&fs_type));
        let category = if mount_point == "/" {
            LocationCategory::MainVolume
        } else {
            LocationCategory::AttachedVolume
        };
        let icon = get_icon_for_path(&mount_point);
        let is_read_only = read_only_from_statfs(&mount_point);
        // Only attached, non-network volumes can be disk images; the boot volume never is.
        let is_disk_image = matches!(category, LocationCategory::AttachedVolume)
            && !is_smb_fs_type(Some(&fs_type))
            && disk_image::is_disk_image_mount(&mount_point);

        Some(VolumeInfo {
            id: volume_id_for_mount(&mount_point),
            name,
            path: mount_point,
            category,
            icon,
            is_ejectable,
            fs_type: Some(fs_type),
            supports_trash,
            is_read_only,
            is_disk_image,
            smb_connection_state: None,
            usb_speed: None,
        })
    })
}

/// Get all locations organized by category, deduplicated.
pub fn list_locations() -> Vec<LocationInfo> {
    let mut locations = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 1. Favorites
    for loc in get_favorites() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 2. Main volume
    if let Some(loc) = get_main_volume()
        && seen_paths.insert(loc.path.clone())
    {
        locations.push(loc);
    }

    // 3. Attached volumes
    for loc in get_attached_volumes() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 4. Cloud drives (skip if already in favorites)
    for loc in get_cloud_drives() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    locations
}

/// Get the user's favorites from the editable store (`favorites.json`).
///
/// Maps each stored `{ id, path, name }` to a `LocationInfo` with `category: Favorite`. Seeds the
/// four defaults on first launch (file absent); see `favorites/CLAUDE.md`.
fn get_favorites() -> Vec<LocationInfo> {
    let fda_pending = crate::fda_gate::is_fda_pending_runtime();

    crate::favorites::store::list()
        .into_iter()
        .filter(|favorite| {
            // While FDA is pending, MUST skip stat on TCC-protected paths: even `Path::exists()`
            // trips TCC for the protected-folder service once `permissions::check_full_disk_access`
            // has registered the bundle with tccd. We assume protected favorites exist (~/Desktop,
            // ~/Documents, ~/Downloads are present on essentially every account); if one really
            // doesn't, navigation surfaces a normal listing error. Non-protected paths are still
            // checked, since for example `/Applications` can be absent on slim systems.
            let protected =
                crate::restricted_paths::tcc_paths::is_potentially_tcc_restricted(Path::new(&favorite.path));
            (fda_pending && protected) || Path::new(&favorite.path).exists()
        })
        .map(|favorite| {
            // Favorites are folders on the boot volume, not mount points. statfs still works: it
            // reports the underlying volume's fs type.
            let fs_type = get_fs_type(&favorite.path);
            let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
            LocationInfo {
                id: format!("fav-{}", favorite.id),
                name: favorite.name,
                path: favorite.path.clone(),
                category: LocationCategory::Favorite,
                icon: get_icon_for_path(&favorite.path),
                is_ejectable: false,
                fs_type,
                supports_trash,
                is_read_only: false,
                is_disk_image: false,
                smb_connection_state: None,
                usb_speed: None,
            }
        })
        .collect()
}

/// Get the main boot volume.
///
/// Built directly from `/` with no volume enumeration: `statfs("/")` and the
/// NSURL name/icon lookups on the local root never block, so this is safe on the
/// main thread and immune to the hung-network-mount freeze that a full mount
/// enumeration would hit (see `DETAILS.md` § "Hung mounts").
fn get_main_volume() -> Option<LocationInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    // Drain autoreleased ObjC objects (NSURL, NSString). Called from
    // spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str("/"));
        let name = get_volume_name(&url, "/");
        let fs_type = get_fs_type("/");
        let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
        Some(LocationInfo {
            id: DEFAULT_VOLUME_ID.to_string(),
            name,
            path: "/".to_string(),
            category: LocationCategory::MainVolume,
            icon: get_icon_for_path("/"),
            is_ejectable: false,
            fs_type,
            supports_trash,
            is_read_only: false,
            is_disk_image: false,
            smb_connection_state: None,
            usb_speed: None,
        })
    })
}

// Legacy compatibility - maintain VolumeInfo type for backwards compatibility
pub use LocationInfo as VolumeInfo;

/// Legacy function - now calls list_locations
pub fn list_mounted_volumes() -> Vec<LocationInfo> {
    list_locations()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_locations_includes_root() {
        let locations = list_locations();
        assert!(!locations.is_empty(), "Should have at least one location");
        // Should have main volume
        assert!(
            locations.iter().any(|l| l.category == LocationCategory::MainVolume),
            "Should include main volume"
        );
    }

    #[test]
    fn test_locations_are_deduplicated() {
        let locations = list_locations();
        let mut seen_paths = HashSet::new();
        for loc in &locations {
            assert!(seen_paths.insert(&loc.path), "Duplicate path found: {}", loc.path);
        }
    }

    #[test]
    fn test_path_to_id() {
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/Volumes/External"), "volumesexternal");
    }

    #[test]
    fn test_resolve_path_volume_fast_root() {
        let result = resolve_path_volume_fast("/");
        assert!(result.is_some(), "Root should resolve to a VolumeInfo");
        let vol = result.unwrap();
        assert_eq!(vol.id, "root");
        assert_eq!(vol.path, "/");
        assert_eq!(vol.category, LocationCategory::MainVolume);
        assert!(vol.fs_type.is_some());
    }

    #[test]
    fn test_locations_have_fs_type_and_supports_trash() {
        let locations = list_locations();
        // Every location should have supports_trash set
        for loc in &locations {
            // Main volume and favorites on APFS should support trash
            if loc.category == LocationCategory::MainVolume {
                assert!(loc.fs_type.is_some(), "Main volume should have fs_type");
                assert!(loc.supports_trash, "Main volume should support trash");
            }
        }
    }
}
