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
pub(crate) mod unmount_approver;
pub mod watcher;

mod cloud;
pub(crate) mod disk_units;
mod fs_type;
mod ids;
mod live_space;
mod mounts;
mod nsurl;
mod smb;

mod rename;
#[cfg(all(test, target_os = "macos"))]
mod rename_real_image;

use cmdr_fs::volume::published_locations::{PublishedLocation, dedupe_locations};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use crate::file_system::volume::ConnectionState;

pub use cloud::get_cloud_drives;
pub(crate) use cloud::resolve_cloud_drive_for_path;
pub(crate) use fs_type::{get_fs_type, get_mount_info, get_mount_point, read_only_from_statfs};
pub use fs_type::{is_network_fs_type, is_smb_fs_type, supports_trash_for_fs_type};
pub(crate) use ids::{volume_id_for, volume_id_for_mount};
pub use live_space::{expect_space_change, live_volume_space};
pub use mounts::get_attached_volumes;
pub(crate) use mounts::{
    has_mount_identity, is_mount_point, is_private_to_another_user, mount_identity_at, mount_roots,
    mount_type_and_source_for, registrable_mount_roots, smb_mounts,
};
pub use nsurl::get_volume_space;
pub(crate) use nsurl::{
    get_icon_for_path, get_volume_name, get_volume_uuid, get_volume_uuid_for_path, is_volume_ejectable,
    volume_name_from_path,
};
pub(crate) use smb::parse_smb_mount_source;
pub use smb::{SmbMountInfo, enrich_from_volume_registry, get_smb_mount_info};

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
    /// Whether the MOUNT behind this location refuses writes right now: a read-only `.dmg`, a
    /// write-protected card, an MTP device reporting locked storage. From `MNT_RDONLY` via
    /// `read_only_from_statfs` for real mounts. Powers the 🔒 indicator and the copy/move write
    /// guard.
    ///
    /// Orthogonal to `capabilities.backend_can_write`, which answers whether the BACKEND serving
    /// this volume implements mutations at all. Both combinations happen: a writable backend on a
    /// read-only mount, and a read-only backend (`ArchiveVolume`) on a perfectly writable disk.
    pub mount_is_read_only: bool,
    /// Whether this volume is backed by a mounted disk image (a `.dmg`). Disk images are
    /// transient install-style mounts: the UI suppresses indexing (badge + first-connect
    /// prompt) and both free-space bars for them. Detected via DiskArbitration; see
    /// `disk_image::is_disk_image_mount`. Always `false` off macOS and for non-volume locations.
    pub is_disk_image: bool,
    /// Whether a cloud provider's own filesystem serves this mount (pCloud's `pcloudfs`,
    /// CloudMounter, …), so every entry read costs a round trip to that provider's daemon.
    /// Groups the row under CLOUD and, like `is_disk_image`, suppresses the index affordances:
    /// a drive index here would walk the provider's whole service to build something another
    /// device's sync invalidates. Set by `is_cloud_provider_mount` in BOTH `get_attached_volumes`
    /// and `resolve_path_volume_fast`, or the two drift.
    ///
    /// ❗ `false` for a `~/Library/CloudStorage` folder, which is an ordinary directory on the
    /// data volume that the cloud-drive arm publishes and the index reads at local speed.
    pub is_cloud_mount: bool,
    /// How live this volume's SESSION is: the switcher dot, the pane's connect
    /// views, and the reconnect subscription all read it. Set for every volume a
    /// connecting backend serves (SMB, SFTP, WebDAV, ADB) plus a saved-but-not-
    /// connected server; `None` for a local disk, a favorite, and the hub row.
    /// ❗ Not an "is this SMB" test — that is `Volume::backend_kind()`, backend-side.
    pub connection_state: Option<ConnectionState>,
    /// Whether the DEVICE behind this row is reachable, which is a different
    /// question from how live a session is. Set by the device providers only: a
    /// phone waiting for its "Allow USB debugging?" tap is present and must never
    /// start a reconnect backoff. `None` for everything that isn't a device.
    /// Whether this place belongs in the volume SWITCHER: the user's own cap on
    /// how many saved things crowd their disks. `Some` only on a server place
    /// (`server_volumes.rs`), which is the only row the cap applies to; `None`
    /// on a local disk, a favorite, and a mounted SMB share, all of which show
    /// unconditionally.
    ///
    /// ❗ The listing publishes EVERY saved place regardless, because a volume id
    /// with no row is one the app denies exists (a hub Enter and a restored tab
    /// both land on an id). Hiding is `navigation/volume-grouping.ts`'s job, and
    /// this field is what it reads.
    pub pinned: Option<bool>,
    /// Where opening this place lands, as an app path, when that isn't `path`: a
    /// server place's saved start folder (`server_volumes.rs`). `None` lands at
    /// `path`, and every row that isn't a server place carries `None`.
    ///
    /// ❗ Rides on the row so a pane picking a saved place goes straight to its
    /// landing without asking the store, and so the switcher and the pane read
    /// one spelling of it.
    pub landing_path: Option<String>,
    pub device_readiness: Option<cmdr_fs::volume::DeviceReadiness>,
    /// Negotiated USB link speed. Set only for MTP/mobile volumes; everything
    /// else carries `None`. Frontend maps to a label like "USB 3.2 Gen 1" and a
    /// theoretical max MB/s for the volume switcher.
    pub usb_speed: Option<crate::usb_speed::UsbSpeed>,
    /// What the backend registered for this volume can do (writable? can it be a
    /// copy source?), straight from `Volume::capabilities()`, so the frontend
    /// never re-derives capability from an id, an `fsType`, or a category.
    /// `None` when no backend is registered for this id (a favorite, or a volume
    /// discovery found before registration): the frontend falls back to its
    /// per-kind defaults. Filled by `enrich_from_volume_registry`, never by a
    /// discovery constructor.
    pub capabilities: Option<cmdr_fs::volume::VolumeCapabilities>,
    /// Single-letter menu shortcut, present only on favorite rows.
    pub favorite_shortcut: Option<String>,
    /// Present only on a favorite row: which volume it lives on and whether a pick can get there
    /// (`favorites/target.rs`). `None` on every other row.
    pub favorite_target: Option<crate::favorites::target::FavoriteTarget>,
    /// What a tab at this volume's root is called, when the mount directory's
    /// name isn't it: an SMB share's own name. `None` everywhere else, where the
    /// root folder's name is the label.
    ///
    /// ❗ A share whose name another server's mount already holds is mounted at a
    /// disambiguated path (`/Volumes/public-1`), and its tab read "public-1" while
    /// the header said "public on localhost:11482".
    pub root_label: Option<String>,
    /// The account a mounted SMB share is signed in as right now, as the mount
    /// table records it (`GUEST` for a guest mount). `None` for everything else.
    ///
    /// ❗ The LIVE account, which the hub shows while a share is connected: the saved
    /// row's account is for the next connect, and showing it read "Connected … as
    /// otheruser" over a mount signed in as testuser.
    pub mount_account: Option<String>,
}

