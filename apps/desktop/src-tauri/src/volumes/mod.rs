//! Volume and location discovery for macOS.
//!
//! Provides a Finder-like location picker with:
//! - Favorites (from Finder sidebar)
//! - Main volume (Macintosh HD)
//! - Attached volumes (external drives)
//! - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
//! - Network locations

pub mod disk_image;
pub mod watcher;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use crate::file_system::volume::SmbConnectionState;

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

/// Determine whether a filesystem type supports trash.
///
/// Local filesystems (APFS, HFS+, ext4, btrfs, xfs, zfs) support trash.
/// Network filesystems (SMB, NFS, AFP, WebDAV, CIFS, FUSE-based SSH) and
/// non-Mac formats (FAT32/exFAT) don't reliably support it. Unknown types
/// default to `true` (optimistic: trash failure is caught at operation time).
pub fn supports_trash_for_fs_type(fs_type: Option<&str>) -> bool {
    let Some(fs) = fs_type else { return true };
    let fs_lower = fs.to_ascii_lowercase();
    match fs_lower.as_str() {
        "apfs" | "hfs" | "ext4" | "btrfs" | "xfs" | "zfs" => true,
        "smbfs" | "nfs" | "afpfs" | "webdav" | "cifs" | "fuse.sshfs" | "msdos" | "exfat" => false,
        _ => true,
    }
}

/// Returns true if the filesystem type is SMB (macOS `smbfs` or Linux `cifs`).
/// Enriches volume entries with SMB connection state from the `VolumeManager`.
///
/// For each volume, looks up the registered `Volume` in `VolumeManager` and reads
/// its `smb_connection_state()` if any. SMB shares without a direct smb2 session
/// (typical OS-mounted shares before auto-upgrade) are tagged as `OsMount` so
/// the FE picker can show the yellow indicator.
///
/// Used by the `list_volumes` IPC call, the `volumes-changed` push, and the MCP
/// `cmdr://state` resource — all three need the same enrichment, so it lives in
/// one place. Add new enrichment fields here, not at each call site.
pub fn enrich_smb_connection_state(volumes: &mut [LocationInfo]) {
    let manager = crate::file_system::get_volume_manager();
    for vol in volumes.iter_mut() {
        if let Some(registered) = manager.get(&vol.id) {
            vol.smb_connection_state = registered.smb_connection_state();
        }

        // SMB shares without a direct smb2 connection show as OsMount (yellow).
        // This covers pre-existing mounts registered as LocalPosixVolume at startup.
        if vol.smb_connection_state.is_none() && is_smb_fs_type(vol.fs_type.as_deref()) {
            vol.smb_connection_state = Some(SmbConnectionState::OsMount);
        }
    }
}

pub fn is_smb_fs_type(fs_type: Option<&str>) -> bool {
    matches!(fs_type, Some("smbfs" | "cifs"))
}

/// Returns true for network filesystem types whose metadata syscalls can block
/// indefinitely on a hung mount (SMB, NFS, AFP, WebDAV, FTP).
///
/// Volume discovery derives these volumes' fields purely from the non-blocking
/// `getfsstat` snapshot and skips the blocking NSURL / NSWorkspace / DiskArbitration
/// enrichment, so one dead network mount can't stall discovery of the others. See
/// `get_attached_volumes` and `DETAILS.md` § "Hung mounts".
pub fn is_network_fs_type(fs_type: Option<&str>) -> bool {
    matches!(fs_type, Some("smbfs" | "cifs" | "nfs" | "afpfs" | "webdav" | "ftp"))
}

/// Information about an SMB mount extracted from `statfs`.
#[derive(Debug, Clone)]
pub struct SmbMountInfo {
    /// Server hostname or IP (for example, "192.168.1.111").
    pub server: String,
    /// Share name (for example, "naspi").
    pub share: String,
    /// Username if present in the mount source (for example, "david").
    pub username: Option<String>,
    /// Port from the mount source (for example, 10480). Defaults to 445.
    pub port: u16,
}

