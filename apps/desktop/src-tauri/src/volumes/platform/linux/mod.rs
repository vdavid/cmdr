//! Linux: volume and location discovery and live mount watching.
//!
//! Provides a sidebar location picker with:
//! - Favorites (Home, Desktop, Documents, Downloads)
//! - Main volume (root `/`)
//! - Mounted volumes (real filesystems from /proc/mounts)
//! - Cloud drives (Dropbox, Google Drive, Nextcloud, OneDrive)
//! - Network mounts (GVFS SMB shares under `/run/user/<uid>/gvfs/`)
//! - Removable media under /run/media/ or /media/
//!
//! The discovery primitives are split across themed submodules, mirroring the macOS arm; this
//! module holds the orchestrators that assemble them, and re-exports what the seam reaches.

// Reached by the seam directly, ❌ never through this root: the watcher announces through
// `volume_broadcast`, which lists through this root, so a re-export here closes a module cycle.
pub(crate) mod watcher;

mod cloud;
mod fs_type;
mod ids;
mod mounts;
mod network_mounts;
mod smb;

#[cfg(test)]
mod test_support;

pub(crate) use cmdr_local::linux_mounts::{
    device_is_mounted as has_mount_identity, mount_device_at as mount_identity_at,
};
pub(crate) use cmdr_local::space::get_volume_space as live_volume_space;
pub(crate) use fs_type::get_mount_point;
pub(crate) use ids::volume_id_for_mount;
pub(crate) use mounts::{mount_rows, registrable_mount_roots};
pub(crate) use smb::{enrich_from_volume_registry, get_smb_mount_info, parse_gvfs_smb_dirname, smb_mounts};
// `/proc/mounts` and a GVFS folder name are both the table, so one reader answers both questions.
pub(crate) use smb::get_smb_mount_info as smb_mount_info_at;

use cmdr_fs::volume::published_locations::dedupe_locations;
use cmdr_local::linux_mounts::{self, MountEntry};
use fs_type::supports_trash_for_mount;
use mounts::get_mounted_volumes;
use std::path::Path;

use crate::volumes::{DEFAULT_VOLUME_ID, LocationCategory, LocationInfo, SmbMountInfo, VolumeInfo};

/// What the synthetic `network` volume reports as its `fs_type`: what Linux calls an SMB mount.
pub(crate) const NETWORK_FS_TYPE: &str = "cifs";

/// The `(fs type, source)` of the mount `path` lexically lies on, from `/proc/mounts` alone.
pub(crate) fn mount_type_and_source_for(path: &Path) -> Option<(String, String)> {
    linux_mounts::mount_entry_for_path(path).map(|entry| (entry.fstype, entry.device))
}

/// Get all locations organized by category, deduplicated.
///
/// Gathers favorites, the main volume, mounted volumes (real filesystems,
/// excluding root and virtual), cloud drives, and GVFS SMB shares in that
/// order, then dedupes through `cmdr_fs::volume::published_locations` (shared
/// with macOS), whose header says which row wins a clash. The ID half of that
/// rule catches one filesystem reachable through two categories (a CIFS mount
/// that's also GVFS-mounted); `get_mounted_volumes` already collapses double
/// mounts within its own category.
pub fn list_locations() -> Vec<LocationInfo> {
    // An unreadable table lists no attached volumes; favorites, root, cloud
    // drives, and GVFS shares don't come from it, so they still show.
    let mounts = linux_mounts::parse_proc_mounts().unwrap_or_default();
    let locations = get_favorites(&mounts)
        .into_iter()
        .chain(get_main_volume(&mounts))
        .chain(get_mounted_volumes(&mounts))
        .chain(cloud::get_cloud_drives(&mounts))
        .chain(network_mounts::get_network_mounts());
    dedupe_locations(locations)
}

/// Get the user's favorites from the editable store (`favorites.json`), one row each.
///
/// ❗ Every stored favorite publishes a row, a missing one included; the reach pass
/// (`favorites/reach.rs`) words it. Seeds the platform defaults on first launch (file absent); see
/// `favorites/CLAUDE.md`.
fn get_favorites(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    crate::favorites::store::list()
        .into_iter()
        .map(|favorite| favorite_location(favorite, mounts, |path| path.exists()))
        .collect()
}

