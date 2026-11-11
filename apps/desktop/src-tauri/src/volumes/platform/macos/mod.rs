//! macOS: volume and location discovery and live mount watching.
//!
//! Provides a Finder-like location picker with:
//! - Favorites (from Finder sidebar)
//! - Main volume (Macintosh HD)
//! - Attached volumes (external drives)
//! - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
//! - Network locations
//!
//! The discovery primitives are split across theme submodules; this module holds
//! the orchestrators that assemble them, and re-exports every submodule item the
//! seam or a macOS-only caller reaches through `crate::volumes`.

pub mod disk_image;
pub(crate) mod unmount_approver;
pub mod watcher;

mod cloud;
pub(crate) mod disk_units;
mod fs_type;
mod ids;
mod mounts;
mod nsurl;
mod smb;

mod lifecycle;
#[cfg(test)]
mod rename_real_image;

use cmdr_fs::volume::published_locations::dedupe_locations;
use std::path::Path;

use crate::volumes::{DEFAULT_VOLUME_ID, LocationCategory, LocationInfo, SmbMountInfo, VolumeInfo};

pub(crate) use crate::file_system::volume::ConnectionState;
pub(crate) use cloud::{get_cloud_drives, resolve_cloud_drive_for_path};
pub(crate) use cmdr_local::live_space::live_volume_space;
pub(crate) use fs_type::{
    get_fs_type, get_mount_point, is_network_fs_type, is_smb_fs_type, read_only_from_statfs, supports_trash_for_mount,
};
pub(crate) use ids::{volume_id_for, volume_id_for_mount};
pub(crate) use mounts::{
    get_attached_volumes, has_mount_identity, is_private_to_another_user, mount_identity_at, mount_points_with_rows,
    mount_rows, mount_type_and_source_for, network_mount_under, registrable_mount_roots, smb_mount_info_at, smb_mounts,
};
pub(crate) use nsurl::{
    get_icon_for_path, get_volume_name, get_volume_uuid, get_volume_uuid_for_path, is_volume_ejectable,
    volume_name_from_path,
};
pub(crate) use smb::{enrich_from_volume_registry, get_smb_mount_info, parse_smb_mount_source};

/// What the synthetic `network` volume reports as its `fs_type`: what macOS calls an SMB mount.
pub(crate) const NETWORK_FS_TYPE: &str = "smbfs";

/// APFS and HFS+ fold case by default. A case-sensitive APFS volume exists, but telling it apart
/// takes a `pathconf` on the mount, which can hang on a share, so every macOS row reads as the
/// default: the stated assumption.
pub(crate) const LOCAL_DISK_CASE_SENSITIVE: bool = false;

/// Whether a MOUNT ROOT is served by a cloud provider's own filesystem, so its
/// entries cost a round trip to that provider's daemon rather than a disk read.
///
/// True for pCloud's `pcloudfs`, CloudMounter's `~/.CMVolumes`, and the rest of
/// `cmdr_fs::volume::friendly_error::Provider::is_cloud_storage`; false for an
/// ordinary disk, a `.dmg`, a VeraCrypt container, and a plain macFUSE mount.
///
/// ❗ Ask this about a mount root, ❌ never an arbitrary path: every path under
/// `~/Library/CloudStorage` names a provider too, but those are FOLDERS on the
/// data volume, which the cloud-drive arm publishes and the drive index reads at
/// local speed. Conflating the two would stop Cmdr indexing a Dropbox folder it
/// has always indexed happily.
///
/// `fs_type` is the type the mount table listed, so this makes no syscall and a
/// hung network mount can't stall discovery through it.
pub(crate) fn is_cloud_provider_mount(mount_root: &str, fs_type: &str) -> bool {
    mount_provider(mount_root, fs_type).is_some_and(|provider| provider.is_cloud_storage())
}

/// Which provider serves the mount at `mount_root`, from the fs type the mount
/// table listed it with: no syscall. [`is_cloud_provider_mount`] asks whether
/// that provider is cloud storage; `mounts::is_user_facing_mount` asks only
/// whether there's one at all.
pub(crate) fn mount_provider(mount_root: &str, fs_type: &str) -> Option<cmdr_fs::volume::friendly_error::Provider> {
    cmdr_fs::volume::friendly_error::provider_for_mount(Path::new(mount_root), fs_type)
}