/// Extracts SMB server, share, and username from a mount path via `statfs`.
///
/// On macOS, `statfs.f_mntfromname` for SMB mounts looks like:
/// - `//user@192.168.1.111/share` (authenticated)
/// - `//192.168.1.111/share` (guest)
///
/// Returns `None` if the path is not an SMB mount or parsing fails.
pub fn get_smb_mount_info(mount_path: &str) -> Option<SmbMountInfo> {
    use std::ffi::CString;

    let c_path = CString::new(mount_path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated C string from `mount_path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };

    // Check filesystem type is SMB
    let fs_type: String = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();
    if !is_smb_fs_type(Some(&fs_type)) {
        return None;
    }

    // Extract mount source (for example, "//david@192.168.1.111/naspi")
    let mount_from: String = stat
        .f_mntfromname
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();

    parse_smb_mount_source(&mount_from)
}

/// Parses an SMB mount source string like `//user@host/share` or `//host/share`.
fn parse_smb_mount_source(source: &str) -> Option<SmbMountInfo> {
    // Strip leading "//"
    let rest = source.strip_prefix("//")?;

    // Split into "user@host/share" or "host/share"
    let (server_part, share) = rest.split_once('/')?;
    if share.is_empty() {
        return None;
    }

    let (username, server) = if let Some((user, host)) = server_part.split_once('@') {
        (Some(user.to_string()), host.to_string())
    } else {
        (None, server_part.to_string())
    };

    // Extract port if present (for example, "192.168.1.111:10480")
    let (server, port) = if let Some((host, port_str)) = server.rsplit_once(':') {
        (host.to_string(), port_str.parse().unwrap_or(445))
    } else {
        (server, 445)
    };

    Some(SmbMountInfo {
        server,
        share: share.to_string(),
        username,
        port,
    })
}

/// Resolve a path to its mount point and filesystem type via `statfs()`.
///
/// On APFS firmlinks, normalizes `/System/Volumes/Data` to `/` (because
/// `statfs("/Users/foo")` returns `/System/Volumes/Data` on modern macOS).
///
/// If `statfs` fails (ENOENT for a deleted directory), walks up parent
/// directories until one succeeds. Returns `None` only if even `/` fails.
pub(crate) fn get_mount_point(path: &str) -> Option<(String, String)> {
    use std::ffi::CString;

    let mut current = path.to_string();
    loop {
        if let Ok(c_path) = CString::new(current.as_str()) {
            let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
            // SAFETY: `c_path` is a valid NUL-terminated C string from `current`, and `stat` is an
            // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
            let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
            if result == 0 {
                // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
                let stat = unsafe { stat.assume_init() };

                let mount_point: String = stat
                    .f_mntonname
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                let fs_type: String = stat
                    .f_fstypename
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();

                // APFS firmlink normalization: /System/Volumes/Data → /
                let mount_point = if mount_point == "/System/Volumes/Data" {
                    "/".to_string()
                } else {
                    mount_point
                };

                return Some((mount_point, fs_type));
            }
        }

        // Walk up to parent on failure (handles deleted directories)
        if current == "/" || current.is_empty() {
            return None;
        }
        current = Path::new(&current)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if current.is_empty() {
            current = "/".to_string();
        }
    }
}

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

/// Read the filesystem type for a path using `libc::statfs`.
///
/// Returns `None` if the `statfs` call fails (for example, the volume was
/// ejected between listing and probing).
fn get_fs_type(path: &str) -> Option<String> {
    use std::ffi::CString;

    let c_path = CString::new(path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();

    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    // f_fstypename is [c_char; 16] on macOS. Convert to &str.
    let name_bytes: Vec<u8> = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(name_bytes).ok()
}

/// Whether the volume mounted at `path` is read-only, from the `statfs` `MNT_RDONLY` flag.
///
/// Covers any read-only mount (a read-only `.dmg`, a locked SD card, an optical disc),
/// powering the copy/move write guard and the 🔒 indicator. Returns `false` if `statfs`
/// fails (treat an unprobeable mount as writable: the OS write attempt is the backstop).
fn read_only_from_statfs(path: &str) -> bool {
    use std::ffi::CString;

    let Ok(c_path) = CString::new(path) else {
        return false;
    };
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return false;
    }
    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    (stat.f_flags & libc::MNT_RDONLY as u32) != 0
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

/// One entry from the kernel mount table, as returned by `getfsstat(MNT_NOWAIT)`.
///
/// Every field comes straight out of `statfs` with no follow-up syscall, so
/// building one never talks to the backing filesystem. That is the whole reason
/// discovery uses `getfsstat` instead of NSFileManager's volume enumeration: the
/// enumeration `getattrlist`s every mount, which blocks 30s–forever on a hung
/// network mount and froze the app at launch. See `DETAILS.md` § "Hung mounts".
struct MountEntry {
    /// Mount point, e.g. `/Volumes/naspi` (`f_mntonname`).
    mount_point: String,
    /// Filesystem type, e.g. `apfs`, `exfat`, `smbfs` (`f_fstypename`).
    fs_type: String,
    /// Mount source, e.g. `//david@192.168.1.111/naspi` for SMB (`f_mntfromname`).
    mount_from: String,
    /// Whether the mount carries the `MNT_RDONLY` flag.
    is_read_only: bool,
}

/// Snapshot the kernel mount table without blocking on any mount.
///
/// `getfsstat(MNT_NOWAIT)` returns cached mount metadata and never round-trips to
/// a filesystem, so a wedged network mount can't stall it — unlike `MNT_WAIT`,
/// which is what makes plain `df` hang. Returns an empty list if the syscall fails.
fn enumerate_mounts() -> Vec<MountEntry> {
    // First pass: ask how many mounts exist (null buffer writes nothing).
    // SAFETY: `getfsstat(NULL, 0, flags)` is the documented count query; with a
    // null buffer and zero size the kernel only returns the mount count.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count <= 0 {
        return Vec::new();
    }

    // A few slots of slack in case a mount appears between the two calls.
    let capacity = count as usize + 4;
    let mut buf: Vec<libc::statfs> = Vec::with_capacity(capacity);
    let bufsize = (capacity * size_of::<libc::statfs>()) as libc::c_int;
    // SAFETY: `buf` has room for `capacity` `statfs` records and `bufsize` matches
    // that byte length, so the kernel fills at most `capacity` records and returns
    // how many it wrote.
    let filled = unsafe { libc::getfsstat(buf.as_mut_ptr(), bufsize, libc::MNT_NOWAIT) };
    if filled <= 0 {
        return Vec::new();
    }
    // SAFETY: the kernel initialized `filled` records; clamp to our capacity in
    // case the mount table grew past the slack between the two calls.
    unsafe { buf.set_len((filled as usize).min(capacity)) };

    buf.iter()
        .map(|s| MountEntry {
            mount_point: cstr_field_to_string(&s.f_mntonname),
            fs_type: cstr_field_to_string(&s.f_fstypename),
            mount_from: cstr_field_to_string(&s.f_mntfromname),
            is_read_only: (s.f_flags & libc::MNT_RDONLY as u32) != 0,
        })
        .collect()
}

/// Convert a NUL-terminated `c_char` array from `statfs` into a `String`
/// (UTF-8 lossy, since mount points and volume names can be non-ASCII).
fn cstr_field_to_string(field: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Whether a mount point should surface as an attached volume in the switcher.
///
/// Mirrors the old NSFileManager filter: only `/Volumes/*`, never the boot
/// volume, the hidden system volumes, or cloud-storage placeholder mounts. Also
/// skips dot-prefixed hidden volumes (e.g. `/Volumes/.timemachine`) that
/// NSFileManager's `SkipHiddenVolumes` used to drop. Pure, so it's unit-testable.
fn is_attached_volume_path(path: &str) -> bool {
    if !path.starts_with("/Volumes/") {
        return false;
    }
    if path.starts_with("/System") || path.contains("/Preboot") || path.contains("/Recovery") {
        return false;
    }
    if path.contains("/Library/CloudStorage") {
        return false;
    }
    // Hidden volume (leading dot on the `/Volumes` child), e.g. `/Volumes/.timemachine`.
    if let Some(name) = Path::new(path).file_name().and_then(|n| n.to_str())
        && name.starts_with('.')
    {
        return false;
    }
    true
}

/// Metadata for a LOCAL mount, resolved via blocking macOS APIs (NSURL +
/// DiskArbitration + NSWorkspace). Only computed for local mounts — network
/// mounts skip it so a hung mount never blocks discovery.
struct LocalVolumeMeta {
    name: String,
    is_ejectable: bool,
    icon: Option<String>,
    is_disk_image: bool,
}

/// The `(id, display_name)` for a network mount, from the non-blocking
/// `f_mntfromname`. SMB mounts key their ID on `(server, port, share)` and read
/// as "share on server"; other network mounts fall back to a path-derived ID and
/// name. No syscalls: everything comes from the `getfsstat` snapshot. Pure.
fn network_id_and_name(mount: &MountEntry) -> (String, String) {
    if is_smb_fs_type(Some(&mount.fs_type))
        && let Some(info) = parse_smb_mount_source(&mount.mount_from)
    {
        let display = crate::network::smb_upgrade::friendly_server_name(&info.server);
        let name = format!("{} on {}", info.share, display);
        return (smb_volume_id(&info.server, info.port, &info.share), name);
    }
    (
        path_to_id(&mount.mount_point),
        volume_name_from_path(&mount.mount_point),
    )
}

/// Classify one mount-table entry into a switcher [`LocationInfo`], or `None` if
/// it isn't a user-facing attached volume.
///
/// Network mounts (SMB, NFS, WebDAV, …) are built purely from the non-blocking
/// `statfs` data already in `mount`; `resolve_local` is NOT called for them. That
/// is the guarantee a hung network mount can't block discovery of the other
/// volumes. Local mounts call `resolve_local` for their name, ejectability, icon,
/// and disk-image status (safe: local disks don't hang). Splitting the blocking
/// enrichment behind a closure also keeps the classification unit-testable.
fn build_attached_location(
    mount: &MountEntry,
    resolve_local: impl FnOnce(&str) -> LocalVolumeMeta,
) -> Option<LocationInfo> {
    let path = mount.mount_point.as_str();
    if !is_attached_volume_path(path) {
        return None;
    }
    let fs_type = mount.fs_type.clone();
    let supports_trash = supports_trash_for_fs_type(Some(&fs_type));

    let (id, name, is_ejectable, icon, is_disk_image) = if is_network_fs_type(Some(&fs_type)) {
        // Network mount: derive everything from the non-blocking snapshot. Never
        // touch NSURL / NSWorkspace / DiskArbitration here — those are exactly the
        // calls that hang on a dead mount. `is_ejectable` is cosmetically moot for
        // network mounts (the eject affordance keys on `smbConnectionState`, and
        // the eject flow forces it true), so a safe `false` costs nothing.
        let (id, name) = network_id_and_name(mount);
        (id, name, false, None, false)
    } else {
        // Local mount: safe to run the blocking enrichment.
        let meta = resolve_local(path);
        (
            path_to_id(path),
            meta.name,
            meta.is_ejectable,
            meta.icon,
            meta.is_disk_image,
        )
    };

    Some(LocationInfo {
        id,
        name,
        path: path.to_string(),
        category: LocationCategory::AttachedVolume,
        icon,
        is_ejectable,
        fs_type: Some(fs_type),
        supports_trash,
        is_read_only: mount.is_read_only,
        is_disk_image,
        smb_connection_state: None,
        usb_speed: None,
    })
}

/// Get attached volumes (external drives, USB, network mounts, etc.).
///
/// Enumerates via the non-blocking `getfsstat` snapshot, then enriches only LOCAL
/// mounts through blocking macOS APIs. A hung network mount contributes its
/// getfsstat-derived entry and never blocks the others. See `DETAILS.md`
/// § "Hung mounts".
pub fn get_attached_volumes() -> Vec<LocationInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    // Drain autoreleased ObjC objects from the per-local-mount NSURL enrichment.
    // Called from spawn_blocking / helper threads that lack AppKit's pool.
    autoreleasepool(|_| {
        let mut volumes: Vec<LocationInfo> = enumerate_mounts()
            .iter()
            .filter_map(|mount| {
                build_attached_location(mount, |path| {
                    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                    LocalVolumeMeta {
                        name: get_volume_name(&url, path),
                        is_ejectable: get_bool_resource(&url, "NSURLVolumeIsEjectableKey").unwrap_or(false),
                        icon: get_icon_for_path(path),
                        is_disk_image: disk_image::is_disk_image_mount(path),
                    }
                })
            })
            .collect();

        // Sort alphabetically
        volumes.sort_by_key(|a| a.name.to_lowercase());
        volumes
    })
}

/// Get cloud drives (Dropbox, iCloud, Google Drive, etc.).
pub fn get_cloud_drives() -> Vec<LocationInfo> {
    // Skip during FDA-pending onboarding: enumerating `~/Library/CloudStorage`
    // touches an FDA-gated path. The list re-emits via `volumes-changed`
    // once the gate clears (see `start_indexing_after_fda_decision`).
    if crate::fda_gate::is_fda_pending_runtime() {
        return Vec::new();
    }

    let mut drives = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // iCloud Drive
    let icloud_path = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if icloud_path.exists() {
        drives.push(cloud_volume_info(
            ICLOUD_VOLUME_ID.to_string(),
            "iCloud Drive".to_string(),
            &icloud_path,
        ));
    }

    // Scan ~/Library/CloudStorage for other cloud providers
    let cloud_storage_path = home.join("Library/CloudStorage");
    if let Ok(entries) = std::fs::read_dir(&cloud_storage_path) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Parse cloud provider name from directory
                let (provider_name, id) = parse_cloud_provider_name(dir_name);
                if !provider_name.is_empty() {
                    drives.push(cloud_volume_info(id, provider_name, &path));
                }
            }
        }
    }

    // Sort alphabetically
    drives.sort_by_key(|a| a.name.to_lowercase());
    drives
}

