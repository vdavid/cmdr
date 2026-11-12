//! How each OS finds its volumes: the locations it lists, the mount a path sits on, its SMB
//! mounts, and the watcher that notices a mount come and go. A platform seam
//! (`docs/guides/platform-seams.md`).

use std::path::Path;

use cmdr_fs::volume::SpaceInfo;
use tauri::AppHandle;

use crate::volumes::{LocationInfo, NetworkMount, SmbMountInfo, VolumeInfo};

#[cfg_attr(target_os = "macos", path = "macos/mod.rs")]
#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

/// `(mount point, fs type, mount source)`.
type MountPointFacts = Option<(String, String, String)>;
/// `(mount point, mount source, fs type)` per row.
type MountRows = Option<Vec<(String, String, String)>>;
/// `(mount point, what its source says)` per SMB mount.
type SmbMounts = Option<Vec<(String, SmbMountInfo)>>;

const _: fn() -> Vec<LocationInfo> = list_locations;
const _: fn(&str) -> Option<VolumeInfo> = resolve_path_volume_fast;
const _: fn(&Path) -> Option<NetworkMount> = network_mount_under;
const _: fn(&mut [LocationInfo]) = enrich_from_volume_registry;
const _: fn(&str) -> Option<SmbMountInfo> = get_smb_mount_info;
const _: fn(&str) -> Option<SmbMountInfo> = smb_mount_info_at;
const _: fn() -> SmbMounts = smb_mounts;
const _: fn() -> MountRows = mount_rows;
const _: fn() -> Option<Vec<String>> = registrable_mount_roots;
const _: fn(&str) -> MountPointFacts = get_mount_point;
const _: fn(&str) -> String = volume_id_for_mount;
const _: fn(&str) -> String = local_volume_id_for_path;
const _: fn(&str) -> Option<u64> = mount_identity_at;
const _: fn(u64) -> Option<bool> = has_mount_identity;
const _: fn(&Path) -> Option<(String, String)> = mount_type_and_source_for;
const _: fn(&str) -> Option<SpaceInfo> = live_volume_space;
const _: fn(&AppHandle) = start_volume_watcher;
const _: fn() = stop_volume_watcher;
const _: &str = NETWORK_FS_TYPE;
const _: bool = LOCAL_DISK_CASE_SENSITIVE;

/// Every location the switcher lists, deduplicated: favorites, the main volume, attached and
/// network mounts, and cloud drives. Blocking: run it off the async thread
/// (`volume_listing::discover_local`).
pub(crate) use imp::list_locations;

/// The volume containing `path`, from the mount table and per-path reads only, without listing
/// every location. `None` when no mount answers for it.
pub(crate) use imp::resolve_path_volume_fast;

/// The network mount `path` lexically lies on, from the kernel's cached table, ❌ never a `statfs`
/// on the path. `None` for a local disk, or when the table can't be read.
pub(crate) use imp::network_mount_under;

/// Copies what only the registered `Volume` knows (its capabilities, its connection state) onto
/// each discovered location. The one place a `LocationInfo` learns from the registry.
pub(crate) use imp::enrich_from_volume_registry;

/// What the SMB mount AT `mount_path` says about its server, share, and account; `None` for
/// anything else, a folder inside a share included.
pub(crate) use imp::get_smb_mount_info;

/// [`get_smb_mount_info`] from the mount table alone, ❌ never a syscall on the mount itself, so
/// a dead share can't block it.
pub(crate) use imp::smb_mount_info_at;

/// Every SMB mount in the table, `None` when the table can't be read (❌ never read as "none").
pub(crate) use imp::smb_mounts;

/// The whole mount table, another account's mounts included, `None` when it can't be read.
pub(crate) use imp::mount_rows;

/// Every mount root the registry should hold, `None` when the table can't be read.
pub(crate) use imp::registrable_mount_roots;

/// The mount `path` sits on, as `(mount point, fs type, source)`.
pub(crate) use imp::get_mount_point;

/// The volume ID for the mount at `mount_path`. ❌ Never derive one any other way.
pub(crate) use imp::volume_id_for_mount;

/// The local volume a local path lies on, from the path alone (no mount table, no syscall), for
/// a command handed a path without its volume. ❌ Never default a missing volume to
/// `DEFAULT_VOLUME_ID` by hand: on Windows each drive and UNC share is its own volume
/// (`cmdr_fs::volume::path_grammar`).
pub(crate) use imp::local_volume_id_for_path;

/// The non-blocking identity of the mount at `path` (its filesystem ID or device), `None` when
/// the table doesn't list it.
pub(crate) use imp::mount_identity_at;

/// Whether a mount with this identity is still in the table, `None` when the table can't say.
pub(crate) use imp::has_mount_identity;

/// The `(fs type, source)` of the mount `path` lexically lies on, from the table alone.
pub(crate) use imp::mount_type_and_source_for;

/// The free-space figure a polling readout shows for the filesystem at `path`.
pub(crate) use imp::live_volume_space;

/// Starts watching for mounts and unmounts, registering and announcing each. Call once, from
/// the setup hook.
pub(crate) use imp::watcher::start_volume_watcher;

/// Stops what [`start_volume_watcher`] started, where something outlives the window. Idempotent.
pub(crate) use imp::watcher::stop_volume_watcher;

/// The `fs_type` the synthetic `network` volume reports: whatever this OS calls an SMB mount, so
/// a consumer classifying by fs type sees a share and not an unknown filesystem.
pub(crate) use imp::NETWORK_FS_TYPE;

/// Whether this OS's local disks tell `Test_user_1` from `test_user_1`, the answer every local-disk row's
/// `case_sensitive` carries. Read from no mount: a per-mount `pathconf` could block on a dead
/// one, so each arm states what its local filesystems do by default.
pub(crate) use imp::LOCAL_DISK_CASE_SENSITIVE;

// What only macOS has: disk images, whole-disk units, the DiskArbitration unmount approver, and
// the readers the eject and SMB paths use beyond the seam.
#[cfg(target_os = "macos")]
pub(crate) use imp::{
    disk_units, get_attached_volumes, get_cloud_drives, is_network_fs_type, is_smb_fs_type, mount_points_with_rows,
    unmount_approver, watcher,
};

#[cfg(all(test, target_os = "macos"))]
pub(crate) use imp::location_for_test;

// What only Linux has: GVFS names its SMB share folders, and the mount helper reads them back.
#[cfg(target_os = "linux")]
pub(crate) use imp::parse_gvfs_smb_dirname;