/// Volume ID for the iCloud Drive cloud drive entry. Hardcoded here so callers
/// outside this module (e.g. `friendly_error::listing_error_for_restricted_empty_root`)
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

    let (mount_point, fs_type, source) = get_mount_point(path)?;

    // Drain autoreleased ObjC objects (NSURL, NSString).
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&mount_point));

        let smb = get_smb_mount_info(&mount_point);
        let name = resolved_volume_name(smb.as_ref(), || get_volume_name(&url, &mount_point));
        // Local mounts only, as `get_attached_volumes` answers it: a network mount ends through
        // its session, and the lookup can hang on a dead one.
        let is_ejectable = !is_network_fs_type(Some(&fs_type)) && is_volume_ejectable(&url, &mount_point);
        let supports_trash = supports_trash_for_mount(Some(&source), Some(&fs_type));
        // Same two answers `get_attached_volumes` gives this mount, from the same
        // predicate: a switcher whose checkmark lands on a CLOUD row while the pane
        // calls the volume an attached drive is the drift this pairing prevents.
        let is_cloud_mount = is_cloud_provider_mount(&mount_point, &fs_type);
        let category = match (mount_point.as_str(), is_cloud_mount) {
            ("/", _) => LocationCategory::MainVolume,
            (_, true) => LocationCategory::CloudDrive,
            (_, false) => LocationCategory::AttachedVolume,
        };
        let icon = get_icon_for_path(&mount_point);
        let mount_is_read_only = read_only_from_statfs(&mount_point);
        // Only attached, non-network volumes can be disk images; the boot volume never is.
        let is_disk_image = matches!(category, LocationCategory::AttachedVolume)
            && !is_smb_fs_type(Some(&fs_type))
            && disk_image::is_disk_image_mount(&mount_point);
        // Ask for a UUID only where `get_attached_volumes` does (local mounts), or
        // the same volume would get two different IDs depending on which path
        // discovered it. A network mount's UUID probe can also hang.
        let uuid = match smb.is_some() || is_network_fs_type(Some(&fs_type)) {
            true => None,
            false => get_volume_uuid(&url),
        };

        Some(VolumeInfo {
            id: volume_id_for(&mount_point, Some(&fs_type), smb.as_ref(), uuid.as_deref()),
            name,
            path: mount_point,
            category,
            icon,
            is_ejectable,
            fs_type: Some(fs_type),
            supports_trash,
            mount_is_read_only,
            is_disk_image,
            shares_disk_with_others: false,
            is_cloud_mount,
            case_sensitive: LOCAL_DISK_CASE_SENSITIVE,
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
    })
}

/// The name [`resolve_path_volume_fast`] gives a volume: an SMB share's the switcher row's
/// ("private on localhost:11482", `mounts::smb_share_name`), so a favorite added there stores the
/// name that says which server it means; any other volume its NSURL name (`local`).
fn resolved_volume_name(smb: Option<&SmbMountInfo>, local: impl FnOnce() -> String) -> String {
    match smb {
        Some(info) => mounts::smb_share_name(info),
        None => local(),
    }
}

/// Get all locations organized by category, deduplicated.
///
/// Gathers favorites, the main volume, attached volumes, and cloud drives in
/// that order, then dedupes through `cmdr_fs::volume::published_locations`
/// (shared with the Linux arm), whose header says which row wins a clash.
pub fn list_locations() -> Vec<LocationInfo> {
    // One `getfsstat(MNT_NOWAIT)` snapshot for the whole listing. A table nobody could read means
    // no attached rows this pass (the switcher keeps what it has and the next discovery asks
    // again); ❌ nothing downstream may read that as "the disk is empty".
    let mounts = cmdr_local::mount_table::enumerate_mounts().unwrap_or_default();
    let locations = get_favorites(&mounts)
        .into_iter()
        .chain(get_main_volume())
        .chain(mounts::attached_volumes_in(&mounts))
        .chain(get_cloud_drives());
    dedupe_locations(locations)
}

/// Get the user's favorites from the editable store (`favorites.json`), one row each.
///
/// ❗ Every stored favorite publishes a row, a missing one included: the reach pass
/// (`favorites/reach.rs`) words what a pick would do, and hiding is what made favorites on an
/// offline share silently vanish. Seeds the defaults on first launch (file absent); see
/// `favorites/CLAUDE.md`.
fn get_favorites(mounts: &[cmdr_local::mount_table::MountEntry]) -> Vec<LocationInfo> {
    let fda_pending = crate::fda_gate::is_fda_pending_runtime();
    crate::favorites::store::list()
        .into_iter()
        .map(|favorite| favorite_location(favorite, mounts, fda_pending, |path| path.exists()))
        .collect()
}