/// Build a `CloudDrive` [`LocationInfo`] for a cloud-drive root folder. Shared
/// by [`get_cloud_drives`] (the switcher list) and [`resolve_cloud_drive_for_path`]
/// (the per-path resolver) so the two can't drift on ID, category, or fields.
fn cloud_volume_info(id: String, name: String, root: &Path) -> LocationInfo {
    let path = root.to_string_lossy().to_string();
    let fs_type = get_fs_type(&path);
    let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
    LocationInfo {
        id,
        name,
        icon: get_icon_for_path(&path),
        path,
        category: LocationCategory::CloudDrive,
        is_ejectable: false,
        fs_type,
        supports_trash,
        is_read_only: false,
        is_disk_image: false,
        smb_connection_state: None,
        usb_speed: None,
    }
}

/// Resolve a path to its containing cloud drive, if any. Returns the same
/// `VolumeInfo` shape [`get_cloud_drives`] would for that drive, so the volume
/// switcher's checkmark (matched by `id`) lands on it.
fn resolve_cloud_drive_for_path(path: &str) -> Option<VolumeInfo> {
    let home = dirs::home_dir()?;
    let (id, name, root) = match_cloud_drive_root(&home, path)?;
    Some(cloud_volume_info(id, name, &root))
}

/// If `path` is the root of, or anywhere inside, a known cloud-drive folder,
/// return `(volume_id, display_name, cloud_root)`.
///
/// Cloud drives (iCloud Drive, Dropbox, Google Drive, …) are plain folders on
/// the data volume, so `statfs` resolves any path inside them to `/`. Without
/// this match, the volume switcher would highlight "Macintosh HD" instead of
/// the cloud drive whenever the user is anywhere inside one.
///
/// Pure (no I/O, matches by path prefix only) so it's unit-testable and cheap
/// to call on every navigation. The I/O wrapper is [`resolve_cloud_drive_for_path`].
fn match_cloud_drive_root(home: &Path, path: &str) -> Option<(String, String, PathBuf)> {
    let candidate = Path::new(path);

    // iCloud Drive: a fixed folder under the home directory.
    let icloud_root = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if candidate.starts_with(&icloud_root) {
        return Some((ICLOUD_VOLUME_ID.to_string(), "iCloud Drive".to_string(), icloud_root));
    }

    // Other providers: ~/Library/CloudStorage/<provider-dir>/… The first path
    // component under CloudStorage names the provider; deeper components are
    // subfolders we want to attribute to that same drive.
    let cloud_storage_root = home.join("Library/CloudStorage");
    let rel = candidate.strip_prefix(&cloud_storage_root).ok()?;
    let provider_dir = rel.components().next()?.as_os_str().to_str()?;
    let (name, id) = parse_cloud_provider_name(provider_dir);
    if name.is_empty() {
        return None;
    }
    Some((id, name, cloud_storage_root.join(provider_dir)))
}