/// One favorite's discovery row, the twin of macOS `volumes::favorite_location`. ❗ No syscall on a
/// network path, ever: the containing mount's type comes from the parsed table, and only a folder
/// on a local disk is stat'd (`exists`, injected so a test can prove which ones aren't). Linux has
/// no TCC, so there's no FDA-pending skip.
fn favorite_location(
    favorite: crate::favorites::store::Favorite,
    mounts: &[MountEntry],
    exists: impl FnOnce(&Path) -> bool,
) -> LocationInfo {
    use crate::favorites::target::{FavoriteTarget, Probe, on_disk};

    let path = Path::new(&favorite.path);
    // ❗ Only an OS path has a containing mount: the table lookup matches `/` for anything.
    let mount = path
        .is_absolute()
        .then(|| linux_mounts::mount_entry_for_path_from_entries(path, mounts))
        .flatten();
    let fs_type = mount.map(|mount| mount.fstype.clone());
    let probe = match fs_type.as_deref().is_some_and(linux_mounts::is_network_fs_type) {
        true => Probe::NetworkMount,
        false => Probe::Stat,
    };
    let on_disk = on_disk(&favorite.path, probe, exists);
    LocationInfo {
        id: format!("fav-{}", favorite.id),
        name: favorite.name,
        // A scheme path has no mount, so no OS trash either.
        supports_trash: mount.is_some_and(|mount| supports_trash_for_mount(Some(&mount.device), Some(&mount.fstype))),
        path: favorite.path,
        category: LocationCategory::Favorite,
        icon: None,
        is_ejectable: false,
        fs_type,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: favorite.shortcut,
        favorite_target: Some(FavoriteTarget::discovered(favorite.volume, on_disk)),
        root_label: None,
        mount_account: None,
    }
}

/// Get the root filesystem as the main volume.
fn get_main_volume(mounts: &[MountEntry]) -> Option<LocationInfo> {
    let mount = linux_mounts::mount_entry_for_path_from_entries(Path::new("/"), mounts);
    let fs_type = mount.map(|mount| mount.fstype.clone());
    let supports_trash = mount.is_some_and(|mount| supports_trash_for_mount(Some(&mount.device), Some(&mount.fstype)));
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
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    })
}

/// Build a `VolumeInfo` for the volume containing `path` using only
/// mount table data. Does NOT call `list_locations()`.
pub fn resolve_path_volume_fast(path: &str) -> Option<VolumeInfo> {
    // A GVFS share never reaches the mount table (one FUSE mount serves them all), so the
    // walk below would answer that FUSE root: the share's own row, as discovery lists it.
    if let Some(root) = smb::gvfs_share_root(path) {
        let dirname = Path::new(root).file_name()?.to_str()?;
        let gvfs = parse_gvfs_smb_dirname(dirname)?;
        return Some(network_mounts::gvfs_share_location(root.to_string(), gvfs.share));
    }
    let (mount_point, fs_type, source) = get_mount_point(path)?;

    let name = mounts::mount_display_name(&mount_point);
    let supports_trash = supports_trash_for_mount(Some(&source), Some(&fs_type));
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
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    })
}

/// The twin of macOS `volumes::network_mount_under`, named as `resolve_path_volume_fast` names it.
pub(crate) fn network_mount_under(path: &Path) -> Option<crate::volumes::NetworkMount> {
    network_mount_in(&linux_mounts::parse_proc_mounts()?, path)
}