/// One favorite's discovery row. ❗ No syscall on a network path, ever: the containing mount's
/// type comes from the snapshot, and only a folder on a local disk is stat'd (`exists`, injected
/// so a test can prove which ones aren't). The NSWorkspace icon is asked only for a folder that's
/// there, since it's a call on the path too.
///
/// While FDA is pending a TCC-protected folder is taken on trust with no stat: even
/// `Path::exists()` trips TCC once `permissions::check_full_disk_access` has registered the bundle
/// with tccd.
fn favorite_location(
    favorite: crate::favorites::store::Favorite,
    mounts: &[cmdr_local::mount_table::MountEntry],
    fda_pending: bool,
    exists: impl FnOnce(&Path) -> bool,
) -> LocationInfo {
    use crate::favorites::target::{FavoriteTarget, OnDisk, Probe, on_disk};

    let path = Path::new(&favorite.path);
    let mount = path.is_absolute().then(|| mounts::mount_under(mounts, path)).flatten();
    let fs_type = mount.map(|mount| mount.fs_type.clone());
    let probe = if is_network_fs_type(fs_type.as_deref()) {
        Probe::NetworkMount
    } else if fda_pending && crate::restricted_paths::tcc_paths::is_potentially_tcc_restricted(path) {
        Probe::TakenOnTrust
    } else {
        Probe::Stat
    };
    let on_disk = on_disk(&favorite.path, probe, exists);
    let icon = match on_disk {
        OnDisk::Yes => get_icon_for_path(&favorite.path),
        // Not on trust: an NSWorkspace icon read is one of the calls the FDA gate holds back.
        OnDisk::Assumed | OnDisk::No | OnDisk::Unchecked => None,
    };
    LocationInfo {
        id: format!("fav-{}", favorite.id),
        name: favorite.name,
        // A scheme path has no mount, so no OS trash either.
        supports_trash: mount
            .is_some_and(|mount| supports_trash_for_mount(Some(&mount.mount_from), Some(&mount.fs_type))),
        path: favorite.path,
        category: LocationCategory::Favorite,
        icon,
        is_ejectable: false,
        fs_type,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        case_sensitive: LOCAL_DISK_CASE_SENSITIVE,
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
        // The boot volume's type (APFS, HFS+) answers the rule without its source.
        let supports_trash = supports_trash_for_mount(None, fs_type.as_deref());
        Some(LocationInfo {
            id: DEFAULT_VOLUME_ID.to_string(),
            name,
            path: "/".to_string(),
            category: LocationCategory::MainVolume,
            icon: get_icon_for_path("/"),
            is_ejectable: false,
            fs_type,
            supports_trash,
            mount_is_read_only: false,
            is_disk_image: false,
            shares_disk_with_others: false,
            is_cloud_mount: false,
            case_sensitive: LOCAL_DISK_CASE_SENSITIVE,
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
    })
}

/// A row with nothing set, for a test that fills in what it reads.
#[cfg(test)]
pub(crate) fn location_for_test() -> LocationInfo {
    LocationInfo {
        id: String::new(),
        name: String::new(),
        path: String::new(),
        category: LocationCategory::AttachedVolume,
        icon: None,
        is_ejectable: false,
        fs_type: None,
        supports_trash: false,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        case_sensitive: LOCAL_DISK_CASE_SENSITIVE,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// ❗ The resolver names an SMB share the way its switcher row does, or a favorite added on it
    /// stores the bare share name and an offline pick says "private" without saying which server.
    #[test]
    fn the_resolver_names_an_smb_share_like_its_switcher_row() {
        let info = parse_smb_mount_source("//localhost:11482/private").expect("an SMB source");
        assert_eq!(
            resolved_volume_name(Some(&info), || "private".to_string()),
            "private on localhost:11482"
        );
        assert_eq!(resolved_volume_name(None, || "Backup".to_string()), "Backup");
    }

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
        // IDs matter more than paths: an ID is identity (index DB, saved paths,
        // registry routing, and the frontend's keyed lists), and a doubly-mounted
        // share has two distinct paths under one ID, so a path-only assertion
        // passes on exactly the case that breaks the app. Favorites are exempt
        // from the path half: one may point at a volume's own root.
        let locations = list_locations();
        let mut seen_paths = HashSet::new();
        let mut seen_ids = HashSet::new();
        for loc in &locations {
            if loc.category != LocationCategory::Favorite {
                assert!(seen_paths.insert(&loc.path), "Duplicate path found: {}", loc.path);
            }
            assert!(
                seen_ids.insert(&loc.id),
                "Duplicate volume ID found: {} (at {})",
                loc.id,
                loc.path
            );
        }
    }

    // -- favorites discovery --

    use crate::favorites::store::{Favorite, FavoriteVolume};
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

    fn snapshot() -> Vec<cmdr_local::mount_table::MountEntry> {
        vec![
            mounts::mount_entry_for_test("/", "apfs"),
            mounts::mount_entry_for_test("/Volumes/naspi", "smbfs"),
        ]
    }

    fn never_asked(path: &Path) -> bool {
        panic!("discovery must not stat {path:?}")
    }

    /// ❗ The hung-mount guard: a favorite on a wedged share must not cost `list_locations` its
    /// 2 s budget, so a folder under a network mount in the snapshot gets no `exists()`, no icon,
    /// and no `statfs` (its type comes from the snapshot).
    #[test]
    fn a_favorite_on_a_network_mount_is_listed_without_touching_it() {
        let row = favorite_location(favorite("/Volumes/naspi/docs"), &snapshot(), false, never_asked);
        assert_eq!(row.fs_type.as_deref(), Some("smbfs"));
        assert!(row.icon.is_none());
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::Unchecked);
    }

    /// ❗ No favorite is filtered out any more: a missing folder still publishes its row, and the
    /// reach pass words it.
    #[test]
    fn a_missing_local_favorite_still_publishes_a_row() {
        let mut gone = favorite("/Users/nobody/gone");
        gone.volume = Some(FavoriteVolume {
            id: "root".to_string(),
            root: "/".to_string(),
            name: "Macintosh HD".to_string(),
        });
        let row = favorite_location(gone, &snapshot(), false, |_| false);
        assert_eq!(row.id, "fav-f1");
        assert_eq!(row.fs_type.as_deref(), Some("apfs"));
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::No);
        assert_eq!(target.reach, FavoriteReach::NotFound);
        assert_eq!(target.volume_id.as_deref(), Some("root"));
    }

    /// While the FDA gate is pending, a TCC-protected favorite is taken on trust: even `exists()`
    /// would raise a system popup over onboarding.
    #[test]
    fn a_protected_favorite_is_not_stat_while_fda_is_pending() {
        let home = dirs::home_dir().expect("a home dir");
        let desktop = home.join("Desktop").to_string_lossy().into_owned();
        let row = favorite_location(favorite(&desktop), &snapshot(), true, never_asked);
        let target = row.favorite_target.expect("a favorite row carries its target");
        assert_eq!(target.discovered.on_disk, OnDisk::Assumed);
        assert_eq!(target.reach, FavoriteReach::Ready);
    }

    /// ❗ First-launch onboarding: the seeded `~/Desktop` has no stored volume yet and is
    /// TCC-protected, so discovery can't look. Taken on trust, it must still read `Ready` and be
    /// claimed by the boot volume, ❌ never greyed "not found" for the whole onboarding.
    #[test]
    fn a_seeded_protected_favorite_is_ready_and_claimed_while_fda_is_pending() {
        let home = dirs::home_dir().expect("a home dir");
        let desktop = home.join("Desktop").to_string_lossy().into_owned();
        let mut rows = vec![
            LocationInfo {
                id: DEFAULT_VOLUME_ID.to_string(),
                ..favorite_location(favorite("/"), &snapshot(), false, |_| true)
            },
            favorite_location(favorite(&desktop), &snapshot(), true, never_asked),
        ];
        rows[0].category = LocationCategory::MainVolume;
        rows[0].favorite_target = None;
        let facts = crate::favorites::reach::ReachFacts {
            mtp_enabled: true,
            adb_enabled: true,
        };
        let claims = crate::favorites::reach::annotate(&mut rows, &facts);
        let target = rows[1]
            .favorite_target
            .as_ref()
            .expect("a favorite row carries its target");
        assert_eq!(target.reach, FavoriteReach::Ready);
        assert_eq!(target.volume_id.as_deref(), Some(DEFAULT_VOLUME_ID));
        assert_eq!(claims.len(), 1, "the boot volume claims it");
    }

    #[test]
    fn a_scheme_path_favorite_is_listed_without_a_mount() {
        let row = favorite_location(favorite("sftp://ada@nas:22/srv"), &snapshot(), false, never_asked);
        assert_eq!(row.fs_type, None);
        assert!(!row.supports_trash);
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