/// Lets discovery collapse a doubly-mounted filesystem down to one published
/// location (`cmdr_fs::volume::canonical_root`). The Linux twin implements the
/// same trait on its own `LocationInfo`, which is what keeps the rule shared
/// without merging the two types.
impl cmdr_fs::volume::canonical_root::MountRootCandidate for LocationInfo {
    fn volume_id(&self) -> &str {
        &self.id
    }

    fn mount_root(&self) -> &str {
        &self.path
    }
}

impl PublishedLocation for LocationInfo {
    fn location_id(&self) -> &str {
        &self.id
    }

    fn location_path(&self) -> &str {
        &self.path
    }

    fn is_favorite(&self) -> bool {
        self.category == LocationCategory::Favorite
    }
}

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

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

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

    let (mount_point, fs_type) = get_mount_point(path)?;

    // Drain autoreleased ObjC objects (NSURL, NSString).
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&mount_point));

        let smb = get_smb_mount_info(&mount_point);
        let name = resolved_volume_name(smb.as_ref(), || get_volume_name(&url, &mount_point));
        // Local mounts only, as `get_attached_volumes` answers it: a network mount ends through
        // its session, and the lookup can hang on a dead one.
        let is_ejectable = !is_network_fs_type(Some(&fs_type)) && is_volume_ejectable(&url, &mount_point);
        let supports_trash = supports_trash_for_fs_type(Some(&fs_type));
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
            is_cloud_mount,
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
/// (shared with `volumes_linux/`), whose header says which row wins a clash.
pub fn list_locations() -> Vec<LocationInfo> {
    // One `getfsstat(MNT_NOWAIT)` snapshot for the whole listing. A table nobody could read means
    // no attached rows this pass (the switcher keeps what it has and the next discovery asks
    // again); ❌ nothing downstream may read that as "the disk is empty".
    let mounts = mounts::enumerate_mounts().unwrap_or_default();
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
fn get_favorites(mounts: &[mounts::MountEntry]) -> Vec<LocationInfo> {
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
    mounts: &[mounts::MountEntry],
    fda_pending: bool,
    exists: impl FnOnce(&Path) -> bool,
) -> LocationInfo {
    use crate::favorites::target::{FavoriteTarget, OnDisk, Probe, on_disk};

    let path = Path::new(&favorite.path);
    let fs_type = path
        .is_absolute()
        .then(|| mounts::fs_type_under(mounts, path))
        .flatten()
        .map(str::to_string);
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
        supports_trash: fs_type.is_some() && supports_trash_for_fs_type(fs_type.as_deref()),
        path: favorite.path,
        category: LocationCategory::Favorite,
        icon,
        is_ejectable: false,
        fs_type,
        mount_is_read_only: false,
        is_disk_image: false,
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
            mount_is_read_only: false,
            is_disk_image: false,
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
    })
}

/// The volume-shaped name for the same struct: discovery calls every entry a
/// location, and everything downstream of it calls the ones backed by a mount a
/// volume.
pub use LocationInfo as VolumeInfo;

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

    fn snapshot() -> Vec<mounts::MountEntry> {
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