/// Parse cloud provider name from CloudStorage directory name.
/// E.g., "Dropbox" -> "Dropbox", "GoogleDrive-email@gmail.com" -> "Google Drive"
fn parse_cloud_provider_name(dir_name: &str) -> (String, String) {
    if dir_name.starts_with("Dropbox") {
        return ("Dropbox".to_string(), "cloud-dropbox".to_string());
    }
    if dir_name.starts_with("GoogleDrive") {
        return ("Google Drive".to_string(), "cloud-google-drive".to_string());
    }
    if dir_name.starts_with("OneDrive") {
        // Handle OneDrive-Personal, OneDrive-Business, etc.
        if dir_name.contains("Business") {
            return (
                "OneDrive for Business".to_string(),
                "cloud-onedrive-business".to_string(),
            );
        }
        return ("OneDrive".to_string(), "cloud-onedrive".to_string());
    }
    if dir_name.starts_with("Box") {
        return ("Box".to_string(), "cloud-box".to_string());
    }
    if dir_name.starts_with("pCloud") {
        return ("pCloud".to_string(), "cloud-pcloud".to_string());
    }
    // Generic cloud provider
    if !dir_name.is_empty() {
        let clean_name = dir_name.split('-').next().unwrap_or(dir_name);
        return (clean_name.to_string(), format!("cloud-{}", clean_name.to_lowercase()));
    }
    (String::new(), String::new())
}