/// [`network_mount_under`] over a table already read.
///
/// ❗ A GVFS SMB share answers its own folder, the way `resolve_path_volume_fast` does, ❌ never the
/// FUSE root the table names: one FUSE mount serves every share, and its root is a volume no row
/// lists, so a favorite saved on it reads "unplugged" (`DETAILS.md` § "A path inside a GVFS share
/// resolves to the share").
fn network_mount_in(mounts: &[MountEntry], path: &Path) -> Option<crate::volumes::NetworkMount> {
    if let Some(root) = path.to_str().and_then(smb::gvfs_share_root) {
        let dirname = Path::new(root).file_name()?.to_str()?;
        return Some(crate::volumes::NetworkMount {
            mount_point: root.to_string(),
            name: parse_gvfs_smb_dirname(dirname)?.share,
        });
    }
    let entry = linux_mounts::mount_entry_for_path_from_entries(path, mounts)?;
    if cmdr_fs::filesystem_kind::mount_is_local_disk(Some(&entry.device), Some(&entry.fstype)) {
        return None;
    }
    Some(crate::volumes::NetworkMount {
        mount_point: entry.mountpoint.clone(),
        name: mounts::mount_display_name(&entry.mountpoint),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::parse_test_mounts;

    fn proc_mount(device: &str, mountpoint: &str, fstype: &str) -> MountEntry {
        MountEntry {
            device: device.to_string(),
            mountpoint: mountpoint.to_string(),
            fstype: fstype.to_string(),
            options: String::new(),
        }
    }

    #[test]
    fn network_mount_in_names_the_share_and_skips_local_disks() {
        let mounts = vec![
            proc_mount("/dev/sda1", "/", "ext4"),
            proc_mount("//nas/media", "/mnt/media", "cifs"),
            proc_mount("/dev/sdb1", "/mnt/media-1", "ext4"),
        ];
        let network = |path: &str| network_mount_in(&mounts, Path::new(path));
        assert_eq!(
            network("/mnt/media/films"),
            Some(crate::volumes::NetworkMount {
                mount_point: "/mnt/media".to_string(),
                name: "media".to_string(),
            })
        );
        assert_eq!(network("/mnt/media-1/x"), None);
        assert_eq!(network("/home/me"), None);
    }

    /// ❗ One FUSE mount serves every GVFS share, so the table's own answer for a folder on one is
    /// that FUSE root. The add gate would then favorite the folder on the root's volume, which no
    /// row lists ("unplugged"), rather than on the share's (`favorites-offline-smb.spec.ts`).
    #[test]
    fn network_mount_in_names_a_gvfs_share_rather_than_the_fuse_root() {
        let mounts = vec![
            proc_mount("/dev/sda1", "/", "ext4"),
            proc_mount("gvfsd-fuse", "/run/user/1000/gvfs", "fuse.gvfsd-fuse"),
        ];
        let share = "/run/user/1000/gvfs/smb-share:server=nas,share=public";
        assert_eq!(
            network_mount_in(&mounts, Path::new(&format!("{share}/docs/2026"))),
            Some(crate::volumes::NetworkMount {
                mount_point: share.to_string(),
                name: "public".to_string(),
            })
        );
        // Anything else GVFS serves has no row of its own, and the resolver answers the FUSE root.
        assert_eq!(
            network_mount_in(&mounts, Path::new("/run/user/1000/gvfs/sftp:host=nas/home")),
            Some(crate::volumes::NetworkMount {
                mount_point: "/run/user/1000/gvfs".to_string(),
                name: mounts::mount_display_name("/run/user/1000/gvfs"),
            })
        );
    }

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

    // -- favorites discovery --

    use crate::favorites::store::Favorite;
    use crate::favorites::target::{FavoriteReach, OnDisk};

    fn favorite(path: &str) -> Favorite {
        Favorite {
            id: "f1".to_string(),
            path: path.to_string(),
            name: "docs".to_string(),
            shortcut: None,
            volume: None,
        }
    }

    fn with_a_share() -> Vec<MountEntry> {
        linux_mounts::parse_proc_mounts_from_content(
            "/dev/sda1 / ext4 rw,relatime 0 0\n//nas/naspi /mnt/naspi cifs rw,relatime 0 0\n",
        )
    }

    fn never_asked(path: &Path) -> bool {
        panic!("discovery must not stat {path:?}")
    }

    /// ❗ The hung-mount guard: a favorite on a wedged CIFS share gets no `exists()`.
    #[test]
    fn a_favorite_on_a_network_mount_is_listed_without_touching_it() {
        let row = favorite_location(favorite("/mnt/naspi/docs"), &with_a_share(), never_asked);
        assert_eq!(row.fs_type.as_deref(), Some("cifs"));
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::Unchecked);
    }

    /// ❗ No favorite is filtered out any more: a missing folder still publishes its row.
    #[test]
    fn a_missing_local_favorite_still_publishes_a_row() {
        let row = favorite_location(favorite("/home/nobody/gone"), &with_a_share(), |_| false);
        assert_eq!(row.id, "fav-f1");
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::No);
        assert_eq!(target.reach, FavoriteReach::NotFound);
    }

    #[test]
    fn a_scheme_path_favorite_has_no_mount() {
        let row = favorite_location(favorite("sftp://ada@nas:22/srv"), &with_a_share(), never_asked);
        assert_eq!(row.fs_type, None);
        assert!(!row.supports_trash);
    }
}