/// Display name derived purely from a mount path: the last path component, or
/// "Macintosh HD" for the boot volume. The non-blocking fallback used for network
/// mounts (`network_id_and_name`) and when an NSURL localized-name lookup misses.
fn volume_name_from_path(path: &str) -> String {
    if path == "/" {
        return "Macintosh HD".to_string();
    }
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// Get the display name for a volume.
fn get_volume_name(url: &objc2_foundation::NSURL, path: &str) -> String {
    // Try localized name first
    if let Some(name) = get_string_resource(url, "NSURLVolumeLocalizedNameKey") {
        return name;
    }
    if let Some(name) = get_string_resource(url, "NSURLVolumeNameKey") {
        return name;
    }
    // Fallback to path-based name
    volume_name_from_path(path)
}

pub(crate) use crate::file_system::volume::{path_to_id, smb_volume_id};

/// Volume ID for a mount path, SMB-aware.
///
/// For SMB mounts (smbfs), the ID is keyed by `(server, port, share)` via
/// [`smb_volume_id`], not by the path-shape. Two SMB shares with the same
/// case-folded name on different servers (a NAS sharing `Public`, a Docker
/// container sharing `public`) thus get distinct IDs, instead of colliding on
/// `volumespublic`. See [`smb_volume_id`] for the full rationale.
///
/// Falls back to [`path_to_id`] for non-SMB mounts and for SMB mounts where
/// `statfs` no longer recovers the mount info (typical right after unmount).
/// The unmount path should generally use [`VolumeManager::find_by_root`]
/// instead, which doesn't depend on `statfs`.
pub(crate) fn volume_id_for_mount(mount_path: &str) -> String {
    if let Some(info) = get_smb_mount_info(mount_path) {
        smb_volume_id(&info.server, info.port, &info.share)
    } else {
        path_to_id(mount_path)
    }
}

/// Get icon for a path as base64-encoded WebP.
///
/// Returns `None` while the FDA decision is pending. NSWorkspace icon
/// resolution touches several TCC-gated services (MediaLibrary, AppData,
/// Desktop/Documents/Downloads/Pictures/Movies/Music) even when the input
/// path itself isn't on those lists, so during onboarding we skip the
/// fetch and let the frontend fall back to a generic folder/volume icon.
/// `start_indexing_after_fda_decision` (deny path) and a fresh launch with
/// FDA granted (allow path) both clear the gate and re-emit
/// `volumes-changed`, populating icons.
fn get_icon_for_path(path: &str) -> Option<String> {
    if crate::fda_gate::is_fda_pending_runtime() {
        return None;
    }
    crate::icons::get_icon_for_path(path)
}

/// Get a resource value from an NSURL and convert it using the provided extractor.
fn get_nsurl_resource<T>(
    url: &objc2_foundation::NSURL,
    key: &str,
    extractor: impl FnOnce(objc2::rc::Retained<objc2::runtime::AnyObject>) -> Option<T>,
) -> Option<T> {
    use objc2::rc::Retained;
    use objc2_foundation::NSString;

    let key = NSString::from_str(key);
    let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
    // SAFETY: `url` is a live `NSURL` and `key` a live `NSString`; `getResourceValue:forKey:error:`
    // writes the looked-up value into `value` (left `None` when the key is absent) and the cached
    // resource value is autoreleased into the caller's pool. We only read `value` after success.
    let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

    if success.is_ok() {
        value.and_then(extractor)
    } else {
        None
    }
}

/// Get a boolean resource value from an NSURL.
fn get_bool_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<bool> {
    use objc2_foundation::NSNumber;
    get_nsurl_resource(url, key, |obj| obj.downcast::<NSNumber>().ok().map(|n| n.boolValue()))
}

/// Get a string resource value from an NSURL.
fn get_string_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<String> {
    use objc2_foundation::NSString;
    get_nsurl_resource(url, key, |obj| obj.downcast::<NSString>().ok().map(|s| s.to_string()))
}

/// Get a u64 resource value from an NSURL (for capacity values).
fn get_u64_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<u64> {
    use objc2_foundation::NSNumber;
    get_nsurl_resource(url, key, |obj| {
        obj.downcast::<NSNumber>().ok().map(|n| n.unsignedLongLongValue())
    })
}

/// Information about volume space.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpaceInfo {
    /// In bytes.
    pub total_bytes: u64,
    /// In bytes.
    pub available_bytes: u64,
}

/// Get space information for a volume containing the given path.
pub fn get_volume_space(path: &str) -> Option<VolumeSpaceInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::NSURL;

    // Drain autoreleased ObjC objects (NSURL, NSString, NSNumber).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(path));

        let total = get_u64_resource(&url, "NSURLVolumeTotalCapacityKey")?;
        let available = get_u64_resource(&url, "NSURLVolumeAvailableCapacityForImportantUsageKey")
            .filter(|&v| v > 0)
            .or_else(|| get_u64_resource(&url, "NSURLVolumeAvailableCapacityKey"))?;

        Some(VolumeSpaceInfo {
            total_bytes: total,
            available_bytes: available,
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
    fn test_parse_cloud_provider_name() {
        assert_eq!(
            parse_cloud_provider_name("Dropbox"),
            ("Dropbox".to_string(), "cloud-dropbox".to_string())
        );
        assert_eq!(
            parse_cloud_provider_name("GoogleDrive-user@gmail.com"),
            ("Google Drive".to_string(), "cloud-google-drive".to_string())
        );
        assert_eq!(
            parse_cloud_provider_name("OneDrive-Personal"),
            ("OneDrive".to_string(), "cloud-onedrive".to_string())
        );
    }

    #[test]
    fn test_match_cloud_drive_root() {
        let home = Path::new("/Users/test");
        let id = |p: &str| match_cloud_drive_root(home, p).map(|(id, ..)| id);

        // iCloud Drive: root and any descendant resolve to the iCloud volume.
        let icloud = "/Users/test/Library/Mobile Documents/com~apple~CloudDocs";
        assert_eq!(id(icloud).as_deref(), Some("cloud-icloud"));
        assert_eq!(
            id(&format!("{icloud}/Projects/notes.md")).as_deref(),
            Some("cloud-icloud")
        );

        // Dropbox: the bug repro. A deep subfolder must still highlight Dropbox.
        let dropbox = "/Users/test/Library/CloudStorage/Dropbox";
        assert_eq!(id(dropbox).as_deref(), Some("cloud-dropbox"));
        assert_eq!(
            id(&format!("{dropbox}/Work/2026/Q2/report.pdf")).as_deref(),
            Some("cloud-dropbox")
        );

        // Google Drive: the CloudStorage dir carries the account suffix.
        assert_eq!(
            id("/Users/test/Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive/x").as_deref(),
            Some("cloud-google-drive")
        );

        // Non-cloud paths resolve to no cloud drive (statfs handles them).
        assert_eq!(id("/"), None);
        assert_eq!(id("/Users/test/Documents"), None);
        assert_eq!(id("/Volumes/External/photos"), None);
        // The CloudStorage container itself isn't a cloud drive.
        assert_eq!(id("/Users/test/Library/CloudStorage"), None);
        // A sibling that merely shares a name prefix must not match (component-wise).
        assert_eq!(
            id("/Users/test/Library/Mobile Documents/com~apple~CloudDocsBackup"),
            None
        );

        // The full tuple carries name and the cloud root (not the navigated subpath).
        let (id, name, root) = match_cloud_drive_root(home, &format!("{dropbox}/Work")).expect("Dropbox match");
        assert_eq!(id, "cloud-dropbox");
        assert_eq!(name, "Dropbox");
        assert_eq!(root, PathBuf::from(dropbox));
    }

    #[test]
    fn test_path_to_id() {
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/Volumes/External"), "volumesexternal");
    }

    #[test]
    fn test_get_volume_space_root() {
        let space = get_volume_space("/");
        assert!(space.is_some(), "Should get space info for root volume");

        let space = space.unwrap();
        assert!(space.total_bytes > 0, "Total bytes should be positive");
        assert!(space.available_bytes > 0, "Available bytes should be positive");
        assert!(
            space.available_bytes <= space.total_bytes,
            "Available should be <= total"
        );
    }

    #[test]
    fn test_get_volume_space_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let space = get_volume_space(home.to_str().unwrap());
        assert!(space.is_some(), "Should get space info for home directory");
    }

    #[test]
    fn test_get_volume_space_nonexistent() {
        // Nonexistent paths return None - the NSURL resource API doesn't resolve to ancestor volumes
        let space = get_volume_space("/nonexistent/path/that/does/not/exist");
        assert!(space.is_none(), "Nonexistent paths should return None");
    }

    // ========================================================================
    // Filesystem type and trash support tests
    // ========================================================================

    #[test]
    fn test_supports_trash_local_filesystems() {
        assert!(supports_trash_for_fs_type(Some("apfs")));
        assert!(supports_trash_for_fs_type(Some("hfs")));
        assert!(supports_trash_for_fs_type(Some("ext4")));
        assert!(supports_trash_for_fs_type(Some("btrfs")));
        assert!(supports_trash_for_fs_type(Some("xfs")));
        assert!(supports_trash_for_fs_type(Some("zfs")));
    }

    #[test]
    fn test_supports_trash_network_filesystems() {
        assert!(!supports_trash_for_fs_type(Some("smbfs")));
        assert!(!supports_trash_for_fs_type(Some("nfs")));
        assert!(!supports_trash_for_fs_type(Some("afpfs")));
        assert!(!supports_trash_for_fs_type(Some("webdav")));
        assert!(!supports_trash_for_fs_type(Some("cifs")));
        assert!(!supports_trash_for_fs_type(Some("fuse.sshfs")));
    }

    #[test]
    fn test_supports_trash_removable_formats() {
        assert!(!supports_trash_for_fs_type(Some("msdos")));
        assert!(!supports_trash_for_fs_type(Some("exfat")));
    }

    #[test]
    fn test_supports_trash_case_insensitive() {
        assert!(supports_trash_for_fs_type(Some("APFS")));
        assert!(supports_trash_for_fs_type(Some("HFS")));
        assert!(supports_trash_for_fs_type(Some("EXT4")));
        assert!(supports_trash_for_fs_type(Some("BTRFS")));
        assert!(!supports_trash_for_fs_type(Some("SMBFS")));
        assert!(!supports_trash_for_fs_type(Some("NFS")));
        assert!(!supports_trash_for_fs_type(Some("CIFS")));
        assert!(!supports_trash_for_fs_type(Some("ExFAT")));
        assert!(!supports_trash_for_fs_type(Some("MSDOS")));
    }

    #[test]
    fn test_supports_trash_unknown_defaults_true() {
        assert!(supports_trash_for_fs_type(Some("ntfs")));
    }

    #[test]
    fn test_supports_trash_none_defaults_true() {
        assert!(supports_trash_for_fs_type(None));
    }

    // ========================================================================
    // Mount point resolution tests
    // ========================================================================

    #[test]
    fn test_get_mount_point_root() {
        let result = get_mount_point("/");
        assert!(result.is_some(), "Root should resolve to a mount point");
        let (mount_point, fs_type) = result.unwrap();
        assert_eq!(mount_point, "/", "Root mount point should be /");
        assert!(
            fs_type == "apfs" || fs_type == "hfs",
            "Root should be apfs or hfs, got: {fs_type}"
        );
    }

    #[test]
    fn test_get_mount_point_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let result = get_mount_point(home.to_str().unwrap());
        assert!(result.is_some(), "Home should resolve to a mount point");
        let (mount_point, _fs_type) = result.unwrap();
        // APFS firmlink normalization: must NOT return /System/Volumes/Data
        assert_eq!(
            mount_point, "/",
            "Home mount point should be / (not /System/Volumes/Data)"
        );
    }

    #[test]
    fn test_get_mount_point_nonexistent() {
        let result = get_mount_point("/nonexistent/deeply/nested/path");
        assert!(result.is_some(), "Nonexistent path should walk up to root");
        let (mount_point, _fs_type) = result.unwrap();
        assert_eq!(mount_point, "/", "Nonexistent path should resolve to /");
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
    fn test_get_fs_type_root() {
        let fs_type = get_fs_type("/");
        assert!(fs_type.is_some(), "Root volume should have a filesystem type");
        let fs = fs_type.unwrap();
        assert!(!fs.is_empty(), "Filesystem type should not be empty");
        // On modern macOS, root is APFS
        assert!(fs == "apfs" || fs == "hfs", "Root should be apfs or hfs, got: {fs}");
    }

    #[test]
    fn test_get_fs_type_nonexistent_path() {
        let fs_type = get_fs_type("/nonexistent/path/that/does/not/exist");
        // statfs on a nonexistent path fails
        assert!(fs_type.is_none(), "Nonexistent path should return None");
    }

    #[test]
    fn test_get_fs_type_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let fs_type = get_fs_type(home.to_str().unwrap());
        assert!(fs_type.is_some(), "Home dir should have a filesystem type");
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

    // ========================================================================
    // Hung-mount guard: getfsstat-based discovery (Bug: dead mount froze launch)
    // ========================================================================

    fn mount(mount_point: &str, fs_type: &str, mount_from: &str, is_read_only: bool) -> MountEntry {
        MountEntry {
            mount_point: mount_point.to_string(),
            fs_type: fs_type.to_string(),
            mount_from: mount_from.to_string(),
            is_read_only,
        }
    }

    /// A `resolve_local` that fails the test if invoked. Used to prove a network
    /// mount is classified WITHOUT any blocking NSURL/DiskArbitration/NSWorkspace
    /// call — the guarantee that a hung mount can't stall discovery.
    fn forbidden_resolver(_path: &str) -> LocalVolumeMeta {
        panic!("resolve_local must NOT run for a network mount");
    }

    #[test]
    fn is_network_fs_type_covers_the_hanging_filesystems() {
        for fs in ["smbfs", "cifs", "nfs", "afpfs", "webdav", "ftp"] {
            assert!(is_network_fs_type(Some(fs)), "{fs} should count as network");
        }
        for fs in ["apfs", "hfs", "exfat", "msdos"] {
            assert!(!is_network_fs_type(Some(fs)), "{fs} should count as local");
        }
        assert!(!is_network_fs_type(None));
    }

    #[test]
    fn attached_volume_path_filter_matches_switcher_rules() {
        assert!(is_attached_volume_path("/Volumes/MyDrive"));
        assert!(is_attached_volume_path("/Volumes/naspi"));
        // Boot volume, system, cloud, and non-/Volumes paths are excluded.
        assert!(!is_attached_volume_path("/"));
        assert!(!is_attached_volume_path("/System/Volumes/Data"));
        assert!(!is_attached_volume_path("/Volumes/Recovery"));
        assert!(!is_attached_volume_path("/Users/david"));
        assert!(!is_attached_volume_path("/Volumes/Foo/Library/CloudStorage/Dropbox"));
        // Hidden volumes (NSFileManager used to drop these via SkipHiddenVolumes).
        assert!(!is_attached_volume_path("/Volumes/.timemachine"));
    }

    #[test]
    fn volume_name_from_path_uses_last_component() {
        assert_eq!(volume_name_from_path("/"), "Macintosh HD");
        assert_eq!(volume_name_from_path("/Volumes/My Backup"), "My Backup");
    }

    #[test]
    fn smb_mount_classifies_without_blocking_enrichment() {
        // A wedged SMB mount must be classified purely from getfsstat data; the
        // blocking resolver must never run, so a dead NAS can't stall discovery.
        let m = mount("/Volumes/naspi", "smbfs", "//david@192.168.1.111/naspi", false);
        let loc = build_attached_location(&m, forbidden_resolver).expect("SMB mount is an attached volume");

        assert_eq!(loc.id, smb_volume_id("192.168.1.111", 445, "naspi"));
        assert!(loc.name.contains("naspi"), "name shows the share: {}", loc.name);
        assert!(loc.name.contains(" on "), "name shows 'share on server': {}", loc.name);
        assert_eq!(loc.fs_type.as_deref(), Some("smbfs"));
        assert_eq!(loc.category, LocationCategory::AttachedVolume);
        assert!(!loc.is_ejectable, "network mounts take the safe non-blocking default");
        assert!(loc.icon.is_none());
        assert!(!loc.is_disk_image);
    }

    #[test]
    fn nfs_mount_classifies_without_blocking_enrichment() {
        let m = mount("/Volumes/export", "nfs", "server:/export", true);
        let loc = build_attached_location(&m, forbidden_resolver).expect("NFS mount is an attached volume");
        assert_eq!(loc.id, path_to_id("/Volumes/export"));
        assert_eq!(loc.name, "export");
        assert!(loc.is_read_only, "MNT_RDONLY flag propagates from getfsstat");
        assert_eq!(loc.fs_type.as_deref(), Some("nfs"));
    }

    #[test]
    fn local_mount_runs_the_enrichment_closure() {
        // Local mounts DO get the (safe) blocking enrichment; here we inject a
        // fake so the test stays hermetic and asserts the values flow through.
        let m = mount("/Volumes/USB", "exfat", "/dev/disk4s1", false);
        let loc = build_attached_location(&m, |path| {
            assert_eq!(path, "/Volumes/USB");
            LocalVolumeMeta {
                name: "My USB".to_string(),
                is_ejectable: true,
                icon: Some("icon-data".to_string()),
                is_disk_image: false,
            }
        })
        .expect("local mount is an attached volume");

        assert_eq!(loc.id, path_to_id("/Volumes/USB"));
        assert_eq!(loc.name, "My USB");
        assert!(loc.is_ejectable);
        assert_eq!(loc.icon.as_deref(), Some("icon-data"));
        assert_eq!(loc.fs_type.as_deref(), Some("exfat"));
    }

    #[test]
    fn filtered_mount_yields_no_location() {
        // The boot volume and system mounts are dropped before any enrichment.
        assert!(build_attached_location(&mount("/", "apfs", "/dev/disk3s1", false), forbidden_resolver).is_none());
        assert!(
            build_attached_location(&mount("/System/Volumes/Data", "apfs", "x", false), forbidden_resolver).is_none()
        );
    }

    #[test]
    fn enumerate_mounts_finds_the_boot_volume() {
        // getfsstat should always return at least the root mount on a live system,
        // and it must never block (this test would hang if it did).
        let mounts = enumerate_mounts();
        assert!(!mounts.is_empty(), "getfsstat returned no mounts");
        assert!(mounts.iter().any(|m| m.mount_point == "/"), "root mount missing");
    }
}
